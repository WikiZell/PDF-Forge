//! PDFium wrapper: page rendering and page-size queries.
//!
//! No UI or OS-specific dependencies. The PDFium shared library is located and
//! loaded at runtime; a missing library yields [`RenderError::LibraryMissing`]
//! with the path that was tried so the UI can show a clear message.
//!
//! JavaScript (V8) and XFA are not compiled in (the `pdfium_enable_v8` /
//! `pdfium_enable_xfa` crate features are off), so documents cannot execute scripts.

use std::fmt;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use image::ImageFormat;
use pdfium_render::prelude::*;

/// Hard cap on rendered bitmap dimensions, to bound memory for hostile page sizes.
pub const MAX_RENDER_DIMENSION: u32 = 16_384;

#[derive(Debug)]
pub enum RenderError {
    /// The PDFium shared library could not be loaded.
    LibraryMissing { tried: PathBuf, detail: String },
    PasswordRequired,
    WrongPassword,
    Open(String),
    PageOutOfRange { page: usize, page_count: usize },
    Render(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LibraryMissing { tried, detail } => write!(
                f,
                "The PDF rendering library (PDFium) could not be loaded from \"{}\": {}",
                tried.display(),
                detail
            ),
            Self::PasswordRequired => write!(f, "This document is password protected."),
            Self::WrongPassword => write!(f, "The password is incorrect."),
            Self::Open(d) => write!(f, "The document could not be opened: {d}"),
            Self::PageOutOfRange { page, page_count } => {
                write!(f, "Page {} is out of range (document has {page_count} pages).", page + 1)
            }
            Self::Render(d) => write!(f, "Rendering failed: {d}"),
        }
    }
}

impl std::error::Error for RenderError {}

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

/// Load the PDFium library from `dir` (the file name is platform specific).
/// Safe to call more than once; the first successful load wins.
pub fn init(dir: &Path) -> Result<(), RenderError> {
    if PDFIUM.get().is_some() {
        return Ok(());
    }
    let lib = Pdfium::pdfium_platform_library_name_at_path(dir);
    let bindings = Pdfium::bind_to_library(&lib).map_err(|e| RenderError::LibraryMissing {
        tried: lib.clone(),
        detail: e.to_string(),
    })?;
    let _ = PDFIUM.set(Pdfium::new(bindings));
    Ok(())
}

fn pdfium() -> Result<&'static Pdfium, RenderError> {
    PDFIUM.get().ok_or_else(|| RenderError::LibraryMissing {
        tried: PathBuf::new(),
        detail: "init() was not called".into(),
    })
}

/// Page size in PDF points (1/72 inch), after applying the page rotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

/// Axis-aligned rectangle in view space: points (1/72 in), origin top-left of the
/// displayed (rotated) page, y growing downwards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// One run of text on a page with its bounding box in view space.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutlineItem {
    pub title: String,
    /// 0-based target page, if the bookmark resolves to one.
    pub page: Option<usize>,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub page: usize,
    pub rects: Vec<Rect>,
}

/// Upper bound on search results returned, to bound memory on hostile input.
pub const MAX_SEARCH_HITS: usize = 5_000;

/// A PNG-encoded rendered page.
pub struct RenderedPage {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// An open document. The password (if any) is used only during `open` and is not retained.
pub struct Document {
    inner: Mutex<PdfDocument<'static>>,
    page_count: usize,
}

impl Document {
    pub fn open(path: &Path, password: Option<&str>) -> Result<Self, RenderError> {
        let doc = pdfium()?
            .load_pdf_from_file(path, password)
            .map_err(|e| map_open_error(e, password.is_some()))?;
        let page_count = doc.pages().len() as usize;
        Ok(Self { inner: Mutex::new(doc), page_count })
    }

    /// Open a document held in memory (used to preview unsaved results).
    pub fn from_bytes(bytes: Vec<u8>, password: Option<&str>) -> Result<Self, RenderError> {
        let doc = pdfium()?
            .load_pdf_from_byte_vec(bytes, password)
            .map_err(|e| map_open_error(e, password.is_some()))?;
        let page_count = doc.pages().len() as usize;
        Ok(Self { inner: Mutex::new(doc), page_count })
    }

    pub fn page_count(&self) -> usize {
        self.page_count
    }

    pub fn page_size(&self, page: usize) -> Result<PageSize, RenderError> {
        let doc = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let p = self.page(&doc, page)?;
        Ok(PageSize { width: p.width().value, height: p.height().value })
    }

    /// Render `page` (0-based) at `scale` (1.0 = 72 dpi, i.e. one pixel per point).
    pub fn render_page(&self, page: usize, scale: f32) -> Result<RenderedPage, RenderError> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(RenderError::Render("invalid scale".into()));
        }
        let doc = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let p = self.page(&doc, page)?;
        let w = (p.width().value * scale).round().max(1.0);
        let h = (p.height().value * scale).round().max(1.0);
        if w > MAX_RENDER_DIMENSION as f32 || h > MAX_RENDER_DIMENSION as f32 {
            return Err(RenderError::Render(format!(
                "requested size {w}x{h} exceeds the {MAX_RENDER_DIMENSION}px limit"
            )));
        }
        let cfg = PdfRenderConfig::new().set_target_size(w as Pixels, h as Pixels);
        let bitmap = p.render_with_config(&cfg).map_err(|e| RenderError::Render(e.to_string()))?;
        let img = bitmap.as_image().map_err(|e| RenderError::Render(e.to_string()))?;
        let (width, height) = (img.width(), img.height());
        let mut png = Vec::new();
        img.to_rgba8()
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .map_err(|e| RenderError::Render(e.to_string()))?;
        Ok(RenderedPage { png, width, height })
    }

    /// Text runs of a page with positions, for the selectable text layer.
    pub fn page_text(&self, page: usize) -> Result<Vec<TextRun>, RenderError> {
        let doc = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let p = self.page(&doc, page)?;
        let map = ViewMap::new(&p);
        let text = p.text().map_err(|e| RenderError::Render(e.to_string()))?;
        let mut out = Vec::new();
        for seg in text.segments().iter() {
            let t = seg.text();
            if t.trim().is_empty() {
                continue;
            }
            out.push(TextRun { text: t, rect: map.rect(seg.bounds()) });
        }
        Ok(out)
    }

    /// Flattened bookmark tree (document order, with depth).
    pub fn outline(&self) -> Vec<OutlineItem> {
        let doc = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let mut items = Vec::new();
        if let Some(root) = doc.bookmarks().root() {
            walk_outline(&root, 0, &mut items);
            // The root's siblings are top-level entries too.
            let mut sib = root.next_sibling();
            while let Some(b) = sib {
                walk_outline(&b, 0, &mut items);
                sib = b.next_sibling();
            }
        }
        items
    }

    /// Search every page. Results are ordered by page then position.
    pub fn search(&self, query: &str, match_case: bool) -> Result<Vec<SearchHit>, RenderError> {
        if query.is_empty() {
            return Ok(vec![]);
        }
        let mut hits = Vec::new();
        for i in 0..self.page_count {
            hits.extend(self.search_page(i, query, match_case)?);
            if hits.len() >= MAX_SEARCH_HITS {
                hits.truncate(MAX_SEARCH_HITS);
                break;
            }
        }
        Ok(hits)
    }

    /// Search a single page (0-based). Hits are in reading order as PDFium reports them.
    pub fn search_page(&self, page: usize, query: &str, match_case: bool) -> Result<Vec<SearchHit>, RenderError> {
        if query.is_empty() {
            return Ok(vec![]);
        }
        let doc = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let opts = PdfSearchOptions::new().match_case(match_case);
        let p = self.page(&doc, page)?;
        let map = ViewMap::new(&p);
        let mut hits = Vec::new();
        let Ok(text) = p.text() else { return Ok(hits) };
        let Ok(search) = text.search(query, &opts) else { return Ok(hits) };
        while let Some(segs) = search.find_next() {
            let rects: Vec<Rect> = segs.iter().map(|s| map.rect(s.bounds())).collect();
            if !rects.is_empty() {
                hits.push(SearchHit { page, rects });
            }
            if hits.len() >= MAX_SEARCH_HITS {
                break;
            }
        }
        Ok(hits)
    }

    fn page<'d>(
        &self,
        doc: &'d PdfDocument<'static>,
        page: usize,
    ) -> Result<PdfPage<'d>, RenderError> {
        if page >= self.page_count {
            return Err(RenderError::PageOutOfRange { page, page_count: self.page_count });
        }
        doc.pages()
            .get(page as PdfPageIndex)
            .map_err(|e| RenderError::Render(e.to_string()))
    }
}

fn map_open_error(e: PdfiumError, had_password: bool) -> RenderError {
    match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            if had_password {
                RenderError::WrongPassword
            } else {
                RenderError::PasswordRequired
            }
        }
        other => RenderError::Open(other.to_string()),
    }
}

fn walk_outline(b: &PdfBookmark<'_>, depth: usize, out: &mut Vec<OutlineItem>) {
    let mut page = b.destination().and_then(|d| d.page_index().ok());
    if page.is_none() {
        if let Some(a) = b.action() {
            if let Some(l) = a.as_local_destination_action() {
                page = l.destination().ok().and_then(|d| d.page_index().ok());
            }
        }
    }
    let page = page.map(|i| i as usize);
    out.push(OutlineItem { title: b.title().unwrap_or_default(), page, depth });
    if depth < 32 {
        for c in b.iter_direct_children() {
            walk_outline(&c, depth + 1, out);
        }
    }
}

/// Maps PDF user space (origin bottom-left, unrotated) to view space (top-left, rotated).
struct ViewMap {
    rotation: i32,
    w0: f32,
    h0: f32,
    ox: f32,
    oy: f32,
}

impl ViewMap {
    fn new(p: &PdfPage<'_>) -> Self {
        let rotation = match p.rotation() {
            Ok(PdfPageRenderRotation::Degrees90) => 90,
            Ok(PdfPageRenderRotation::Degrees180) => 180,
            Ok(PdfPageRenderRotation::Degrees270) => 270,
            _ => 0,
        };
        let (vw, vh) = (p.width().value, p.height().value);
        let (w0, h0) = if rotation == 90 || rotation == 270 { (vh, vw) } else { (vw, vh) };
        let (ox, oy) = p
            .boundaries()
            .bounding()
            .map(|b| (b.bounds.left().value, b.bounds.bottom().value))
            .unwrap_or((0.0, 0.0));
        Self { rotation, w0, h0, ox, oy }
    }

    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        let (x, y) = (x - self.ox, y - self.oy);
        match self.rotation {
            90 => (y, x),
            180 => (self.w0 - x, y),
            270 => (self.h0 - y, self.w0 - x),
            _ => (x, self.h0 - y),
        }
    }

    fn rect(&self, r: PdfRect) -> Rect {
        let (ax, ay) = self.point(r.left().value, r.bottom().value);
        let (bx, by) = self.point(r.right().value, r.top().value);
        Rect { x: ax.min(bx), y: ay.min(by), w: (ax - bx).abs(), h: (ay - by).abs() }
    }
}
