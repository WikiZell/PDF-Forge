//! Saved-signature library (JSON index + PNG assets in the app data dir) and the
//! "sign this document" command.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pdf_core::sign::{Content, Mode, Placement, Stroke};
use serde::{Deserialize, Serialize};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::AppError;
use crate::state::AppState;

type Result<T> = std::result::Result<T, AppError>;

const MAX_UPLOAD: usize = 20 * 1024 * 1024;
const MAX_SIGNATURES: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Drawn {
    /// strokes of [x, y, pressure]
    pub strokes: Vec<Vec<[f32; 3]>>,
    pub view: [f32; 2],
    pub width: f32,
    pub color: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SigMeta {
    pub id: String,
    pub name: String,
    /// "drawn" | "typed" | "imported"
    pub kind: String,
    pub created: u64,
    /// width / height
    pub aspect: f32,
    #[serde(default)]
    pub drawn: Option<Drawn>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub font: Option<String>,
}

#[derive(Default)]
pub struct SigLock(Mutex<()>);

fn lib_dir(state: &AppState) -> PathBuf {
    state.storage.dir().join("signatures")
}

fn read_index(dir: &Path) -> Vec<SigMeta> {
    std::fs::read(dir.join("index.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write_index(dir: &Path, list: &[SigMeta]) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(io)?;
    let bytes = serde_json::to_vec_pretty(list).map_err(|e| AppError::new("internal", e.to_string()))?;
    pdf_core::atomic_write(&dir.join("index.json"), &bytes, false).map_err(io)
}

fn io(e: std::io::Error) -> AppError {
    AppError::new("internal", e.to_string())
}

fn new_id(dir: &Path) -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut n = nanos as u64 ^ (std::process::id() as u64).rotate_left(32);
    loop {
        n = n.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let id = format!("{:012x}", n >> 16);
        if !dir.join(format!("{id}.png")).exists() {
            return id;
        }
    }
}

fn clean_name(n: &str) -> String {
    let s: String = n.chars().filter(|c| !c.is_control()).take(60).collect();
    let s = s.trim();
    if s.is_empty() { "Signature".into() } else { s.to_string() }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[tauri::command]
pub fn sig_list(state: State<'_, AppState>) -> Vec<SigMeta> {
    read_index(&lib_dir(&state))
}

#[tauri::command]
pub fn sig_asset(id: String, state: State<'_, AppState>) -> Result<Response> {
    if !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::new("not_allowed", "Invalid signature id."));
    }
    let bytes = std::fs::read(lib_dir(&state).join(format!("{id}.png"))).map_err(io)?;
    Ok(Response::new(bytes))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDrawn {
    name: String,
    strokes: Vec<Vec<[f32; 3]>>,
    view: [f32; 2],
    width: f32,
    color: [f32; 3],
}

#[tauri::command]
pub fn sig_create_drawn(d: NewDrawn, state: State<'_, AppState>, lock: State<'_, SigLock>) -> Result<SigMeta> {
    let _g = lock.0.lock().unwrap_or_else(|p| p.into_inner());
    let total: usize = d.strokes.iter().map(Vec::len).sum();
    if total == 0 || total > 200_000 || d.view[0] < 1.0 || d.view[1] < 1.0 {
        return Err(AppError::new("invalid_edit", "The drawing is empty or too large."));
    }
    let dir = lib_dir(&state);
    let mut list = read_index(&dir);
    if list.len() >= MAX_SIGNATURES {
        return Err(AppError::new("invalid_edit", "The signature library is full."));
    }
    let meta = SigMeta {
        id: new_id(&dir),
        name: clean_name(&d.name),
        kind: "drawn".into(),
        created: now(),
        aspect: d.view[0] / d.view[1],
        drawn: Some(Drawn { strokes: d.strokes, view: d.view, width: d.width.clamp(0.5, 40.0), color: d.color }),
        text: None,
        font: None,
    };
    list.push(meta.clone());
    write_index(&dir, &list)?;
    Ok(meta)
}

fn raw_body(req: &Request<'_>) -> Result<Vec<u8>> {
    match req.body() {
        InvokeBody::Raw(b) if !b.is_empty() && b.len() <= MAX_UPLOAD => Ok(b.clone()),
        InvokeBody::Raw(_) => Err(AppError::new("too_large", "The image is empty or too large.")),
        _ => Err(AppError::new("internal", "Expected a binary body.")),
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Header values are percent-encoded by the frontend so they stay ASCII.
fn header(req: &Request<'_>, name: &str) -> Option<String> {
    req.headers().get(name).and_then(|v| v.to_str().ok()).map(percent_decode)
}

/// Preview an imported image: remove the background at `threshold` and crop. Returns PNG bytes.
/// Body = raw PNG/JPEG; header `threshold` = 0..255.
#[tauri::command]
pub async fn sig_process_import(request: Request<'_>) -> Result<Response> {
    let bytes = raw_body(&request)?;
    let threshold: u8 = header(&request, "threshold").and_then(|t| t.parse().ok()).unwrap_or(200);
    let out = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>> {
        let img = pdf_core::sigimage::decode(&bytes).map_err(|e| AppError::new("open_failed", e.to_string()))?;
        let cut = pdf_core::sigimage::remove_background(&img, threshold).map_err(|e| AppError::new("invalid_edit", e.to_string()))?;
        Ok(pdf_core::sigimage::encode_png(&cut))
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))??;
    Ok(Response::new(out))
}

/// Save a raster signature. Body = PNG (already processed / rendered); headers: name, kind, text, font.
#[tauri::command]
pub fn sig_create_image(request: Request<'_>, state: State<'_, AppState>, lock: State<'_, SigLock>) -> Result<SigMeta> {
    let bytes = raw_body(&request)?;
    let kind = match header(&request, "kind").as_deref() {
        Some("typed") => "typed",
        _ => "imported",
    };
    let img = pdf_core::sigimage::decode(&bytes).map_err(|e| AppError::new("open_failed", e.to_string()))?;
    let img = pdf_core::sigimage::crop_to_content(&img).map_err(|e| AppError::new("invalid_edit", e.to_string()))?;
    let _g = lock.0.lock().unwrap_or_else(|p| p.into_inner());
    let dir = lib_dir(&state);
    let mut list = read_index(&dir);
    if list.len() >= MAX_SIGNATURES {
        return Err(AppError::new("invalid_edit", "The signature library is full."));
    }
    std::fs::create_dir_all(&dir).map_err(io)?;
    let id = new_id(&dir);
    pdf_core::atomic_write(&dir.join(format!("{id}.png")), &pdf_core::sigimage::encode_png(&img), false).map_err(io)?;
    let meta = SigMeta {
        id,
        name: clean_name(&header(&request, "name").unwrap_or_default()),
        kind: kind.into(),
        created: now(),
        aspect: img.width() as f32 / img.height() as f32,
        drawn: None,
        text: header(&request, "text").map(|t| t.chars().take(100).collect()),
        font: header(&request, "font").map(|t| t.chars().take(40).collect()),
    };
    list.push(meta.clone());
    write_index(&dir, &list)?;
    Ok(meta)
}

#[tauri::command]
pub fn sig_rename(id: String, name: String, state: State<'_, AppState>, lock: State<'_, SigLock>) -> Result<()> {
    let _g = lock.0.lock().unwrap_or_else(|p| p.into_inner());
    let dir = lib_dir(&state);
    let mut list = read_index(&dir);
    let m = list.iter_mut().find(|m| m.id == id).ok_or_else(|| AppError::new("invalid_edit", "No such signature."))?;
    m.name = clean_name(&name);
    write_index(&dir, &list)
}

#[tauri::command]
pub fn sig_delete(id: String, state: State<'_, AppState>, lock: State<'_, SigLock>) -> Result<()> {
    let _g = lock.0.lock().unwrap_or_else(|p| p.into_inner());
    let dir = lib_dir(&state);
    let mut list = read_index(&dir);
    let before = list.len();
    list.retain(|m| m.id != id);
    if list.len() == before {
        return Err(AppError::new("invalid_edit", "No such signature."));
    }
    if id.chars().all(|c| c.is_ascii_hexdigit()) {
        let _ = std::fs::remove_file(dir.join(format!("{id}.png")));
    }
    write_index(&dir, &list)
}

// ------------------------------------------------------------------ signing

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementDto {
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rotation: f32,
    /// "signature" | "text"
    pub kind: String,
    pub sig_id: Option<String>,
    pub text: Option<String>,
    pub font_size: Option<f32>,
}

/// Content for a library signature (used by bulk signing).
pub fn content_for(dir: &Path, list: &[SigMeta], id: &str) -> Result<Content> {
    let m = list.iter().find(|m| m.id == id).ok_or_else(|| AppError::new("invalid_edit", "The signature in this rule is no longer in the library."))?;
    match &m.drawn {
        Some(d) => Ok(Content::Ink {
            strokes: d.strokes.iter().map(|s| Stroke { points: s.clone() }).collect(),
            view: (d.view[0], d.view[1]),
            width: d.width,
            color: d.color,
        }),
        None => {
            if !id.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(AppError::new("not_allowed", "Invalid signature id."));
            }
            let bytes = std::fs::read(dir.join(format!("{id}.png"))).map_err(io)?;
            Ok(Content::Image(pdf_core::sigimage::decode(&bytes).map_err(|e| AppError::new("open_failed", e.to_string()))?))
        }
    }
}

pub fn library_dir(state: &AppState) -> PathBuf {
    lib_dir(state)
}

pub fn library_list(dir: &Path) -> Vec<SigMeta> {
    read_index(dir)
}

/// Resolve a placement into pdf-core content using the library on disk.
pub fn to_placement(dir: &Path, list: &[SigMeta], p: &PlacementDto) -> Result<Placement> {
    let content = if p.kind == "text" {
        let t = p.text.clone().unwrap_or_default();
        if t.is_empty() {
            return Err(AppError::new("invalid_edit", "A text field is empty."));
        }
        Content::Text { text: t.chars().take(500).collect(), font_size: p.font_size.unwrap_or(12.0).clamp(4.0, 200.0), color: [0.0, 0.0, 0.0] }
    } else {
        let id = p.sig_id.as_deref().ok_or_else(|| AppError::new("invalid_edit", "Missing signature."))?;
        let m = list.iter().find(|m| m.id == id).ok_or_else(|| AppError::new("invalid_edit", "A placed signature was deleted from the library."))?;
        match &m.drawn {
            Some(d) => Content::Ink {
                strokes: d.strokes.iter().map(|s| Stroke { points: s.clone() }).collect(),
                view: (d.view[0], d.view[1]),
                width: d.width,
                color: d.color,
            },
            None => {
                let bytes = std::fs::read(dir.join(format!("{id}.png"))).map_err(io)?;
                Content::Image(pdf_core::sigimage::decode(&bytes).map_err(|e| AppError::new("open_failed", e.to_string()))?)
            }
        }
    };
    Ok(Placement { page: p.page, x: p.x, y: p.y, w: p.w, h: p.h, rotation: p.rotation, content })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedResult {
    path: String,
    applied: usize,
    warnings: Vec<String>,
}

/// Write a signed copy of `path` to a new file the user picks. The original is never touched.
#[tauri::command]
pub async fn sign_document(
    app: AppHandle,
    path: String,
    password: Option<String>,
    placements: Vec<PlacementDto>,
    flatten: bool,
    state: State<'_, AppState>,
) -> Result<Option<SignedResult>> {
    if placements.is_empty() {
        return Err(AppError::new("invalid_edit", "Nothing has been placed yet."));
    }
    let canon = state.allowed.check_readable(Path::new(&path), &state.limits)?;
    let limits = state.limits;
    let dir = lib_dir(&state);
    let stem = canon.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
    let default = pdf_core::sanitize_file_name(&format!("{stem}_signed.pdf"));
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().add_filter("PDF documents", &["pdf"]).set_file_name(&default).blocking_save_file().and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))?;
    let Some(mut out) = picked else { return Ok(None) };
    if out.extension().is_none_or(|e| !e.eq_ignore_ascii_case("pdf")) {
        out.set_extension("pdf");
    }
    if out == canon {
        return Err(AppError::new("save_failed", "Choose a different file name: the original is never overwritten by signing."));
    }
    let res = tauri::async_runtime::spawn_blocking(move || -> Result<SignedResult> {
        let mut doc = pdf_core::load_file(&canon, password.as_deref(), &limits).map_err(|e| {
            use pdf_core::LoadError::*;
            let code = match &e { NeedsPassword => "password_required", WrongPassword => "wrong_password", _ => "open_failed" };
            AppError::new(code, e.to_string())
        })?;
        let list = read_index(&dir);
        let ps: Vec<Placement> = placements.iter().map(|p| to_placement(&dir, &list, p)).collect::<Result<_>>()?;
        let rep = pdf_core::sign::apply(&mut doc, &ps, if flatten { Mode::Flatten } else { Mode::Annotate })
            .map_err(|e| AppError::new("save_failed", e.to_string()))?;
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).map_err(|e| AppError::new("save_failed", e.to_string()))?;
        pdf_core::atomic_write(&out, &bytes, true).map_err(|e| AppError::new("save_failed", e.to_string()))?;
        Ok(SignedResult { path: out.to_string_lossy().into_owned(), applied: rep.applied, warnings: rep.warnings })
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))??;
    let _ = state.allowed.grant(Path::new(&res.path));
    Ok(Some(res))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decoding_handles_utf8_and_malformed_input() {
        assert_eq!(percent_decode("Jos%C3%A9%20Garc%C3%ADa"), "José García");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
        assert_eq!(percent_decode(""), "");
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  "), "Signature");
        assert_eq!(clean_name("a\u{0}b"), "ab");
        assert_eq!(clean_name(&"x".repeat(100)).len(), 60);
    }

    fn meta(id: &str, drawn: bool) -> SigMeta {
        SigMeta {
            id: id.into(), name: "n".into(), kind: if drawn { "drawn" } else { "typed" }.into(), created: 0, aspect: 2.0,
            drawn: drawn.then(|| Drawn { strokes: vec![vec![[0.0, 0.0, 0.5], [1.0, 1.0, 0.5]]], view: [10.0, 5.0], width: 2.0, color: [0.0; 3] }),
            text: None, font: None,
        }
    }
    fn dto(kind: &str, sig: Option<&str>, text: Option<&str>) -> PlacementDto {
        PlacementDto { page: 0, x: 1.0, y: 1.0, w: 50.0, h: 20.0, rotation: 0.0, kind: kind.into(), sig_id: sig.map(Into::into), text: text.map(Into::into), font_size: Some(12.0) }
    }

    #[test]
    fn placements_resolve_drawn_text_and_reject_bad_references() {
        let dir = std::env::temp_dir().join("pf-sigtest-none");
        let list = vec![meta("abc123", true)];
        assert!(matches!(to_placement(&dir, &list, &dto("signature", Some("abc123"), None)).unwrap().content, Content::Ink { .. }));
        assert!(matches!(to_placement(&dir, &list, &dto("text", None, Some("hi"))).unwrap().content, Content::Text { .. }));
        assert!(to_placement(&dir, &list, &dto("text", None, Some(""))).is_err());
        assert!(to_placement(&dir, &list, &dto("signature", Some("missing"), None)).is_err());
        assert!(to_placement(&dir, &list, &dto("signature", None, None)).is_err());
    }

    #[test]
    fn raster_signature_is_loaded_from_the_library_dir() {
        let dir = std::env::temp_dir().join(format!("pf-sigtest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = pdf_core::sigimage::encode_png(&image::RgbaImage::from_pixel(8, 4, image::Rgba([0, 0, 0, 255])));
        std::fs::write(dir.join("beef.png"), png).unwrap();
        let list = vec![meta("beef", false)];
        let p = to_placement(&dir, &list, &dto("signature", Some("beef"), None)).unwrap();
        assert!(matches!(p.content, Content::Image(ref i) if i.dimensions() == (8, 4)));
    }
}
