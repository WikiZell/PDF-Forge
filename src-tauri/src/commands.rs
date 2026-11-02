//! Thin Tauri commands. Business logic lives in `pdf-core` / `pdf-render`.

use std::path::{Path, PathBuf};

use pdf_render::Document;
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::AppError;
use crate::state::AppState;
use crate::storage::{RecentFile, ViewState};

/// Per-call budget for PDFium work on untrusted input. PDFium cannot be interrupted, so on
/// timeout the worker is abandoned and the caller gets an error instead of a frozen UI.
const WORK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

async fn run_timed<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(WORK_TIMEOUT))
        .await
        .map_err(|e| AppError::new("internal", e.to_string()))?
        .map_err(|_| AppError::new("timeout", "The operation took too long and was abandoned."))?
}

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
    path: String,
    file_name: String,
    page_count: usize,
    view_state: Option<ViewState>,
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

    let doc = run_timed(move || -> Result<Document> {
        // Cheap structural pre-check. Encrypted or repairable files can fail here yet still
        // open in PDFium, so only the page-limit violation is fatal.
        if let Err(e @ pdf_core::InspectError::TooManyPages { .. }) = pdf_core::inspect(&p, max_pages) {
            return Err(e.into());
        }
        Ok(Document::open(&p, password.as_deref())?)
    })
    .await?;

    if doc.page_count() > max_pages {
        return Err(AppError::new(
            "too_many_pages",
            format!("Document has {} pages; the configured maximum is {max_pages}.", doc.page_count()),
        ));
    }
    let page_count = doc.page_count();
    let id = state.insert(doc);
    let file_name = canon.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let path = canon.to_string_lossy().into_owned();
    let _ = state.storage.add_recent(&path, &file_name);
    let view_state = state.storage.view_state(&path);
    Ok(DocSummary { id, path, file_name, page_count, view_state })
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
    let r = run_timed(move || Ok(d.render_page(page, scale)?)).await?;
    Ok(Response::new(r.png))
}

#[derive(Serialize)]
pub struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl From<pdf_render::Rect> for Rect {
    fn from(r: pdf_render::Rect) -> Self {
        Self { x: r.x, y: r.y, w: r.w, h: r.h }
    }
}

#[derive(Serialize)]
pub struct TextRun {
    text: String,
    rect: Rect,
}

#[derive(Serialize)]
pub struct OutlineItem {
    title: String,
    page: Option<usize>,
    depth: usize,
}

#[derive(Serialize)]
pub struct SearchHit {
    page: usize,
    rects: Vec<Rect>,
}

#[tauri::command]
pub fn page_sizes(id: u64, state: State<'_, AppState>) -> Result<Vec<PageSizePt>> {
    let d = doc(&state, id)?;
    (0..d.page_count())
        .map(|i| d.page_size(i).map(|s| PageSizePt { width: s.width, height: s.height }).map_err(Into::into))
        .collect()
}

#[tauri::command]
pub async fn page_text(id: u64, page: usize, state: State<'_, AppState>) -> Result<Vec<TextRun>> {
    let d = doc(&state, id)?;
    run_timed(move || {
        Ok(d.page_text(page)?
            .into_iter()
            .map(|r| TextRun { text: r.text, rect: r.rect.into() })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn outline(id: u64, state: State<'_, AppState>) -> Result<Vec<OutlineItem>> {
    let d = doc(&state, id)?;
    run_timed(move || {
        Ok(d.outline().into_iter().map(|o| OutlineItem { title: o.title, page: o.page, depth: o.depth }).collect())
    })
    .await
}

#[tauri::command]
pub async fn search(id: u64, query: String, match_case: bool, state: State<'_, AppState>) -> Result<Vec<SearchHit>> {
    let d = doc(&state, id)?;
    run_timed(move || {
        Ok(d.search(&query, match_case)?
            .into_iter()
            .map(|h| SearchHit { page: h.page, rects: h.rects.into_iter().map(Into::into).collect() })
            .collect())
    })
    .await
}

#[tauri::command]
pub fn recent_files(state: State<'_, AppState>) -> Vec<RecentFile> {
    state.storage.recent()
}

/// Re-grant a path from the recent list (the user selected it in an earlier session).
#[tauri::command]
pub fn open_recent(path: String, state: State<'_, AppState>) -> Result<String> {
    if !state.storage.is_recent(&path) {
        return Err(AppError::new("not_allowed", "That file is not in the recent files list."));
    }
    match state.allowed.grant(Path::new(&path)) {
        Ok(p) => Ok(p.to_string_lossy().into_owned()),
        Err(e) => {
            let _ = state.storage.remove_recent(&path);
            Err(AppError::new("open_failed", format!("The file could not be found: {e}")))
        }
    }
}

#[tauri::command]
pub fn forget_recent(path: String, state: State<'_, AppState>) {
    let _ = state.storage.remove_recent(&path);
}

#[tauri::command]
pub fn save_view_state(path: String, view: ViewState, state: State<'_, AppState>) {
    if state.allowed.check_readable(Path::new(&path), &state.limits).is_ok() {
        let _ = state.storage.save_view_state(&path, view);
    }
}

#[tauri::command]
pub fn clear_app_data(state: State<'_, AppState>) -> Result<()> {
    state.storage.clear_all().map_err(|e| AppError::new("internal", e.to_string()))
}
