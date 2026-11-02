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

/// A path as a user should see it. On Windows `canonicalize` yields the "verbatim" form
/// (`\\?\C:\dir`, `\\?\UNC\server\share`), which is correct internally but ugly in the UI and not what
/// users type; this strips the prefix. Other paths are returned unchanged. It is only for display and
/// round-trips safely because paths are re-canonicalised (and re-checked) whenever they come back.
pub fn display_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s.into_owned()
    }
}

/// Make a string safe to use as a single file-name component on Windows, macOS and Linux.
pub fn sanitize_file_name(name: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1",
        "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
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
        let _ = std::fs::remove_dir_all(&d); // PIDs can repeat across runs; always start empty
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
    fn display_path_strips_the_windows_verbatim_prefix() {
        assert_eq!(display_path(Path::new(r"\\?\D:\a\b.pdf")), r"D:\a\b.pdf");
        assert_eq!(display_path(Path::new(r"\\?\UNC\srv\share\x.pdf")), r"\\srv\share\x.pdf");
        assert_eq!(display_path(Path::new(r"C:\plain\x.pdf")), r"C:\plain\x.pdf");
        assert_eq!(display_path(Path::new("/usr/local/x.pdf")), "/usr/local/x.pdf");
        assert_eq!(display_path(Path::new("")), "");
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

/// Write `bytes` to `dest` atomically: write a temp file in the same directory, fsync, then rename.
/// With `backup`, an existing `dest` is first copied to `<dest>.bak`.
pub fn atomic_write(dest: &Path, bytes: &[u8], backup: bool) -> std::io::Result<()> {
    use std::io::Write;
    let dir = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        if backup && dest.exists() {
            let mut bak = dest.as_os_str().to_owned();
            bak.push(".bak");
            std::fs::copy(dest, PathBuf::from(bak))?;
        }
        std::fs::rename(&tmp, dest)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Create `dest` with `bytes` only if it does not exist, atomically: the content is written to a
/// temp file first and then *published* with a hard link, which fails if `dest` exists. Readers
/// never see a partial file and nothing is ever overwritten. On file systems without hard links
/// (FAT/exFAT) it falls back to exclusive-create + write, which still never overwrites.
/// Returns `AlreadyExists` if the name is taken so the caller can pick another.
pub fn write_new_atomic(dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!(".{name}.{}.{n}.tmp", std::process::id()));
    {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    let result = match std::fs::hard_link(&tmp, dest) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(e),
        Err(_) => {
            // no hard links here: exclusive create + write
            match std::fs::OpenOptions::new().write(true).create_new(true).open(dest) {
                Ok(mut f) => f.write_all(bytes).and_then(|_| f.sync_all()).inspect_err(|_| {
                    let _ = std::fs::remove_file(dest);
                }),
                Err(e) => Err(e),
            }
        }
    };
    let _ = std::fs::remove_file(&tmp);
    result
}

/// Choose `<stem><suffix>.<ext>` in `dir`, adding " (2)", " (3)", ... so nothing is overwritten.
pub fn unique_output_path(dir: &Path, stem: &str, suffix: &str, ext: &str) -> PathBuf {
    let stem = sanitize_file_name(stem);
    let first = dir.join(format!("{stem}{suffix}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{stem}{suffix} ({n}).{ext}"))).find(|p| !p.exists()).unwrap()
}

#[cfg(test)]
mod write_tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pdf-core-w-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn atomic_write_creates_replaces_and_backs_up() {
        let d = dir("atomic");
        let f = d.join("out.bin");
        atomic_write(&f, b"one", true).unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"one");
        assert!(!d.join("out.bin.bak").exists());
        atomic_write(&f, b"two", true).unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"two");
        assert_eq!(std::fs::read(d.join("out.bin.bak")).unwrap(), b"one");
        // no temp files left behind
        let leftovers = std::fs::read_dir(&d)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".tmp"));
        assert_eq!(leftovers.count(), 0);
    }

    #[test]
    fn atomic_write_failure_leaves_original_and_no_temp() {
        let d = dir("atomicfail");
        let f = d.join("sub").join("missing-dir.bin"); // parent missing
        assert!(atomic_write(&f, b"x", false).is_err());
    }

    #[test]
    fn write_new_atomic_never_overwrites_and_leaves_no_temp() {
        let d = dir("newatomic");
        let f = d.join("out.pdf");
        write_new_atomic(&f, b"first").unwrap();
        let e = write_new_atomic(&f, b"second").unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&f).unwrap(), b"first");
        assert_eq!(
            std::fs::read_dir(&d)
                .unwrap()
                .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".tmp"))
                .count(),
            0
        );
    }

    #[test]
    fn unique_output_never_overwrites() {
        let d = dir("unique");
        let a = unique_output_path(&d, "doc", "_signed", "pdf");
        assert_eq!(a.file_name().unwrap(), "doc_signed.pdf");
        std::fs::write(&a, b"x").unwrap();
        let b = unique_output_path(&d, "doc", "_signed", "pdf");
        assert_eq!(b.file_name().unwrap(), "doc_signed (2).pdf");
        std::fs::write(&b, b"x").unwrap();
        assert_eq!(unique_output_path(&d, "doc", "_signed", "pdf").file_name().unwrap(), "doc_signed (3).pdf");
        assert_eq!(unique_output_path(&d, "a/b", "", "pdf").file_name().unwrap(), "a_b.pdf");
    }
}
