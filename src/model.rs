use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

/// 占用类型
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
#[allow(dead_code)] // 部分变体仅在特定平台使用
pub enum LockType {
    /// 文件描述符（进程打开了该文件）
    FileHandle,
    /// 当前工作目录（进程的 cwd 在该目录下）
    WorkingDir,
    /// 可执行文件（进程的 exe 指向该文件）
    Executable,
    /// 内存映射（mmap/Section，如共享库 .so/.dll）
    MemoryMap,
    /// 文件锁（flock/fcntl/Windows 锁）
    FileLock,
    /// 目录句柄
    DirHandle,
    /// 其他/未知
    Other(String),
}

impl std::fmt::Display for LockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockType::FileHandle => write!(f, "File Handle"),
            LockType::WorkingDir => write!(f, "Working Dir"),
            LockType::Executable => write!(f, "Executable"),
            LockType::MemoryMap => write!(f, "Memory Map"),
            LockType::FileLock => write!(f, "File Lock"),
            LockType::DirHandle => write!(f, "Dir Handle"),
            LockType::Other(s) => write!(f, "{}", s),
        }
    }
}

/// 占用文件的进程信息
#[derive(Debug, Clone, Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub lock_type: LockType,
    pub cmdline: Option<String>,
    pub user: Option<String>,
    /// 是否为阻塞性占用（影响文件剪切/移动/删除）
    #[serde(default)]
    pub blocking: bool,
}

impl ProcessInfo {
    /// 创建 ProcessInfo 并自动根据进程名/PID 计算 blocking 字段
    pub fn new(
        pid: u32,
        name: String,
        lock_type: LockType,
        cmdline: Option<String>,
        user: Option<String>,
    ) -> Self {
        let mut info = Self {
            pid,
            name,
            lock_type,
            cmdline,
            user,
            blocking: true, // 临时值，下面立即修正
        };
        info.blocking = info.is_blocking();
        info
    }

    /// 判断该占用是否阻塞文件操作（剪切/移动/删除）
    /// 返回 false 的进程不会阻止文件操作，在界面上灰色显示且不可勾选终止
    pub fn is_blocking(&self) -> bool {
        // 目录句柄（DirHandle）总是非阻塞的：
        // handle.exe 检测到的目录句柄是 FILE_LIST_DIRECTORY 共享访问，
        // 不会阻止目录内文件的移动/删除/重命名操作。
        // 进程打开文件时自动获取的父目录句柄也属于此类。
        // 注意：WorkingDir（进程工作目录）是单独的类型，会阻止删除该目录，不受此规则影响。
        if self.lock_type == LockType::DirHandle {
            return false;
        }

        let name_lower = self.name.to_lowercase();

        // 以下系统进程以共享模式打开文件/目录，不会阻塞文件操作
        // 即使持有句柄也可安全忽略

        // explorer.exe：文件管理器浏览，强制终止会导致桌面消失
        if name_lower == "explorer.exe" {
            return false;
        }

        // Windows Search 服务：索引时短暂打开
        if name_lower.contains("searchindexer")
            || name_lower.contains("searchprotocol")
            || name_lower.contains("searchfilterhost")
        {
            return false;
        }

        // Windows Defender / 杀毒软件：扫描时短暂打开
        if name_lower == "msmpeng.exe" || name_lower == "mpcmdrun.exe" {
            return false;
        }

        // 缩略图/预览服务：生成缩略图时短暂打开
        if name_lower.contains("thumbnailextractionhost") || name_lower.contains("prevhost") {
            return false;
        }

        // macOS: Finder 和 Spotlight 以共享模式打开文件，不会阻塞文件操作
        if name_lower == "finder"
            || name_lower == "mds"
            || name_lower == "mds_stores"
            || name_lower == "mdworker"
            || name_lower == "mdworker_shared"
            || name_lower == "fseventsd"
        {
            return false;
        }

        // Linux: 文件索引和桌面环境服务
        if name_lower == "tracker-miner-f"
            || name_lower == "tracker-miner-fs-3"
            || name_lower == "baloo_file"
            || name_lower == "nautilus"
            || name_lower == "dolphin"
            || name_lower == "thunar"
        {
            return false;
        }

        // 自身进程不阻塞
        if self.pid == std::process::id() {
            return false;
        }

        true
    }
}

/// 单个文件的占用检测结果
#[derive(Debug, Clone, Serialize)]
pub struct FileLockInfo {
    pub path: PathBuf,
    pub lockers: Vec<ProcessInfo>,
}

/// 整体扫描结果
#[derive(Debug)]
pub struct ScanResult {
    #[allow(dead_code)]
    pub targets: Vec<PathBuf>,
    pub total_files_scanned: usize,
    pub locked_files: Vec<FileLockInfo>,
    pub errors: Vec<ScanError>,
    pub elapsed: Duration,
}

/// 扫描过程中的非致命错误
#[derive(Debug)]
pub struct ScanError {
    pub path: PathBuf,
    pub reason: String,
}

impl std::fmt::Display for ProcessInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {} ({})", self.pid, self.name, self.lock_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_type_display_english() {
        assert_eq!(LockType::FileHandle.to_string(), "File Handle");
        assert_eq!(LockType::WorkingDir.to_string(), "Working Dir");
        assert_eq!(LockType::Executable.to_string(), "Executable");
        assert_eq!(LockType::MemoryMap.to_string(), "Memory Map");
        assert_eq!(LockType::FileLock.to_string(), "File Lock");
        assert_eq!(LockType::DirHandle.to_string(), "Dir Handle");
        assert_eq!(LockType::Other("WMI".to_string()).to_string(), "WMI");
    }

    #[test]
    fn is_blocking_dir_handle_explorer() {
        // explorer.exe 的目录句柄不阻塞
        let proc = ProcessInfo::new(
            100,
            "explorer.exe".to_string(),
            LockType::DirHandle,
            None,
            None,
        );
        assert!(
            !proc.is_blocking(),
            "explorer.exe DirHandle should not be blocking"
        );
        assert!(!proc.blocking, "blocking field should be auto-set to false");
    }

    #[test]
    fn is_blocking_dir_handle_other_process() {
        // 所有进程的目录句柄都是非阻塞的（共享访问，不阻止文件操作）
        let proc = ProcessInfo::new(100, "code.exe".to_string(), LockType::DirHandle, None, None);
        assert!(
            !proc.is_blocking(),
            "DirHandle should always be non-blocking"
        );
        assert!(
            !proc.blocking,
            "blocking field should be auto-set to false for DirHandle"
        );
    }

    #[test]
    fn is_blocking_explorer_file_handle() {
        let proc = ProcessInfo::new(
            100,
            "explorer.exe".to_string(),
            LockType::FileHandle,
            None,
            None,
        );
        assert!(
            !proc.is_blocking(),
            "explorer.exe FileHandle should not be blocking"
        );
        assert!(!proc.blocking, "blocking field should be auto-set to false");
    }

    #[test]
    fn is_blocking_search_indexer() {
        let proc = ProcessInfo::new(
            200,
            "SearchIndexer.exe".to_string(),
            LockType::FileHandle,
            None,
            None,
        );
        assert!(!proc.is_blocking(), "SearchIndexer should not be blocking");
    }

    #[test]
    fn is_blocking_normal_process() {
        let proc = ProcessInfo::new(
            300,
            "notepad.exe".to_string(),
            LockType::FileHandle,
            Some("notepad.exe C:\\test.txt".to_string()),
            Some("user".to_string()),
        );
        assert!(proc.is_blocking(), "Normal process should be blocking");
        assert!(proc.blocking, "blocking field should be auto-set to true");
    }

    #[test]
    fn is_blocking_search_protocol() {
        let proc = ProcessInfo::new(
            400,
            "SearchProtocolHost.exe".to_string(),
            LockType::FileHandle,
            None,
            None,
        );
        assert!(
            !proc.is_blocking(),
            "SearchProtocolHost should not be blocking"
        );
    }

    #[test]
    fn is_blocking_windows_defender() {
        let proc = ProcessInfo::new(
            500,
            "MsMpEng.exe".to_string(),
            LockType::FileHandle,
            None,
            None,
        );
        assert!(
            !proc.is_blocking(),
            "Windows Defender should not be blocking"
        );
    }

    #[test]
    fn is_blocking_thumbnail_host() {
        let proc = ProcessInfo::new(
            600,
            "ThumbnailExtractionHost.exe".to_string(),
            LockType::FileHandle,
            None,
            None,
        );
        assert!(
            !proc.is_blocking(),
            "ThumbnailExtractionHost should not be blocking"
        );
    }

    #[test]
    fn process_info_display() {
        let proc = ProcessInfo::new(
            1234,
            "test.exe".to_string(),
            LockType::FileHandle,
            None,
            None,
        );
        assert_eq!(proc.to_string(), "[1234] test.exe (File Handle)");
    }

    #[test]
    fn new_auto_sets_blocking_field() {
        // 验证 ProcessInfo::new 自动正确设置 blocking 字段
        let blocking_proc = ProcessInfo::new(
            1,
            "notepad.exe".to_string(),
            LockType::FileHandle,
            None,
            None,
        );
        assert!(blocking_proc.blocking);

        let non_blocking_proc = ProcessInfo::new(
            2,
            "explorer.exe".to_string(),
            LockType::FileHandle,
            None,
            None,
        );
        assert!(!non_blocking_proc.blocking);

        let non_blocking_mac =
            ProcessInfo::new(3, "Finder".to_string(), LockType::FileHandle, None, None);
        assert!(!non_blocking_mac.blocking);

        let non_blocking_linux =
            ProcessInfo::new(4, "nautilus".to_string(), LockType::DirHandle, None, None);
        assert!(!non_blocking_linux.blocking);
    }
}
