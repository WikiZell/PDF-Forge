//! Writes sample outputs (merged/reordered, signed in each mode) into $SAMPLES_DIR so other tools
//! (Chrome, the installed app on Windows) can open them. Ignored unless SAMPLES_DIR is set.
mod common;
use common::*;
use image::{Rgba, RgbaImage};
use pdf_core::build;
use pdf_core::sign::{apply, Content, Mode, Placement, Stroke};

#[test]
fn write_samples() {
    let Ok(dir) = std::env::var("SAMPLES_DIR") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();

    // merged + reordered + rotated
    let mut p = project_of(vec![("a.pdf", make_pdf("Alpha", 3, true)), ("b.pdf", make_pdf("Bravo", 2, true))]);
    p.move_pages(&[4], 0).unwrap();
    p.rotate(&[1], 90).unwrap();
    std::fs::write(dir.join("merged.pdf"), build(&p, None).unwrap().0).unwrap();

    for (name, mode) in [("signed-annot.pdf", Mode::Annotate), ("signed-flat.pdf", Mode::Flatten)] {
        let mut d = make_pdf("Sign", 2, false);
        let img = RgbaImage::from_fn(120, 40, |x, y| {
            if (x / 8 + y / 8) % 2 == 0 {
                Rgba([200, 0, 0, 255])
            } else {
                Rgba([0, 0, 200, 160])
            }
        });
        let ink = Content::Ink {
            strokes: vec![Stroke {
                points: vec![[0.0, 80.0, 0.5], [40.0, 10.0, 0.9], [90.0, 70.0, 0.4], [150.0, 20.0, 0.7]],
            }],
            view: (150.0, 90.0),
            width: 4.0,
            color: [0.0, 0.1, 0.6],
        };
        let ps = vec![
            Placement {
                page: 0,
                x: 60.0,
                y: 200.0,
                w: 120.0,
                h: 40.0,
                rotation: 0.0,
                content: Content::Image(img.clone()),
            },
            Placement { page: 0, x: 60.0, y: 260.0, w: 150.0, h: 90.0, rotation: -15.0, content: ink },
            Placement {
                page: 0,
                x: 60.0,
                y: 150.0,
                w: 120.0,
                h: 20.0,
                rotation: 0.0,
                content: Content::Text { text: "Signed 2026-11-02".into(), font_size: 14.0, color: [0.0; 3] },
            },
            Placement { page: 1, x: 100.0, y: 100.0, w: 120.0, h: 40.0, rotation: 90.0, content: Content::Image(img) },
        ];
        apply(&mut d, &ps, mode).unwrap();
        let mut b = Vec::new();
        d.save_to(&mut b).unwrap();
        std::fs::write(dir.join(name), b).unwrap();
    }
}

fn text_pdf(pages: &[&str]) -> String {
    let n = pages.len();
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 3 + i)).collect();
    let mut objs = vec![
        "<</Type/Catalog/Pages 2 0 R>>".to_string(),
        format!("<</Type/Pages/Kids[{}]/Count {n}>>", kids.join(" ")),
    ];
    for i in 0..n {
        objs.push(format!(
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 595 842]/Contents {} 0 R/Resources<</Font<</F1 {} 0 R>>>>>>",
            3 + n + i,
            3 + 2 * n
        ));
    }
    for t in pages {
        let c = format!("BT /F1 14 Tf 72 150 Td ({t}) Tj ET");
        objs.push(format!("<</Length {}>>stream\n{c}\nendstream", c.len()));
    }
    objs.push("<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_string());
    let mut out = String::from("%PDF-1.4\n");
    for (i, o) in objs.iter().enumerate() {
        out.push_str(&format!("{} 0 obj{o}endobj\n", i + 1));
    }
    out.push_str(&format!("trailer<</Root 1 0 R/Size {}>>\n%%EOF\n", objs.len() + 1));
    out
}

/// Inputs for the bulk-signing test: 3 with the anchor text, 1 without, 1 corrupt.
#[test]
fn write_bulk_inputs() {
    let Ok(dir) = std::env::var("SAMPLES_DIR") else { return };
    let d = std::path::PathBuf::from(dir).join("bulk-in");
    std::fs::create_dir_all(&d).unwrap();
    let w = |n: &str, s: String| std::fs::write(d.join(n), s).unwrap();
    w("contract_a.pdf", text_pdf(&["Page one", "Terms. Signature: ____"]));
    w("contract_b.pdf", text_pdf(&["Agreement. Signature: ____"]));
    w("contract_c.pdf", text_pdf(&["Page one", "Page two", "Signature: ________"]));
    w("no_anchor.pdf", text_pdf(&["There is nothing to sign here"]));
    w("broken.pdf", "%PDF-1.4 not a real pdf".to_string());
}

/// A heavy "scanned document": N pages, each a JPEG image of a letter-size page (text-like blocks).
/// Run with SCAN_PAGES=1000 in release mode; writes $SAMPLES_DIR/scan-<N>.pdf
#[test]
fn write_scanned_document() {
    let (Ok(dir), Ok(n)) = (std::env::var("SAMPLES_DIR"), std::env::var("SCAN_PAGES")) else { return };
    let n: usize = n.parse().unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    use image::{codecs::jpeg::JpegEncoder, GrayImage, Luma};
    use lopdf::dictionary;
    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    let (w, h) = (850u32, 1100u32); // 100 dpi letter
    let mut seed = 0x2545F4914F6CDD1Du64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for p in 0..n {
        let mut img = GrayImage::from_pixel(w, h, Luma([245]));
        // paper noise + lines of "text" blocks, different on every page
        for y in (80..h - 80).step_by(22) {
            let mut x = 70;
            while x < w - 70 {
                let len = 10 + (rnd() % 60) as u32;
                for yy in y..y + 12 {
                    for xx in x..(x + len).min(w - 70) {
                        img.put_pixel(xx, yy, Luma([20 + (rnd() % 40) as u8]));
                    }
                }
                x += len + 8;
            }
        }
        for _ in 0..4000 {
            let (x, y) = ((rnd() % w as u64) as u32, (rnd() % h as u64) as u32);
            img.put_pixel(x, y, Luma([200 + (rnd() % 40) as u8]));
        }
        let mut jpg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpg, 55).encode_image(&img).unwrap();
        let mut idict = lopdf::dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8, "Filter" => "DCTDecode",
        };
        idict.set("Length", jpg.len() as i64);
        let img_id = doc.add_object(lopdf::Stream::new(idict, jpg));
        let content =
            doc.add_object(lopdf::Stream::new(lopdf::dictionary! {}, b"q 612 0 0 792 0 0 cm /Im0 Do Q".to_vec()));
        let page = doc.add_object(lopdf::dictionary! {
            "Type" => "Page", "Parent" => lopdf::Object::Reference(pages_id),
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => lopdf::Object::Reference(content),
            "Resources" => lopdf::dictionary! { "XObject" => lopdf::dictionary! { "Im0" => lopdf::Object::Reference(img_id) } },
        });
        kids.push(lopdf::Object::Reference(page));
        if p % 200 == 0 {
            eprintln!("generated page {p}");
        }
    }
    doc.objects.insert(
        pages_id,
        lopdf::Object::Dictionary(lopdf::dictionary! { "Type" => "Pages", "Count" => n as i64, "Kids" => kids }),
    );
    let cat = doc.add_object(lopdf::dictionary! { "Type" => "Catalog", "Pages" => lopdf::Object::Reference(pages_id) });
    doc.trailer.set("Root", lopdf::Object::Reference(cat));
    let path = std::path::PathBuf::from(dir).join(format!("scan-{n}.pdf"));
    let mut f = std::fs::File::create(&path).unwrap();
    doc.save_to(&mut f).unwrap();
    eprintln!("wrote {} ({} MB)", path.display(), std::fs::metadata(&path).unwrap().len() / 1_000_000);
}
