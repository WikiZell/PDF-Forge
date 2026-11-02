//! The only place for OS-specific behaviour. Everything else must stay portable.

use std::path::PathBuf;

/// Directory names (relative to the Tauri resource dir) searched for the PDFium library,
/// per target. Only Windows x64 is wired up for shipping; see scripts/fetch-pdfium.mjs.
pub const PDFIUM_RESOURCE_SUBDIR: &str = "resources/pdfium";

/// Extra development-time locations for the PDFium library.
pub fn dev_pdfium_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(d) = std::env::var_os("PDFIUM_DIR") {
        v.push(PathBuf::from(d));
    }
    // Source-tree location populated by `npm run fetch-pdfium`.
    v.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PDFIUM_RESOURCE_SUBDIR));
    v
}
