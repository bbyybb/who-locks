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
