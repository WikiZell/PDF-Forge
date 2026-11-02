//! Safe loading of untrusted PDFs: size limit, panic containment, password handling.

use std::fmt;
use std::path::Path;

use lopdf::{Document, LoadOptions};

use crate::fs_safety::Limits;

#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    NeedsPassword,
    WrongPassword,
    TooLarge { size: u64, max: u64 },
    TooManyPages { pages: usize, max: usize },
    Corrupt(String),
    Io(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NeedsPassword => write!(f, "This document is password protected."),
            Self::WrongPassword => write!(f, "The password is incorrect."),
            Self::TooLarge { size, max } => write!(f, "File is {size} bytes; the maximum is {max} bytes."),
            Self::TooManyPages { pages, max } => write!(f, "Document has {pages} pages; the maximum is {max}."),
            Self::Corrupt(e) => write!(f, "The file is damaged or not a PDF ({e})."),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for LoadError {}

/// Parse PDF bytes. Never panics: parser panics on hostile input become `Corrupt`.
/// The result is fully decrypted (so outputs derived from it are unencrypted).
pub fn load_bytes(bytes: &[u8], password: Option<&str>, limits: &Limits) -> Result<Document, LoadError> {
    if bytes.len() as u64 > limits.max_file_size {
        return Err(LoadError::TooLarge { size: bytes.len() as u64, max: limits.max_file_size });
    }
    let pw = password.map(str::to_owned);
    let buf = bytes.to_vec();
    let parsed = std::panic::catch_unwind(move || {
        let opts = match &pw {
            Some(p) => LoadOptions::with_password(p),
            None => LoadOptions::default(),
        };
        Document::load_mem_with_options(&buf, opts).map(|d| (d, pw.is_some()))
    })
    .map_err(|_| LoadError::Corrupt("parser fault".into()))?;

    let (doc, had_pw) = match parsed {
        Ok(v) => v,
        Err(e) => {
            let msg = e.to_string();
            let lower = msg.to_ascii_lowercase();
            return Err(if lower.contains("password") || lower.contains("decrypt") {
                if had_password(password) {
                    LoadError::WrongPassword
                } else {
                    LoadError::NeedsPassword
                }
            } else {
                LoadError::Corrupt(msg)
            });
        }
    };
    let _ = had_pw;
    // Still encrypted => decryption with the given (or empty) password did not take effect.
    if doc.is_encrypted() {
        return Err(if had_password(password) { LoadError::WrongPassword } else { LoadError::NeedsPassword });
    }
    let pages = doc.get_pages().len();
    if pages == 0 {
        return Err(LoadError::Corrupt("no pages".into()));
    }
    if pages > limits.max_pages {
        return Err(LoadError::TooManyPages { pages, max: limits.max_pages });
    }
    Ok(doc)
}

fn had_password(p: Option<&str>) -> bool {
    p.is_some_and(|s| !s.is_empty())
}

pub fn load_file(path: &Path, password: Option<&str>, limits: &Limits) -> Result<Document, LoadError> {
    let meta = std::fs::metadata(path).map_err(|e| LoadError::Io(e.to_string()))?;
    if meta.len() > limits.max_file_size {
        return Err(LoadError::TooLarge { size: meta.len(), max: limits.max_file_size });
    }
    let bytes = std::fs::read(path).map_err(|e| LoadError::Io(e.to_string()))?;
    load_bytes(&bytes, password, limits)
}
