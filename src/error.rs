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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_path_not_found() {
        let err = Error::PathNotFound(PathBuf::from("/some/path"));
        assert_eq!(err.to_string(), "Path not found: /some/path");
    }

    #[test]
    fn display_permission_denied() {
        let err = Error::PermissionDenied {
            path: PathBuf::from("/secret"),
            reason: "access denied".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Permission denied: /secret — access denied"
        );
    }

    #[test]
    fn display_platform_api() {
        let err = Error::PlatformApi {
            api: "RmGetList",
            code: 234,
            detail: "buffer too small".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Platform API call failed: RmGetList (code 234)"
        );
    }

    #[test]
    fn display_kill_failed() {
        let err = Error::KillFailed {
            pid: 1234,
            reason: "access denied".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Failed to kill process PID=1234: access denied"
        );
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
        assert!(err.to_string().contains("file missing"));
    }

    #[test]
    fn error_is_debug() {
        let err = Error::PathNotFound(PathBuf::from("/test"));
        let debug = format!("{:?}", err);
        assert!(debug.contains("PathNotFound"));
    }
}
