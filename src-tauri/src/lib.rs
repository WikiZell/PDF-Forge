mod commands;
mod error;
mod platform;
mod state;

use std::path::PathBuf;

use pdf_core::AllowedPaths;
use tauri::{DragDropEvent, Emitter, Manager, WindowEvent};

use crate::state::AppState;

fn is_pdf(p: &std::path::Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// Try each candidate directory until PDFium loads; report every location tried on failure.
fn load_pdfium(app: &tauri::App) -> Result<(), String> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(d) = app.path().resolve(platform::PDFIUM_RESOURCE_SUBDIR, tauri::path::BaseDirectory::Resource) {
        dirs.push(d);
    }
    dirs.extend(platform::dev_pdfium_dirs());
    let mut last = String::new();
    for d in &dirs {
        match pdf_render::init(d) {
            Ok(()) => return Ok(()),
            Err(e) => last = e.to_string(),
        }
    }
    Err(format!(
        "{last} Searched: {}. Reinstall PDF Forge, or run `npm run fetch-pdfium` when building from source.",
        dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join("; ")
    ))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data = app.path().app_data_dir().ok();
            let state = AppState::new(AllowedPaths::new(app_data));

            if let Err(msg) = load_pdfium(app) {
                *state.pdfium_error.lock().unwrap() = Some(msg);
            }

            // CLI argument / "Open with": the OS hands us the path, so it counts as user-selected.
            if let Some(arg) = std::env::args_os().skip(1).map(PathBuf::from).find(|p| is_pdf(p) && p.is_file()) {
                if let Ok(canon) = state.allowed.grant(&arg) {
                    *state.launch_path.lock().unwrap() = Some(canon.to_string_lossy().into_owned());
                }
            }
            app.manage(state);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Drag-and-drop: grant in Rust, then tell the frontend to open it.
            if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                let state = window.state::<AppState>();
                if let Some(p) = paths.iter().find(|p| is_pdf(p)) {
                    if let Ok(canon) = state.allowed.grant(p) {
                        let _ = window.emit("open-request", canon.to_string_lossy().into_owned());
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::pdfium_status,
            commands::take_launch_path,
            commands::pick_pdf,
            commands::open_pdf,
            commands::close_pdf,
            commands::page_size,
            commands::render_page,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PDF Forge");
}
