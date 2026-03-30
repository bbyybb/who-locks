use crate::error::Error;
use crate::model::{FileLockInfo, ProcessInfo};
use std::path::Path;

/// 核心 trait：检测文件被哪些进程占用
pub trait LockDetector: Send + Sync {
    /// 检测单个文件的占用情况
    fn detect_file(&self, path: &Path) -> Result<Vec<ProcessInfo>, Error>;

    /// 批量检测多个文件，返回有占用的文件列表
    fn detect_batch(&self, paths: &[&Path]) -> Result<Vec<FileLockInfo>, Error> {
        let mut results = Vec::new();
        for path in paths {
            match self.detect_file(path) {
                Ok(lockers) if !lockers.is_empty() => {
                    results.push(FileLockInfo {
                        path: path.to_path_buf(),
                        lockers,
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    log::warn!("Failed to detect locks for {}: {}", path.display(), e);
                }
            }
        }
        Ok(results)
    }

    /// 平台名称
    #[allow(dead_code)]
    fn platform_name(&self) -> &'static str;
}

/// 根据编译目标创建对应平台的检测器
pub fn create_detector() -> Box<dyn LockDetector> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsDetector::new())
    }

    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxDetector::new())
    }

    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacosDetector::new())
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        compile_error!("who-locks only supports Windows, Linux and macOS");
    }
}

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::model::{LockType, ProcessInfo};
    use std::path::Path;

    /// 测试默认 detect_batch 实现的行为
    struct TestDetector {
        locked_path: std::path::PathBuf,
    }
    impl TestDetector {
        fn new(locked: &str) -> Self {
            Self {
                locked_path: std::path::PathBuf::from(locked),
            }
        }
    }
    impl LockDetector for TestDetector {
        fn detect_file(&self, path: &Path) -> Result<Vec<ProcessInfo>, Error> {
            if path == self.locked_path {
                Ok(vec![ProcessInfo::new(
                    42,
                    "test_proc".to_string(),
                    LockType::FileHandle,
                    None,
                    None,
                )])
            } else {
                Ok(Vec::new())
            }
        }
        fn platform_name(&self) -> &'static str {
            "test"
        }
    }

    /// 检测器错误返回时默认 batch 应记录警告但不中断
    struct ErrorDetector;
    impl LockDetector for ErrorDetector {
        fn detect_file(&self, _path: &Path) -> Result<Vec<ProcessInfo>, Error> {
            Err(Error::PathNotFound(std::path::PathBuf::from("/err")))
        }
        fn platform_name(&self) -> &'static str {
            "error"
        }
    }

    #[test]
    fn default_batch_returns_locked_files() {
        let det = TestDetector::new("/locked.txt");
        let paths: Vec<&Path> = vec![Path::new("/locked.txt"), Path::new("/not_locked.txt")];
        let result = det.detect_batch(&paths).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, std::path::PathBuf::from("/locked.txt"));
        assert_eq!(result[0].lockers[0].pid, 42);
    }

    #[test]
    fn default_batch_empty_input() {
        let det = TestDetector::new("/locked.txt");
        let result = det.detect_batch(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn default_batch_all_unlocked() {
        let det = TestDetector::new("/locked.txt");
        let paths: Vec<&Path> = vec![Path::new("/a.txt"), Path::new("/b.txt")];
        let result = det.detect_batch(&paths).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn default_batch_handles_errors_gracefully() {
        let det = ErrorDetector;
        let paths: Vec<&Path> = vec![Path::new("/test")];
        // 默认实现在 detect_file 返回错误时 log::warn 并跳过，不 panic
        let result = det.detect_batch(&paths).unwrap();
        assert!(result.is_empty());
    }
}
