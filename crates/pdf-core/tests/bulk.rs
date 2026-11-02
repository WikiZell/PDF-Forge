mod common;
use common::*;
use image::{Rgba, RgbaImage};
use lopdf::Document;
use pdf_core::bulk::*;
use pdf_core::sign::{Content, Mode};
use pdf_core::Limits;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

struct PdfiumFinder;
impl AnchorFinder for PdfiumFinder {
    fn find(&self, file: &Path, pages: &[usize], text: &str, mc: bool) -> Result<Vec<(usize, AnchorRect)>, String> {
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

fn pdfium_ready() -> bool {
    match std::env::var("PDFIUM_DIR") {
        Ok(d) => pdf_render::init(Path::new(&d)).is_ok(),
        Err(_) if std::env::var("PDFIUM_REQUIRED").is_ok() => panic!("PDFIUM_DIR not set"),
        Err(_) => false,
    }
}

fn workdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bulk-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(dir: &Path, name: &str, mut doc: Document) -> PathBuf {
    let p = dir.join(name);
    let mut b = Vec::new();
    doc.save_to(&mut b).unwrap();
    std::fs::write(&p, b).unwrap();
    p
}

fn black() -> Content {
    Content::Image(RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 255])))
}

fn rule(pages: PageSel, position: Position) -> Rule {
    Rule { width: 60.0, height: 30.0, pages, position, stamp: None }
}

fn job<'a>(files: Vec<PathBuf>, out: &Path, rule: Rule, finder: &'a dyn AnchorFinder) -> Job<'a> {
    Job {
        files,
        out_dir: out.to_path_buf(),
        suffix: "_signed".into(),
        rule,
        content: black(),
        mode: Mode::Annotate,
        today: Date { year: 2026, month: 11, day: 2 },
        workers: 4,
        limits: Limits::default(),
        finder,
    }
}

fn run_job(j: &Job) -> Vec<FileResult> {
    run(j, &AtomicBool::new(false), &|_| {}).unwrap()
}

/// bbox of dark pixels on `page` of the rendered file (1px = 1pt).
fn ink_bbox(file: &Path, page: usize) -> Option<(i32, i32, i32, i32)> {
    let d = pdf_render::Document::open(file, None).unwrap();
    let img = image::load_from_memory(&d.render_page(page, 1.0).unwrap().png).unwrap().to_rgba8();
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, -1, -1);
    for (x, y, p) in img.enumerate_pixels() {
        if p[0] < 100 && p[1] < 100 && p[2] < 100 {
            x0 = x0.min(x as i32);
            y0 = y0.min(y as i32);
            x1 = x1.max(x as i32);
            y1 = y1.max(y as i32);
        }
    }
    (x1 >= 0).then_some((x0, y0, x1 + 1, y1 + 1))
}

#[test]
fn fixed_position_many_files_in_parallel_originals_untouched_unique_outputs() {
    if !pdfium_ready() {
        eprintln!("skipping: no PDFium");
        return;
    }
    let inp = workdir("fixed-in");
    let out = workdir("fixed-out");
    let files: Vec<PathBuf> =
        (0..24).map(|i| write(&inp, &format!("doc{i:02}.pdf"), make_pdf(&format!("D{i}"), 2 + i % 3, false))).collect();
    let before: Vec<Vec<u8>> = files.iter().map(|f| std::fs::read(f).unwrap()).collect();

    let progress = AtomicUsize::new(0);
    let j = job(files.clone(), &out, rule(PageSel::Last, Position::Fixed { x: 100.0, y: 200.0 }), &PdfiumFinder);
    let res = run(&j, &AtomicBool::new(false), &|p| {
        progress.fetch_max(p.done, Ordering::Relaxed);
        assert_eq!(p.total, 24);
    })
    .unwrap();
    assert_eq!(progress.load(Ordering::Relaxed), 24);
    assert_eq!(res.len(), 24);
    for (i, r) in res.iter().enumerate() {
        assert_eq!(r.input, files[i], "results keep input order");
        let Outcome::Signed { output, placed } = &r.outcome else { panic!("{:?}", r.outcome) };
        assert_eq!(*placed, 1);
        assert_eq!(output.file_name().unwrap().to_string_lossy(), format!("doc{i:02}_signed.pdf"));
        // signature is on the LAST page only, at the requested place
        let pages = lopdf::Document::load(output).unwrap().get_pages().len();
        assert_eq!(pages, 2 + i % 3);
        let bb = ink_bbox(output, pages - 1).expect("signature visible on last page");
        // the black image is 60x30 at (100,200) (text on the page is small and also dark: compare containment)
        assert!(bb.0 <= 101 && bb.1 <= 201 && bb.2 >= 159 && bb.3 >= 229, "{bb:?}");
        assert!(
            ink_bbox(output, 0).map_or(true, |b| b.1 > 300 || b.3 < 200 || pages == 1),
            "first page has no signature box"
        );
    }
    for (f, b) in files.iter().zip(&before) {
        assert_eq!(&std::fs::read(f).unwrap(), b, "original {} must be byte-identical", f.display());
    }
    // exactly 24 outputs, no leftovers
    let n = std::fs::read_dir(&out).unwrap().count();
    assert_eq!(n, 24);
}

#[test]
fn anchor_text_places_signature_relative_to_the_anchor_and_reports_missing_anchors() {
    if !pdfium_ready() {
        return;
    }
    let inp = workdir("anchor-in");
    let out = workdir("anchor-out");
    // "Sign here" appears on every page (marker text); one file has different text.
    let a = write(&inp, "a.pdf", make_pdf("Sign here", 3, false));
    let b = write(&inp, "b.pdf", make_pdf("Nothing", 2, false));
    let c = write(&inp, "c.pdf", make_pdf("Sign here", 1, false));
    let pos = Position::Anchor {
        text: "Sign here".into(),
        match_case: true,
        occurrence: Occurrence::First,
        side: Side::Right,
        offset_x: 10.0,
        offset_y: 0.0,
    };
    let j = job(vec![a.clone(), b.clone(), c.clone()], &out, rule(PageSel::Last, pos), &PdfiumFinder);

    // dry run writes nothing and predicts the same outcome
    let dry = dry_run(&j);
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
    assert!(matches!(dry[0].outcome, Outcome::Signed { placed: 1, .. }));
    assert!(matches!(&dry[1].outcome, Outcome::Skipped { reason } if reason.contains("not found")));

    let res = run_job(&j);
    let Outcome::Signed { output, .. } = &res[0].outcome else { panic!("{:?}", res[0].outcome) };
    assert!(matches!(&res[1].outcome, Outcome::Skipped { reason } if reason.contains("Sign here")));
    assert!(matches!(res[2].outcome, Outcome::Signed { .. }));

    // anchor text is at x=20; the signature must start ~10pt right of the end of that text
    let anchor = PdfiumFinder.find(&a, &[2], "Sign here", true).unwrap()[0].1;
    let bb = ink_bbox(output, 2).unwrap();
    assert!(
        ((bb.3 as f32) - (anchor.y + 30.0)).abs() <= 3.0,
        "signature bottom {} vs anchor-relative {}",
        bb.3,
        anchor.y + 30.0
    );
    let expect_x = anchor.x + anchor.w + 10.0;
    // the page also has small dark text; the signature (60pt wide) defines the right edge of all ink
    assert!(
        ((bb.2 as f32) - (expect_x + 60.0)).abs() <= 3.0,
        "signature right edge {} vs expected {}",
        bb.2,
        expect_x + 60.0
    );
    assert!(std::fs::read_dir(&out).unwrap().count() == 2, "skipped file produces no output");
}

#[test]
fn below_side_and_all_pages_and_stamp() {
    if !pdfium_ready() {
        return;
    }
    let inp = workdir("stamp-in");
    let out = workdir("stamp-out");
    let a = write(&inp, "a.pdf", make_pdf("Sign here", 3, false));
    let mut r = rule(
        PageSel::All,
        Position::Anchor {
            text: "Sign here".into(),
            match_case: false,
            occurrence: Occurrence::Last,
            side: Side::Below,
            offset_x: 0.0,
            offset_y: 5.0,
        },
    );
    r.stamp =
        Some(StampRule { kind: StampKind::Date(DateFormat::Iso), font_size: 12.0, offset_x: 0.0, offset_y: -20.0 });
    let j = job(vec![a], &out, r, &PdfiumFinder);
    let res = run_job(&j);
    let Outcome::Signed { output, placed } = &res[0].outcome else { panic!("{:?}", res[0].outcome) };
    assert_eq!(*placed, 3, "one per page");
    let d = pdf_render::Document::open(output, None).unwrap();
    // PDFium extracts text from the annotation appearance? it does not; check the annotation contents instead
    let lo = lopdf::Document::load(output).unwrap();
    let mut texts = Vec::new();
    for (_, &pid) in lo.get_pages().iter() {
        if let Ok(lopdf::Object::Array(a)) = lo.get_dictionary(pid).unwrap().get(b"Annots") {
            for e in a {
                if let Ok(r) = e.as_reference() {
                    if let Ok(lopdf::Object::String(s, _)) = lo.get_dictionary(r).unwrap().get(b"Contents") {
                        texts.push(String::from_utf8_lossy(s).into_owned());
                    }
                }
            }
        }
    }
    assert_eq!(texts.iter().filter(|t| *t == "2026-11-02").count(), 3, "date stamp on every page: {texts:?}");
    assert_eq!(d.page_count(), 3);
}

#[test]
fn bad_inputs_are_reported_per_file_without_stopping_the_batch() {
    if !pdfium_ready() {
        return;
    }
    use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
    use lopdf::{EncryptionState, EncryptionVersion, Permissions};
    let inp = workdir("bad-in");
    let out = workdir("bad-out");
    let good = write(&inp, "good.pdf", make_pdf("G", 2, false));
    let corrupt = inp.join("corrupt.pdf");
    std::fs::write(&corrupt, b"%PDF-1.4 this is not really a pdf").unwrap();
    let empty = inp.join("empty.pdf");
    std::fs::write(&empty, b"").unwrap();
    let missing = inp.join("missing.pdf");
    let mut e = make_pdf("E", 2, false);
    let id = lopdf::Object::String(b"0123456789abcdef".to_vec(), lopdf::StringFormat::Hexadecimal);
    e.trailer.set("ID", vec![id.clone(), id]);
    let cf: std::sync::Arc<dyn CryptFilter> = std::sync::Arc::new(Aes128CryptFilter);
    let st = EncryptionState::try_from(EncryptionVersion::V4 {
        document: &e,
        encrypt_metadata: true,
        crypt_filters: [(b"StdCF".to_vec(), cf)].into_iter().collect(),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: "o",
        user_password: "u",
        permissions: Permissions::PRINTABLE,
    })
    .unwrap();
    e.encrypt(&st).unwrap();
    let enc = write(&inp, "enc.pdf", e);
    let one_page = write(&inp, "one.pdf", make_pdf("O", 1, false));

    let j = job(
        vec![good.clone(), corrupt, empty, missing, enc, one_page],
        &out,
        rule(PageSel::Number(2), Position::Fixed { x: 10.0, y: 10.0 }),
        &PdfiumFinder,
    );
    let res = run_job(&j);
    assert!(matches!(res[0].outcome, Outcome::Signed { .. }));
    assert!(matches!(res[1].outcome, Outcome::Failed { .. }), "corrupt: {:?}", res[1].outcome);
    assert!(matches!(res[2].outcome, Outcome::Failed { .. }));
    assert!(matches!(res[3].outcome, Outcome::Failed { .. }));
    assert!(
        matches!(&res[4].outcome, Outcome::Skipped { reason } if reason == "password protected"),
        "{:?}",
        res[4].outcome
    );
    assert!(
        matches!(&res[5].outcome, Outcome::Skipped { reason } if reason.contains("page 2 does not exist")),
        "{:?}",
        res[5].outcome
    );
    let csv = report_csv(&res);
    assert_eq!(csv.lines().count(), 7);
    assert!(csv.contains(",signed,") && csv.contains(",skipped,") && csv.contains(",failed,"));
    assert!(report_text(&res).contains("1 signed, 2 skipped, 3 failed"));
}

#[test]
fn never_overwrites_existing_outputs() {
    if !pdfium_ready() {
        return;
    }
    let inp = workdir("ow-in");
    let out = workdir("ow-out");
    let a = write(&inp, "doc.pdf", make_pdf("A", 1, false));
    std::fs::write(out.join("doc_signed.pdf"), b"PRECIOUS").unwrap();
    std::fs::write(out.join("doc_signed (2).pdf"), b"PRECIOUS2").unwrap();
    let j = job(vec![a], &out, rule(PageSel::First, Position::Fixed { x: 10.0, y: 10.0 }), &PdfiumFinder);
    let res = run_job(&j);
    let Outcome::Signed { output, .. } = &res[0].outcome else { panic!() };
    assert_eq!(output.file_name().unwrap().to_string_lossy(), "doc_signed (3).pdf");
    assert_eq!(std::fs::read(out.join("doc_signed.pdf")).unwrap(), b"PRECIOUS");
    assert_eq!(std::fs::read(out.join("doc_signed (2).pdf")).unwrap(), b"PRECIOUS2");
}

#[test]
fn same_names_from_different_folders_get_distinct_outputs_even_in_parallel() {
    if !pdfium_ready() {
        return;
    }
    let out = workdir("dup-out");
    let files: Vec<PathBuf> = (0..16)
        .map(|i| {
            let d = workdir(&format!("dup-in{i}"));
            write(&d, "contract.pdf", make_pdf("C", 1, false))
        })
        .collect();
    let mut j = job(files, &out, rule(PageSel::First, Position::Fixed { x: 10.0, y: 10.0 }), &PdfiumFinder);
    j.workers = 8;
    let res = run_job(&j);
    let mut names: Vec<String> = res
        .iter()
        .map(|r| match &r.outcome {
            Outcome::Signed { output, .. } => output.file_name().unwrap().to_string_lossy().into_owned(),
            o => panic!("{o:?}"),
        })
        .collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 16, "no two jobs may write the same file");
}

#[test]
fn cancel_stops_new_work_and_marks_the_rest_cancelled() {
    if !pdfium_ready() {
        return;
    }
    let inp = workdir("cancel-in");
    let out = workdir("cancel-out");
    let files: Vec<PathBuf> = (0..10).map(|i| write(&inp, &format!("d{i}.pdf"), make_pdf("X", 1, false))).collect();
    let mut j = job(files, &out, rule(PageSel::First, Position::Fixed { x: 10.0, y: 10.0 }), &PdfiumFinder);
    j.workers = 1;
    let cancel = AtomicBool::new(false);
    let seen = Mutex::new(0);
    let res = run(&j, &cancel, &|p| {
        *seen.lock().unwrap() = p.done;
        if p.done == 3 {
            cancel.store(true, Ordering::Relaxed);
        }
    })
    .unwrap();
    let signed = res.iter().filter(|r| matches!(r.outcome, Outcome::Signed { .. })).count();
    let cancelled =
        res.iter().filter(|r| matches!(&r.outcome, Outcome::Skipped { reason } if reason == "cancelled")).count();
    assert_eq!(signed, 3);
    assert_eq!(cancelled, 7);
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 3);
}

#[test]
fn invalid_jobs_are_rejected() {
    let out = workdir("invalid-out");
    let mut j = job(
        vec![PathBuf::from("x.pdf")],
        &out,
        rule(PageSel::First, Position::Fixed { x: 0.0, y: 0.0 }),
        &PdfiumFinder,
    );
    j.suffix = "  ".into();
    assert!(matches!(run(&j, &AtomicBool::new(false), &|_| {}), Err(JobError::EmptySuffix)));
    j.suffix = "_s".into();
    j.files.clear();
    assert!(matches!(run(&j, &AtomicBool::new(false), &|_| {}), Err(JobError::NoFiles)));
}

#[test]
fn flatten_mode_bakes_signature_into_the_page() {
    if !pdfium_ready() {
        return;
    }
    let inp = workdir("flat-in");
    let out = workdir("flat-out");
    let a = write(&inp, "a.pdf", make_pdf("A", 1, false));
    let mut j = job(vec![a], &out, rule(PageSel::First, Position::Fixed { x: 100.0, y: 200.0 }), &PdfiumFinder);
    j.mode = Mode::Flatten;
    let res = run_job(&j);
    let Outcome::Signed { output, .. } = &res[0].outcome else { panic!() };
    let lo = lopdf::Document::load(output).unwrap();
    let pid = *lo.get_pages().values().next().unwrap();
    let annots = lo
        .get_dictionary(pid)
        .unwrap()
        .get(b"Annots")
        .ok()
        .and_then(|a| a.as_array().ok())
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(annots, 2, "only the 2 original link annotations remain; the signature is in the content");
    assert!(ink_bbox(output, 0).is_some());
}
