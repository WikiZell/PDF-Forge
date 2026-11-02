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
