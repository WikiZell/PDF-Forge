mod common;
use common::*;
use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::{Document, EncryptionState, EncryptionVersion, Permissions};
use pdf_core::{build, load_bytes, split_by_bookmarks, Limits, LoadError};
use std::collections::BTreeMap;
use std::sync::Arc;

fn with_id(mut d: Document) -> Document {
    let id = lopdf::Object::String(b"0123456789abcdef".to_vec(), lopdf::StringFormat::Hexadecimal);
    d.trailer.set("ID", vec![id.clone(), id]);
    d
}

fn save(mut d: Document) -> Vec<u8> {
    let mut v = Vec::new();
    d.save_to(&mut v).unwrap();
    v
}

fn encrypted_bytes(user_pw: &str) -> Vec<u8> {
    let mut doc = with_id(make_pdf("E", 3, false));
    let perms = Permissions::PRINTABLE | Permissions::COPYABLE;
    let cf: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let ver = EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), cf)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: "owner",
        user_password: user_pw,
        permissions: perms,
    };
    let state = EncryptionState::try_from(ver).unwrap();
    doc.encrypt(&state).unwrap();
    save(doc)
}

fn encrypted_rc4(user_pw: &str) -> Vec<u8> {
    let mut doc = with_id(make_pdf("R", 2, false));
    let ver = EncryptionVersion::V2 {
        document: &doc,
        owner_password: "owner",
        user_password: user_pw,
        key_length: 128,
        permissions: Permissions::PRINTABLE,
    };
    let state = EncryptionState::try_from(ver).unwrap();
    doc.encrypt(&state).unwrap();
    save(doc)
}

#[test]
fn encrypted_with_user_password_requires_it() {
    let bytes = encrypted_bytes("secret");
    let lim = Limits::default();
    assert_eq!(load_bytes(&bytes, None, &lim).err(), Some(LoadError::NeedsPassword));
    assert_eq!(load_bytes(&bytes, Some("nope"), &lim).err(), Some(LoadError::WrongPassword));
    let doc = load_bytes(&bytes, Some("secret"), &lim).expect("correct password opens");
    assert_eq!(markers(&doc), ["E-P0", "E-P1", "E-P2"]);
}

#[test]
fn rc4_encrypted_file_opens_with_password_and_output_is_decrypted() {
    let bytes = encrypted_rc4("pw");
    let lim = Limits::default();
    assert_eq!(load_bytes(&bytes, None, &lim).err(), Some(LoadError::NeedsPassword));
    let doc = load_bytes(&bytes, Some("pw"), &lim).unwrap();
    let p = project_of(vec![("r.pdf", doc)]);
    let (out, rep) = build(&p, None).unwrap();
    assert!(rep.warnings.iter().any(|w| w.contains("not encrypted")) || !load(&out).is_encrypted());
    let reloaded = load_bytes(&out, None, &lim).expect("output needs no password");
    assert_eq!(markers(&reloaded), ["R-P0", "R-P1"]);
}

#[test]
fn owner_only_encryption_opens_without_password() {
    let bytes = encrypted_bytes(""); // empty user password
    let doc = load_bytes(&bytes, None, &Limits::default()).expect("empty user password");
    assert_eq!(markers(&doc).len(), 3);
}

#[test]
fn merge_of_encrypted_and_plain_sources_works_after_unlock() {
    let enc = load_bytes(&encrypted_bytes("secret"), Some("secret"), &Limits::default()).unwrap();
    let p = project_of(vec![("a.pdf", make_pdf("A", 2, false)), ("e.pdf", enc)]);
    let (bytes, _) = build(&p, None).unwrap();
    assert_eq!(markers(&load(&bytes)), ["A-P0", "A-P1", "E-P0", "E-P1", "E-P2"]);
}

#[test]
fn garbage_and_degenerate_inputs_are_errors_not_panics() {
    let lim = Limits::default();
    let cases: Vec<Vec<u8>> = vec![
        vec![],
        b"%PDF-".to_vec(),
        b"%PDF-1.7\n".to_vec(),
        b"hello world, not a pdf".to_vec(),
        vec![0u8; 4096],
        (0..=255u8).cycle().take(10_000).collect(),
        b"%PDF-1.4\n1 0 obj<</Type/Catalog>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF".to_vec(), // no pages
        b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 1 0 R>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF".to_vec(), // cyclic tree
    ];
    for (i, c) in cases.iter().enumerate() {
        assert!(load_bytes(c, None, &lim).is_err(), "case {i} must be rejected");
    }
}

#[test]
fn truncated_valid_pdf_is_handled() {
    let good = save(make_pdf("T", 5, true));
    let lim = Limits::default();
    for cut in [10, 100, good.len() / 4, good.len() / 2, good.len() - 20, good.len() - 1] {
        // must not panic; either rejected or repaired into something we can still write out
        if let Ok(d) = load_bytes(&good[..cut], None, &lim) {
            let p = project_of(vec![("t.pdf", d)]);
            let _ = build(&p, None);
        }
    }
}

#[test]
fn byte_flip_mutation_never_panics_through_load_and_build() {
    // Deterministic mini-fuzz: xorshift mutations of a valid file, through load -> edit -> build.
    let good = save(make_pdf("M", 4, true));
    let lim = Limits::default();
    let mut state = 0x9E3779B97F4A7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let (mut loaded, mut built) = (0, 0);
    let iters: usize = std::env::var("FUZZ_ITERS").ok().and_then(|v| v.parse().ok()).unwrap_or(400);
    for _ in 0..iters {
        let mut b = good.clone();
        for _ in 0..(1 + next() % 8) {
            let i = (next() as usize) % b.len();
            match next() % 3 {
                0 => b[i] = next() as u8,
                1 => b[i] ^= 1 << (next() % 8),
                _ => {
                    b.remove(i);
                }
            }
        }
        if let Ok(d) = load_bytes(&b, None, &lim) {
            loaded += 1;
            // signing must also survive damaged-but-loadable files (both modes)
            for mode in [pdf_core::sign::Mode::Annotate, pdf_core::sign::Mode::Flatten] {
                let mut signed = d.clone();
                let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]));
                let pl = pdf_core::sign::Placement {
                    page: 0,
                    x: 10.0,
                    y: 10.0,
                    w: 40.0,
                    h: 20.0,
                    rotation: 30.0,
                    content: pdf_core::sign::Content::Image(img),
                };
                if pdf_core::sign::apply(&mut signed, &[pl], mode).is_ok() {
                    let mut out = Vec::new();
                    let _ = signed.save_to(&mut out);
                }
            }
            let mut p = project_of(vec![("m.pdf", d)]);
            if p.pages.len() > 1 {
                let _ = p.move_pages(&[0], 1);
                let _ = p.duplicate(&[0]);
            }
            if build(&p, None).is_ok() {
                built += 1;
            }
        }
    }
    assert!(loaded * 8 > iters, "mutations should often still load ({loaded}/{iters})");
    assert!(built * 16 > iters, "and often still build ({built}/{iters})");
}

#[test]
fn limits_are_enforced() {
    let good = save(make_pdf("L", 5, false));
    let small = Limits { max_file_size: 100, ..Limits::default() };
    assert!(matches!(load_bytes(&good, None, &small), Err(LoadError::TooLarge { .. })));
    let few = Limits { max_pages: 3, ..Limits::default() };
    assert!(matches!(load_bytes(&good, None, &few), Err(LoadError::TooManyPages { pages: 5, max: 3 })));
}

#[test]
fn split_by_bookmarks_uses_top_level_pages() {
    let doc = make_pdf("S", 5, false); // bookmark i -> page i, (child under bm0 is not top level)
    let parts = split_by_bookmarks(&doc);
    assert_eq!(parts.len(), 5);
    assert_eq!(parts[0].pages, vec![0]);
    assert_eq!(parts[4].pages, vec![4]);
    assert_eq!(parts[2].label, "S-bm2");
}

#[test]
fn split_by_bookmarks_includes_leading_pages_and_merges_shared_pages() {
    // Keep only bookmarks for pages 2 and 4 of 6 => parts: [0,1] "start", [2,3] bm2, [4,5] bm4
    let mut doc = make_pdf("S", 6, false);
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let ol = doc.get_dictionary(root).unwrap().get(b"Outlines").unwrap().as_reference().unwrap();
    let ids: Vec<_> = {
        let mut v = vec![];
        let mut cur = doc.get_dictionary(ol).unwrap().get(b"First").ok().cloned();
        while let Some(lopdf::Object::Reference(id)) = cur.take() {
            v.push(id);
            cur = doc.get_dictionary(id).unwrap().get(b"Next").ok().cloned();
        }
        v
    };
    // relink: bm2 -> bm4 only
    let (a, b) = (ids[2], ids[4]);
    doc.get_dictionary_mut(a).unwrap().remove(b"Prev");
    doc.get_dictionary_mut(a).unwrap().set("Next", lopdf::Object::Reference(b));
    doc.get_dictionary_mut(b).unwrap().set("Prev", lopdf::Object::Reference(a));
    doc.get_dictionary_mut(b).unwrap().remove(b"Next");
    doc.get_dictionary_mut(ol).unwrap().set("First", lopdf::Object::Reference(a));
    let parts = split_by_bookmarks(&doc);
    let shape: Vec<_> = parts.iter().map(|p| (p.label.as_str(), p.pages.clone())).collect();
    assert_eq!(shape, [("start", vec![0, 1]), ("S-bm2", vec![2, 3]), ("S-bm4", vec![4, 5])]);
}

#[test]
fn split_parts_build_independently() {
    let p = project_of(vec![("a.pdf", make_pdf("A", 6, false))]);
    let parts = pdf_core::split_every_n(6, 2);
    for (i, part) in parts.iter().enumerate() {
        let (bytes, _) = build(&p, Some(&part.pages)).unwrap();
        let d = load(&bytes);
        assert_eq!(markers(&d), [format!("A-P{}", i * 2), format!("A-P{}", i * 2 + 1)]);
    }
}
