//! Bulk signing commands: rule files, preview, background run with progress events.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use pdf_core::bulk::{
    self, AnchorFinder, AnchorRect, Date, DateFormat, Job, Occurrence, Outcome, PageSel, Position, Prepared, Rule,
    Side, StampKind, StampRule,
};
use pdf_core::sign::Mode;
use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::AppError;
use crate::signatures;
use crate::state::AppState;

type Result<T> = std::result::Result<T, AppError>;

#[derive(Default)]
pub struct BulkState {
    jobs: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    next: AtomicU64,
    out_dir: Mutex<Option<PathBuf>>,
}

// ------------------------------------------------------------------ rule file

/// On-disk / over-the-wire rule. Documented in docs/bulk-rule-format.md.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleFile {
    pub version: u32,
    pub signature: String,
    pub width: f32,
    pub height: f32,
    /// "first" | "last" | "all" | a 1-based page number
    pub pages: PagesField,
    pub position: PositionField,
    #[serde(default)]
    pub stamp: Option<StampField>,
    #[serde(default)]
    pub flatten: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PagesField {
    Number(usize),
    Name(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum PositionField {
    #[serde(rename = "fixed")]
    Fixed { x: f32, y: f32 },
    #[serde(rename = "anchor", rename_all = "camelCase")]
    Anchor {
        text: String,
        #[serde(default)]
        match_case: bool,
        #[serde(default = "first")]
        occurrence: String,
        #[serde(default = "right")]
        side: String,
        #[serde(default)]
        offset_x: f32,
        #[serde(default)]
        offset_y: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum StampField {
    #[serde(rename = "date", rename_all = "camelCase")]
    Date {
        #[serde(default = "iso")]
        format: String,
        #[serde(default = "twelve")]
        font_size: f32,
        #[serde(default)]
        offset_x: f32,
        #[serde(default)]
        offset_y: f32,
    },
    #[serde(rename = "text", rename_all = "camelCase")]
    Text {
        text: String,
        #[serde(default = "twelve")]
        font_size: f32,
        #[serde(default)]
        offset_x: f32,
        #[serde(default)]
        offset_y: f32,
    },
}

fn first() -> String {
    "first".into()
}
fn right() -> String {
    "right".into()
}
fn iso() -> String {
    "iso".into()
}
fn twelve() -> f32 {
    12.0
}

fn bad(msg: impl Into<String>) -> AppError {
    AppError::new("invalid_edit", msg)
}

fn finite(v: f32, what: &str) -> Result<f32> {
    if v.is_finite() && v.abs() <= 100_000.0 {
        Ok(v)
    } else {
        Err(bad(format!("{what} is not a valid number.")))
    }
}

/// Validate a rule and convert it to the engine's types.
pub fn to_rule(r: &RuleFile) -> Result<Rule> {
    if r.version != 1 {
        return Err(bad(format!("Unsupported rule file version {}. This app reads version 1.", r.version)));
    }
    if !(4.0..=2000.0).contains(&r.width) || !(4.0..=2000.0).contains(&r.height) {
        return Err(bad("Signature width and height must be between 4 and 2000 points."));
    }
    let pages = match &r.pages {
        PagesField::Number(n) if *n >= 1 => PageSel::Number(*n),
        PagesField::Number(_) => return Err(bad("Page numbers start at 1.")),
        PagesField::Name(n) => match n.as_str() {
            "first" => PageSel::First,
            "last" => PageSel::Last,
            "all" => PageSel::All,
            other => {
                return Err(bad(format!(
                    "\"{other}\" is not a valid page selection (use first, last, all or a number)."
                )))
            }
        },
    };
    let position = match &r.position {
        PositionField::Fixed { x, y } => Position::Fixed { x: finite(*x, "x")?, y: finite(*y, "y")? },
        PositionField::Anchor { text, match_case, occurrence, side, offset_x, offset_y } => {
            if text.trim().is_empty() || text.chars().count() > 200 {
                return Err(bad("The anchor text must be 1-200 characters."));
            }
            Position::Anchor {
                text: text.clone(),
                match_case: *match_case,
                occurrence: match occurrence.as_str() {
                    "first" => Occurrence::First,
                    "last" => Occurrence::Last,
                    o => return Err(bad(format!("Unknown occurrence \"{o}\"."))),
                },
                side: match side.as_str() {
                    "right" => Side::Right,
                    "below" => Side::Below,
                    "same" => Side::Same,
                    o => return Err(bad(format!("Unknown side \"{o}\"."))),
                },
                offset_x: finite(*offset_x, "offsetX")?,
                offset_y: finite(*offset_y, "offsetY")?,
            }
        }
    };
    let stamp = match &r.stamp {
        None => None,
        Some(StampField::Date { format, font_size, offset_x, offset_y }) => Some(StampRule {
            kind: StampKind::Date(match format.as_str() {
                "iso" => DateFormat::Iso,
                "dmy" => DateFormat::Dmy,
                "mdy" => DateFormat::Mdy,
                o => return Err(bad(format!("Unknown date format \"{o}\"."))),
            }),
            font_size: stamp_size(*font_size)?,
            offset_x: finite(*offset_x, "stamp offsetX")?,
            offset_y: finite(*offset_y, "stamp offsetY")?,
        }),
        Some(StampField::Text { text, font_size, offset_x, offset_y }) => {
            if text.is_empty() || text.chars().count() > 200 {
                return Err(bad("The stamp text must be 1-200 characters."));
            }
            Some(StampRule {
                kind: StampKind::Text(text.clone()),
                font_size: stamp_size(*font_size)?,
                offset_x: finite(*offset_x, "stamp offsetX")?,
                offset_y: finite(*offset_y, "stamp offsetY")?,
            })
        }
    };
    Ok(Rule { width: finite(r.width, "width")?, height: finite(r.height, "height")?, pages, position, stamp })
}

fn stamp_size(s: f32) -> Result<f32> {
    if (4.0..=200.0).contains(&s) {
        Ok(s)
    } else {
        Err(bad("Stamp font size must be between 4 and 200 points."))
    }
}

/// Parse and validate rule-file JSON (size-limited by the caller).
pub fn parse_rule(json: &[u8]) -> Result<RuleFile> {
    let r: RuleFile = serde_json::from_slice(json).map_err(|e| bad(format!("The rule file is not valid: {e}")))?;
    to_rule(&r)?;
    Ok(r)
}

// ------------------------------------------------------------------ PDFium anchor finder

struct PdfiumFinder;

impl AnchorFinder for PdfiumFinder {
    fn find(
        &self,
        file: &Path,
        pages: &[usize],
        text: &str,
        mc: bool,
    ) -> std::result::Result<Vec<(usize, AnchorRect)>, String> {
        let d = pdf_render::Document::open(file, None).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for &p in pages {
            for h in d.search_page(p, text, mc).map_err(|e| e.to_string())? {
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for r in &h.rects {
                    x0 = x0.min(r.x);
                    y0 = y0.min(r.y);
                    x1 = x1.max(r.x + r.w);
                    y1 = y1.max(r.y + r.h);
                }
                out.push((p, AnchorRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }));
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------ commands

fn check_files(state: &AppState, files: &[String]) -> Result<Vec<PathBuf>> {
    if files.is_empty() {
        return Err(bad("No files were selected."));
    }
    if files.len() > 20_000 {
        return Err(bad("Too many files in one batch (limit 20,000)."));
    }
    files.iter().map(|f| state.allowed.check_readable(Path::new(f), &state.limits()).map_err(Into::into)).collect()
}

#[tauri::command]
pub fn bulk_default_workers() -> usize {
    bulk::default_workers()
}

/// List the PDFs directly inside a user-chosen folder (not recursive). Grants each file.
#[tauri::command]
pub async fn bulk_pick_folder(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<String>> {
    let dir = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().blocking_pick_folder().and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))?;
    let Some(dir) = dir else { return Ok(vec![]) };
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| AppError::new("open_failed", e.to_string()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for f in files.into_iter().take(20_000) {
        out.push(pdf_core::display_path(&state.allowed.grant(&f)?));
    }
    Ok(out)
}

#[tauri::command]
pub async fn bulk_pick_output_dir(
    app: AppHandle,
    state: State<'_, AppState>,
    bulk: State<'_, BulkState>,
) -> Result<Option<String>> {
    let _ = &state;
    let dir = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().blocking_pick_folder().and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))?;
    let Some(dir) = dir else { return Ok(None) };
    let canon = dir.canonicalize().map_err(|e| AppError::new("open_failed", e.to_string()))?;
    *bulk.out_dir.lock().unwrap_or_else(|p| p.into_inner()) = Some(canon.clone());
    Ok(Some(pdf_core::display_path(&canon)))
}

#[tauri::command]
pub async fn bulk_rule_load(app: AppHandle) -> Result<Option<RuleFile>> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().add_filter("Signing rule", &["json"]).blocking_pick_file().and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))?;
    let Some(p) = picked else { return Ok(None) };
    let meta = std::fs::metadata(&p).map_err(|e| AppError::new("open_failed", e.to_string()))?;
    if meta.len() > 1024 * 1024 {
        return Err(bad("The rule file is too large."));
    }
    let bytes = std::fs::read(&p).map_err(|e| AppError::new("open_failed", e.to_string()))?;
    parse_rule(&bytes).map(Some)
}

#[tauri::command]
pub async fn bulk_rule_save(app: AppHandle, rule: RuleFile) -> Result<Option<String>> {
    to_rule(&rule)?;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Signing rule", &["json"])
            .set_file_name("signing-rule.json")
            .blocking_save_file()
            .and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))?;
    let Some(mut p) = picked else { return Ok(None) };
    if p.extension().is_none() {
        p.set_extension("json");
    }
    let bytes = serde_json::to_vec_pretty(&rule).map_err(|e| AppError::new("internal", e.to_string()))?;
    pdf_core::atomic_write(&p, &bytes, true).map_err(|e| AppError::new("save_failed", e.to_string()))?;
    Ok(Some(pdf_core::display_path(&p)))
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ItemDto {
    input: String,
    /// "signed" | "skipped" | "failed"
    status: &'static str,
    detail: String,
    output: Option<String>,
}

fn dto(r: &bulk::FileResult) -> ItemDto {
    let input = pdf_core::display_path(&r.input);
    match &r.outcome {
        Outcome::Signed { output, placed } => ItemDto {
            input,
            status: "signed",
            detail: format!("{placed} signature(s)"),
            output: (!output.as_os_str().is_empty()).then(|| pdf_core::display_path(output)),
        },
        Outcome::Skipped { reason } => ItemDto { input, status: "skipped", detail: reason.clone(), output: None },
        Outcome::Failed { error } => ItemDto { input, status: "failed", detail: error.clone(), output: None },
    }
}

/// Apply the rule to every file without writing anything: what would be signed, skipped or fail.
#[tauri::command]
pub async fn bulk_preview(rule: RuleFile, files: Vec<String>, state: State<'_, AppState>) -> Result<Vec<ItemDto>> {
    let paths = check_files(&state, &files)?;
    let limits = state.limits();
    let dir = signatures::library_dir(&state);
    let list = signatures::library_list(&dir);
    let content = signatures::content_for(&dir, &list, &rule.signature)?;
    let rule_e = to_rule(&rule)?;
    let mode = if rule.flatten { Mode::Flatten } else { Mode::Annotate };
    tauri::async_runtime::spawn_blocking(move || {
        let job = Job {
            files: paths,
            out_dir: PathBuf::new(),
            suffix: "_signed".into(),
            rule: rule_e,
            content,
            mode,
            today: today_utc(),
            workers: 1,
            limits,
            finder: &PdfiumFinder,
        };
        Ok(bulk::dry_run(&job).iter().map(dto).collect())
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))?
}

fn today_utc() -> Date {
    // Civil date from the Unix epoch (Howard Hinnant's algorithm); the UI passes the local date for real runs.
    let secs =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    civil_from_days(secs.div_euclid(86_400))
}

pub fn civil_from_days(z: i64) -> Date {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    Date { year: (y + if m <= 2 { 1 } else { 0 }) as i32, month: m, day: d }
}

/// PNG preview of the rule applied to one file (the first page that receives a signature).
#[tauri::command]
pub async fn bulk_preview_image(rule: RuleFile, file: String, state: State<'_, AppState>) -> Result<Response> {
    let paths = check_files(&state, &[file])?;
    let job_inputs = (state.limits(), signatures::library_dir(&state));
    let list = signatures::library_list(&job_inputs.1);
    let content = signatures::content_for(&job_inputs.1, &list, &rule.signature)?;
    let rule_e = to_rule(&rule)?;
    let mode = if rule.flatten { Mode::Flatten } else { Mode::Annotate };
    let png = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>> {
        let job = Job {
            files: paths.clone(),
            out_dir: PathBuf::new(),
            suffix: "_signed".into(),
            rule: rule_e,
            content,
            mode,
            today: today_utc(),
            workers: 1,
            limits: job_inputs.0,
            finder: &PdfiumFinder,
        };
        match bulk::prepare(&paths[0], &job) {
            Prepared::Ready { bytes, first_page, .. } => {
                let doc = pdf_render::Document::from_bytes(bytes, None)?;
                Ok(doc.render_page(first_page, 1.4)?.png)
            }
            Prepared::Skipped(r) => Err(bad(format!("This file would be skipped: {r}"))),
            Prepared::Failed(e) => Err(bad(format!("This file would fail: {e}"))),
        }
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))??;
    Ok(Response::new(png))
}

#[derive(Deserialize)]
pub struct TodayDto {
    year: i32,
    month: u32,
    day: u32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    job_id: u64,
    done: usize,
    total: usize,
    item: ItemDto,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FinishedEvent {
    job_id: u64,
    items: Vec<ItemDto>,
    report_path: Option<String>,
    error: Option<String>,
    cancelled: bool,
}

/// Start a bulk job in the background. Progress and completion arrive as events.
#[tauri::command]
pub async fn bulk_run<R: tauri::Runtime>(
    app: AppHandle<R>,
    rule: RuleFile,
    files: Vec<String>,
    workers: usize,
    today: TodayDto,
    state: State<'_, AppState>,
    bulk_state: State<'_, BulkState>,
) -> Result<u64> {
    let paths = check_files(&state, &files)?;
    let out_dir = bulk_state
        .out_dir
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
        .ok_or_else(|| bad("Choose an output folder first."))?;
    let date = Date { year: today.year, month: today.month.clamp(1, 12), day: today.day.clamp(1, 31) };
    let dir = signatures::library_dir(&state);
    let list = signatures::library_list(&dir);
    let content = signatures::content_for(&dir, &list, &rule.signature)?;
    let rule_e = to_rule(&rule)?;
    let mode = if rule.flatten { Mode::Flatten } else { Mode::Annotate };
    let limits = state.limits();

    let id = bulk_state.next.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = Arc::new(AtomicBool::new(false));
    bulk_state.jobs.lock().unwrap_or_else(|p| p.into_inner()).insert(id, cancel.clone());

    std::thread::spawn(move || {
        let job = Job {
            files: paths,
            out_dir: out_dir.clone(),
            suffix: "_signed".into(),
            rule: rule_e,
            content,
            mode,
            today: date,
            workers: workers.max(1),
            limits,
            finder: &PdfiumFinder,
        };
        let app2 = app.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bulk::run(&job, &cancel, &move |p| {
                let _ = app2.emit(
                    "bulk-progress",
                    ProgressEvent { job_id: id, done: p.done, total: p.total, item: dto(&p.last) },
                );
            })
        }));
        let finished = match result {
            Ok(Ok(results)) => {
                let csv = bulk::report_csv(&results);
                let rp = pdf_core::unique_output_path(&out_dir, "signing-report", "", "csv");
                let report_path =
                    pdf_core::atomic_write(&rp, csv.as_bytes(), false).ok().map(|_| pdf_core::display_path(&rp));
                FinishedEvent {
                    job_id: id,
                    items: results.iter().map(dto).collect(),
                    report_path,
                    error: None,
                    cancelled: cancel.load(Ordering::Relaxed),
                }
            }
            Ok(Err(e)) => FinishedEvent {
                job_id: id,
                items: vec![],
                report_path: None,
                error: Some(e.to_string()),
                cancelled: false,
            },
            Err(_) => FinishedEvent {
                job_id: id,
                items: vec![],
                report_path: None,
                error: Some("The job stopped unexpectedly.".into()),
                cancelled: false,
            },
        };
        let _ = app.emit("bulk-finished", finished);
        if let Some(b) = app.try_state::<BulkState>() {
            b.jobs.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
        }
    });
    Ok(id)
}

#[tauri::command]
pub fn bulk_cancel(job_id: u64, bulk_state: State<'_, BulkState>) {
    if let Some(flag) = bulk_state.jobs.lock().unwrap_or_else(|p| p.into_inner()).get(&job_id) {
        flag.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
      "version": 1, "signature": "abc123", "width": 150, "height": 50, "pages": "last",
      "position": { "type": "anchor", "text": "Signature:", "side": "right", "offsetX": 8, "offsetY": -4 },
      "stamp": { "type": "date", "format": "dmy", "fontSize": 10, "offsetY": 52 },
      "flatten": true
    }"#;

    #[test]
    fn parses_a_valid_rule_with_defaults() {
        let r = parse_rule(GOOD.as_bytes()).unwrap();
        let e = to_rule(&r).unwrap();
        assert_eq!(e.pages, PageSel::Last);
        assert!(matches!(
            e.position,
            Position::Anchor { occurrence: Occurrence::First, side: Side::Right, match_case: false, .. }
        ));
        assert!(matches!(e.stamp, Some(StampRule { kind: StampKind::Date(DateFormat::Dmy), .. })));
        assert!(r.flatten);
    }

    #[test]
    fn page_selection_forms() {
        let mut r = parse_rule(GOOD.as_bytes()).unwrap();
        r.pages = PagesField::Number(3);
        assert_eq!(to_rule(&r).unwrap().pages, PageSel::Number(3));
        r.pages = PagesField::Number(0);
        assert!(to_rule(&r).is_err());
        r.pages = PagesField::Name("middle".into());
        assert!(to_rule(&r).is_err());
        let j = GOOD.replace("\"last\"", "5");
        assert_eq!(to_rule(&parse_rule(j.as_bytes()).unwrap()).unwrap().pages, PageSel::Number(5));
    }

    #[test]
    fn rejects_bad_rules() {
        for (from, to) in [
            ("\"version\": 1", "\"version\": 2"),
            ("\"width\": 150", "\"width\": 0"),
            ("\"width\": 150", "\"width\": 99999"),
            ("\"text\": \"Signature:\"", "\"text\": \"\""),
            ("\"side\": \"right\"", "\"side\": \"diagonal\""),
            ("\"format\": \"dmy\"", "\"format\": \"yyyy\""),
            ("\"fontSize\": 10", "\"fontSize\": 1"),
        ] {
            let j = GOOD.replace(from, to);
            assert!(parse_rule(j.as_bytes()).is_err(), "{to} should be rejected");
        }
        assert!(parse_rule(b"not json").is_err());
        assert!(parse_rule(br#"{"version":1,"signature":"a","width":10,"height":10,"pages":"first","position":{"type":"fixed","x":1,"y":2},"extra":1}"#).is_err(), "unknown fields are rejected");
    }

    #[test]
    fn rule_roundtrips_through_json() {
        let r = parse_rule(GOOD.as_bytes()).unwrap();
        let again = parse_rule(&serde_json::to_vec(&r).unwrap()).unwrap();
        assert_eq!(r, again);
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), Date { year: 1970, month: 1, day: 1 });
        assert_eq!(civil_from_days(19_723), Date { year: 2024, month: 1, day: 1 });
        assert_eq!(civil_from_days(20_572), Date { year: 2026, month: 4, day: 29 });
        assert_eq!(civil_from_days(11_016), Date { year: 2000, month: 2, day: 29 });
    }

    /// Drives the real `bulk_run` command through Tauri's mock runtime and checks that the
    /// progress and finished events reach a listener, with real PDFium anchor search.
    #[test]
    fn bulk_run_emits_progress_then_finished_events() {
        use std::sync::mpsc;
        use tauri::{
            test::{get_ipc_response, mock_builder, mock_context, noop_assets},
            webview::InvokeRequest,
            Listener, WebviewWindowBuilder,
        };

        let Some(dir) = std::env::var_os("PDFIUM_DIR") else {
            if std::env::var_os("PDFIUM_REQUIRED").is_some() {
                panic!("PDFIUM_DIR not set");
            }
            eprintln!("skipping: no PDFium");
            return;
        };
        pdf_render::init(Path::new(&dir)).expect("pdfium");

        let root = std::env::temp_dir().join(format!("bulk-events-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (inp, out, data) = (root.join("in"), root.join("out"), root.join("data"));
        for d in [&inp, &out, &data.join("signatures")] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(
            data.join("signatures/index.json"),
            br#"[{"id":"abc123","name":"t","kind":"drawn","created":0,"aspect":3.0,"drawn":{"strokes":[[[0,0,0.5],[30,10,0.5]]],"view":[30,10],"width":2,"color":[0,0,0]}}]"#,
        ).unwrap();
        let pdf = |text: &str| {
            format!("%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 400]/Contents 4 0 R/Resources<</Font<</F1 5 0 R>>>>>>endobj\n4 0 obj<</Length 60>>stream\nBT /F1 14 Tf 20 100 Td ({text}) Tj ET\nendstream endobj\n5 0 obj<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>endobj\ntrailer<</Root 1 0 R/Size 6>>\n%%EOF\n")
        };
        let files: Vec<String> =
            [("a.pdf", "Signature: ____"), ("b.pdf", "nothing here"), ("c.pdf", "Signature: ____")]
                .iter()
                .map(|(n, t)| {
                    let p = inp.join(n);
                    std::fs::write(&p, pdf(t)).unwrap();
                    p.to_string_lossy().into_owned()
                })
                .collect();

        let state =
            AppState::new(pdf_core::AllowedPaths::new(Some(data.clone())), crate::storage::Storage::new(data.clone()));
        for f in &files {
            state.allowed.grant(Path::new(f)).unwrap();
        }
        let app = mock_builder()
            .manage(state)
            .manage(BulkState::default())
            .invoke_handler(tauri::generate_handler![bulk_run])
            .build(mock_context(noop_assets()))
            .unwrap();
        *app.state::<BulkState>().out_dir.lock().unwrap() = Some(out.canonicalize().unwrap());
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();

        let (tx, rx) = mpsc::channel::<(String, String)>();
        for name in ["bulk-progress", "bulk-finished"] {
            let tx = tx.clone();
            app.listen(name, move |e| {
                let _ = tx.send((name.to_string(), e.payload().to_string()));
            });
        }
        let rule = serde_json::json!({
            "version": 1, "signature": "abc123", "width": 100, "height": 40, "pages": "last",
            "position": {"type": "anchor", "text": "Signature:", "side": "right", "offsetX": 5}, "flatten": false
        });
        let res = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "bulk_run".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body: tauri::ipc::InvokeBody::Json(serde_json::json!({
                    "rule": rule, "files": files, "workers": 2, "today": {"year": 2026, "month": 11, "day": 2}
                })),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        );
        assert!(res.is_ok(), "bulk_run returned {res:?}");

        let mut progress = Vec::new();
        let finished = loop {
            let (name, payload) = rx.recv_timeout(std::time::Duration::from_secs(30)).expect("events arrive");
            let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
            if name == "bulk-finished" {
                break v;
            }
            progress.push(v);
        };
        assert_eq!(progress.len(), 3, "one progress event per file");
        assert_eq!(progress.iter().map(|p| p["done"].as_u64().unwrap()).max(), Some(3));
        assert!(progress.iter().all(|p| p["total"] == 3 && p["jobId"] == finished["jobId"]));
        let statuses: Vec<&str> =
            finished["items"].as_array().unwrap().iter().map(|i| i["status"].as_str().unwrap()).collect();
        assert_eq!(statuses, ["signed", "skipped", "signed"]);
        assert!(finished["items"][1]["detail"].as_str().unwrap().contains("not found"));
        assert_eq!(finished["cancelled"], false);
        let report = finished["reportPath"].as_str().expect("report path");
        assert!(std::fs::read_to_string(report).unwrap().contains(",skipped,"));
        assert!(
            out.join("a_signed.pdf").exists()
                && out.join("c_signed.pdf").exists()
                && !out.join("b_signed.pdf").exists()
        );
    }
}
