//! Needs a PDFium shared library. Set PDFIUM_DIR to its directory
//! (the `lib/` folder on Linux/macOS, `bin/` on Windows). Without it the
//! tests are skipped locally; CI sets PDFIUM_REQUIRED=1 to make that a failure.

use std::path::{Path, PathBuf};

const MINIMAL_PDF: &str = "%PDF-1.4\n\
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n\
2 0 obj<</Type/Pages/Kids[3 0 R 4 0 R]/Count 2>>endobj\n\
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 100]/Contents 5 0 R>>endobj\n\
4 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 400]/Rotate 90>>endobj\n\
5 0 obj<</Length 44>>stream\n0 0 0 rg 10 10 100 50 re f\nendstream endobj\n\
trailer<</Root 1 0 R/Size 6>>\n%%EOF\n";

fn setup() -> Option<PathBuf> {
    match std::env::var("PDFIUM_DIR") {
        Ok(d) => {
            pdf_render::init(Path::new(&d)).expect("pdfium loads");
            let p = std::env::temp_dir().join(format!("render-test-{}.pdf", std::process::id()));
            std::fs::write(&p, MINIMAL_PDF).unwrap();
            Some(p)
        }
        Err(_) if std::env::var("PDFIUM_REQUIRED").is_ok() => panic!("PDFIUM_DIR not set"),
        Err(_) => {
            eprintln!("skipping: PDFIUM_DIR not set");
            None
        }
    }
}

#[test]
fn opens_and_renders_with_rotation_applied() {
    let Some(pdf) = setup() else { return };
    let doc = pdf_render::Document::open(&pdf, None).unwrap();
    assert_eq!(doc.page_count(), 2);
    let s0 = doc.page_size(0).unwrap();
    assert_eq!((s0.width, s0.height), (200.0, 100.0));
    let s1 = doc.page_size(1).unwrap();
    assert_eq!((s1.width, s1.height), (400.0, 300.0)); // rotated 90
    let r = doc.render_page(0, 2.0).unwrap();
    assert_eq!((r.width, r.height), (400, 200));
    assert_eq!(&r.png[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn out_of_range_and_oversize_are_errors() {
    let Some(pdf) = setup() else { return };
    let doc = pdf_render::Document::open(&pdf, None).unwrap();
    assert!(matches!(doc.render_page(9, 1.0), Err(pdf_render::RenderError::PageOutOfRange { .. })));
    assert!(doc.render_page(0, 1.0e6).is_err());
    assert!(doc.render_page(0, f32::NAN).is_err());
}

#[test]
fn garbage_file_fails_to_open() {
    let Some(_) = setup() else { return };
    let p = std::env::temp_dir().join(format!("render-garbage-{}.pdf", std::process::id()));
    std::fs::write(&p, b"nope").unwrap();
    assert!(pdf_render::Document::open(&p, None).is_err());
}

#[test]
fn missing_library_gives_clear_error() {
    // Runs without PDFium: init from an empty dir must not panic.
    let r = pdf_render::init(Path::new("/nonexistent-dir"));
    if std::env::var("PDFIUM_DIR").is_err() {
        let e = r.unwrap_err();
        assert!(e.to_string().contains("PDFium"));
    }
}

const TEXT_PDF: &str = "%PDF-1.4\n\
1 0 obj<</Type/Catalog/Pages 2 0 R/Outlines 10 0 R>>endobj\n\
2 0 obj<</Type/Pages/Kids[3 0 R 4 0 R 5 0 R]/Count 3>>endobj\n\
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 200]/Contents 6 0 R/Resources<</Font<</F1 9 0 R>>>>>>endobj\n\
4 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 200]/Rotate 90/Contents 7 0 R/Resources<</Font<</F1 9 0 R>>>>>>endobj\n\
5 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 200]/Contents 8 0 R/Resources<</Font<</F1 9 0 R>>>>>>endobj\n\
6 0 obj<</Length 52>>stream\nBT /F1 20 Tf 50 100 Td (Hello World) Tj ET\nendstream endobj\n\
7 0 obj<</Length 58>>stream\nBT /F1 20 Tf 50 100 Td (Second hello page) Tj ET\nendstream endobj\n\
8 0 obj<</Length 48>>stream\nBT /F1 20 Tf 50 20 Td (Third page) Tj ET\nendstream endobj\n\
9 0 obj<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>endobj\n\
10 0 obj<</Type/Outlines/First 11 0 R/Last 13 0 R/Count 3>>endobj\n\
11 0 obj<</Title(Chapter 1)/Parent 10 0 R/Next 13 0 R/First 12 0 R/Last 12 0 R/Count 1/Dest[3 0 R/Fit]>>endobj\n\
12 0 obj<</Title(Section 1.1)/Parent 11 0 R/Dest[5 0 R/Fit]>>endobj\n\
13 0 obj<</Title(Chapter 2)/Parent 10 0 R/Prev 11 0 R/Dest[4 0 R/Fit]>>endobj\n\
trailer<</Root 1 0 R/Size 14>>\n%%EOF\n";

fn text_doc() -> Option<pdf_render::Document> {
    let _ = setup()?;
    let p = std::env::temp_dir().join(format!("render-text-{}.pdf", std::process::id()));
    std::fs::write(&p, TEXT_PDF).unwrap();
    Some(pdf_render::Document::open(&p, None).unwrap())
}

#[test]
fn text_runs_have_view_space_boxes() {
    let Some(doc) = text_doc() else { return };
    let runs = doc.page_text(0).unwrap();
    assert_eq!(runs.len(), 1);
    assert!(runs[0].text.contains("Hello World"));
    let r = runs[0].rect;
    // baseline y=100 in a 200pt-high page => top-left origin y around 80..100
    assert!((r.x - 50.0).abs() < 2.0, "x={}", r.x);
    assert!(r.y > 70.0 && r.y < 100.0, "y={}", r.y);
    assert!(r.w > 80.0 && r.h > 10.0);
}

#[test]
fn text_rect_is_mapped_through_page_rotation() {
    let Some(doc) = text_doc() else { return };
    // page 2 is rotated 90: view is 200 wide x 300 high, text runs top-to-bottom
    let s = doc.page_size(1).unwrap();
    assert_eq!((s.width, s.height), (200.0, 300.0));
    let r = doc.page_text(1).unwrap()[0].rect;
    assert!(r.x >= 0.0 && r.x + r.w <= 200.5, "x range {} {}", r.x, r.w);
    assert!(r.y >= 0.0 && r.y + r.h <= 300.5, "y range {} {}", r.y, r.h);
    assert!(r.h > r.w, "rotated text should be taller than wide: {r:?}");
}

#[test]
fn outline_is_flattened_with_depth_and_pages() {
    let Some(doc) = text_doc() else { return };
    let o = doc.outline();
    let got: Vec<_> = o.iter().map(|i| (i.title.as_str(), i.depth, i.page)).collect();
    assert_eq!(got, vec![("Chapter 1", 0, Some(0)), ("Section 1.1", 1, Some(2)), ("Chapter 2", 0, Some(1))]);
}

#[test]
fn search_finds_hits_across_pages_case_insensitively() {
    let Some(doc) = text_doc() else { return };
    let hits = doc.search("hello", false).unwrap();
    let pages: Vec<_> = hits.iter().map(|h| h.page).collect();
    assert_eq!(pages, vec![0, 1]);
    assert!(!hits[0].rects.is_empty());
    let cs = doc.search("hello", true).unwrap();
    assert_eq!(cs.iter().map(|h| h.page).collect::<Vec<_>>(), vec![1]);
    assert_eq!(doc.search("HELLO", true).unwrap().len(), 0);
    assert!(doc.search("", false).unwrap().is_empty());
    assert!(doc.search("zzzz", false).unwrap().is_empty());
}

#[test]
fn search_single_page_and_open_from_bytes() {
    let Some(_) = setup() else { return };
    let doc = pdf_render::Document::from_bytes(TEXT_PDF.as_bytes().to_vec(), None).unwrap();
    assert_eq!(doc.page_count(), 3);
    assert_eq!(doc.search_page(1, "hello", false).unwrap().len(), 1);
    assert!(doc.search_page(2, "hello", false).unwrap().is_empty());
    assert!(doc.search_page(9, "hello", false).is_err());
    assert!(pdf_render::Document::from_bytes(b"junk".to_vec(), None).is_err());
}
