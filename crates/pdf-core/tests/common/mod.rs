#![allow(dead_code)]
//! Test PDF generator: pages carry a text marker, bookmarks, links and form fields so that
//! page-remapping can be verified structurally.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use pdf_core::{PageKind, Project};

pub fn s(t: &str) -> Object {
    Object::String(t.as_bytes().to_vec(), StringFormat::Literal)
}

/// `n` pages with marker text "{tag}-P{i}". Each page i has: a bookmark "{tag}-bm{i}" -> page i
/// (page 0 also has a child bookmark -> last page), a Link /Dest -> page (i+1)%n, a GoTo-action
/// link on page 0 -> last page, a named-destination link on page 1 (if any), and widgets for
/// fields "name" (page 0) and "email" (page 1) when `forms`.
pub fn make_pdf(tag: &str, n: usize, forms: bool) -> Document {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica" });
    let page_ids: Vec<ObjectId> = (0..n).map(|_| doc.new_object_id()).collect();
    let mut field_ids: Vec<Object> = Vec::new();

    for (i, &pid) in page_ids.iter().enumerate() {
        let content = doc.add_object(Stream::new(
            dictionary! {},
            format!("BT /F1 12 Tf 20 20 Td ({tag}-P{i}) Tj ET").into_bytes(),
        ));
        let mut annots: Vec<Object> = Vec::new();
        let next = page_ids[(i + 1) % n];
        annots.push(Object::Reference(doc.add_object(dictionary! {
            "Type"=>"Annot","Subtype"=>"Link","Rect"=>vec![0.into(),0.into(),50.into(),50.into()],
            "Dest"=>vec![Object::Reference(next), "Fit".into()],
        })));
        if i == 0 {
            annots.push(Object::Reference(doc.add_object(dictionary! {
                "Type"=>"Annot","Subtype"=>"Link","Rect"=>vec![60.into(),0.into(),90.into(),50.into()],
                "A"=>dictionary!{ "S"=>"GoTo", "D"=>vec![Object::Reference(page_ids[n-1]), "Fit".into()] },
            })));
        }
        if i == 1 {
            annots.push(Object::Reference(doc.add_object(dictionary! {
                "Type"=>"Annot","Subtype"=>"Link","Rect"=>vec![60.into(),0.into(),90.into(),50.into()],
                "Dest"=>s("named_last"),
            })));
        }
        if forms && i < 2 {
            let (name, ) = if i == 0 { ("name",) } else { ("email",) };
            let w = doc.add_object(dictionary! {
                "Type"=>"Annot","Subtype"=>"Widget","FT"=>"Tx","T"=>s(name),
                "Rect"=>vec![100.into(),100.into(),200.into(),120.into()],"P"=>Object::Reference(pid),
            });
            annots.push(Object::Reference(w));
            field_ids.push(Object::Reference(w));
        }
        doc.objects.insert(pid, Object::Dictionary(dictionary! {
            "Type"=>"Page","Parent"=>Object::Reference(pages_id),
            "Contents"=>Object::Reference(content),"Annots"=>annots,
        }));
    }
    // MediaBox and Resources are inherited from the Pages node on purpose.
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {
        "Type"=>"Pages","Count"=>n as i64,
        "Kids"=>page_ids.iter().map(|p| Object::Reference(*p)).collect::<Vec<_>>(),
        "MediaBox"=>vec![0.into(),0.into(),300.into(),400.into()],
        "Resources"=>dictionary!{ "Font"=>dictionary!{ "F1"=>Object::Reference(font) } },
    }));

    // Outline: one entry per page; first has a child pointing at the last page.
    let root = doc.new_object_id();
    let items: Vec<ObjectId> = (0..n).map(|_| doc.new_object_id()).collect();
    let child = doc.new_object_id();
    for (i, &id) in items.iter().enumerate() {
        let mut d = dictionary! {
            "Title"=>s(&format!("{tag}-bm{i}")), "Parent"=>Object::Reference(root),
            "Dest"=>vec![Object::Reference(page_ids[i]), "Fit".into()],
        };
        if i > 0 { d.set("Prev", Object::Reference(items[i-1])); }
        if i + 1 < n { d.set("Next", Object::Reference(items[i+1])); }
        if i == 0 {
            d.set("First", Object::Reference(child));
            d.set("Last", Object::Reference(child));
            d.set("Count", Object::Integer(1));
        }
        doc.objects.insert(id, Object::Dictionary(d));
    }
    doc.objects.insert(child, Object::Dictionary(dictionary! {
        "Title"=>s(&format!("{tag}-child")), "Parent"=>Object::Reference(items[0]),
        "A"=>dictionary!{ "S"=>"GoTo", "D"=>vec![Object::Reference(page_ids[n-1]), "Fit".into()] },
    }));
    doc.objects.insert(root, Object::Dictionary(dictionary! {
        "Type"=>"Outlines","First"=>Object::Reference(items[0]),"Last"=>Object::Reference(items[n-1]),"Count"=>(n as i64 + 1),
    }));

    let mut catalog = dictionary! {
        "Type"=>"Catalog","Pages"=>Object::Reference(pages_id),"Outlines"=>Object::Reference(root),
        "Dests"=>dictionary!{ "named_last"=>vec![Object::Reference(page_ids[n-1]), "Fit".into()] },
    };
    if forms {
        catalog.set("AcroForm", dictionary! { "Fields"=>field_ids, "DA"=>s("/F1 10 Tf 0 g") });
    }
    let cat = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(cat));
    doc
}

pub fn project_of(docs: Vec<(&str, Document)>) -> Project {
    let mut p = Project::new();
    for (name, d) in docs {
        let i = p.add_source(d, name);
        p.append_source(i).unwrap();
    }
    p
}

// ---------- reading results back ----------

pub fn load(bytes: &[u8]) -> Document {
    Document::load_mem(bytes).expect("output reloads")
}

/// Marker text of each output page, in order, e.g. ["A-P2","A-P0"] (blank pages give "").
pub fn markers(doc: &Document) -> Vec<String> {
    doc.get_pages().values().map(|&id| marker_of(doc, id)).collect()
}

pub fn marker_of(doc: &Document, page: ObjectId) -> String {
    let c = String::from_utf8_lossy(&doc.get_page_content(page)).into_owned();
    match (c.find('('), c.find(')')) {
        (Some(a), Some(b)) if b > a => c[a + 1..b].to_string(),
        _ => String::new(),
    }
}

pub fn marker_for_ref(doc: &Document, o: &Object) -> Option<String> {
    match o {
        Object::Reference(id) if doc.get_pages().values().any(|p| p == id) => Some(marker_of(doc, *id)),
        _ => None,
    }
}

/// Bookmarks as (title, depth, target marker) in document order.
pub fn bookmarks(doc: &Document) -> Vec<(String, usize, Option<String>)> {
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let cat = doc.get_dictionary(root).unwrap();
    let mut out = Vec::new();
    if let Ok(Object::Reference(ol)) = cat.get(b"Outlines") {
        let first = doc.get_dictionary(*ol).unwrap().get(b"First").ok().cloned();
        walk(doc, first, 0, &mut out);
    }
    out
}

fn walk(doc: &Document, mut cur: Option<Object>, depth: usize, out: &mut Vec<(String, usize, Option<String>)>) {
    while let Some(Object::Reference(id)) = cur.take() {
        let d = doc.get_dictionary(id).unwrap();
        let title = match d.get(b"Title").unwrap() { Object::String(b, _) => String::from_utf8_lossy(b).into_owned(), _ => String::new() };
        let dest = d.get(b"Dest").ok().cloned().or_else(|| {
            d.get(b"A").ok().and_then(|a| match a {
                Object::Dictionary(a) => a.get(b"D").ok().cloned(),
                _ => None,
            })
        });
        let target = dest.and_then(|d| match d {
            Object::Array(a) => a.first().and_then(|o| marker_for_ref(doc, o)),
            _ => None,
        });
        out.push((title, depth, target));
        walk(doc, d.get(b"First").ok().cloned(), depth + 1, out);
        cur = d.get(b"Next").ok().cloned();
    }
}

/// Link destinations on each page: Vec per page of target markers (None = non-page/ext).
pub fn link_targets(doc: &Document) -> Vec<Vec<Option<String>>> {
    doc.get_pages().values().map(|&pid| {
        let page = doc.get_dictionary(pid).unwrap();
        let mut v = Vec::new();
        if let Ok(a) = page.get(b"Annots") {
            let list = match a { Object::Array(l) => l.clone(), Object::Reference(r) => doc.get_object(*r).unwrap().as_array().unwrap().clone(), _ => vec![] };
            for e in list {
                let ad = match &e { Object::Reference(r) => doc.get_dictionary(*r).unwrap().clone(), Object::Dictionary(d) => d.clone(), _ => continue };
                if ad.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) != Some(b"Link") { continue; }
                let dest = ad.get(b"Dest").ok().cloned().or_else(|| match ad.get(b"A") { Ok(Object::Dictionary(a)) => a.get(b"D").ok().cloned(), _ => None });
                v.push(dest.and_then(|d| match d { Object::Array(a) => a.first().and_then(|o| marker_for_ref(doc, o)), _ => None }));
            }
        }
        v
    }).collect()
}

pub fn field_names(doc: &Document) -> Vec<String> {
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let cat = doc.get_dictionary(root).unwrap();
    let Ok(af) = cat.get(b"AcroForm") else { return vec![] };
    let af = match af { Object::Dictionary(d) => d.clone(), Object::Reference(r) => doc.get_dictionary(*r).unwrap().clone(), _ => return vec![] };
    let fields = match af.get(b"Fields").unwrap() { Object::Array(a) => a.clone(), _ => vec![] };
    fields.iter().filter_map(|f| f.as_reference().ok()).filter_map(|id| match doc.get_dictionary(id).unwrap().get(b"T") { Ok(Object::String(b, _)) => Some(String::from_utf8_lossy(b).into_owned()), _ => None }).collect()
}

pub fn src_entry(src: usize, page: usize) -> PageKind { PageKind::Source { src, page } }

pub fn is_page_dict_clean(doc: &Document) -> bool {
    // every page has MediaBox & Resources directly (inheritance flattened) and Parent -> the single tree
    let mut parent: Option<Object> = None;
    doc.get_pages().values().all(|&p| {
        let d: &Dictionary = doc.get_dictionary(p).unwrap();
        let par = d.get(b"Parent").unwrap().clone();
        let same = parent.as_ref().map_or(true, |x| *x == par);
        parent = Some(par);
        same && d.has(b"MediaBox") && d.has(b"Resources")
    })
}
