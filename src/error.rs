use std::fmt;

#[derive(Debug)]
pub enum EnticError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Http(reqwest::Error),
    ApfelNotFound,
    ApfelTimeout,
    IndexNotBuilt,
    McpBinaryNotFound,
}

impl fmt::Display for EnticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnticError::Io(e) => write!(f, "IO error: {e}"),
            EnticError::Json(e) => write!(f, "JSON error: {e}"),
            EnticError::Http(e) => write!(f, "HTTP error: {e}"),
            EnticError::ApfelNotFound => write!(
                f,
                "apfel not found.\nInstallation: brew tap Arthur-Ficial/tap && brew install Arthur-Ficial/tap/apfel"
            ),
            EnticError::ApfelTimeout => write!(f, "apfel server start timeout (exceeded 10s)"),
            EnticError::IndexNotBuilt => write!(
                f,
                "Index not found. Run `entic index <path>` first."
            ),
            EnticError::McpBinaryNotFound => write!(
                f,
                "entic-mcp binary not found next to entic binary. Run `cargo build` to build both."
            ),
        }
    }
}

impl From<std::io::Error> for EnticError {
    fn from(e: std::io::Error) -> Self { EnticError::Io(e) }
}

impl From<serde_json::Error> for EnticError {
    fn from(e: serde_json::Error) -> Self { EnticError::Json(e) }
}

impl From<reqwest::Error> for EnticError {
    fn from(e: reqwest::Error) -> Self { EnticError::Http(e) }
}

pub type Result<T> = std::result::Result<T, EnticError>;
