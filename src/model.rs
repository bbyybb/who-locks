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
        // 注意：在 Windows 上，WorkingDir（进程工作目录）会阻止删除该目录，不受此规则影响。
        if self.lock_type == LockType::DirHandle {
            return false;
        }

        // ── macOS / Linux：Unix 文件系统语义 ──
        // Unix 系统下，文件/目录被进程打开时 OS 不阻止 unlink/rename/move：
        //   - FileHandle: open() 不阻止删除，删除后 inode 保留直到 fd 关闭
        //   - WorkingDir: 进程 cwd 不阻止父目录删除
        //   - Executable: 运行中的二进制可以被删除/替换
        //   - MemoryMap: mmap 映射的文件可以被删除
        // 只有 FileLock（flock/fcntl）才是真正的协作锁，可能阻止操作。
        // 因此在 macOS/Linux 上，除 FileLock 外全部标记为非阻塞。
        #[cfg(not(target_os = "windows"))]
        {
            if self.lock_type != LockType::FileLock {
                return false;
            }
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
        // Windows: FileHandle 阻塞；macOS/Linux: FileHandle 不阻塞（Unix 语义）
        #[cfg(target_os = "windows")]
        {
            assert!(
                proc.is_blocking(),
                "Normal process should be blocking on Windows"
            );
            assert!(proc.blocking, "blocking field should be auto-set to true");
        }
        #[cfg(not(target_os = "windows"))]
        {
            assert!(!proc.is_blocking(), "Unix: FileHandle is non-blocking");
            assert!(!proc.blocking);
        }
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
        // Windows: FileHandle 阻塞；macOS/Linux: FileHandle 不阻塞
        #[cfg(target_os = "windows")]
        assert!(blocking_proc.blocking);
        #[cfg(not(target_os = "windows"))]
        assert!(!blocking_proc.blocking);

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

    // --- 额外的 LockType 测试 ---

    #[test]
    fn lock_type_other_display() {
        assert_eq!(LockType::Other("Custom".to_string()).to_string(), "Custom");
        assert_eq!(LockType::Other("WMI".to_string()).to_string(), "WMI");
        assert_eq!(LockType::Other("".to_string()).to_string(), "");
    }

    #[test]
    fn lock_type_equality() {
        assert_eq!(LockType::FileHandle, LockType::FileHandle);
        assert_ne!(LockType::FileHandle, LockType::MemoryMap);
        assert_eq!(
            LockType::Other("WMI".to_string()),
            LockType::Other("WMI".to_string())
        );
        assert_ne!(
            LockType::Other("A".to_string()),
            LockType::Other("B".to_string())
        );
    }

    #[test]
    fn lock_type_clone() {
        let lt = LockType::FileHandle;
        let cloned = lt.clone();
        assert_eq!(lt, cloned);

        let lt2 = LockType::Other("test".to_string());
        let cloned2 = lt2.clone();
        assert_eq!(lt2, cloned2);
    }

    // --- is_blocking 详细覆盖 ---

    #[test]
    fn is_blocking_working_dir_platform_aware() {
        let proc = ProcessInfo::new(
            100,
            "code.exe".to_string(),
            LockType::WorkingDir,
            None,
            None,
        );
        // Windows: WorkingDir 会阻止目录删除
        // macOS/Linux: WorkingDir 不阻止目录删除（Unix 语义）
        #[cfg(target_os = "windows")]
        assert!(
            proc.is_blocking(),
            "WorkingDir should be blocking on Windows"
        );
        #[cfg(not(target_os = "windows"))]
        assert!(!proc.is_blocking(), "Unix: WorkingDir is non-blocking");
    }

    #[test]
    fn is_blocking_self_process() {
        // 自身进程不应阻塞
        let self_pid = std::process::id();
        let proc = ProcessInfo::new(
            self_pid,
            "test_process".to_string(),
            LockType::FileHandle,
            None,
            None,
        );
        assert!(!proc.is_blocking(), "Self process should not be blocking");
    }

    #[test]
    fn is_blocking_macos_finder_case_insensitive() {
        // macOS Finder（大小写不敏感）
        for name in ["Finder", "finder", "FINDER"] {
            let proc = ProcessInfo::new(100, name.to_string(), LockType::FileHandle, None, None);
            assert!(!proc.is_blocking(), "{} should not be blocking", name);
        }
    }

    #[test]
    fn is_blocking_macos_spotlight_processes() {
        // macOS Spotlight 相关进程
        for name in [
            "mds",
            "mds_stores",
            "mdworker",
            "mdworker_shared",
            "fseventsd",
        ] {
            let proc = ProcessInfo::new(100, name.to_string(), LockType::FileHandle, None, None);
            assert!(!proc.is_blocking(), "{} should not be blocking", name);
        }
    }

    #[test]
    fn is_blocking_linux_file_managers() {
        // Linux 桌面文件管理器
        for name in [
            "tracker-miner-f",
            "tracker-miner-fs-3",
            "baloo_file",
            "nautilus",
            "dolphin",
            "thunar",
        ] {
            let proc = ProcessInfo::new(100, name.to_string(), LockType::FileHandle, None, None);
            assert!(!proc.is_blocking(), "{} should not be blocking", name);
        }
    }

    #[test]
    fn is_blocking_windows_defender_variants() {
        for name in ["MsMpEng.exe", "msmpeng.exe", "MpCmdRun.exe", "mpcmdrun.exe"] {
            let proc = ProcessInfo::new(100, name.to_string(), LockType::FileHandle, None, None);
            assert!(!proc.is_blocking(), "{} should not be blocking", name);
        }
    }

    #[test]
    fn is_blocking_prevhost() {
        let proc = ProcessInfo::new(
            100,
            "prevhost.exe".to_string(),
            LockType::FileHandle,
            None,
            None,
        );
        assert!(!proc.is_blocking(), "prevhost should not be blocking");
    }

    // --- ProcessInfo 序列化测试 ---

    #[test]
    fn process_info_serializes_to_json() {
        let proc = ProcessInfo::new(
            1234,
            "test.exe".to_string(),
            LockType::FileHandle,
            Some("test.exe arg".to_string()),
            Some("user".to_string()),
        );
        let json = serde_json::to_value(&proc).unwrap();
        assert_eq!(json["pid"], 1234);
        assert_eq!(json["name"], "test.exe");
        assert_eq!(json["cmdline"], "test.exe arg");
        assert_eq!(json["user"], "user");
        // Windows: FileHandle 阻塞；macOS/Linux: 不阻塞
        #[cfg(target_os = "windows")]
        assert_eq!(json["blocking"], true);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(json["blocking"], false);
    }

    #[test]
    fn process_info_serializes_optional_fields() {
        let proc = ProcessInfo::new(1, "test".to_string(), LockType::MemoryMap, None, None);
        let json = serde_json::to_value(&proc).unwrap();
        assert!(json["cmdline"].is_null());
        assert!(json["user"].is_null());
    }

    #[test]
    fn file_lock_info_serializes() {
        let info = FileLockInfo {
            path: std::path::PathBuf::from("/test/file.txt"),
            lockers: vec![ProcessInfo::new(
                100,
                "proc".to_string(),
                LockType::FileHandle,
                None,
                None,
            )],
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["path"], "/test/file.txt");
        assert!(json["lockers"].is_array());
        assert_eq!(json["lockers"][0]["pid"], 100);
    }

    #[test]
    fn lock_type_serializes() {
        let json = serde_json::to_value(&LockType::FileHandle).unwrap();
        assert_eq!(json, "FileHandle");

        let json = serde_json::to_value(&LockType::Other("WMI".to_string())).unwrap();
        assert!(json.is_object() || json.is_string());
    }

    // --- ScanResult 和 ScanError ---

    #[test]
    fn scan_error_display() {
        let err = ScanError {
            path: std::path::PathBuf::from("/test"),
            reason: "permission denied".to_string(),
        };
        assert_eq!(err.path.display().to_string(), "/test");
        assert_eq!(err.reason, "permission denied");
    }

    #[test]
    fn process_info_display_with_lock_type() {
        let proc = ProcessInfo::new(42, "vim".to_string(), LockType::WorkingDir, None, None);
        assert_eq!(proc.to_string(), "[42] vim (Working Dir)");
    }

    // --- Unix 平台特有测试 ---

    /// macOS/Linux 上，只有 FileLock 是真正阻塞的
    #[test]
    #[cfg(not(target_os = "windows"))]
    fn unix_only_file_lock_is_blocking() {
        let flock = ProcessInfo::new(100, "proc".to_string(), LockType::FileLock, None, None);
        assert!(flock.is_blocking(), "Unix: FileLock should be blocking");
    }

    /// macOS/Linux 上，所有其他锁类型都不阻塞
    #[test]
    #[cfg(not(target_os = "windows"))]
    fn unix_non_file_lock_types_are_non_blocking() {
        let types = [
            LockType::FileHandle,
            LockType::WorkingDir,
            LockType::Executable,
            LockType::MemoryMap,
            LockType::DirHandle,
        ];
        for lt in types {
            let proc = ProcessInfo::new(100, "any_process".to_string(), lt.clone(), None, None);
            assert!(!proc.is_blocking(), "Unix: {:?} should be non-blocking", lt);
        }
    }

    /// Windows 上，FileLock 同样是阻塞的
    #[test]
    #[cfg(target_os = "windows")]
    fn windows_file_lock_is_blocking() {
        let flock = ProcessInfo::new(100, "proc".to_string(), LockType::FileLock, None, None);
        assert!(flock.is_blocking(), "Windows: FileLock should be blocking");
    }

    /// Windows 上，普通进程的 FileHandle / WorkingDir / Executable / MemoryMap 是阻塞的
    #[test]
    #[cfg(target_os = "windows")]
    fn windows_file_handle_is_blocking() {
        let types = [
            LockType::FileHandle,
            LockType::WorkingDir,
            LockType::Executable,
            LockType::MemoryMap,
        ];
        for lt in types {
            let proc = ProcessInfo::new(100, "notepad.exe".to_string(), lt.clone(), None, None);
            assert!(proc.is_blocking(), "Windows: {:?} should be blocking", lt);
        }
    }
}
