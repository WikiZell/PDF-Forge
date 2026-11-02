//! Lightweight structural inspection via lopdf, used before handing a file to PDFium.

use std::fmt;
use std::path::Path;

use lopdf::Document;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocInfo {
    pub page_count: usize,
    pub encrypted: bool,
}

#[derive(Debug)]
pub enum InspectError {
    Parse(String),
    TooManyPages { pages: usize, max: usize },
}

impl fmt::Display for InspectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "Could not parse PDF structure: {e}"),
            Self::TooManyPages { pages, max } => {
                write!(f, "Document has {pages} pages; the configured maximum is {max}.")
            }
        }
    }
}

impl std::error::Error for InspectError {}

/// Read the page count without decrypting content. Encrypted files report
/// `encrypted = true` and a page count of 0 (PDFium reports the real count once unlocked).
pub fn inspect(path: &Path, max_pages: usize) -> Result<DocInfo, InspectError> {
    let doc = Document::load(path).map_err(|e| InspectError::Parse(e.to_string()))?;
    if doc.is_encrypted() {
        return Ok(DocInfo { page_count: 0, encrypted: true });
    }
    let pages = doc.get_pages().len();
    if pages > max_pages {
        return Err(InspectError::TooManyPages { pages, max: max_pages });
    }
    Ok(DocInfo { page_count: pages, encrypted: false })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object, Stream};

    fn make_pdf(pages: usize, path: &Path) {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut kids = vec![];
        for _ in 0..pages {
            let content = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
            let page = doc.add_object(dictionary! {
                "Type" => "Page", "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
                "Contents" => content,
            });
            kids.push(Object::Reference(page));
        }
        doc.objects.insert(pages_id, Object::Dictionary(dictionary! {
            "Type" => "Pages", "Count" => pages as i64, "Kids" => kids,
        }));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        doc.save(path).unwrap();
    }

    #[test]
    fn counts_pages() {
        let p = std::env::temp_dir().join(format!("inspect3-{}.pdf", std::process::id()));
        make_pdf(3, &p);
        assert_eq!(inspect(&p, 100).unwrap(), DocInfo { page_count: 3, encrypted: false });
    }

    #[test]
    fn enforces_page_limit() {
        let p = std::env::temp_dir().join(format!("inspect5-{}.pdf", std::process::id()));
        make_pdf(5, &p);
        assert!(matches!(inspect(&p, 4), Err(InspectError::TooManyPages { pages: 5, max: 4 })));
    }

    #[test]
    fn garbage_is_a_parse_error() {
        let p = std::env::temp_dir().join(format!("garbage-{}.pdf", std::process::id()));
        std::fs::write(&p, b"not a pdf").unwrap();
        assert!(matches!(inspect(&p, 100), Err(InspectError::Parse(_))));
    }
}
