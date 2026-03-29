use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)] // Some variants are only used on specific platforms
pub enum Error {
    #[error("Path not found: {0}")]
    PathNotFound(PathBuf),

    #[error("Permission denied: {path} — {reason}")]
    PermissionDenied { path: PathBuf, reason: String },

    #[error("Platform API call failed: {api} (code {code})")]
    PlatformApi {
        api: &'static str,
        code: i64,
        detail: String,
    },

    #[error("Failed to kill process PID={pid}: {reason}")]
    KillFailed { pid: u32, reason: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    WalkDir(#[from] walkdir::Error),
}
