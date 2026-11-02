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
