//! Geometry verification: render the signed output with PDFium (independent of our math) and check
//! the placed content appears at the requested view-space rectangle.
mod common;
use image::{Rgba, RgbaImage};
use lopdf::{dictionary, Document, Object};
use pdf_core::sign::{apply, Content, Mode, Placement, Stroke};
use std::path::Path;

fn pdfium_ready() -> bool {
    match std::env::var("PDFIUM_DIR") {
        Ok(d) => pdf_render::init(Path::new(&d)).is_ok(),
        Err(_) if std::env::var("PDFIUM_REQUIRED").is_ok() => panic!("PDFIUM_DIR not set"),
        Err(_) => false,
    }
}

/// One blank page, 300x400 media box, optional /Rotate and a CropBox with an offset origin.
fn blank_page(rotate: i64, crop: Option<[i64; 4]>) -> Document {
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let content = doc.add_object(lopdf::Stream::new(dictionary! {}, b"".to_vec()));
    let mut page = dictionary! {
        "Type"=>"Page","Parent"=>Object::Reference(pages),"Contents"=>Object::Reference(content),
        "MediaBox"=>vec![0.into(),0.into(),300.into(),400.into()],"Resources"=>dictionary!{},
    };
    if rotate != 0 { page.set("Rotate", rotate); }
    if let Some(c) = crop { page.set("CropBox", c.iter().map(|v| Object::Integer(*v)).collect::<Vec<_>>()); }
    let pid = doc.add_object(page);
    doc.objects.insert(pages, Object::Dictionary(dictionary! { "Type"=>"Pages","Kids"=>vec![Object::Reference(pid)],"Count"=>1 }));
    let cat = doc.add_object(dictionary! { "Type"=>"Catalog","Pages"=>Object::Reference(pages) });
    doc.trailer.set("Root", Object::Reference(cat));
    doc
}

fn solid(w: u32, h: u32) -> Content {
    Content::Image(RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255])))
}

/// Render page 0 at 1 px/pt and return the bounding box (x0,y0,x1,y1) of non-white pixels, plus size.
fn ink_bbox(doc: &mut Document, tag: &str) -> ((i32, i32, i32, i32), (u32, u32)) {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let f = std::env::temp_dir().join(format!("sign-{tag}-{}.pdf", std::process::id()));
    std::fs::write(&f, &bytes).unwrap();
    let d = pdf_render::Document::open(&f, None).expect("PDFium opens the signed file");
    let r = d.render_page(0, 1.0).unwrap();
    let img = image::load_from_memory(&r.png).unwrap().to_rgba8();
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, -1, -1);
    for (x, y, p) in img.enumerate_pixels() {
        if p[0] < 128 && p[3] > 0 {
            x0 = x0.min(x as i32);
            y0 = y0.min(y as i32);
            x1 = x1.max(x as i32);
            y1 = y1.max(y as i32);
        }
    }
    ((x0, y0, x1 + 1, y1 + 1), (img.width(), img.height()))
}

fn close(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32), tol: i32, what: &str) {
    assert!(
        (a.0 - b.0).abs() <= tol && (a.1 - b.1).abs() <= tol && (a.2 - b.2).abs() <= tol && (a.3 - b.3).abs() <= tol,
        "{what}: got {a:?}, expected {b:?}"
    );
}

fn place(content: Content) -> Placement {
    Placement { page: 0, x: 50.0, y: 60.0, w: 80.0, h: 40.0, rotation: 0.0, content }
}

#[test]
fn image_lands_at_requested_view_rect_for_every_page_rotation_and_both_modes() {
    if !pdfium_ready() { eprintln!("skipping: no PDFium"); return; }
    for rot in [0, 90, 180, 270] {
        for mode in [Mode::Annotate, Mode::Flatten] {
            let mut doc = blank_page(rot, None);
            let rep = apply(&mut doc, &[place(solid(40, 20))], mode).unwrap();
            assert_eq!(rep.applied, 1);
            let (bb, size) = ink_bbox(&mut doc, &format!("img{rot}{mode:?}"));
            let expected_size = if rot % 180 == 0 { (300, 400) } else { (400, 300) };
            assert_eq!(size, expected_size);
            close(bb, (50, 60, 130, 100), 2, &format!("rot {rot} {mode:?}"));
        }
    }
}

#[test]
fn cropbox_offset_is_honoured() {
    if !pdfium_ready() { return; }
    for mode in [Mode::Annotate, Mode::Flatten] {
        let mut doc = blank_page(0, Some([20, 30, 260, 350])); // 240x320 visible
        apply(&mut doc, &[place(solid(40, 20))], mode).unwrap();
        let (bb, size) = ink_bbox(&mut doc, &format!("crop{mode:?}"));
        assert_eq!(size, (240, 320));
        close(bb, (50, 60, 130, 100), 2, &format!("cropbox {mode:?}"));
    }
}

#[test]
fn rotated_placement_has_rotated_bounding_box() {
    if !pdfium_ready() { return; }
    for rot in [0, 90] {
        for mode in [Mode::Annotate, Mode::Flatten] {
            let mut doc = blank_page(rot, None);
            let mut p = place(solid(40, 20));
            p.rotation = 90.0; // 80x40 box rotated about its centre (90,80) => 40 wide x 80 tall
            apply(&mut doc, &[p], mode).unwrap();
            let (bb, _) = ink_bbox(&mut doc, &format!("rotp{rot}{mode:?}"));
            close(bb, (70, 40, 110, 120), 2, &format!("page rot {rot} {mode:?}"));
        }
    }
    // 45 degrees: centre stays put and bbox grows
    let mut doc = blank_page(0, None);
    let mut p = place(solid(40, 20));
    p.rotation = 45.0;
    apply(&mut doc, &[p], Mode::Annotate).unwrap();
    let (bb, _) = ink_bbox(&mut doc, "rot45");
    let half = ((80.0f32 + 40.0) * std::f32::consts::FRAC_1_SQRT_2 / 2.0).round() as i32; // ~42
    close(bb, (90 - half, 80 - half, 90 + half, 80 + half), 3, "45deg");
}

#[test]
fn transparent_pixels_do_not_paint() {
    if !pdfium_ready() { return; }
    // left half opaque black, right half fully transparent
    let mut im = RgbaImage::from_pixel(40, 20, Rgba([0, 0, 0, 0]));
    for x in 0..20 { for y in 0..20 { im.put_pixel(x, y, Rgba([0, 0, 0, 255])); } }
    for mode in [Mode::Annotate, Mode::Flatten] {
        let mut doc = blank_page(0, None);
        apply(&mut doc, &[place(Content::Image(im.clone()))], mode).unwrap();
        let (bb, _) = ink_bbox(&mut doc, &format!("alpha{mode:?}"));
        close(bb, (50, 60, 90, 100), 2, &format!("alpha {mode:?}"));
    }
}

#[test]
fn ink_signature_is_drawn_inside_its_box_in_both_modes() {
    if !pdfium_ready() { return; }
    let strokes = vec![
        Stroke { points: vec![[0.0, 0.0, 0.5], [200.0, 100.0, 0.5]] },
        Stroke { points: vec![[0.0, 100.0, 0.8], [200.0, 0.0, 0.8]] },
    ];
    for rot in [0, 90, 180, 270] {
        for mode in [Mode::Annotate, Mode::Flatten] {
            let mut doc = blank_page(rot, None);
            let c = Content::Ink { strokes: strokes.clone(), view: (200.0, 100.0), width: 3.0, color: [0.0, 0.0, 0.0] };
            apply(&mut doc, &[place(c)], mode).unwrap();
            let (bb, _) = ink_bbox(&mut doc, &format!("ink{rot}{mode:?}"));
            // strokes touch the corners; allow for line width
            close(bb, (50, 60, 130, 100), 4, &format!("ink rot {rot} {mode:?}"));
        }
    }
}

#[test]
fn text_is_drawn_inside_its_box_and_escapes_parentheses() {
    if !pdfium_ready() { return; }
    for mode in [Mode::Annotate, Mode::Flatten] {
        let mut doc = blank_page(0, None);
        let c = Content::Text { text: "Date (05/11/2026) \\ ok".into(), font_size: 14.0, color: [0.0, 0.0, 0.0] };
        let rep = apply(&mut doc, &[place(c)], mode).unwrap();
        assert!(rep.warnings.is_empty());
        let (bb, _) = ink_bbox(&mut doc, &format!("text{mode:?}"));
        assert!(bb.2 > bb.0 && bb.3 > bb.1, "text must paint something ({mode:?})");
        assert!(bb.0 >= 48 && bb.1 >= 58 && bb.3 <= 102, "text starts inside its box: {bb:?}");
    }
}

#[test]
fn unsupported_characters_are_reported_not_fatal() {
    let mut doc = blank_page(0, None);
    let c = Content::Text { text: "日本語 ok".into(), font_size: 12.0, color: [0.0; 3] };
    let rep = apply(&mut doc, &[place(c)], Mode::Flatten).unwrap();
    assert_eq!(rep.warnings.len(), 1);
}

#[test]
fn annotation_mode_creates_real_annotations_with_appearance_streams() {
    let mut doc = blank_page(0, None);
    let ink = Content::Ink { strokes: vec![Stroke { points: vec![[0.0, 0.0, 0.5], [10.0, 10.0, 0.5]] }], view: (10.0, 10.0), width: 2.0, color: [0.0; 3] };
    apply(&mut doc, &[
        place(solid(10, 10)),
        place(ink),
        place(Content::Text { text: "x".into(), font_size: 10.0, color: [0.0; 3] }),
    ], Mode::Annotate).unwrap();
    let pid = *doc.get_pages().values().next().unwrap();
    let annots = doc.get_dictionary(pid).unwrap().get(b"Annots").unwrap().as_array().unwrap().clone();
    let subtypes: Vec<String> = annots.iter().map(|a| {
        let d = doc.get_dictionary(a.as_reference().unwrap()).unwrap();
        assert!(d.has(b"Rect") && d.has(b"AP"), "every annotation needs Rect and an appearance");
        let ap = d.get(b"AP").unwrap().as_dict().unwrap();
        let n = ap.get(b"N").unwrap().as_reference().unwrap();
        assert!(matches!(doc.get_object(n).unwrap(), Object::Stream(_)));
        String::from_utf8_lossy(d.get(b"Subtype").unwrap().as_name().unwrap()).into_owned()
    }).collect();
    assert_eq!(subtypes, ["Stamp", "Ink", "FreeText"]);
    // a flattened copy has none
    let mut flat = blank_page(0, None);
    apply(&mut flat, &[place(solid(10, 10))], Mode::Flatten).unwrap();
    let pid = *flat.get_pages().values().next().unwrap();
    assert!(!flat.get_dictionary(pid).unwrap().has(b"Annots"));
}

#[test]
fn existing_annotations_and_contents_are_preserved() {
    let mut doc = common::make_pdf("Z", 2, false); // pages with link annots and marker text
    let before = common::link_targets(&doc);
    apply(&mut doc, &[Placement { page: 1, ..place(solid(10, 10)) }], Mode::Annotate).unwrap();
    apply(&mut doc, &[Placement { page: 0, ..place(solid(10, 10)) }], Mode::Flatten).unwrap();
    assert_eq!(common::markers(&doc), ["Z-P0", "Z-P1"], "original page content intact");
    assert_eq!(common::link_targets(&doc), before, "links untouched");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    assert_eq!(common::markers(&common::load(&bytes)), ["Z-P0", "Z-P1"]);
}

#[test]
fn bad_input_is_rejected() {
    let mut doc = blank_page(0, None);
    let mut p = place(solid(4, 4));
    p.page = 3;
    assert!(apply(&mut doc, &[p], Mode::Annotate).is_err());
    let mut p = place(solid(4, 4));
    p.w = f32::NAN;
    assert!(apply(&mut doc, &[p], Mode::Annotate).is_err());
    let mut p = place(solid(4, 4));
    p.h = 0.0;
    assert!(apply(&mut doc, &[p], Mode::Annotate).is_err());
}

#[test]
fn inherited_resources_on_shared_page_tree_are_not_corrupted_by_flatten() {
    // two pages sharing one Resources dict (via the Pages node); flattening page 0 must not touch page 1
    let mut doc = common::make_pdf("S", 2, false);
    apply(&mut doc, &[place(solid(10, 10))], Mode::Flatten).unwrap();
    let pages: Vec<_> = doc.get_pages().values().copied().collect();
    let r1 = doc.get_dictionary(pages[1]).unwrap().has(b"Resources");
    assert!(!r1, "page 1 should still inherit, not have gained a private copy");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    assert_eq!(common::markers(&common::load(&bytes)).len(), 2);
}
