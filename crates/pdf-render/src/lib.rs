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
