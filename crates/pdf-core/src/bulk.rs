//! Bulk signing engine: apply one signing rule to many files in parallel.
//!
//! The engine is UI- and PDFium-free. Anchor text is located through the [`AnchorFinder`] trait
//! (implemented with PDFium text search in the app). Each output is written atomically to a new,
//! never-existing file name; originals are never touched.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::fs_safety::{atomic_write, unique_output_path, Limits};
use crate::load::{load_file, LoadError};
use crate::sign::{apply, Content, Mode, Placement};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSel {
    First,
    Last,
    All,
    /// 1-based page number
    Number(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occurrence {
    First,
    Last,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Box starts at the right edge of the anchor text.
    Right,
    /// Box starts below the anchor text, left-aligned with it.
    Below,
    /// Box starts at the anchor's top-left corner.
    Same,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Position {
    /// Top-left corner of the signature box in view space (points, y down).
    Fixed { x: f32, y: f32 },
    Anchor { text: String, match_case: bool, occurrence: Occurrence, side: Side, offset_x: f32, offset_y: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateFormat {
    Iso,
    Dmy,
    Mdy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    pub fn format(&self, f: DateFormat) -> String {
        match f {
            DateFormat::Iso => format!("{:04}-{:02}-{:02}", self.year, self.month, self.day),
            DateFormat::Dmy => format!("{:02}/{:02}/{:04}", self.day, self.month, self.year),
            DateFormat::Mdy => format!("{:02}/{:02}/{:04}", self.month, self.day, self.year),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StampKind {
    Date(DateFormat),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct StampRule {
    pub kind: StampKind,
    pub font_size: f32,
    /// Offset of the stamp's top-left from the signature box's top-left, in points.
    pub offset_x: f32,
    pub offset_y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub width: f32,
    pub height: f32,
    pub pages: PageSel,
    pub position: Position,
    pub stamp: Option<StampRule>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnchorRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Finds text on pages. Implemented with PDFium search in the app.
pub trait AnchorFinder: Sync {
    /// For each requested 0-based page, hits in reading order as `(page, rect)`.
    fn find(&self, file: &Path, pages: &[usize], text: &str, match_case: bool) -> Result<Vec<(usize, AnchorRect)>, String>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Signed; `placed` signatures were written to `output`.
    Signed { output: PathBuf, placed: usize },
    Skipped { reason: String },
    Failed { error: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileResult {
    pub input: PathBuf,
    pub outcome: Outcome,
}

#[derive(Debug)]
pub enum JobError {
    EmptySuffix,
    NoFiles,
    OutputDir(String),
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySuffix => write!(f, "The output suffix must not be empty, so originals are never overwritten."),
            Self::NoFiles => write!(f, "No files were selected."),
            Self::OutputDir(e) => write!(f, "The output folder is not usable: {e}"),
        }
    }
}
impl std::error::Error for JobError {}

pub struct Job<'a> {
    pub files: Vec<PathBuf>,
    pub out_dir: PathBuf,
    pub suffix: String,
    pub rule: Rule,
    pub content: Content,
    pub mode: Mode,
    pub today: Date,
    pub workers: usize,
    pub limits: Limits,
    pub finder: &'a dyn AnchorFinder,
}

/// Default worker count: available parallelism minus one (at least one).
/// Note: this is *logical* cores, the closest portable std approximation of "physical minus one".
pub fn default_workers() -> usize {
    std::thread::available_parallelism().map(|n| n.get().saturating_sub(1).max(1)).unwrap_or(1)
}

enum Plan {
    Place(Vec<(usize, f32, f32)>), // (page, x, y)
    Skip(String),
    Fail(String),
}

fn pages_for(sel: PageSel, count: usize) -> Result<Vec<usize>, String> {
    match sel {
        PageSel::First => Ok(vec![0]),
        PageSel::Last => Ok(vec![count - 1]),
        PageSel::All => Ok((0..count).collect()),
        PageSel::Number(n) if n >= 1 && n <= count => Ok(vec![n - 1]),
        PageSel::Number(n) => Err(format!("page {n} does not exist (document has {count} pages)")),
    }
}

fn plan(file: &Path, page_count: usize, rule: &Rule, finder: &dyn AnchorFinder) -> Plan {
    let pages = match pages_for(rule.pages, page_count) {
        Ok(p) => p,
        Err(r) => return Plan::Skip(r),
    };
    match &rule.position {
        Position::Fixed { x, y } => Plan::Place(pages.into_iter().map(|p| (p, *x, *y)).collect()),
        Position::Anchor { text, match_case, occurrence, side, offset_x, offset_y } => {
            let hits = match finder.find(file, &pages, text, *match_case) {
                Ok(h) => h,
                Err(e) => return Plan::Fail(format!("text search failed: {e}")),
            };
            let mut out = Vec::new();
            for &p in &pages {
                let on_page: Vec<&AnchorRect> = hits.iter().filter(|h| h.0 == p).map(|h| &h.1).collect();
                let chosen = match occurrence {
                    Occurrence::First => on_page.first(),
                    Occurrence::Last => on_page.last(),
                };
                if let Some(r) = chosen {
                    let (bx, by) = match side {
                        Side::Right => (r.x + r.w, r.y),
                        Side::Below => (r.x, r.y + r.h),
                        Side::Same => (r.x, r.y),
                    };
                    out.push((p, bx + offset_x, by + offset_y));
                }
            }
            if out.is_empty() {
                Plan::Skip(format!("anchor text \"{text}\" not found"))
            } else {
                Plan::Place(out)
            }
        }
    }
}

fn build_placements(planned: &[(usize, f32, f32)], rule: &Rule, content: &Content, today: Date) -> Vec<Placement> {
    let mut v = Vec::new();
    for &(page, x, y) in planned {
        v.push(Placement { page, x, y, w: rule.width, h: rule.height, rotation: 0.0, content: content.clone() });
        if let Some(s) = &rule.stamp {
            let text = match &s.kind {
                StampKind::Date(f) => today.format(*f),
                StampKind::Text(t) => t.clone(),
            };
            let w = (text.chars().count() as f32 * s.font_size * 0.6 + 4.0).max(8.0);
            v.push(Placement {
                page,
                x: x + s.offset_x,
                y: y + s.offset_y,
                w,
                h: (s.font_size * 1.4).ceil(),
                rotation: 0.0,
                content: Content::Text { text, font_size: s.font_size, color: [0.0, 0.0, 0.0] },
            });
        }
    }
    v
}

/// Result of applying a rule to one file without writing anything.
pub enum Prepared {
    /// `first_page` is the first page (0-based) that received a signature.
    Ready { bytes: Vec<u8>, placed: usize, first_page: usize },
    Skipped(String),
    Failed(String),
}

fn load_err_outcome(e: LoadError) -> Prepared {
    match e {
        LoadError::NeedsPassword | LoadError::WrongPassword => Prepared::Skipped("password protected".into()),
        LoadError::TooLarge { .. } | LoadError::TooManyPages { .. } => Prepared::Skipped(e.to_string()),
        other => Prepared::Failed(other.to_string()),
    }
}

/// Load `file`, apply the rule, and return the signed bytes (no output file is created).
pub fn prepare(file: &Path, job: &Job<'_>) -> Prepared {
    let mut doc = match load_file(file, None, &job.limits) {
        Ok(d) => d,
        Err(e) => return load_err_outcome(e),
    };
    let count = doc.get_pages().len();
    let planned = match plan(file, count, &job.rule, job.finder) {
        Plan::Place(p) => p,
        Plan::Skip(r) => return Prepared::Skipped(r),
        Plan::Fail(e) => return Prepared::Failed(e),
    };
    let placements = build_placements(&planned, &job.rule, &job.content, job.today);
    if let Err(e) = apply(&mut doc, &placements, job.mode) {
        return Prepared::Failed(e.to_string());
    }
    let mut bytes = Vec::new();
    if let Err(e) = doc.save_to(&mut bytes) {
        return Prepared::Failed(format!("could not serialise: {e}"));
    }
    Prepared::Ready { bytes, placed: planned.len(), first_page: planned.iter().map(|p| p.0).min().unwrap_or(0) }
}

fn process_file(file: &Path, job: &Job<'_>) -> FileResult {
    let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| prepare(file, job))) {
        Err(_) => Outcome::Failed { error: "internal error while processing this file".into() },
        Ok(Prepared::Skipped(reason)) => Outcome::Skipped { reason },
        Ok(Prepared::Failed(error)) => Outcome::Failed { error },
        Ok(Prepared::Ready { bytes, placed, .. }) => {
            let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
            // unique_output_path + create-new semantics: retry if another worker grabbed the name.
            let mut result = Outcome::Failed { error: "could not create an output file".into() };
            for _ in 0..50 {
                let out = unique_output_path(&job.out_dir, &stem, &job.suffix, "pdf");
                match reserve(&out) {
                    Ok(()) => {
                        result = match atomic_write(&out, &bytes, false) {
                            Ok(()) => Outcome::Signed { output: out, placed },
                            Err(e) => {
                                let _ = std::fs::remove_file(&out);
                                Outcome::Failed { error: format!("could not write output: {e}") }
                            }
                        };
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => {
                        result = Outcome::Failed { error: format!("could not create output: {e}") };
                        break;
                    }
                }
            }
            result
        }
    };
    FileResult { input: file.to_path_buf(), outcome }
}

/// Atomically claim a file name (fails if it exists) so parallel workers never collide.
fn reserve(path: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new().write(true).create_new(true).open(path).map(|_| ())
}

#[derive(Debug, Clone)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub last: FileResult,
}

/// Run the job. Results are returned in input order. `cancel` stops picking up new files (files
/// already being processed finish); unprocessed files are reported as skipped ("cancelled").
pub fn run(job: &Job<'_>, cancel: &AtomicBool, progress: &(dyn Fn(Progress) + Sync)) -> Result<Vec<FileResult>, JobError> {
    if job.suffix.trim().is_empty() {
        return Err(JobError::EmptySuffix);
    }
    if job.files.is_empty() {
        return Err(JobError::NoFiles);
    }
    std::fs::create_dir_all(&job.out_dir).map_err(|e| JobError::OutputDir(e.to_string()))?;
    if !job.out_dir.is_dir() {
        return Err(JobError::OutputDir("not a folder".into()));
    }
    let total = job.files.len();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<FileResult>>> = (0..total).map(|_| Mutex::new(None)).collect();
    let workers = job.workers.clamp(1, 64).min(total);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= total {
                    break;
                }
                let r = process_file(&job.files[i], job);
                *slots[i].lock().unwrap_or_else(|p| p.into_inner()) = Some(r.clone());
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                progress(Progress { done: d, total, last: r });
            });
        }
    });
    Ok(slots
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            m.into_inner().unwrap_or_else(|p| p.into_inner()).unwrap_or_else(|| FileResult {
                input: job.files[i].clone(),
                outcome: Outcome::Skipped { reason: "cancelled".into() },
            })
        })
        .collect())
}

/// What would happen to each file, without writing anything (used by the preview).
pub fn dry_run(job: &Job<'_>) -> Vec<FileResult> {
    job.files
        .iter()
        .map(|f| {
            let outcome = match prepare(f, job) {
                Prepared::Ready { placed, .. } => Outcome::Signed { output: PathBuf::new(), placed },
                Prepared::Skipped(reason) => Outcome::Skipped { reason },
                Prepared::Failed(error) => Outcome::Failed { error },
            };
            FileResult { input: f.clone(), outcome }
        })
        .collect()
}

// ---------------------------------------------------------------- reporting

fn csv_cell(s: &str) -> String {
    // Neutralise spreadsheet formula injection, then apply RFC 4180 quoting.
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_string() };
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s }
}

pub fn report_csv(results: &[FileResult]) -> String {
    let mut out = String::from("input,status,output,signatures_placed,reason\r\n");
    for r in results {
        let (status, output, placed, reason) = match &r.outcome {
            Outcome::Signed { output, placed } => ("signed", output.to_string_lossy().into_owned(), placed.to_string(), String::new()),
            Outcome::Skipped { reason } => ("skipped", String::new(), String::new(), reason.clone()),
            Outcome::Failed { error } => ("failed", String::new(), String::new(), error.clone()),
        };
        out.push_str(&format!(
            "{},{},{},{},{}\r\n",
            csv_cell(&r.input.to_string_lossy()),
            status,
            csv_cell(&output),
            placed,
            csv_cell(&reason)
        ));
    }
    out
}

pub fn report_text(results: &[FileResult]) -> String {
    let signed = results.iter().filter(|r| matches!(r.outcome, Outcome::Signed { .. })).count();
    let skipped = results.iter().filter(|r| matches!(r.outcome, Outcome::Skipped { .. })).count();
    let failed = results.len() - signed - skipped;
    let mut s = format!("Bulk signing report: {signed} signed, {skipped} skipped, {failed} failed ({} files)\n\n", results.len());
    for r in results {
        let name = r.input.to_string_lossy();
        match &r.outcome {
            Outcome::Signed { output, placed } => s.push_str(&format!("SIGNED   {name} -> {} ({placed} placed)\n", output.display())),
            Outcome::Skipped { reason } => s.push_str(&format!("SKIPPED  {name}: {reason}\n")),
            Outcome::Failed { error } => s.push_str(&format!("FAILED   {name}: {error}\n")),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_format() {
        let d = Date { year: 2026, month: 3, day: 9 };
        assert_eq!(d.format(DateFormat::Iso), "2026-03-09");
        assert_eq!(d.format(DateFormat::Dmy), "09/03/2026");
        assert_eq!(d.format(DateFormat::Mdy), "03/09/2026");
    }

    #[test]
    fn page_selection() {
        assert_eq!(pages_for(PageSel::Last, 5).unwrap(), vec![4]);
        assert_eq!(pages_for(PageSel::All, 3).unwrap(), vec![0, 1, 2]);
        assert_eq!(pages_for(PageSel::Number(2), 3).unwrap(), vec![1]);
        assert!(pages_for(PageSel::Number(4), 3).is_err());
        assert!(pages_for(PageSel::Number(0), 3).is_err());
    }

    #[test]
    fn csv_escapes_and_neutralises_formulas() {
        let r = vec![
            FileResult { input: "a,b.pdf".into(), outcome: Outcome::Skipped { reason: "say \"hi\"".into() } },
            FileResult { input: "=cmd.pdf".into(), outcome: Outcome::Failed { error: "line1\nline2".into() } },
        ];
        let csv = report_csv(&r);
        assert!(csv.contains("\"a,b.pdf\",skipped,,,\"say \"\"hi\"\"\""));
        assert!(csv.contains("'=cmd.pdf"));
        assert!(csv.contains("\"line1\nline2\""));
        assert!(csv.starts_with("input,status,output,signatures_placed,reason\r\n"));
    }

    #[test]
    fn default_workers_is_at_least_one() {
        assert!(default_workers() >= 1);
    }
}
