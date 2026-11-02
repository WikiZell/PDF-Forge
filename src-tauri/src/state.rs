use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use pdf_core::{AllowedPaths, Limits};
use pdf_render::Document;

pub struct AppState {
    pub allowed: AllowedPaths,
    pub limits: Limits,
    docs: Mutex<HashMap<u64, Arc<Document>>>,
    next_id: AtomicU64,
    /// Result of loading PDFium at startup: None = ok, Some(message) = failed.
    pub pdfium_error: Mutex<Option<String>>,
    /// Path handed to us by the OS at launch (CLI argument / file association).
    pub launch_path: Mutex<Option<String>>,
}

impl AppState {
    pub fn new(allowed: AllowedPaths) -> Self {
        Self {
            allowed,
            limits: Limits::default(),
            docs: Mutex::default(),
            next_id: AtomicU64::new(1),
            pdfium_error: Mutex::default(),
            launch_path: Mutex::default(),
        }
    }

    pub fn insert(&self, doc: Document) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.docs.lock().unwrap_or_else(|p| p.into_inner()).insert(id, Arc::new(doc));
        id
    }

    pub fn get(&self, id: u64) -> Option<Arc<Document>> {
        self.docs.lock().unwrap_or_else(|p| p.into_inner()).get(&id).cloned()
    }

    pub fn remove(&self, id: u64) {
        self.docs.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
    }
}
