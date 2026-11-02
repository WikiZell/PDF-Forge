//! Split planning: which source pages go into which output file.

use lopdf::{Document, Object, ObjectId};
use std::collections::HashMap;

/// One planned output: a name suffix and 0-based page indices (into the project page list).
#[derive(Debug, Clone, PartialEq)]
pub struct SplitPart {
    pub label: String,
    pub pages: Vec<usize>,
}

/// Every `n` pages: 1-3, 4-6, ... (last part may be shorter).
pub fn split_every_n(page_count: usize, n: usize) -> Vec<SplitPart> {
    let n = n.max(1);
    (0..page_count)
        .step_by(n)
        .map(|s| {
            let e = (s + n).min(page_count);
            SplitPart { label: range_label(s, e - 1), pages: (s..e).collect() }
        })
        .collect()
}

/// Each range (1-based spec like `1-3,5,7-`) becomes one part, in the order written.
pub fn split_ranges(spec: &str, page_count: usize) -> Result<Vec<SplitPart>, String> {
    let mut parts = Vec::new();
    for piece in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let pages = crate::model::parse_ranges(piece, page_count)?;
        let label = range_label(*pages.iter().min().unwrap(), *pages.iter().max().unwrap());
        parts.push(SplitPart { label, pages });
    }
    if parts.is_empty() {
        return Err("No ranges given.".into());
    }
    Ok(parts)
}

fn range_label(first: usize, last: usize) -> String {
    if first == last {
        format!("p{}", first + 1)
    } else {
        format!("p{}-{}", first + 1, last + 1)
    }
}

/// Title and 0-based page index of each top-level bookmark that points at a page.
pub fn top_level_bookmarks(doc: &Document) -> Vec<(String, usize)> {
    let page_index: HashMap<ObjectId, usize> = doc.get_pages().values().enumerate().map(|(i, id)| (*id, i)).collect();
    let Some(Ok(root)) = doc.trailer.get(b"Root").ok().map(|o| o.as_reference()) else { return vec![] };
    let Ok(cat) = doc.get_dictionary(root) else { return vec![] };
    let Some(Object::Reference(ol)) = cat.get(b"Outlines").ok() else { return vec![] };
    let Ok(old) = doc.get_dictionary(*ol) else { return vec![] };
    let mut out = Vec::new();
    let mut cur = old.get(b"First").ok().cloned();
    let mut guard = 0;
    while let Some(Object::Reference(id)) = cur.take() {
        guard += 1;
        if guard > 100_000 {
            break;
        }
        let Ok(d) = doc.get_dictionary(id) else { break };
        let dest = d.get(b"Dest").ok().cloned().or_else(|| match d.get(b"A") {
            Ok(Object::Dictionary(a)) => a.get(b"D").ok().cloned(),
            Ok(Object::Reference(r)) => doc.get_dictionary(*r).ok().and_then(|a| a.get(b"D").ok().cloned()),
            _ => None,
        });
        let page = match dest {
            Some(Object::Array(a)) => match a.first() {
                Some(Object::Reference(p)) => page_index.get(p).copied(),
                _ => None,
            },
            _ => None,
        };
        if let Some(p) = page {
            let title = match d.get(b"Title") {
                Ok(Object::String(b, _)) => String::from_utf8_lossy(b).into_owned(),
                _ => String::new(),
            };
            out.push((title, p));
        }
        cur = d.get(b"Next").ok().cloned();
    }
    out
}

/// Split a single document at its top-level bookmarks. Pages before the first bookmark form an
/// extra leading part. Bookmarks sharing a page are merged into the first one.
pub fn split_by_bookmarks(doc: &Document) -> Vec<SplitPart> {
    let count = doc.get_pages().len();
    let mut marks = top_level_bookmarks(doc);
    marks.sort_by_key(|m| m.1); // stable: keeps document order for equal pages
    marks.dedup_by_key(|m| m.1);
    let mut parts = Vec::new();
    if let Some(first) = marks.first() {
        if first.1 > 0 {
            parts.push(SplitPart { label: "start".into(), pages: (0..first.1).collect() });
        }
    }
    for (i, (title, start)) in marks.iter().enumerate() {
        let end = marks.get(i + 1).map_or(count, |n| n.1);
        parts.push(SplitPart { label: title.clone(), pages: (*start..end).collect() });
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_n() {
        let p = split_every_n(7, 3);
        assert_eq!(p.iter().map(|x| x.pages.clone()).collect::<Vec<_>>(), vec![vec![0, 1, 2], vec![3, 4, 5], vec![6]]);
        assert_eq!(p[0].label, "p1-3");
        assert_eq!(p[2].label, "p7");
        assert_eq!(split_every_n(2, 0).len(), 2, "n=0 is treated as 1");
        assert!(split_every_n(0, 3).is_empty());
    }

    #[test]
    fn ranges() {
        let p = split_ranges("1-2, 4, 6-", 7).unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p[2].pages, vec![5, 6]);
        assert!(split_ranges("9", 7).is_err());
        assert!(split_ranges(" , ", 7).is_err());
    }
}
