//! Page-organisation sessions: a `pdf_core::Project` plus PDFium handles for thumbnails.
//! Commands are thin; all structural work happens in `pdf-core`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use pdf_core::{BlankSize, PageKind, Project};
use pdf_render::Document as RenderDoc;
use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::AppError;
use crate::state::AppState;

type Result<T> = std::result::Result<T, AppError>;

pub struct Session {
    project: Project,
    renders: Vec<Arc<RenderDoc>>,
    /// Page sizes (points, rotation applied) per source.
    sizes: Vec<Vec<(f32, f32)>>,
    names: Vec<String>,
    /// File that "Save" overwrites (with backup); only set when the session started from one file.
    primary: Option<PathBuf>,
}

#[derive(Default)]
pub struct Sessions {
    map: Mutex<HashMap<u64, Arc<Mutex<Session>>>>,
    next: AtomicU64,
}

impl Sessions {
    fn insert(&self, s: Session) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.map.lock().unwrap_or_else(|p| p.into_inner()).insert(id, Arc::new(Mutex::new(s)));
        id
    }
    fn get(&self, id: u64) -> Result<Arc<Mutex<Session>>> {
        self.map
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::new("no_such_document", "That editing session is no longer open."))
    }
    fn remove(&self, id: u64) {
        self.map.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageDto {
    /// "source" or "blank"
    kind: &'static str,
    src: usize,
    page: usize,
    rotate: i32,
    width: f32,
    height: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDto {
    name: String,
    page_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgState {
    session: u64,
    pages: Vec<PageDto>,
    sources: Vec<SourceDto>,
    can_undo: bool,
    can_redo: bool,
    can_save_over: bool,
}

fn snapshot(id: u64, s: &Session) -> OrgState {
    let pages = s
        .project
        .pages
        .iter()
        .map(|e| match &e.kind {
            PageKind::Source { src, page } => {
                let (w, h) = s.sizes[*src][*page];
                PageDto { kind: "source", src: *src, page: *page, rotate: e.rotate, width: w, height: h }
            }
            PageKind::Blank { width, height } => PageDto {
                kind: "blank",
                src: 0,
                page: 0,
                rotate: e.rotate,
                width: *width as f32,
                height: *height as f32,
            },
        })
        .collect();
    OrgState {
        session: id,
        pages,
        sources: s
            .names
            .iter()
            .zip(&s.renders)
            .map(|(n, r)| SourceDto { name: n.clone(), page_count: r.page_count() })
            .collect(),
        can_undo: s.project.can_undo(),
        can_redo: s.project.can_redo(),
        can_save_over: s.primary.is_some(),
    }
}

fn edit_err(e: pdf_core::EditError) -> AppError {
    AppError::new("invalid_edit", e.to_string())
}

fn load_err(e: pdf_core::LoadError) -> AppError {
    use pdf_core::LoadError::*;
    let code = match &e {
        NeedsPassword => "password_required",
        WrongPassword => "wrong_password",
        TooLarge { .. } => "too_large",
        TooManyPages { .. } => "too_many_pages",
        Corrupt(_) | Io(_) => "open_failed",
    };
    AppError::new(code, e.to_string())
}

/// Parse + open a source (structure via lopdf, pixels via PDFium). Runs on a worker thread.
type LoadedSource = (pdf_core::lopdf::Document, RenderDoc, Vec<(f32, f32)>);

fn load_source(path: &Path, password: Option<&str>, limits: &pdf_core::Limits) -> Result<LoadedSource> {
    let doc = pdf_core::load_file(path, password, limits).map_err(load_err)?;
    let render = RenderDoc::open(path, password)?;
    let sizes = (0..render.page_count())
        .map(|i| render.page_size(i).map(|s| (s.width, s.height)))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if sizes.len() != doc.get_pages().len() {
        return Err(AppError::new("open_failed", "The page structure of this file is inconsistent."));
    }
    Ok((doc, render, sizes))
}

fn file_label(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "document.pdf".into())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| AppError::new("internal", e.to_string()))?
}

#[tauri::command]
pub async fn org_new(state: State<'_, AppState>) -> Result<OrgState> {
    let id = state.sessions.insert(Session {
        project: Project::new(),
        renders: vec![],
        sizes: vec![],
        names: vec![],
        primary: None,
    });
    let s = state.sessions.get(id)?;
    let g = s.lock().unwrap_or_else(|p| p.into_inner());
    Ok(snapshot(id, &g))
}

/// Add a file to a session. `append` adds all its pages at the end (merge).
/// With `insert_at`, its pages are inserted at that position instead.
#[tauri::command]
pub async fn org_add_file(
    session: u64,
    path: String,
    password: Option<String>,
    insert_at: Option<usize>,
    state: State<'_, AppState>,
) -> Result<OrgState> {
    let canon = state.allowed.check_readable(Path::new(&path), &state.limits())?;
    let limits = state.limits();
    let sess = state.sessions.get(session)?;
    let (doc, render, sizes) = {
        let c = canon.clone();
        blocking(move || load_source(&c, password.as_deref(), &limits)).await?
    };
    let mut g = sess.lock().unwrap_or_else(|p| p.into_inner());
    let was_empty = g.project.sources.is_empty();
    let name = file_label(&canon);
    let idx = g.project.add_source(doc, name.clone());
    g.renders.push(Arc::new(render));
    g.sizes.push(sizes);
    g.names.push(name);
    match insert_at {
        Some(at) => g.project.insert_from(idx, None, at).map_err(edit_err)?,
        None => g.project.append_source(idx).map_err(edit_err)?,
    }
    if was_empty {
        g.primary = Some(canon);
    } else {
        g.primary = None; // merged: "Save" over the first file would be surprising
    }
    Ok(snapshot(session, &g))
}

fn with_session<T>(state: &AppState, id: u64, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
    let s = state.sessions.get(id)?;
    let mut g = s.lock().unwrap_or_else(|p| p.into_inner());
    f(&mut g)
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Edit {
    Move {
        indices: Vec<usize>,
        to: usize,
    },
    Rotate {
        indices: Vec<usize>,
        degrees: i32,
    },
    Delete {
        indices: Vec<usize>,
    },
    Duplicate {
        indices: Vec<usize>,
    },
    /// size: "a4" | "letter" | "same" (same as page `like`)
    InsertBlank {
        at: usize,
        size: String,
        like: Option<usize>,
    },
    InsertFrom {
        src: usize,
        pages: Option<Vec<usize>>,
        at: usize,
    },
    Undo,
    Redo,
}

#[tauri::command]
pub fn org_edit(session: u64, edit: Edit, state: State<'_, AppState>) -> Result<OrgState> {
    with_session(&state, session, |s| {
        match edit {
            Edit::Move { indices, to } => s.project.move_pages(&indices, to),
            Edit::Rotate { indices, degrees } => s.project.rotate(&indices, degrees),
            Edit::Delete { indices } => {
                if indices.len() >= s.project.pages.len() {
                    return Err(AppError::new("invalid_edit", "A document must keep at least one page."));
                }
                s.project.delete(&indices)
            }
            Edit::Duplicate { indices } => s.project.duplicate(&indices),
            Edit::InsertBlank { at, size, like } => match size.as_str() {
                "a4" => s.project.insert_blank(at, BlankSize::A4),
                "letter" => s.project.insert_blank(at, BlankSize::Letter),
                _ => {
                    let like = like.unwrap_or(at.saturating_sub(1));
                    let sizes: Vec<(f64, f64)> = s
                        .project
                        .pages
                        .iter()
                        .map(|e| match &e.kind {
                            PageKind::Source { src, page } => {
                                let (w, h) = s.sizes[*src][*page];
                                (w as f64, h as f64)
                            }
                            PageKind::Blank { width, height } => (*width, *height),
                        })
                        .collect();
                    // rotated neighbours are measured as displayed
                    let sizes: Vec<(f64, f64)> = sizes
                        .into_iter()
                        .zip(&s.project.pages)
                        .map(|((w, h), e)| if e.rotate % 180 != 0 { (h, w) } else { (w, h) })
                        .collect();
                    s.project.insert_blank_like(at, like, &sizes)
                }
            },
            Edit::InsertFrom { src, pages, at } => s.project.insert_from(src, pages.as_deref(), at),
            Edit::Undo => {
                s.project.undo();
                Ok(())
            }
            Edit::Redo => {
                s.project.redo();
                Ok(())
            }
        }
        .map_err(edit_err)?;
        Ok(snapshot(session, s))
    })
}

/// Thumbnail for a source page (rotation is applied by the UI).
#[tauri::command]
pub async fn org_thumb(
    session: u64,
    src: usize,
    page: usize,
    scale: f32,
    state: State<'_, AppState>,
) -> Result<Response> {
    let doc = with_session(&state, session, |s| {
        s.renders.get(src).cloned().ok_or_else(|| AppError::new("no_such_document", "Unknown source."))
    })?;
    let r = blocking(move || Ok(doc.render_page(page, scale)?)).await?;
    Ok(Response::new(r.png))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    page_count: usize,
    size_bytes: usize,
    renamed_fields: Vec<Rename>,
    warnings: Vec<String>,
    dropped_links: usize,
    dropped_bookmarks: usize,
}

#[derive(Serialize)]
pub struct Rename {
    source: String,
    from: String,
    to: String,
}

fn build_bytes(s: &Session, subset: Option<&[usize]>) -> Result<(Vec<u8>, pdf_core::BuildReport)> {
    pdf_core::build(&s.project, subset).map_err(|e| AppError::new("save_failed", e.to_string()))
}

fn to_preview(count: usize, bytes: usize, r: pdf_core::BuildReport) -> Preview {
    Preview {
        page_count: count,
        size_bytes: bytes,
        renamed_fields: r
            .renamed_fields
            .into_iter()
            .map(|f| Rename { source: f.source, from: f.from, to: f.to })
            .collect(),
        warnings: r.warnings,
        dropped_links: r.dropped_links,
        dropped_bookmarks: r.dropped_bookmarks,
    }
}

/// Page count, output size and any form-field renames, without writing anything.
#[tauri::command]
pub async fn org_preview(session: u64, subset: Option<Vec<usize>>, state: State<'_, AppState>) -> Result<Preview> {
    let sess = state.sessions.get(session)?;
    blocking(move || {
        let g = sess.lock().unwrap_or_else(|p| p.into_inner());
        let n = subset.as_ref().map_or(g.project.pages.len(), Vec::len);
        let (bytes, rep) = build_bytes(&g, subset.as_deref())?;
        Ok(to_preview(n, bytes.len(), rep))
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    path: String,
    preview: Preview,
}

fn pick_save_path(app: &AppHandle, default_name: &str) -> Option<PathBuf> {
    app.dialog()
        .file()
        .add_filter("PDF documents", &["pdf"])
        .set_file_name(default_name)
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
}

fn ensure_pdf_ext(mut p: PathBuf) -> PathBuf {
    if p.extension().is_none_or(|e| !e.eq_ignore_ascii_case("pdf")) {
        p.set_extension("pdf");
    }
    p
}

/// Save a copy (whole document or `subset` = extract) to a user-chosen new file. Returns None if cancelled.
#[tauri::command]
pub async fn org_save_as(
    app: AppHandle,
    session: u64,
    subset: Option<Vec<usize>>,
    default_name: String,
    state: State<'_, AppState>,
) -> Result<Option<Saved>> {
    let sess = state.sessions.get(session)?;
    let name = pdf_core::sanitize_file_name(&default_name);
    let picked = tauri::async_runtime::spawn_blocking(move || pick_save_path(&app, &name))
        .await
        .map_err(|e| AppError::new("internal", e.to_string()))?;
    let Some(path) = picked else { return Ok(None) };
    let path = ensure_pdf_ext(path);
    let saved = blocking(move || {
        let g = sess.lock().unwrap_or_else(|p| p.into_inner());
        let n = subset.as_ref().map_or(g.project.pages.len(), Vec::len);
        let (bytes, rep) = build_bytes(&g, subset.as_deref())?;
        pdf_core::atomic_write(&path, &bytes, true).map_err(|e| AppError::new("save_failed", e.to_string()))?;
        Ok(Saved { path: path.to_string_lossy().into_owned(), preview: to_preview(n, bytes.len(), rep) })
    })
    .await?;
    // The user chose this path, so it may be opened later.
    let _ = state.allowed.grant(Path::new(&saved.path));
    Ok(Some(saved))
}

/// Explicit "Save": overwrite the original file atomically, keeping `<name>.pdf.bak`.
#[tauri::command]
pub async fn org_save_over(session: u64, state: State<'_, AppState>) -> Result<Saved> {
    let sess = state.sessions.get(session)?;
    let primary = {
        let g = sess.lock().unwrap_or_else(|p| p.into_inner());
        g.primary
            .clone()
            .ok_or_else(|| AppError::new("save_failed", "There is no original file to save over. Use Save a copy."))?
    };
    state.allowed.check_readable(&primary, &state.limits())?;
    blocking(move || {
        let g = sess.lock().unwrap_or_else(|p| p.into_inner());
        let n = g.project.pages.len();
        let (bytes, rep) = build_bytes(&g, None)?;
        pdf_core::atomic_write(&primary, &bytes, true).map_err(|e| AppError::new("save_failed", e.to_string()))?;
        Ok(Saved { path: primary.to_string_lossy().into_owned(), preview: to_preview(n, bytes.len(), rep) })
    })
    .await
}

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum SplitMode {
    EveryN { n: usize },
    Ranges { spec: String },
    Bookmarks,
}

#[derive(Serialize)]
pub struct SplitResult {
    folder: String,
    files: Vec<String>,
}

/// Split into several files in a folder the user picks. Never overwrites existing files.
#[tauri::command]
pub async fn org_split(
    app: AppHandle,
    session: u64,
    mode: SplitMode,
    state: State<'_, AppState>,
) -> Result<Option<SplitResult>> {
    let sess = state.sessions.get(session)?;
    // Plan first so errors (bad ranges) show before any dialog.
    let (parts, stem) = {
        let g = sess.lock().unwrap_or_else(|p| p.into_inner());
        let count = g.project.pages.len();
        let stem = g.names.first().map(|n| n.trim_end_matches(".pdf").to_string()).unwrap_or_else(|| "document".into());
        let parts = match mode {
            SplitMode::EveryN { n } => pdf_core::split_every_n(count, n),
            SplitMode::Ranges { spec } => {
                pdf_core::split_ranges(&spec, count).map_err(|m| AppError::new("invalid_edit", m))?
            }
            SplitMode::Bookmarks => {
                if g.project.sources.len() != 1
                    || g.project.pages.iter().enumerate().any(|(i, e)| e.kind != PageKind::Source { src: 0, page: i })
                {
                    return Err(AppError::new(
                        "invalid_edit",
                        "Splitting by bookmarks needs an unmodified single document. Save your edits first.",
                    ));
                }
                let p = pdf_core::split_by_bookmarks(&g.project.sources[0].doc);
                if p.is_empty() {
                    return Err(AppError::new("invalid_edit", "This document has no bookmarks that point to pages."));
                }
                p
            }
        };
        (parts, stem)
    };
    let dir = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().blocking_pick_folder().and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))?;
    let Some(dir) = dir else { return Ok(None) };
    blocking(move || {
        let g = sess.lock().unwrap_or_else(|p| p.into_inner());
        let mut files = Vec::new();
        for (i, part) in parts.iter().enumerate() {
            let (bytes, _) = build_bytes(&g, Some(&part.pages))?;
            let label = format!("_{:03}_{}", i + 1, pdf_core::sanitize_file_name(&part.label));
            let out = pdf_core::unique_output_path(&dir, &stem, &label, "pdf");
            pdf_core::atomic_write(&out, &bytes, false).map_err(|e| AppError::new("save_failed", e.to_string()))?;
            files.push(out.to_string_lossy().into_owned());
        }
        Ok(SplitResult { folder: dir.to_string_lossy().into_owned(), files })
    })
    .await
    .map(Some)
}

/// Native multi-file picker for merging. Grants every chosen path.
#[tauri::command]
pub async fn pick_pdfs(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<String>> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().add_filter("PDF documents", &["pdf"]).blocking_pick_files()
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))?
    .unwrap_or_default();
    let mut out = Vec::new();
    for fp in picked {
        if let Ok(p) = fp.into_path() {
            out.push(state.allowed.grant(&p)?.to_string_lossy().into_owned());
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn org_close(session: u64, state: State<'_, AppState>) {
    state.sessions.remove(session);
}
