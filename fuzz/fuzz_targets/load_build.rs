#![no_main]
//! Untrusted bytes -> load -> a few edits -> rebuild -> annotate. Must never panic or hang.
use libfuzzer_sys::fuzz_target;
use pdf_core::sign::{apply, Content, Mode, Placement};
use pdf_core::{build, load_bytes, Limits, Project};

fuzz_target!(|data: &[u8]| {
    let limits = Limits { max_file_size: 4 << 20, max_pages: 500 };
    let Ok(doc) = load_bytes(data, None, &limits) else { return };
    let mut signed = doc.clone();
    let mut p = Project::new();
    let i = p.add_source(doc, "f.pdf");
    if p.append_source(i).is_err() || p.pages.is_empty() {
        return;
    }
    let _ = p.duplicate(&[0]);
    let _ = p.rotate(&[0], 90);
    let _ = build(&p, None);
    let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]));
    let pl = Placement { page: 0, x: 10.0, y: 10.0, w: 40.0, h: 20.0, rotation: 30.0, content: Content::Image(img) };
    let _ = apply(&mut signed, &[pl.clone()], Mode::Annotate);
    let _ = apply(&mut signed, &[pl], Mode::Flatten);
});
