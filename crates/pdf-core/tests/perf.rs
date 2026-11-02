//! Performance and memory on a 1,000-page document. Prints metrics (use --nocapture).
//! Thresholds are generous so the test catches regressions of an order of magnitude, not noise.
mod common;
use common::*;
use pdf_core::{build, load_bytes, Limits};
use std::path::Path;
use std::time::Instant;

fn peak_rss_mb() -> Option<f64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let l = s.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: f64 = l.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0)
}

fn thousand_page_doc() -> Vec<u8> {
    let mut d = make_pdf("BIG", 1000, true);
    let mut v = Vec::new();
    d.save_to(&mut v).unwrap();
    v
}

#[test]
fn thousand_pages_load_edit_build_and_render() {
    let t = Instant::now();
    let bytes = thousand_page_doc();
    eprintln!("generate: {:?}, {} KB", t.elapsed(), bytes.len() / 1024);

    let t = Instant::now();
    let doc = load_bytes(&bytes, None, &Limits::default()).unwrap();
    let load_t = t.elapsed();
    eprintln!("load_bytes: {load_t:?}");
    assert_eq!(doc.get_pages().len(), 1000);

    let mut p = project_of(vec![("big.pdf", doc)]);
    let t = Instant::now();
    p.move_pages(&[999], 0).unwrap();
    p.rotate(&(0..1000).step_by(7).collect::<Vec<_>>(), 90).unwrap();
    p.delete(&[500, 501, 502]).unwrap();
    p.duplicate(&[10]).unwrap();
    let edit = t.elapsed();
    eprintln!("1000-page edit ops (move/rotate/delete/dup): {edit:?}");

    let t = Instant::now();
    let (out, rep) = build(&p, None).unwrap();
    let built = t.elapsed();
    eprintln!(
        "build: {built:?}, output {} KB, dropped links {}, dropped bookmarks {}",
        out.len() / 1024,
        rep.dropped_links,
        rep.dropped_bookmarks
    );
    let reloaded = load(&out);
    assert_eq!(reloaded.get_pages().len(), 998);
    assert_eq!(markers(&reloaded)[0], "BIG-P999");

    // extract 10 pages out of 1000: must be fast and small
    let t = Instant::now();
    let (small, _) = build(&p, Some(&(0..10).collect::<Vec<_>>())).unwrap();
    eprintln!("extract 10/1000: {:?}, {} KB", t.elapsed(), small.len() / 1024);
    assert!(small.len() < out.len() / 20);

    if let Some(mb) = peak_rss_mb() {
        eprintln!("peak RSS so far: {mb:.0} MB");
        assert!(mb < 1500.0, "peak RSS {mb:.0} MB is unexpectedly high");
    }
    assert!(load_t.as_secs() < 30 && built.as_secs() < 60, "load {load_t:?} build {built:?}");

    if let Ok(dir) = std::env::var("PDFIUM_DIR") {
        if pdf_render::init(Path::new(&dir)).is_ok() {
            let f = std::env::temp_dir().join(format!("perf-{}.pdf", std::process::id()));
            std::fs::write(&f, &out).unwrap();
            let t = Instant::now();
            let d = pdf_render::Document::open(&f, None).unwrap();
            eprintln!("PDFium open: {:?} ({} pages)", t.elapsed(), d.page_count());
            let t = Instant::now();
            for i in [0, 250, 499, 997] {
                d.render_page(i, 1.0).unwrap();
            }
            eprintln!("render 4 pages: {:?}", t.elapsed());
            let t = Instant::now();
            let hits = d.search("BIG-P99", false).unwrap();
            eprintln!("search across 1000 pages: {:?}, {} hits", t.elapsed(), hits.len());
            let t = Instant::now();
            let ol = d.outline();
            eprintln!("outline ({} items): {:?}", ol.len(), t.elapsed());
            let t = Instant::now();
            let sizes: Vec<_> = (0..d.page_count()).map(|i| d.page_size(i).unwrap()).collect();
            eprintln!("page sizes x{}: {:?}", sizes.len(), t.elapsed());
            if let Some(mb) = peak_rss_mb() {
                eprintln!("peak RSS: {mb:.0} MB");
            }
        }
    }
}
