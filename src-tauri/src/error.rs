use pdf_core::{InspectError, PathError};
use pdf_render::RenderError;
use serde::Serialize;

/// Error sent to the frontend: a stable machine-readable `code` plus a user-facing `message`.
#[derive(Debug, Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
}

impl AppError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl From<PathError> for AppError {
    fn from(e: PathError) -> Self {
        let code = match e {
            PathError::NotAllowed => "not_allowed",
            PathError::TooLarge { .. } => "too_large",
            _ => "open_failed",
        };
        Self::new(code, e.to_string())
    }
}

impl From<InspectError> for AppError {
    fn from(e: InspectError) -> Self {
        let code = match e {
            InspectError::TooManyPages { .. } => "too_many_pages",
            InspectError::Parse(_) => "open_failed",
        };
        Self::new(code, e.to_string())
    }
}

impl From<RenderError> for AppError {
    fn from(e: RenderError) -> Self {
        let code = match e {
            RenderError::LibraryMissing { .. } => "library_missing",
            RenderError::PasswordRequired => "password_required",
            RenderError::WrongPassword => "wrong_password",
            RenderError::Open(_) | RenderError::PageOutOfRange { .. } => "open_failed",
            RenderError::Render(_) => "render_failed",
        };
        Self::new(code, e.to_string())
    }
}
