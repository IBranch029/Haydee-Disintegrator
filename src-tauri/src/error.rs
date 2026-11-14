//! Error taxonomy shared by every engine module.

use serde::{Deserialize, Serialize};

/// Convenience alias used across the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Every fallible operation in the engine maps onto this taxonomy so that the
/// frontend can render actionable diagnostics instead of opaque strings.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("registry failure: {0}")]
    Registry(String),

    #[error("filesystem failure: {0}")]
    Filesystem(String),

    #[error("access denied: {0}")]
    AccessDenied(String),

    #[error("process failure: {0}")]
    Process(String),

    #[error("service failure: {0}")]
    Service(String),

    #[error("scheduled task failure: {0}")]
    Task(String),

    #[error("safety guard rejected the operation: {0}")]
    Safety(String),

    #[error("target not found: {0}")]
    NotFound(String),

    #[error("i/o failure: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization failure: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Stable machine readable discriminator used by the UI telemetry layer.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Registry(_) => "registry",
            Error::Filesystem(_) => "filesystem",
            Error::AccessDenied(_) => "access_denied",
            Error::Process(_) => "process",
            Error::Service(_) => "service",
            Error::Task(_) => "task",
            Error::Safety(_) => "safety",
            Error::NotFound(_) => "not_found",
            Error::Io(_) => "io",
            Error::Serde(_) => "serde",
            Error::Other(_) => "other",
        }
    }
}

/// Structured failure entry emitted while purging. Failures never abort the
/// operation: the engine records them and keeps dissolving the remaining
/// residue so that a single locked file cannot strand a target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failure {
    pub category: String,
    pub target: String,
    pub reason: String,
    pub deferred: bool,
}
