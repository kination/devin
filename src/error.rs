use std::fmt;

#[derive(Debug)]
pub enum DevinError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Http(reqwest::Error),
    ApfelNotFound,
    ApfelTimeout,
    IndexNotBuilt,
    McpBinaryNotFound,
}

impl fmt::Display for DevinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DevinError::Io(e) => write!(f, "IO error: {e}"),
            DevinError::Json(e) => write!(f, "JSON error: {e}"),
            DevinError::Http(e) => write!(f, "HTTP error: {e}"),
            DevinError::ApfelNotFound => write!(
                f,
                "apfel not found.\nInstallation: brew tap Arthur-Ficial/tap && brew install Arthur-Ficial/tap/apfel"
            ),
            DevinError::ApfelTimeout => write!(f, "apfel server start timeout (exceeded 10s)"),
            DevinError::IndexNotBuilt => write!(
                f,
                "Index not found. Run `devin index <path>` first."
            ),
            DevinError::McpBinaryNotFound => write!(
                f,
                "devin-mcp binary not found next to devin binary. Run `cargo build` to build both."
            ),
        }
    }
}

impl From<std::io::Error> for DevinError {
    fn from(e: std::io::Error) -> Self { DevinError::Io(e) }
}

impl From<serde_json::Error> for DevinError {
    fn from(e: serde_json::Error) -> Self { DevinError::Json(e) }
}

impl From<reqwest::Error> for DevinError {
    fn from(e: reqwest::Error) -> Self { DevinError::Http(e) }
}

pub type Result<T> = std::result::Result<T, DevinError>;
