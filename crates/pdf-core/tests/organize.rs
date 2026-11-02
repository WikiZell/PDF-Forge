mod common;
use common::*;
use pdf_core::{build, BlankSize};

fn out(p: &pdf_core::Project, subset: Option<&[usize]>) -> (lopdf::Document, pdf_core::BuildReport) {
    let (bytes, rep) = build(p, subset).expect("build");
    assert!(bytes.starts_with(b"%PDF-"));
    (load(&bytes), rep)
}

#[test]
fn identity_roundtrip_keeps_pages_bookmarks_links() {
    let p = project_of(vec![("a.pdf", make_pdf("A", 4, false))]);
    let (d, rep) = out(&p, None);
    assert_eq!(markers(&d), ["A-P0", "A-P1", "A-P2", "A-P3"]);
    assert!(is_page_dict_clean(&d), "inherited MediaBox/Resources flattened under one Pages node");
    assert_eq!(rep.dropped_links, 0);
    assert_eq!(rep.dropped_bookmarks, 0);
    let bm = bookmarks(&d);
    assert_eq!(bm.len(), 5);
    assert_eq!(bm[0], ("A-bm0".into(), 0, Some("A-P0".into())));
    assert_eq!(bm[1], ("A-child".into(), 1, Some("A-P3".into())));
    assert_eq!(bm[4], ("A-bm3".into(), 0, Some("A-P3".into())));
    // page0: /Dest->P1, GoTo->P3 ; page1: /Dest->P2, named->P3
    let l = link_targets(&d);
    assert_eq!(l[0], [Some("A-P1".into()), Some("A-P3".into())]);
    assert_eq!(l[1], [Some("A-P2".into()), Some("A-P3".into())]);
}

#[test]
fn reorder_remaps_bookmarks_and_links_to_new_page_objects() {
    let mut p = project_of(vec![("a.pdf", make_pdf("A", 4, false))]);
    p.move_pages(&[3], 0).unwrap(); // P3 P0 P1 P2
    p.move_pages(&[1], 3).unwrap(); // P3 P1 P2 P0
    let (d, _) = out(&p, None);
    assert_eq!(markers(&d), ["A-P3", "A-P1", "A-P2", "A-P0"]);
    let bm = bookmarks(&d);
    // bookmark order is unchanged but each still targets the page with its marker
    for (title, _, target) in &bm {
        match title.as_str() {
            "A-bm0" => assert_eq!(target.as_deref(), Some("A-P0")),
            "A-bm1" => assert_eq!(target.as_deref(), Some("A-P1")),
            "A-bm2" => assert_eq!(target.as_deref(), Some("A-P2")),
            "A-bm3" => assert_eq!(target.as_deref(), Some("A-P3")),
            "A-child" => assert_eq!(target.as_deref(), Some("A-P3")),
            _ => panic!("unexpected {title}"),
        }
    }
    let l = link_targets(&d);
    // output page 0 is A-P3: its link pointed to A-P0 (3+1)%4
    assert_eq!(l[0], [Some("A-P0".into())]);
    // output page 3 is A-P0: /Dest -> A-P1 and GoTo -> A-P3
    assert_eq!(l[3], [Some("A-P1".into()), Some("A-P3".into())]);
    // output page 1 is A-P1: /Dest -> A-P2, named -> A-P3
    assert_eq!(l[1], [Some("A-P2".into()), Some("A-P3".into())]);
}

#[test]
fn delete_drops_links_and_bookmarks_to_removed_pages_but_keeps_children() {
    let mut p = project_of(vec![("a.pdf", make_pdf("A", 4, false))]);
    p.delete(&[3]).unwrap(); // remove A-P3
    let (d, rep) = out(&p, None);
    assert_eq!(markers(&d), ["A-P0", "A-P1", "A-P2"]);
    let titles: Vec<_> = bookmarks(&d).into_iter().map(|b| b.0).collect();
    assert_eq!(titles, ["A-bm0", "A-bm1", "A-bm2"], "bm3 and the child (both -> P3) are dropped");
    assert_eq!(rep.dropped_bookmarks, 2);
    let l = link_targets(&d);
    assert_eq!(l[0], [Some("A-P1".into())], "GoTo link to deleted page removed");
    assert_eq!(l[1], [Some("A-P2".into())], "named link to deleted page removed");
    assert_eq!(l[2], [], "link from P2 to deleted P3 removed");
    assert_eq!(rep.dropped_links, 3);
    // no page-less leftovers: every outline/link target resolves to a page in the output
    for (_, _, t) in bookmarks(&d) { assert!(t.is_some()); }
}

#[test]
fn deleting_a_parent_bookmark_page_promotes_its_children() {
    let mut p = project_of(vec![("a.pdf", make_pdf("A", 4, false))]);
    p.delete(&[0]).unwrap(); // bm0 -> P0 gone, its child -> P3 survives
    let (d, _) = out(&p, None);
    let bm = bookmarks(&d);
    assert_eq!(bm[0], ("A-child".into(), 0, Some("A-P3".into())), "child promoted to top level");
}

#[test]
fn rotate_composes_with_existing_rotation() {
    let mut src = make_pdf("A", 2, false);
    let first = *src.get_pages().values().next().unwrap();
    if let Some(lopdf::Object::Dictionary(d)) = src.objects.get_mut(&first) { d.set("Rotate", 90); }
    let mut p = project_of(vec![("a.pdf", src)]);
    p.rotate(&[0, 1], 90).unwrap();
    let (d, _) = out(&p, None);
    let rot: Vec<i64> = d.get_pages().values().map(|&id| d.get_dictionary(id).unwrap().get(b"Rotate").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0)).collect();
    assert_eq!(rot, [180, 90]);
}

#[test]
fn duplicate_makes_independent_page_with_remapped_links() {
    let mut p = project_of(vec![("a.pdf", make_pdf("A", 3, false))]);
    p.duplicate(&[1]).unwrap();
    let (d, _) = out(&p, None);
    assert_eq!(markers(&d), ["A-P0", "A-P1", "A-P1", "A-P2"]);
    let l = link_targets(&d);
    assert_eq!(l[1], l[2], "the copy keeps the same links");
    // The links on the original page and on the copy must be distinct objects.
    let pages: Vec<_> = d.get_pages().values().copied().collect();
    let a = d.get_dictionary(pages[1]).unwrap().get(b"Annots").unwrap().as_array().unwrap().clone();
    let b = d.get_dictionary(pages[2]).unwrap().get(b"Annots").unwrap().as_array().unwrap().clone();
    assert!(a.iter().all(|x| !b.contains(x)));
}

#[test]
fn blank_pages_have_requested_size() {
    let mut p = project_of(vec![("a.pdf", make_pdf("A", 2, false))]);
    p.insert_blank(1, BlankSize::A4).unwrap();
    p.insert_blank(0, BlankSize::Letter).unwrap();
    let (d, _) = out(&p, None);
    assert_eq!(markers(&d), ["", "A-P0", "", "A-P1"]);
    let pages: Vec<_> = d.get_pages().values().copied().collect();
    let mb = |i: usize| -> Vec<f32> { d.get_dictionary(pages[i]).unwrap().get(b"MediaBox").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect() };
    assert_eq!(mb(0)[2..], [612.0, 792.0]);
    assert!((mb(2)[2] - 595.276).abs() < 0.01 && (mb(2)[3] - 841.89).abs() < 0.01);
}

#[test]
fn extract_subset_writes_only_those_pages_in_given_order() {
    let p = project_of(vec![("a.pdf", make_pdf("A", 5, false))]);
    let (d, rep) = out(&p, Some(&[3, 1]));
    assert_eq!(markers(&d), ["A-P3", "A-P1"]);
    assert_eq!(bookmarks(&d).iter().filter(|b| b.2.is_none()).count(), 0);
    assert!(rep.dropped_bookmarks >= 1);
    // unrelated content is pruned: output is much smaller than full doc
    let (full, _) = build(&p, None).unwrap();
    let (part, _) = build(&p, Some(&[3, 1])).unwrap();
    assert!(part.len() < full.len());
}

#[test]
fn merge_two_documents_in_order_with_independent_remapping() {
    let p = project_of(vec![("a.pdf", make_pdf("A", 3, false)), ("b.pdf", make_pdf("B", 2, false))]);
    let (d, _) = out(&p, None);
    assert_eq!(markers(&d), ["A-P0", "A-P1", "A-P2", "B-P0", "B-P1"]);
    let titles: Vec<_> = bookmarks(&d).iter().map(|b| b.0.clone()).collect();
    assert_eq!(titles, ["A-bm0", "A-child", "A-bm1", "A-bm2", "B-bm0", "B-child", "B-bm1"]);
    for (t, _, target) in bookmarks(&d) {
        let tag = &t[..1];
        assert!(target.unwrap().starts_with(tag), "{t} must target a page of its own document");
    }
    let l = link_targets(&d);
    assert!(l[3].iter().flatten().all(|m| m.starts_with('B')));
    assert!(l[0].iter().flatten().all(|m| m.starts_with('A')));
}

#[test]
fn merge_renames_colliding_form_fields_and_reports_every_rename() {
    let p = project_of(vec![
        ("a.pdf", make_pdf("A", 2, true)),
        ("b.pdf", make_pdf("B", 2, true)),
        ("c.pdf", make_pdf("C", 2, true)),
    ]);
    let (d, rep) = out(&p, None);
    let mut names = field_names(&d);
    names.sort();
    assert_eq!(names, ["email", "email_2", "email_3", "name", "name_2", "name_3"]);
    let mut r: Vec<_> = rep.renamed_fields.iter().map(|r| (r.source.as_str(), r.from.as_str(), r.to.as_str())).collect();
    r.sort();
    assert_eq!(r, [
        ("b.pdf", "email", "email_2"), ("b.pdf", "name", "name_2"),
        ("c.pdf", "email", "email_3"), ("c.pdf", "name", "name_3"),
    ]);
}

#[test]
fn form_fields_on_deleted_pages_are_removed_from_the_form() {
    let mut p = project_of(vec![("a.pdf", make_pdf("A", 3, true))]);
    p.delete(&[1]).unwrap(); // removes the page with "email"
    let (d, _) = out(&p, None);
    assert_eq!(field_names(&d), ["name"]);
}

#[test]
fn insert_pages_from_another_document_at_position() {
    let mut p = project_of(vec![("a.pdf", make_pdf("A", 3, false))]);
    let b = p.add_source(make_pdf("B", 3, false), "b.pdf");
    p.insert_from(b, Some(&[2, 0]), 1).unwrap();
    let (d, _) = out(&p, None);
    assert_eq!(markers(&d), ["A-P0", "B-P2", "B-P0", "A-P1", "A-P2"]);
}

#[test]
fn page_scripts_are_stripped() {
    let mut src = make_pdf("A", 1, false);
    let pid = *src.get_pages().values().next().unwrap();
    if let Some(lopdf::Object::Dictionary(d)) = src.objects.get_mut(&pid) {
        d.set("AA", lopdf::dictionary! { "O" => lopdf::dictionary!{ "S"=>"JavaScript", "JS"=>s("app.alert(1)") } });
    }
    let p = project_of(vec![("a.pdf", src)]);
    let (d, _) = out(&p, None);
    let pid = *d.get_pages().values().next().unwrap();
    assert!(!d.get_dictionary(pid).unwrap().has(b"AA"));
}

#[test]
fn empty_and_out_of_range_subsets_are_errors() {
    let p = project_of(vec![("a.pdf", make_pdf("A", 2, false))]);
    assert!(build(&p, Some(&[])).is_err());
    assert!(build(&p, Some(&[5])).is_err());
}

#[test]
fn output_survives_a_second_pass() {
    // build -> load -> build again must stay structurally valid (idempotent-ish).
    let mut p = project_of(vec![("a.pdf", make_pdf("A", 4, true))]);
    p.move_pages(&[0], 3).unwrap();
    let (b1, _) = build(&p, None).unwrap();
    let p2 = project_of(vec![("o.pdf", load(&b1))]);
    let (d2, _) = out(&p2, None);
    assert_eq!(markers(&d2), ["A-P1", "A-P2", "A-P3", "A-P0"]);
    assert_eq!(bookmarks(&d2).iter().filter(|b| b.2.is_none()).count(), 0);
}
