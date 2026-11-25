use serde::Serialize;

pub type Result<T> = std::result::Result<T, AtomError>;

/// Every failure surfaced to the UI carries a stable code so the frontend can
/// react without string matching, plus a human sentence.
#[derive(Debug, thiserror::Error)]
pub enum AtomError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0} is not available in this environment")]
    Unavailable(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Parse(String),
    #[error("{0}")]
    Backend(String),
    #[error("{0}")]
    Cancelled(String),
    #[error("{0}")]
    Internal(String),
}

impl AtomError {
    pub fn code(&self) -> &'static str {
        match self {
            AtomError::Invalid(_) => "invalid_request",
            AtomError::NotFound(_) => "not_found",
            AtomError::Conflict(_) => "conflict",
            AtomError::Unavailable(_) => "unavailable",
            AtomError::Forbidden(_) => "forbidden",
            AtomError::Io(_) => "io_error",
            AtomError::Parse(_) => "parse_error",
            AtomError::Backend(_) => "backend_error",
            AtomError::Cancelled(_) => "cancelled",
            AtomError::Internal(_) => "internal_error",
        }
    }

    pub fn status(&self) -> u16 {
        match self {
            AtomError::Invalid(_) => 400,
            AtomError::Forbidden(_) => 403,
            AtomError::NotFound(_) => 404,
            AtomError::Conflict(_) => 409,
            AtomError::Unavailable(_) => 503,
            AtomError::Cancelled(_) => 499,
            _ => 500,
        }
    }
}

impl From<std::io::Error> for AtomError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => AtomError::NotFound(e.to_string()),
            std::io::ErrorKind::PermissionDenied => AtomError::Forbidden(e.to_string()),
            _ => AtomError::Io(e.to_string()),
        }
    }
}

impl From<serde_json::Error> for AtomError {
    fn from(e: serde_json::Error) -> Self {
        AtomError::Parse(e.to_string())
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
}

impl Serialize for AtomError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let body = ErrorBody { code: self.code(), message: self.to_string() };
        body.serialize(s)
    }
}
