//! Page-list editing model. Edits are held as a list of page references and only
//! turned into a PDF on save/export (see `build`). Source documents are never modified.

use lopdf::Document;

/// Where a page's content comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum PageKind {
    /// Page `page` (0-based, document order) of source `src`.
    Source { src: usize, page: usize },
    /// A new blank page, size in PDF points.
    Blank { width: f64, height: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageEntry {
    pub kind: PageKind,
    /// Extra clockwise rotation applied on top of the source page's own, a multiple of 90.
    pub rotate: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlankSize {
    A4,
    Letter,
    Custom(f64, f64),
}

impl BlankSize {
    pub fn points(self) -> (f64, f64) {
        match self {
            BlankSize::A4 => (595.276, 841.89),
            BlankSize::Letter => (612.0, 792.0),
            BlankSize::Custom(w, h) => (w, h),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
    IndexOutOfRange(usize),
    NoSuchSource(usize),
    EmptySelection,
    InvalidRotation(i32),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IndexOutOfRange(i) => write!(f, "Page {} does not exist.", i + 1),
            Self::NoSuchSource(i) => write!(f, "Unknown source document {i}."),
            Self::EmptySelection => write!(f, "No pages selected."),
            Self::InvalidRotation(d) => write!(f, "Rotation must be a multiple of 90 degrees (got {d})."),
        }
    }
}
impl std::error::Error for EditError {}

pub struct Source {
    pub doc: Document,
    pub name: String,
    pub page_count: usize,
}

/// A working set: loaded source documents plus the ordered page list built from them.
#[derive(Default)]
pub struct Project {
    pub sources: Vec<Source>,
    pub pages: Vec<PageEntry>,
    undo: Vec<Vec<PageEntry>>,
    redo: Vec<Vec<PageEntry>>,
}

const MAX_UNDO: usize = 200;

impl Project {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a source document; returns its index. Does not add pages.
    pub fn add_source(&mut self, doc: Document, name: impl Into<String>) -> usize {
        let page_count = doc.get_pages().len();
        self.sources.push(Source { doc, name: name.into(), page_count });
        self.sources.len() - 1
    }

    fn source_pages(&self, src: usize) -> Result<Vec<PageEntry>, EditError> {
        let s = self.sources.get(src).ok_or(EditError::NoSuchSource(src))?;
        Ok((0..s.page_count)
            .map(|page| PageEntry { kind: PageKind::Source { src, page }, rotate: 0 })
            .collect())
    }

    /// Append every page of a source. (Not undoable by itself; used when setting up.)
    pub fn append_source(&mut self, src: usize) -> Result<(), EditError> {
        let mut v = self.source_pages(src)?;
        self.pages.append(&mut v);
        Ok(())
    }

    fn checkpoint(&mut self) {
        self.undo.push(self.pages.clone());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn validate(&self, idx: &[usize]) -> Result<Vec<usize>, EditError> {
        if idx.is_empty() {
            return Err(EditError::EmptySelection);
        }
        for &i in idx {
            if i >= self.pages.len() {
                return Err(EditError::IndexOutOfRange(i));
            }
        }
        let mut v = idx.to_vec();
        v.sort_unstable();
        v.dedup();
        Ok(v)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(prev) => {
                self.redo.push(std::mem::replace(&mut self.pages, prev));
                true
            }
            None => false,
        }
    }
    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(&mut self.pages, next));
                true
            }
            None => false,
        }
    }

    /// Move the selected pages so they start at `to`, an index into the list *after* removing them.
    /// Selected pages keep their relative order.
    pub fn move_pages(&mut self, idx: &[usize], to: usize) -> Result<(), EditError> {
        let sel = self.validate(idx)?;
        self.checkpoint();
        let moved: Vec<PageEntry> = sel.iter().map(|&i| self.pages[i].clone()).collect();
        for &i in sel.iter().rev() {
            self.pages.remove(i);
        }
        let at = to.min(self.pages.len());
        for (k, p) in moved.into_iter().enumerate() {
            self.pages.insert(at + k, p);
        }
        Ok(())
    }

    /// Rotate by `degrees` (multiple of 90, positive = clockwise).
    pub fn rotate(&mut self, idx: &[usize], degrees: i32) -> Result<(), EditError> {
        if degrees % 90 != 0 {
            return Err(EditError::InvalidRotation(degrees));
        }
        let sel = self.validate(idx)?;
        self.checkpoint();
        for i in sel {
            self.pages[i].rotate = (self.pages[i].rotate + degrees).rem_euclid(360);
        }
        Ok(())
    }

    pub fn delete(&mut self, idx: &[usize]) -> Result<(), EditError> {
        let sel = self.validate(idx)?;
        self.checkpoint();
        for &i in sel.iter().rev() {
            self.pages.remove(i);
        }
        Ok(())
    }

    /// Insert a copy of each selected page directly after the selection's last page.
    pub fn duplicate(&mut self, idx: &[usize]) -> Result<(), EditError> {
        let sel = self.validate(idx)?;
        self.checkpoint();
        let copies: Vec<PageEntry> = sel.iter().map(|&i| self.pages[i].clone()).collect();
        let at = sel[sel.len() - 1] + 1;
        for (k, p) in copies.into_iter().enumerate() {
            self.pages.insert(at + k, p);
        }
        Ok(())
    }

    /// Insert a blank page so it ends up at index `at`.
    pub fn insert_blank(&mut self, at: usize, size: BlankSize) -> Result<(), EditError> {
        if at > self.pages.len() {
            return Err(EditError::IndexOutOfRange(at));
        }
        self.checkpoint();
        let (width, height) = size.points();
        self.pages.insert(at, PageEntry { kind: PageKind::Blank { width, height }, rotate: 0 });
        Ok(())
    }

    /// Insert a blank page matching the point-size of the page at `neighbour`.
    pub fn insert_blank_like(&mut self, at: usize, neighbour: usize, sizes: &[(f64, f64)]) -> Result<(), EditError> {
        let (w, h) = *sizes.get(neighbour).ok_or(EditError::IndexOutOfRange(neighbour))?;
        self.insert_blank(at, BlankSize::Custom(w, h))
    }

    /// Insert pages of source `src` (`None` = all) so the first lands at `at`.
    pub fn insert_from(&mut self, src: usize, pages: Option<&[usize]>, at: usize) -> Result<(), EditError> {
        if at > self.pages.len() {
            return Err(EditError::IndexOutOfRange(at));
        }
        let all = self.source_pages(src)?;
        let chosen: Vec<PageEntry> = match pages {
            None => all,
            Some(list) => {
                let mut v = Vec::new();
                for &p in list {
                    v.push(all.get(p).cloned().ok_or(EditError::IndexOutOfRange(p))?);
                }
                v
            }
        };
        if chosen.is_empty() {
            return Err(EditError::EmptySelection);
        }
        self.checkpoint();
        for (k, p) in chosen.into_iter().enumerate() {
            self.pages.insert(at + k, p);
        }
        Ok(())
    }
}

/// Parse a 1-based page-range string such as `"1-3,5,8-"` into 0-based indices
/// (ascending order as written; duplicates allowed). Open ends mean first/last page.
pub fn parse_ranges(spec: &str, page_count: usize) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), Some(b.trim())),
            None => (part, None),
        };
        let parse = |s: &str| s.parse::<usize>().map_err(|_| format!("\"{part}\" is not a valid page range."));
        let start = if a.is_empty() { 1 } else { parse(a)? };
        let end = match b {
            None => start,
            Some("") => page_count,
            Some(b) => parse(b)?,
        };
        if start == 0 || end == 0 || start > page_count || end > page_count {
            return Err(format!("\"{part}\" is outside 1-{page_count}."));
        }
        if start <= end {
            out.extend(start - 1..end);
        } else {
            out.extend((end - 1..start).rev());
        }
    }
    if out.is_empty() {
        return Err("No pages in range.".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proj(n: usize) -> Project {
        let mut p = Project::new();
        let mut d = Document::with_version("1.5");
        // pages are irrelevant for list-model tests; fake the count
        let _ = &mut d;
        p.sources.push(Source { doc: d, name: "a".into(), page_count: n });
        p.append_source(0).unwrap();
        p
    }
    fn order(p: &Project) -> Vec<usize> {
        p.pages.iter().map(|e| match e.kind { PageKind::Source { page, .. } => page, _ => 99 }).collect()
    }

    #[test]
    fn move_keeps_selection_order() {
        let mut p = proj(6);
        p.move_pages(&[1, 2], 4).unwrap(); // after removal list is [0,3,4,5]; insert at 4 = end
        assert_eq!(order(&p), vec![0, 3, 4, 5, 1, 2]);
        p.move_pages(&[5, 4], 0).unwrap();
        assert_eq!(order(&p), vec![1, 2, 0, 3, 4, 5]);
    }

    #[test]
    fn rotate_wraps_and_validates() {
        let mut p = proj(2);
        p.rotate(&[0], 90).unwrap();
        p.rotate(&[0], -180).unwrap();
        assert_eq!(p.pages[0].rotate, 270);
        assert_eq!(p.rotate(&[0], 45), Err(EditError::InvalidRotation(45)));
        assert_eq!(p.rotate(&[9], 90), Err(EditError::IndexOutOfRange(9)));
        assert_eq!(p.rotate(&[], 90), Err(EditError::EmptySelection));
    }

    #[test]
    fn delete_duplicate_and_blank() {
        let mut p = proj(4);
        p.duplicate(&[1, 2]).unwrap();
        assert_eq!(order(&p), vec![0, 1, 2, 1, 2, 3]);
        p.delete(&[0, 3]).unwrap();
        assert_eq!(order(&p), vec![1, 2, 2, 3]);
        p.insert_blank(0, BlankSize::A4).unwrap();
        p.insert_blank(5, BlankSize::Letter).unwrap();
        assert_eq!(p.pages.len(), 6);
        assert!(matches!(p.pages[0].kind, PageKind::Blank { .. }));
        assert_eq!(p.insert_blank(9, BlankSize::A4), Err(EditError::IndexOutOfRange(9)));
    }

    #[test]
    fn undo_redo_restore_exact_state_and_redo_clears_on_new_edit() {
        let mut p = proj(3);
        let start = p.pages.clone();
        p.delete(&[0]).unwrap();
        p.rotate(&[0], 90).unwrap();
        assert!(p.undo() && p.undo());
        assert_eq!(p.pages, start);
        assert!(!p.undo());
        assert!(p.redo());
        assert_eq!(p.pages.len(), 2);
        p.rotate(&[1], 90).unwrap(); // new edit
        assert!(!p.can_redo());
    }

    #[test]
    fn failed_edit_does_not_touch_undo_stack() {
        let mut p = proj(2);
        let _ = p.delete(&[7]);
        assert!(!p.can_undo());
    }

    #[test]
    fn range_parsing() {
        assert_eq!(parse_ranges("1-3,5", 6).unwrap(), vec![0, 1, 2, 4]);
        assert_eq!(parse_ranges("4-", 6).unwrap(), vec![3, 4, 5]);
        assert_eq!(parse_ranges("-2", 6).unwrap(), vec![0, 1]);
        assert_eq!(parse_ranges("3-1", 6).unwrap(), vec![2, 1, 0]);
        assert!(parse_ranges("0", 6).is_err());
        assert!(parse_ranges("7", 6).is_err());
        assert!(parse_ranges("a", 6).is_err());
        assert!(parse_ranges("", 6).is_err());
    }
}
