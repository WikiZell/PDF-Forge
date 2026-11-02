//! Untrusted-input guards: size limits, path allow-listing, file-name sanitising.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Limits applied to every PDF treated as untrusted input.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_file_size: u64,
    pub max_pages: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_file_size: 2 * 1024 * 1024 * 1024, max_pages: 100_000 }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PathError {
    NotAllowed,
    NotAFile,
    TooLarge { size: u64, max: u64 },
    Io(String),
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAllowed => write!(f, "This path was not selected by the user."),
            Self::NotAFile => write!(f, "The path is not a regular file."),
            Self::TooLarge { size, max } => {
                write!(f, "File is {size} bytes; the configured maximum is {max} bytes.")
            }
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PathError {}

/// Paths the user selected via a dialog, drag-and-drop, CLI argument or file
/// association. The frontend may only refer to files in this set (or inside the
/// app data directory). Paths are canonicalised so `..` and symlink tricks fail.
#[derive(Default)]
pub struct AllowedPaths {
    files: Mutex<HashSet<PathBuf>>,
    app_data: Option<PathBuf>,
}

impl AllowedPaths {
    pub fn new(app_data: Option<PathBuf>) -> Self {
        let app_data = app_data.and_then(|p| p.canonicalize().ok());
        Self { files: Mutex::default(), app_data }
    }

    /// Record a user-selected path. Call only from trusted code paths
    /// (dialog result, OS-delivered drop/argument), never from frontend input.
    pub fn grant(&self, path: &Path) -> Result<PathBuf, PathError> {
        let canon = path.canonicalize().map_err(|e| PathError::Io(e.to_string()))?;
        self.files.lock().unwrap_or_else(|p| p.into_inner()).insert(canon.clone());
        Ok(canon)
    }

    /// Validate a frontend-supplied path and enforce the size limit.
    pub fn check_readable(&self, path: &Path, limits: &Limits) -> Result<PathBuf, PathError> {
        let canon = path.canonicalize().map_err(|e| PathError::Io(e.to_string()))?;
        let granted = self.files.lock().unwrap_or_else(|p| p.into_inner()).contains(&canon);
        let in_app = self.app_data.as_ref().is_some_and(|d| canon.starts_with(d));
        if !granted && !in_app {
            return Err(PathError::NotAllowed);
        }
        let meta = std::fs::metadata(&canon).map_err(|e| PathError::Io(e.to_string()))?;
        if !meta.is_file() {
            return Err(PathError::NotAFile);
        }
        if meta.len() > limits.max_file_size {
            return Err(PathError::TooLarge { size: meta.len(), max: limits.max_file_size });
        }
        Ok(canon)
    }
}

/// Make a string safe to use as a single file-name component on Windows, macOS and Linux.
pub fn sanitize_file_name(name: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let mut s = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    if s.is_empty() || RESERVED.contains(&stem.as_str()) {
        s = format!("_{s}");
    }
    if s.len() > 200 {
        let mut end = 200;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::File::create(&p).unwrap().write_all(bytes).unwrap();
        p
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pdf-core-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn ungranted_path_is_rejected() {
        let d = temp_dir("ungranted");
        let f = temp_file(&d, "a.pdf", b"x");
        let a = AllowedPaths::new(None);
        assert_eq!(a.check_readable(&f, &Limits::default()), Err(PathError::NotAllowed));
    }

    #[test]
    fn granted_path_is_accepted_and_dotdot_is_canonicalised() {
        let d = temp_dir("granted");
        let f = temp_file(&d, "a.pdf", b"x");
        let a = AllowedPaths::new(None);
        a.grant(&f).unwrap();
        let sneaky = d.join("..").join(d.file_name().unwrap()).join("a.pdf");
        assert!(a.check_readable(&sneaky, &Limits::default()).is_ok());
    }

    #[test]
    fn sibling_of_granted_file_is_rejected() {
        let d = temp_dir("sibling");
        let f = temp_file(&d, "a.pdf", b"x");
        let g = temp_file(&d, "b.pdf", b"x");
        let a = AllowedPaths::new(None);
        a.grant(&f).unwrap();
        assert_eq!(a.check_readable(&g, &Limits::default()), Err(PathError::NotAllowed));
    }

    #[test]
    fn size_limit_enforced() {
        let d = temp_dir("size");
        let f = temp_file(&d, "a.pdf", &[0u8; 100]);
        let a = AllowedPaths::new(None);
        a.grant(&f).unwrap();
        let lim = Limits { max_file_size: 10, ..Limits::default() };
        assert!(matches!(a.check_readable(&f, &lim), Err(PathError::TooLarge { .. })));
    }

    #[test]
    fn app_data_dir_is_allowed() {
        let d = temp_dir("appdata");
        let f = temp_file(&d, "sig.json", b"{}");
        let a = AllowedPaths::new(Some(d.clone()));
        assert!(a.check_readable(&f, &Limits::default()).is_ok());
    }

    #[test]
    fn sanitize_handles_hostile_names() {
        assert_eq!(sanitize_file_name("../../etc/passwd"), ".._.._etc_passwd");
        assert_eq!(sanitize_file_name("a:b*c?.pdf"), "a_b_c_.pdf");
        assert_eq!(sanitize_file_name("CON.pdf"), "_CON.pdf");
        assert_eq!(sanitize_file_name("name. . "), "name");
        assert_eq!(sanitize_file_name(""), "_");
        assert!(sanitize_file_name(&"é".repeat(300)).len() <= 200);
    }
}
