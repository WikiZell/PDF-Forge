//! Independent verification: open built output with PDFium (a different parser from lopdf)
//! and check bookmarks and text agree with the page order. Skipped without PDFIUM_DIR.
mod common;
use common::*;
use pdf_core::build;
use std::path::Path;

fn pdfium_ready() -> bool {
    match std::env::var("PDFIUM_DIR") {
        Ok(d) => pdf_render::init(Path::new(&d)).is_ok(),
        Err(_) if std::env::var("PDFIUM_REQUIRED").is_ok() => panic!("PDFIUM_DIR not set"),
        Err(_) => false,
    }
}

#[test]
fn pdfium_agrees_with_reordered_merged_output() {
    if !pdfium_ready() {
        eprintln!("skipping: no PDFium");
        return;
    }
    let mut p = project_of(vec![("a.pdf", make_pdf("A", 3, true)), ("b.pdf", make_pdf("B", 3, true))]);
    p.move_pages(&[5], 0).unwrap(); // B-P2 first
    p.delete(&[3]).unwrap(); // drop A-P2 (list now B2 A0 A1 A2 B0 B1 -> removes A2)
    p.rotate(&[0], 90).unwrap();
    let (bytes, _) = build(&p, None).unwrap();
    let f = std::env::temp_dir().join(format!("xcheck-{}.pdf", std::process::id()));
    std::fs::write(&f, &bytes).unwrap();

    let doc = pdf_render::Document::open(&f, None).expect("PDFium opens the output");
    let texts: Vec<String> = (0..doc.page_count())
        .map(|i| doc.page_text(i).unwrap().iter().map(|r| r.text.clone()).collect::<String>())
        .collect();
    let expect = ["B-P2", "A-P0", "A-P1", "B-P0", "B-P1"];
    assert_eq!(doc.page_count(), expect.len());
    for (t, e) in texts.iter().zip(expect) {
        assert!(t.contains(e), "page text {t:?} should contain {e}");
    }
    // PDFium's own resolution of each bookmark's page must match where its target text lives.
    for item in doc.outline() {
        let Some(pg) = item.page else { continue };
        let tag = item.title.rsplit('-').next().unwrap();
        if let Some(n) = tag.strip_prefix("bm") {
            let want = format!("{}-P{}", &item.title[..1], n);
            assert!(texts[pg].contains(&want), "{} -> page {} has {:?}, want {}", item.title, pg, texts[pg], want);
        }
    }
    // rendering the rotated first page works and swaps dimensions
    let s = doc.page_size(0).unwrap();
    assert_eq!((s.width, s.height), (400.0, 300.0));
    assert!(doc.render_page(0, 1.0).is_ok());
}

#[test]
fn pdfium_opens_pdf_with_blank_pages() {
    if !pdfium_ready() {
        return;
    }
    let mut p = project_of(vec![("a.pdf", make_pdf("A", 2, false))]);
    p.insert_blank(1, pdf_core::BlankSize::A4).unwrap();
    let (bytes, _) = build(&p, None).unwrap();
    let f = std::env::temp_dir().join(format!("xcheck-blank-{}.pdf", std::process::id()));
    std::fs::write(&f, &bytes).unwrap();
    let doc = pdf_render::Document::open(&f, None).unwrap();
    assert_eq!(doc.page_count(), 3);
    assert!(doc.render_page(1, 0.5).is_ok());
}
