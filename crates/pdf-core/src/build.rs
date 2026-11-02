//! Turns a [`Project`] page list into a new PDF.
//!
//! Strategy: copy every source document's objects into one new document (renumbered so ids
//! never collide), build a fresh page tree from the page list, then rebuild the pieces that
//! reference pages by object id — bookmarks, link annotations and the AcroForm field tree —
//! so they follow the pages. Unreachable objects are pruned afterwards.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};

use crate::model::{PageKind, Project};

#[derive(Debug, Clone, PartialEq)]
pub struct FieldRename {
    pub source: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct BuildReport {
    /// Form fields renamed because another source already used the name. Report each to the user.
    pub renamed_fields: Vec<FieldRename>,
    pub dropped_links: usize,
    pub dropped_bookmarks: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum BuildError {
    NoPages,
    BadIndex(usize),
    Pdf(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPages => write!(f, "There are no pages to write."),
            Self::BadIndex(i) => write!(f, "Page {} does not exist.", i + 1),
            Self::Pdf(e) => write!(f, "Could not write the PDF: {e}"),
        }
    }
}
impl std::error::Error for BuildError {}

type Result<T> = std::result::Result<T, BuildError>;

// ---------- small object helpers ----------

pub(crate) fn deref<'a>(doc: &'a Document, mut o: &'a Object) -> Option<&'a Object> {
    for _ in 0..32 {
        match o {
            Object::Reference(id) => o = doc.objects.get(id)?,
            _ => return Some(o),
        }
    }
    None
}

pub(crate) fn dict<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Dictionary> {
    match deref(doc, o)? {
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

pub(crate) fn get<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    deref(doc, d.get(key).ok()?)
}

pub(crate) fn array<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Vec<Object>> {
    match deref(doc, o)? {
        Object::Array(a) => Some(a),
        _ => None,
    }
}

pub(crate) fn name_of(o: &Object) -> Option<&[u8]> {
    match o {
        Object::Name(n) => Some(n),
        _ => None,
    }
}

pub(crate) fn int(o: &Object) -> Option<i64> {
    match o {
        Object::Integer(i) => Some(*i),
        Object::Real(r) => Some(*r as i64),
        _ => None,
    }
}

fn text_string(b: &[u8]) -> String {
    if b.starts_with(&[0xFE, 0xFF]) {
        let u: Vec<u16> = b[2..].chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        b.iter().map(|&c| c as char).collect()
    }
}

fn to_text_string(s: &str) -> Object {
    if s.is_ascii() {
        Object::String(s.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut v = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_be_bytes());
        }
        Object::String(v, StringFormat::Hexadecimal)
    }
}

const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Copy inheritable page attributes from ancestor /Pages nodes into the page itself,
/// since the page is about to be re-parented under a new tree.
fn flatten_inherited(doc: &mut Document, page: ObjectId) {
    let mut found: Vec<(Vec<u8>, Object)> = Vec::new();
    let mut have: HashSet<Vec<u8>> = HashSet::new();
    let mut cur = Some(page);
    let mut guard = 0;
    while let Some(id) = cur {
        guard += 1;
        if guard > 64 {
            break;
        }
        let Some(Object::Dictionary(d)) = doc.objects.get(&id) else { break };
        for k in INHERITED {
            if !have.contains(k) {
                if let Ok(v) = d.get(k) {
                    have.insert(k.to_vec());
                    found.push((k.to_vec(), v.clone()));
                }
            }
        }
        cur = d.get(b"Parent").ok().and_then(|p| if let Object::Reference(r) = p { Some(*r) } else { None });
    }
    if let Some(Object::Dictionary(d)) = doc.objects.get_mut(&page) {
        for (k, v) in found {
            if !d.has(&k) {
                d.set(k, v);
            }
        }
    }
}

// ---------- destinations ----------

/// Resolve a destination object (array, name, string, or dict with /D) to an explicit array.
fn explicit_dest(doc: &Document, root: Option<&Dictionary>, o: &Object) -> Option<Vec<Object>> {
    match deref(doc, o)? {
        Object::Array(a) => Some(a.clone()),
        Object::Dictionary(d) => d.get(b"D").ok().and_then(|x| explicit_dest(doc, root, x)),
        Object::Name(n) => lookup_named(doc, root?, n),
        Object::String(s, _) => lookup_named(doc, root?, s),
        _ => None,
    }
}

fn lookup_named(doc: &Document, catalog: &Dictionary, key: &[u8]) -> Option<Vec<Object>> {
    // PDF 1.1 style: /Dests dictionary
    if let Some(d) = get(doc, catalog, b"Dests").and_then(|o| dict(doc, o)) {
        if let Ok(v) = d.get(key) {
            return explicit_dest(doc, None, v);
        }
    }
    // Name tree: /Names /Dests
    let names = get(doc, catalog, b"Names").and_then(|o| dict(doc, o))?;
    let tree = get(doc, names, b"Dests").and_then(|o| dict(doc, o))?;
    name_tree_lookup(doc, tree, key, 0).and_then(|v| explicit_dest(doc, None, v))
}

fn name_tree_lookup<'a>(doc: &'a Document, node: &'a Dictionary, key: &[u8], depth: usize) -> Option<&'a Object> {
    if depth > 32 {
        return None;
    }
    if let Some(arr) = get(doc, node, b"Names").and_then(|o| array(doc, o)) {
        for pair in arr.chunks(2) {
            if let [Object::String(k, _), v] = pair {
                if k.as_slice() == key {
                    return Some(v);
                }
            }
        }
    }
    if let Some(kids) = get(doc, node, b"Kids").and_then(|o| array(doc, o)) {
        for k in kids {
            if let Some(kd) = dict(doc, k) {
                if let Some(r) = name_tree_lookup(doc, kd, key, depth + 1) {
                    return Some(r);
                }
            }
        }
    }
    None
}

/// Rewrite the page reference (element 0) of an explicit destination. None => target page is gone.
fn remap_dest(mut dest: Vec<Object>, map: &HashMap<ObjectId, ObjectId>) -> Option<Vec<Object>> {
    match dest.first() {
        Some(Object::Reference(old)) => {
            let new = map.get(old)?;
            dest[0] = Object::Reference(*new);
            Some(dest)
        }
        _ => None, // remote or malformed
    }
}

// ---------- outline ----------

struct OutlineNode {
    title: Object,
    dest: Option<Vec<Object>>,
    /// Non-GoTo action to preserve (e.g. URI); GoTo is converted to `dest`.
    action: Option<Object>,
    flags: Option<Object>,
    color: Option<Object>,
    open: bool,
    children: Vec<OutlineNode>,
}

fn read_outline(
    doc: &Document,
    catalog: &Dictionary,
    first: &Object,
    depth: usize,
    budget: &mut usize,
) -> Vec<OutlineNode> {
    let mut out = Vec::new();
    let mut cur = Some(first.clone());
    let mut seen: HashSet<ObjectId> = HashSet::new();
    while let Some(Object::Reference(id)) = cur.take() {
        if !seen.insert(id) || *budget == 0 || depth > 48 {
            break;
        }
        *budget -= 1;
        let Some(Object::Dictionary(d)) = doc.objects.get(&id) else { break };
        let mut dest = d.get(b"Dest").ok().and_then(|x| explicit_dest(doc, Some(catalog), x));
        let mut action = None;
        if dest.is_none() {
            if let Some(a) = d.get(b"A").ok().and_then(|x| dict(doc, x)) {
                if a.get(b"S").ok().and_then(name_of) == Some(b"GoTo") {
                    dest = a.get(b"D").ok().and_then(|x| explicit_dest(doc, Some(catalog), x));
                } else if a.get(b"S").ok().and_then(name_of) == Some(b"URI") {
                    action = Some(Object::Dictionary(a.clone()));
                }
            }
        }
        let children =
            d.get(b"First").ok().map(|f| read_outline(doc, catalog, f, depth + 1, budget)).unwrap_or_default();
        out.push(OutlineNode {
            title: d
                .get(b"Title")
                .ok()
                .and_then(|t| deref(doc, t))
                .cloned()
                .unwrap_or(Object::String(vec![], StringFormat::Literal)),
            dest,
            action,
            flags: d.get(b"F").ok().cloned(),
            color: d.get(b"C").ok().cloned(),
            open: d.get(b"Count").ok().and_then(int).is_some_and(|c| c > 0),
            children,
        });
        cur = d.get(b"Next").ok().cloned();
    }
    out
}

/// Remap destinations; drop bookmarks whose page was removed but keep (promote) their children.
fn remap_outline(nodes: Vec<OutlineNode>, map: &HashMap<ObjectId, ObjectId>, dropped: &mut usize) -> Vec<OutlineNode> {
    let mut out = Vec::new();
    for mut n in nodes {
        let children = remap_outline(std::mem::take(&mut n.children), map, dropped);
        let keep = match (&n.dest, &n.action) {
            (Some(d), _) => match remap_dest(d.clone(), map) {
                Some(nd) => {
                    n.dest = Some(nd);
                    true
                }
                None => false,
            },
            (None, Some(_)) => true,
            (None, None) => true, // heading without a target
        };
        if keep {
            n.children = children;
            out.push(n);
        } else {
            *dropped += 1;
            out.extend(children);
        }
    }
    out
}

fn visible_count(nodes: &[OutlineNode]) -> i64 {
    nodes.iter().map(|n| 1 + if n.open { visible_count(&n.children) } else { 0 }).sum()
}

fn write_outline_level(out: &mut Document, parent: ObjectId, nodes: &[OutlineNode]) -> Option<(ObjectId, ObjectId)> {
    if nodes.is_empty() {
        return None;
    }
    let ids: Vec<ObjectId> = nodes.iter().map(|_| out.new_object_id()).collect();
    for (i, n) in nodes.iter().enumerate() {
        let mut d = Dictionary::new();
        d.set("Title", n.title.clone());
        d.set("Parent", Object::Reference(parent));
        if i > 0 {
            d.set("Prev", Object::Reference(ids[i - 1]));
        }
        if i + 1 < ids.len() {
            d.set("Next", Object::Reference(ids[i + 1]));
        }
        if let Some(dest) = &n.dest {
            d.set("Dest", Object::Array(dest.clone()));
        }
        if let Some(a) = &n.action {
            d.set("A", a.clone());
        }
        if let Some(f) = &n.flags {
            d.set("F", f.clone());
        }
        if let Some(c) = &n.color {
            d.set("C", c.clone());
        }
        if let Some((first, last)) = write_outline_level(out, ids[i], &n.children) {
            d.set("First", Object::Reference(first));
            d.set("Last", Object::Reference(last));
            let c = visible_count(&n.children);
            d.set("Count", Object::Integer(if n.open { c } else { -c }));
        }
        out.objects.insert(ids[i], Object::Dictionary(d));
    }
    Some((ids[0], ids[ids.len() - 1]))
}

// ---------- main entry ----------

/// Build a PDF from the project's page list (or just `subset`, in the order given).
pub fn build(project: &Project, subset: Option<&[usize]>) -> Result<(Vec<u8>, BuildReport)> {
    let idxs: Vec<usize> = match subset {
        Some(s) => s.to_vec(),
        None => (0..project.pages.len()).collect(),
    };
    if idxs.is_empty() {
        return Err(BuildError::NoPages);
    }
    for &i in &idxs {
        if i >= project.pages.len() {
            return Err(BuildError::BadIndex(i));
        }
    }
    let mut report = BuildReport::default();
    let mut out = Document::with_version("1.7");

    // 1. Import each used source once (renumbered), remember its page objects and catalog.
    let mut used: Vec<usize> = Vec::new();
    for &i in &idxs {
        if let PageKind::Source { src, .. } = project.pages[i].kind {
            if !used.contains(&src) {
                used.push(src);
            }
        }
    }
    struct Imported {
        pages: Vec<ObjectId>,
        catalog: Option<ObjectId>,
        name: String,
    }
    let mut imported: HashMap<usize, Imported> = HashMap::new();
    let mut info_done = false;
    for &src in &used {
        let s = &project.sources[src];
        let mut d = s.doc.clone();
        d.renumber_objects_with(out.max_id + 1);
        let pages: Vec<ObjectId> = d.get_pages().values().copied().collect();
        for &p in &pages {
            flatten_inherited(&mut d, p);
        }
        let catalog =
            d.trailer.get(b"Root").ok().and_then(|o| if let Object::Reference(r) = o { Some(*r) } else { None });
        if !info_done {
            if let Ok(Object::Reference(r)) = d.trailer.get(b"Info") {
                out.trailer.set("Info", Object::Reference(*r));
                info_done = true;
            }
        }
        out.max_id = out.max_id.max(d.objects.keys().map(|k| k.0).max().unwrap_or(0));
        out.objects.extend(std::mem::take(&mut d.objects));
        imported.insert(src, Imported { pages, catalog, name: s.name.clone() });
    }

    // 2. Pages: first use of a source page reuses its object, repeats are cloned.
    let pages_id = out.new_object_id();
    let mut kids: Vec<Object> = Vec::new();
    let mut page_map: HashMap<ObjectId, ObjectId> = HashMap::new(); // old page obj -> out page obj (first use)
    let mut out_pages: Vec<(ObjectId, Option<usize>, bool)> = Vec::new(); // (id, src, is_clone)
    let mut first_used: HashSet<ObjectId> = HashSet::new();
    for &i in &idxs {
        let e = &project.pages[i];
        let (id, src, is_clone) = match &e.kind {
            PageKind::Source { src, page } => {
                let imp = &imported[src];
                let old = *imp.pages.get(*page).ok_or(BuildError::BadIndex(*page))?;
                let mut pd = match out.objects.get(&old) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => return Err(BuildError::Pdf("page object missing".into())),
                };
                pd.set("Parent", Object::Reference(pages_id));
                pd.remove(b"AA"); // page-level actions can carry scripts
                let clone = !first_used.insert(old);
                let id = if clone { out.new_object_id() } else { old };
                if !clone {
                    page_map.insert(old, id);
                }
                out.objects.insert(id, Object::Dictionary(pd));
                (id, Some(*src), clone)
            }
            PageKind::Blank { width, height } => {
                let mut pd = Dictionary::new();
                pd.set("Type", Object::Name(b"Page".to_vec()));
                pd.set("Parent", Object::Reference(pages_id));
                pd.set(
                    "MediaBox",
                    Object::Array(vec![0.into(), 0.into(), Object::Real(*width as f32), Object::Real(*height as f32)]),
                );
                pd.set("Resources", Object::Dictionary(Dictionary::new()));
                let id = out.new_object_id();
                out.objects.insert(id, Object::Dictionary(pd));
                (id, None, false)
            }
        };
        if e.rotate != 0 {
            if let Some(Object::Dictionary(pd)) = out.objects.get_mut(&id) {
                let cur = pd.get(b"Rotate").ok().and_then(int).unwrap_or(0) as i32;
                pd.set("Rotate", Object::Integer((cur + e.rotate).rem_euclid(360) as i64));
            }
        }
        kids.push(Object::Reference(id));
        out_pages.push((id, src, is_clone));
    }
    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set("Kids", Object::Array(kids));
    pages_dict.set("Count", Object::Integer(out_pages.len() as i64));
    out.objects.insert(pages_id, Object::Dictionary(pages_dict));

    // 3. Annotations: clone for repeated pages, then fix links.
    let catalogs: HashMap<usize, Dictionary> = imported
        .iter()
        .filter_map(|(src, imp)| match out.objects.get(&imp.catalog?) {
            Some(Object::Dictionary(d)) => Some((*src, d.clone())),
            _ => None,
        })
        .collect();
    let catalog_of = |src: usize| -> Option<Dictionary> { catalogs.get(&src).cloned() };
    let mut surviving_annots: HashSet<ObjectId> = HashSet::new();
    for (pid, src, is_clone) in out_pages.clone() {
        let Some(src) = src else { continue };
        let cat = catalog_of(src);
        let annots_obj = match out.objects.get(&pid) {
            Some(Object::Dictionary(d)) => d.get(b"Annots").ok().cloned(),
            _ => None,
        };
        let Some(annots_obj) = annots_obj else { continue };
        let list: Vec<Object> = match deref(&out, &annots_obj) {
            Some(Object::Array(a)) => a.clone(),
            _ => continue,
        };
        let mut new_list: Vec<Object> = Vec::new();
        for a in list {
            let (ad, aid) = match &a {
                Object::Reference(r) => match out.objects.get(r) {
                    Some(Object::Dictionary(d)) => (d.clone(), Some(*r)),
                    _ => continue,
                },
                Object::Dictionary(d) => (d.clone(), None),
                _ => continue,
            };
            let subtype = ad.get(b"Subtype").ok().and_then(name_of).map(|n| n.to_vec());
            if is_clone && subtype.as_deref() == Some(b"Widget") {
                report.warnings.push("Form fields on duplicated pages are not duplicated.".into());
                continue;
            }
            let mut ad = ad;
            if subtype.as_deref() == Some(b"Link") {
                // Resolve either /Dest or a GoTo action to an explicit destination.
                let mut dest = ad.get(b"Dest").ok().and_then(|x| explicit_dest(&out, cat.as_ref(), x));
                let mut is_goto = dest.is_some();
                if dest.is_none() {
                    if let Some(act) = ad.get(b"A").ok().and_then(|x| dict(&out, x)) {
                        if act.get(b"S").ok().and_then(name_of) == Some(b"GoTo") {
                            is_goto = true;
                            dest = act.get(b"D").ok().and_then(|x| explicit_dest(&out, cat.as_ref(), x));
                        }
                    }
                }
                if is_goto {
                    match dest.and_then(|d| remap_dest(d, &page_map)) {
                        Some(nd) => {
                            ad.remove(b"A");
                            ad.set("Dest", Object::Array(nd));
                        }
                        None => {
                            report.dropped_links += 1;
                            continue; // target page no longer exists
                        }
                    }
                }
            }
            ad.remove(b"P");
            match aid {
                Some(id) if !is_clone => {
                    out.objects.insert(id, Object::Dictionary(ad));
                    surviving_annots.insert(id);
                    new_list.push(Object::Reference(id));
                }
                _ => {
                    let nid = out.add_object(Object::Dictionary(ad));
                    surviving_annots.insert(nid);
                    new_list.push(Object::Reference(nid));
                }
            }
        }
        if let Some(Object::Dictionary(pd)) = out.objects.get_mut(&pid) {
            pd.set("Annots", Object::Array(new_list));
        }
    }

    // 4. Outline: rebuild from every used source, in order.
    let mut all_nodes: Vec<OutlineNode> = Vec::new();
    for &src in &used {
        let Some(cat) = catalog_of(src) else { continue };
        let Some(ol) = get(&out, &cat, b"Outlines").and_then(|o| dict(&out, o)) else { continue };
        if let Ok(first) = ol.get(b"First") {
            let mut budget = 100_000;
            let nodes = read_outline(&out, &cat, first, 0, &mut budget);
            all_nodes.extend(remap_outline(nodes, &page_map, &mut report.dropped_bookmarks));
        }
    }

    // 5. AcroForm: merge fields, keep only those with surviving widgets, rename collisions.
    let mut fields: Vec<Object> = Vec::new();
    let mut used_names: HashSet<String> = HashSet::new();
    let mut da: Option<Object> = None;
    let mut dr_fonts = Dictionary::new();
    let mut need_appearances = false;
    for &src in &used {
        let Some(cat) = catalog_of(src) else { continue };
        let Some(af) = get(&out, &cat, b"AcroForm").and_then(|o| dict(&out, o)).cloned() else { continue };
        if da.is_none() {
            da = af.get(b"DA").ok().cloned();
        }
        need_appearances |= matches!(af.get(b"NeedAppearances"), Ok(Object::Boolean(true)));
        if let Some(fonts) = get(&out, &af, b"DR")
            .and_then(|o| dict(&out, o))
            .and_then(|dr| get(&out, dr, b"Font"))
            .and_then(|o| dict(&out, o))
        {
            for (k, v) in fonts.iter() {
                if !dr_fonts.has(k) {
                    dr_fonts.set(k.clone(), v.clone());
                }
            }
        }
        let Some(top) = get(&out, &af, b"Fields").and_then(|o| array(&out, o)).cloned() else { continue };
        for f in top {
            let Object::Reference(fid) = f else { continue };
            if !field_survives(&mut out, fid, &surviving_annots, 0) {
                continue;
            }
            let Some(Object::Dictionary(fd)) = out.objects.get(&fid) else { continue };
            if let Ok(Object::String(t, _)) = fd.get(b"T") {
                let name = text_string(t);
                let mut final_name = name.clone();
                if used_names.contains(&name) {
                    let mut n = 2;
                    while used_names.contains(&format!("{name}_{n}")) {
                        n += 1;
                    }
                    final_name = format!("{name}_{n}");
                    report.renamed_fields.push(FieldRename {
                        source: imported[&src].name.clone(),
                        from: name,
                        to: final_name.clone(),
                    });
                    if let Some(Object::Dictionary(fd)) = out.objects.get_mut(&fid) {
                        fd.set("T", to_text_string(&final_name));
                    }
                }
                used_names.insert(final_name);
            }
            fields.push(Object::Reference(fid));
        }
    }

    // 6. Catalog.
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    if !all_nodes.is_empty() {
        let root = out.new_object_id();
        if let Some((first, last)) = write_outline_level(&mut out, root, &all_nodes) {
            let mut od = Dictionary::new();
            od.set("Type", Object::Name(b"Outlines".to_vec()));
            od.set("First", Object::Reference(first));
            od.set("Last", Object::Reference(last));
            od.set("Count", Object::Integer(visible_count(&all_nodes)));
            out.objects.insert(root, Object::Dictionary(od));
            catalog.set("Outlines", Object::Reference(root));
        }
    }
    if !fields.is_empty() {
        let mut af = Dictionary::new();
        af.set("Fields", Object::Array(fields));
        if let Some(da) = da {
            af.set("DA", da);
        }
        if !dr_fonts.is_empty() {
            let mut dr = Dictionary::new();
            dr.set("Font", Object::Dictionary(dr_fonts));
            af.set("DR", Object::Dictionary(dr));
        }
        if need_appearances {
            af.set("NeedAppearances", Object::Boolean(true));
        }
        catalog.set("AcroForm", Object::Dictionary(af));
    }
    let cat_id = out.add_object(Object::Dictionary(catalog));
    out.trailer.set("Root", Object::Reference(cat_id));

    if project.sources.iter().any(|s| s.doc.is_encrypted()) {
        report.warnings.push("The output is not encrypted.".into());
    }
    report.warnings.sort();
    report.warnings.dedup();

    prune_unreachable(&mut out);
    let mut bytes = Vec::new();
    out.save_to(&mut bytes).map_err(|e| BuildError::Pdf(e.to_string()))?;
    Ok((bytes, report))
}

/// Does this field (or any descendant) have a widget on a surviving page? Prunes dead kids.
fn field_survives(doc: &mut Document, id: ObjectId, alive: &HashSet<ObjectId>, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    let kids: Option<Vec<Object>> = match doc.objects.get(&id) {
        Some(Object::Dictionary(d)) => match d.get(b"Kids") {
            Ok(Object::Array(a)) => Some(a.clone()),
            _ => None,
        },
        _ => return false,
    };
    match kids {
        None => alive.contains(&id), // a terminal field that is its own widget
        Some(kids) => {
            let mut keep = Vec::new();
            for k in kids {
                if let Object::Reference(kid) = k {
                    if field_survives(doc, kid, alive, depth + 1) {
                        keep.push(Object::Reference(kid));
                    }
                }
            }
            let any = !keep.is_empty();
            if let Some(Object::Dictionary(d)) = doc.objects.get_mut(&id) {
                d.set("Kids", Object::Array(keep));
            }
            any
        }
    }
}

/// Remove objects not reachable from the trailer. Iterative with a visited set, so shared or
/// cyclic object graphs in hostile files cost linear time (lopdf's own `prune_objects` revisits
/// shared objects and can take exponential time; found by fuzzing).
fn prune_unreachable(doc: &mut Document) {
    let mut seen: HashSet<ObjectId> = HashSet::new();
    let mut stack: Vec<&Object> = doc.trailer.iter().map(|(_, v)| v).collect();
    while let Some(o) = stack.pop() {
        match o {
            Object::Reference(id) => {
                if seen.insert(*id) {
                    if let Some(t) = doc.objects.get(id) {
                        stack.push(t);
                    }
                }
            }
            Object::Array(a) => stack.extend(a.iter()),
            Object::Dictionary(d) => stack.extend(d.iter().map(|(_, v)| v)),
            Object::Stream(s) => stack.extend(s.dict.iter().map(|(_, v)| v)),
            _ => {}
        }
    }
    doc.objects.retain(|id, _| seen.contains(id));
}
