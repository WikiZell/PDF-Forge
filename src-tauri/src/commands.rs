//! Thin Tauri commands. Business logic lives in `pdf-core` / `pdf-render`.

use std::path::{Path, PathBuf};

use pdf_render::Document;
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::AppError;
use crate::state::AppState;

type Result<T> = std::result::Result<T, AppError>;

#[derive(Serialize)]
pub struct PdfiumStatus {
    ok: bool,
    message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocSummary {
    id: u64,
    file_name: String,
    page_count: usize,
}

#[derive(Serialize)]
pub struct PageSizePt {
    width: f32,
    height: f32,
}

#[tauri::command]
pub fn pdfium_status(state: State<'_, AppState>) -> PdfiumStatus {
    let err = state.pdfium_error.lock().unwrap_or_else(|p| p.into_inner()).clone();
    PdfiumStatus { ok: err.is_none(), message: err }
}

#[tauri::command]
pub fn take_launch_path(state: State<'_, AppState>) -> Option<String> {
    state.launch_path.lock().unwrap_or_else(|p| p.into_inner()).take()
}

/// Native open dialog. The chosen path is granted here, in Rust, never from frontend input.
#[tauri::command]
pub async fn pick_pdf(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().add_filter("PDF documents", &["pdf"]).blocking_pick_file()
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))?;

    let Some(fp) = picked else { return Ok(None) };
    let path = fp.into_path().map_err(|e| AppError::new("open_failed", e.to_string()))?;
    let canon = state.allowed.grant(&path)?;
    Ok(Some(canon.to_string_lossy().into_owned()))
}

/// Open a PDF the user previously selected. `password` is used for this call only.
#[tauri::command]
pub async fn open_pdf(
    path: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<DocSummary> {
    let canon: PathBuf = state.allowed.check_readable(Path::new(&path), &state.limits)?;
    let max_pages = state.limits.max_pages;
    let p = canon.clone();

    let doc = tauri::async_runtime::spawn_blocking(move || -> Result<Document> {
        // Cheap structural pre-check. Encrypted or repairable files can fail here yet still
        // open in PDFium, so only the page-limit violation is fatal.
        if let Err(e @ pdf_core::InspectError::TooManyPages { .. }) = pdf_core::inspect(&p, max_pages) {
            return Err(e.into());
        }
        Ok(Document::open(&p, password.as_deref())?)
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))??;

    if doc.page_count() > max_pages {
        return Err(AppError::new(
            "too_many_pages",
            format!("Document has {} pages; the configured maximum is {max_pages}.", doc.page_count()),
        ));
    }
    let page_count = doc.page_count();
    let id = state.insert(doc);
    let file_name = canon.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(DocSummary { id, file_name, page_count })
}

#[tauri::command]
pub fn close_pdf(id: u64, state: State<'_, AppState>) {
    state.remove(id);
}

fn doc(state: &AppState, id: u64) -> Result<std::sync::Arc<Document>> {
    state.get(id).ok_or_else(|| AppError::new("no_such_document", "That document is no longer open."))
}

#[tauri::command]
pub fn page_size(id: u64, page: usize, state: State<'_, AppState>) -> Result<PageSizePt> {
    let s = doc(&state, id)?.page_size(page)?;
    Ok(PageSizePt { width: s.width, height: s.height })
}

/// Renders one page to PNG and returns the raw bytes (no base64 / JSON overhead).
#[tauri::command]
pub async fn render_page(id: u64, page: usize, scale: f32, state: State<'_, AppState>) -> Result<Response> {
    let d = doc(&state, id)?;
    let r = tauri::async_runtime::spawn_blocking(move || d.render_page(page, scale))
        .await
        .map_err(|e| AppError::new("internal", e.to_string()))??;
    Ok(Response::new(r.png))
}
