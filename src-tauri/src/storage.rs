//! App-data persistence: recent files and per-file view state (JSON, written atomically).
//! No PDF content is stored. Passwords are never stored.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MAX_RECENT: usize = 20;
const MAX_VIEW_STATES: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    pub path: String,
    pub name: String,
    pub opened_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ViewState {
    pub page: usize,
    /// "fit-width", "fit-page" or a percent like "150"
    pub zoom: String,
    pub layout: String,
    #[serde(default)]
    pub touched: u64,
}

pub struct Storage {
    dir: PathBuf,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Storage {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn read<T: for<'de> Deserialize<'de> + Default>(&self, file: &str) -> T {
        std::fs::read(self.dir.join(file))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn write<T: Serialize>(&self, file: &str, v: &T) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let bytes = serde_json::to_vec_pretty(v).map_err(std::io::Error::other)?;
        pdf_core::atomic_write(&self.dir.join(file), &bytes, false)
    }

    pub fn recent(&self) -> Vec<RecentFile> {
        self.read("recent.json")
    }

    pub fn add_recent(&self, path: &str, name: &str) -> std::io::Result<()> {
        let mut list = self.recent();
        list.retain(|r| r.path != path);
        list.insert(0, RecentFile { path: path.into(), name: name.into(), opened_at: now() });
        list.truncate(MAX_RECENT);
        self.write("recent.json", &list)
    }

    pub fn remove_recent(&self, path: &str) -> std::io::Result<()> {
        let mut list = self.recent();
        list.retain(|r| r.path != path);
        self.write("recent.json", &list)
    }

    pub fn is_recent(&self, path: &str) -> bool {
        self.recent().iter().any(|r| r.path == path)
    }

    pub fn view_state(&self, path: &str) -> Option<ViewState> {
        self.read::<HashMap<String, ViewState>>("view_state.json").remove(path)
    }

    pub fn save_view_state(&self, path: &str, mut vs: ViewState) -> std::io::Result<()> {
        let mut all: HashMap<String, ViewState> = self.read("view_state.json");
        vs.touched = now();
        all.insert(path.into(), vs);
        if all.len() > MAX_VIEW_STATES {
            let mut keys: Vec<_> = all.iter().map(|(k, v)| (v.touched, k.clone())).collect();
            keys.sort();
            for (_, k) in keys.into_iter().take(all.len() - MAX_VIEW_STATES) {
                all.remove(&k);
            }
        }
        self.write("view_state.json", &all)
    }

    /// Settings > "Clear all app data": removes every file this app created in its data dir.
    pub fn clear_all(&self) -> std::io::Result<()> {
        match std::fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(tag: &str) -> Storage {
        let d = std::env::temp_dir().join(format!("pf-storage-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        Storage::new(d)
    }

    #[test]
    fn recent_is_mru_deduplicated_and_capped() {
        let s = store("recent");
        for i in 0..25 {
            s.add_recent(&format!("/p/{i}.pdf"), &format!("{i}.pdf")).unwrap();
        }
        s.add_recent("/p/10.pdf", "10.pdf").unwrap();
        let r = s.recent();
        assert_eq!(r.len(), MAX_RECENT);
        assert_eq!(r[0].path, "/p/10.pdf");
        assert_eq!(r.iter().filter(|x| x.path == "/p/10.pdf").count(), 1);
        assert!(s.is_recent("/p/24.pdf"));
        assert!(!s.is_recent("/p/0.pdf"));
        s.remove_recent("/p/10.pdf").unwrap();
        assert!(!s.is_recent("/p/10.pdf"));
    }

    #[test]
    fn view_state_roundtrip_and_corrupt_file_is_ignored() {
        let s = store("view");
        assert!(s.view_state("/a.pdf").is_none());
        s.save_view_state("/a.pdf", ViewState { page: 7, zoom: "150".into(), layout: "continuous".into(), touched: 0 }).unwrap();
        let v = s.view_state("/a.pdf").unwrap();
        assert_eq!((v.page, v.zoom.as_str()), (7, "150"));
        std::fs::write(s.dir().join("view_state.json"), b"{not json").unwrap();
        assert!(s.view_state("/a.pdf").is_none());
        s.save_view_state("/b.pdf", ViewState::default()).unwrap(); // recovers
        assert!(s.view_state("/b.pdf").is_some());
    }

    #[test]
    fn clear_all_removes_everything() {
        let s = store("clear");
        s.add_recent("/a.pdf", "a.pdf").unwrap();
        s.clear_all().unwrap();
        assert!(s.recent().is_empty());
        assert!(!s.dir().exists());
        s.clear_all().unwrap(); // idempotent
    }
}
