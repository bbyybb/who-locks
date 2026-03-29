use crate::error::Error;
use crate::model::{FileLockInfo, LockType, ProcessInfo};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 缓存 /etc/passwd 内容，避免每次 UID 解析都重新读取文件
static PASSWD_CACHE: OnceLock<Vec<(u32, String)>> = OnceLock::new();

fn load_passwd_cache() -> &'static Vec<(u32, String)> {
    PASSWD_CACHE.get_or_init(|| {
        let mut entries = Vec::new();
        if let Ok(content) = std::fs::read_to_string("/etc/passwd") {
            for line in content.lines() {
                let fields: Vec<&str> = line.split(':').collect();
                if fields.len() >= 3 {
                    if let Ok(uid) = fields[2].parse::<u32>() {
                        entries.push((uid, fields[0].to_string()));
                    }
                }
            }
        }
        entries
    })
}

pub struct LinuxDetector;

impl LinuxDetector {
    pub fn new() -> Self {
        Self
    }

    /// 构建反转索引：一次遍历 /proc，检测所有占用类型
    /// 包括：fd、cwd、exe、mmap (map_files)、flock
    fn build_fd_index(
        &self,
        target_paths: &std::collections::HashSet<PathBuf>,
    ) -> HashMap<PathBuf, Vec<ProcessInfo>> {
        let mut index: HashMap<PathBuf, Vec<ProcessInfo>> = HashMap::new();

        let proc_dir = match std::fs::read_dir("/proc") {
            Ok(d) => d,
            Err(_) => return index,
        };

        // 预先解析 /proc/locks 获取 flock 信息
        let flock_map = parse_proc_locks(target_paths);

        for entry in proc_dir.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            let pid: u32 = match name_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            // 延迟加载进程基本信息
            let mut proc_base: Option<(String, Option<String>, Option<String>)> = None;
            let mut ensure_base = |pid: u32| -> (String, Option<String>, Option<String>) {
                if let Some(ref base) = proc_base {
                    return base.clone();
                }
                let base = read_process_base(pid);
                proc_base = Some(base.clone());
                base
            };

            // 1. 检查 cwd（当前工作目录）
            if let Ok(cwd) = std::fs::read_link(format!("/proc/{}/cwd", pid)) {
                if target_paths.contains(&cwd) {
                    let (name, cmdline, user) = ensure_base(pid);
                    index.entry(cwd).or_default().push(ProcessInfo::new(
                        pid,
                        name,
                        LockType::WorkingDir,
                        cmdline,
                        user,
                    ));
                }
            }

            // 2. 检查 exe（可执行文件本身）
            if let Ok(exe) = std::fs::read_link(format!("/proc/{}/exe", pid)) {
                if target_paths.contains(&exe) {
                    let (name, cmdline, user) = ensure_base(pid);
                    index.entry(exe).or_default().push(ProcessInfo::new(
                        pid,
                        name,
                        LockType::Executable,
                        cmdline,
                        user,
                    ));
                }
            }

            // 3. 检查所有 fd（文件描述符，包括文件和目录句柄）
            let fd_dir = format!("/proc/{}/fd", pid);
            if let Ok(fd_entries) = std::fs::read_dir(&fd_dir) {
                for fd_entry in fd_entries.flatten() {
                    let link_target = match std::fs::read_link(fd_entry.path()) {
                        Ok(t) => t,
                        Err(_) => continue,
                    };

                    if target_paths.contains(&link_target) {
                        let (name, cmdline, user) = ensure_base(pid);
                        // 判断是文件还是目录句柄
                        let lock_type = if link_target.is_dir() {
                            LockType::DirHandle
                        } else {
                            LockType::FileHandle
                        };
                        index
                            .entry(link_target)
                            .or_default()
                            .push(ProcessInfo::new(pid, name, lock_type, cmdline, user));
                    }
                }
            }

            // 4. 检查 mmap 内存映射（/proc/pid/map_files/）
            let map_dir = format!("/proc/{}/map_files", pid);
            if let Ok(map_entries) = std::fs::read_dir(&map_dir) {
                for map_entry in map_entries.flatten() {
                    let link_target = match std::fs::read_link(map_entry.path()) {
                        Ok(t) => t,
                        Err(_) => continue,
                    };

                    if target_paths.contains(&link_target) {
                        // 避免和 fd 重复：如果已经有 FileHandle 类型的记录就跳过
                        let already_has = index
                            .get(&link_target)
                            .map(|v| {
                                v.iter()
                                    .any(|p| p.pid == pid && p.lock_type == LockType::FileHandle)
                            })
                            .unwrap_or(false);

                        if !already_has {
                            let (name, cmdline, user) = ensure_base(pid);
                            index.entry(link_target).or_default().push(ProcessInfo::new(
                                pid,
                                name,
                                LockType::MemoryMap,
                                cmdline,
                                user,
                            ));
                        }
                    }
                }
            }

            // 5. 检查 flock（文件锁）
            if let Some(locked_paths) = flock_map.get(&pid) {
                for locked_path in locked_paths {
                    if target_paths.contains(locked_path) {
                        let (name, cmdline, user) = ensure_base(pid);
                        index
                            .entry(locked_path.clone())
                            .or_default()
                            .push(ProcessInfo::new(
                                pid,
                                name,
                                LockType::FileLock,
                                cmdline,
                                user,
                            ));
                    }
                }
            }
        }

        // 去重：同一进程对同一文件可能有多种占用方式，保留不同 lock_type
        for lockers in index.values_mut() {
            lockers.sort_by(|a, b| {
                a.pid
                    .cmp(&b.pid)
                    .then_with(|| format!("{}", a.lock_type).cmp(&format!("{}", b.lock_type)))
            });
            lockers.dedup_by(|a, b| a.pid == b.pid && a.lock_type == b.lock_type);
        }

        index
    }
}

impl super::LockDetector for LinuxDetector {
    fn detect_file(&self, path: &Path) -> Result<Vec<ProcessInfo>, Error> {
        let canonical = path.canonicalize().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::PathNotFound(path.to_path_buf())
            } else {
                Error::Io(e)
            }
        })?;

        let mut target_set = std::collections::HashSet::new();
        target_set.insert(canonical.clone());

        let index = self.build_fd_index(&target_set);
        Ok(index.get(&canonical).cloned().unwrap_or_default())
    }

    fn detect_batch(&self, paths: &[&Path]) -> Result<Vec<FileLockInfo>, Error> {
        let mut target_set = std::collections::HashSet::new();
        let mut canonical_map: HashMap<PathBuf, PathBuf> = HashMap::new();

        for path in paths {
            if let Ok(canonical) = path.canonicalize() {
                target_set.insert(canonical.clone());
                canonical_map.insert(canonical, path.to_path_buf());
            }
        }

        let index = self.build_fd_index(&target_set);

        let mut results = Vec::new();
        for (canonical, lockers) in index {
            if !lockers.is_empty() {
                let original = canonical_map.get(&canonical).unwrap_or(&canonical);
                results.push(FileLockInfo {
                    path: original.clone(),
                    lockers,
                });
            }
        }

        Ok(results)
    }

    fn platform_name(&self) -> &'static str {
        "Linux (/proc: fd + cwd + exe + mmap + flock)"
    }
}

/// 解析 /proc/locks 获取 flock/fcntl 锁信息
/// 通过 inode 精确匹配目标文件，避免误报
/// 返回 pid -> Vec<PathBuf> 映射
fn parse_proc_locks(
    target_paths: &std::collections::HashSet<PathBuf>,
) -> HashMap<u32, Vec<PathBuf>> {
    let mut result: HashMap<u32, Vec<PathBuf>> = HashMap::new();

    let content = match std::fs::read_to_string("/proc/locks") {
        Ok(c) => c,
        Err(_) => return result,
    };

    // 预先构建目标文件的 inode 映射: (major, minor, inode) -> PathBuf
    // 使用 (major, minor) 而非组合 dev_t，避免 makedev 编码差异
    let mut inode_map: HashMap<(u64, u64, u64), PathBuf> = HashMap::new();
    for path in target_paths {
        if let Ok(meta) = std::fs::metadata(path) {
            use std::os::linux::fs::MetadataExt;
            let dev = meta.st_dev();
            let major = ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff);
            let minor = (dev & 0xff) | ((dev >> 12) & !0xff);
            inode_map.insert((major, minor, meta.st_ino()), path.clone());
        }
    }

    if inode_map.is_empty() {
        return result;
    }

    // /proc/locks 格式:
    // 1: FLOCK  ADVISORY  WRITE 12345 08:01:1234567 0 EOF
    // 字段: id type advisory rw pid major:minor:inode start end
    for line in content.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }

        let pid: u32 = match fields[4].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // 解析 major:minor:inode 字段
        if let Some(inode_info) = fields.get(5) {
            let parts: Vec<&str> = inode_info.split(':').collect();
            if parts.len() == 3 {
                let major: u64 = parts[0].parse().unwrap_or(0);
                let minor: u64 = parts[1].parse().unwrap_or(0);
                let inode: u64 = parts[2].parse().unwrap_or(0);

                // 通过 (major, minor, inode) 精确匹配目标文件
                // 直接使用 major/minor 对比，避免 makedev 编码差异
                if let Some(path) = inode_map.get(&(major, minor, inode)) {
                    result.entry(pid).or_default().push(path.clone());
                }
            }
        }
    }

    // 去重
    for paths in result.values_mut() {
        paths.sort();
        paths.dedup();
    }

    result
}

/// 目录级深度扫描：遍历 /proc 查找所有打开了 target 目录下文件的进程
/// 无需预先枚举目录内文件，直接通过路径前缀匹配找到所有占用
/// 类似 Windows 的 handle.exe 目录扫描
pub fn detect_deep(target: &Path) -> Result<Vec<FileLockInfo>, Error> {
    let canonical = target.canonicalize().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::PathNotFound(target.to_path_buf())
        } else {
            Error::Io(e)
        }
    })?;

    // 单文件：直接用 build_fd_index
    if canonical.is_file() {
        let det = LinuxDetector::new();
        let mut target_set = std::collections::HashSet::new();
        target_set.insert(canonical.clone());
        let index = det.build_fd_index(&target_set);
        return Ok(index
            .into_iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(path, lockers)| FileLockInfo { path, lockers })
            .collect());
    }

    // 目录：遍历 /proc 查找前缀匹配
    let canonical_str = canonical.to_string_lossy().to_string();
    let prefix = if canonical_str.ends_with('/') {
        canonical_str.clone()
    } else {
        format!("{}/", canonical_str)
    };

    let proc_dir = match std::fs::read_dir("/proc") {
        Ok(d) => d,
        Err(_) => return Ok(Vec::new()),
    };

    // 预解析 /proc/locks
    // 无法预知哪些文件被锁，先收集所有锁信息，后续过滤
    let all_locks = parse_proc_locks_all();

    let mut path_map: HashMap<PathBuf, Vec<ProcessInfo>> = HashMap::new();

    for entry in proc_dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let pid: u32 = match name_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let mut proc_base: Option<(String, Option<String>, Option<String>)> = None;
        let mut ensure_base = |pid: u32| -> (String, Option<String>, Option<String>) {
            if let Some(ref base) = proc_base {
                return base.clone();
            }
            let base = read_process_base(pid);
            proc_base = Some(base.clone());
            base
        };

        // 检查 cwd
        if let Ok(cwd) = std::fs::read_link(format!("/proc/{}/cwd", pid)) {
            if cwd == canonical || cwd.starts_with(&prefix) {
                let (pname, cmdline, user) = ensure_base(pid);
                path_map.entry(cwd).or_default().push(ProcessInfo::new(
                    pid,
                    pname,
                    LockType::WorkingDir,
                    cmdline,
                    user,
                ));
            }
        }

        // 检查 exe
        if let Ok(exe) = std::fs::read_link(format!("/proc/{}/exe", pid)) {
            if exe == canonical || exe.starts_with(&prefix) {
                let (pname, cmdline, user) = ensure_base(pid);
                path_map.entry(exe).or_default().push(ProcessInfo::new(
                    pid,
                    pname,
                    LockType::Executable,
                    cmdline,
                    user,
                ));
            }
        }

        // 检查所有 fd
        let fd_dir = format!("/proc/{}/fd", pid);
        if let Ok(fd_entries) = std::fs::read_dir(&fd_dir) {
            for fd_entry in fd_entries.flatten() {
                if let Ok(link_target) = std::fs::read_link(fd_entry.path()) {
                    if link_target == canonical || link_target.starts_with(&prefix) {
                        let (pname, cmdline, user) = ensure_base(pid);
                        let lock_type = if link_target.is_dir() {
                            LockType::DirHandle
                        } else {
                            LockType::FileHandle
                        };
                        path_map
                            .entry(link_target)
                            .or_default()
                            .push(ProcessInfo::new(pid, pname, lock_type, cmdline, user));
                    }
                }
            }
        }

        // 检查 mmap
        let map_dir = format!("/proc/{}/map_files", pid);
        if let Ok(map_entries) = std::fs::read_dir(&map_dir) {
            for map_entry in map_entries.flatten() {
                if let Ok(link_target) = std::fs::read_link(map_entry.path()) {
                    if link_target == canonical || link_target.starts_with(&prefix) {
                        // 避免和 fd 重复
                        let already_has = path_map
                            .get(&link_target)
                            .map(|v| {
                                v.iter()
                                    .any(|p| p.pid == pid && p.lock_type == LockType::FileHandle)
                            })
                            .unwrap_or(false);
                        if !already_has {
                            let (pname, cmdline, user) = ensure_base(pid);
                            path_map
                                .entry(link_target)
                                .or_default()
                                .push(ProcessInfo::new(
                                    pid,
                                    pname,
                                    LockType::MemoryMap,
                                    cmdline,
                                    user,
                                ));
                        }
                    }
                }
            }
        }

        // 检查 flock
        if let Some(locked_paths) = all_locks.get(&pid) {
            for locked_path in locked_paths {
                if *locked_path == canonical || locked_path.starts_with(&prefix) {
                    let (pname, cmdline, user) = ensure_base(pid);
                    path_map
                        .entry(locked_path.clone())
                        .or_default()
                        .push(ProcessInfo::new(
                            pid,
                            pname,
                            LockType::FileLock,
                            cmdline,
                            user,
                        ));
                }
            }
        }
    }

    // 去重
    for lockers in path_map.values_mut() {
        lockers.sort_by(|a, b| {
            a.pid
                .cmp(&b.pid)
                .then_with(|| format!("{}", a.lock_type).cmp(&format!("{}", b.lock_type)))
        });
        lockers.dedup_by(|a, b| a.pid == b.pid && a.lock_type == b.lock_type);
    }

    Ok(path_map
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(path, lockers)| FileLockInfo { path, lockers })
        .collect())
}

/// 解析 /proc/locks 获取所有 flock 信息（不限于目标路径集合）
/// 返回 pid -> Vec<PathBuf> 映射，通过 inode 反查文件路径
fn parse_proc_locks_all() -> HashMap<u32, Vec<PathBuf>> {
    let mut result: HashMap<u32, Vec<PathBuf>> = HashMap::new();

    let content = match std::fs::read_to_string("/proc/locks") {
        Ok(c) => c,
        Err(_) => return result,
    };

    for line in content.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }

        let pid: u32 = match fields[4].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        if let Some(inode_info) = fields.get(5) {
            let parts: Vec<&str> = inode_info.split(':').collect();
            if parts.len() == 3 {
                let major: u64 = parts[0].parse().unwrap_or(0);
                let minor: u64 = parts[1].parse().unwrap_or(0);
                let inode: u64 = parts[2].parse().unwrap_or(0);

                // 尝试通过 /proc/pid/fd 找到 inode 对应的文件路径
                if let Some(path) = find_path_by_inode(pid, major, minor, inode) {
                    result.entry(pid).or_default().push(path);
                }
            }
        }
    }

    for paths in result.values_mut() {
        paths.sort();
        paths.dedup();
    }

    result
}

/// 在 /proc/pid/fd 中查找匹配 (major, minor, inode) 的文件路径
fn find_path_by_inode(pid: u32, major: u64, minor: u64, inode: u64) -> Option<PathBuf> {
    use std::os::linux::fs::MetadataExt;

    let fd_dir = format!("/proc/{}/fd", pid);
    let entries = std::fs::read_dir(&fd_dir).ok()?;

    for entry in entries.flatten() {
        if let Ok(link_target) = std::fs::read_link(entry.path()) {
            if let Ok(meta) = std::fs::metadata(&link_target) {
                let dev = meta.st_dev();
                let dev_major = ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff);
                let dev_minor = (dev & 0xff) | ((dev >> 12) & !0xff);
                if dev_major == major && dev_minor == minor && meta.st_ino() == inode {
                    return Some(link_target);
                }
            }
        }
    }

    None
}

/// 读取进程基本信息: (name, cmdline, user)
fn read_process_base(pid: u32) -> (String, Option<String>, Option<String>) {
    let name = std::fs::read_to_string(format!("/proc/{}/comm", pid))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let cmdline = std::fs::read_to_string(format!("/proc/{}/cmdline", pid))
        .ok()
        .map(|s| s.replace('\0', " ").trim().to_string())
        .filter(|s| !s.is_empty());

    let user = read_proc_user(pid);

    (name, cmdline, user)
}

/// 从 /proc/[pid]/status 读取 Uid 并解析用户名
fn read_proc_user(pid: u32) -> Option<String> {
    let status = std::fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            let uid_str = rest.split_whitespace().next()?;
            let uid: u32 = uid_str.parse().ok()?;
            return resolve_uid(uid);
        }
    }
    None
}

/// 将 UID 解析为用户名（使用缓存的 /etc/passwd 内容）
fn resolve_uid(uid: u32) -> Option<String> {
    let cache = load_passwd_cache();
    for (cached_uid, name) in cache {
        if *cached_uid == uid {
            return Some(name.clone());
        }
    }
    Some(uid.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// detect_deep 对不存在的路径应返回 PathNotFound 错误
    #[test]
    fn detect_deep_nonexistent_path() {
        let result = detect_deep(Path::new("/nonexistent_path_who_locks_test"));
        assert!(result.is_err(), "Should fail for non-existent path");
    }

    /// detect_deep 对 /tmp 目录应正常返回（可能为空）
    #[test]
    fn detect_deep_tmp_dir() {
        let result = detect_deep(Path::new("/tmp"));
        assert!(result.is_ok(), "Should succeed for /tmp directory");
    }

    /// detect_deep 对单文件应正常返回
    #[test]
    fn detect_deep_existing_file() {
        let result = detect_deep(Path::new("/etc/hostname"));
        // /etc/hostname 可能不存在于所有发行版，如果不存在则跳过
        if Path::new("/etc/hostname").exists() {
            assert!(result.is_ok(), "Should succeed for existing file");
        }
    }

    /// parse_proc_locks_all 应返回有效的 HashMap（不崩溃）
    #[test]
    fn parse_proc_locks_all_no_panic() {
        let result = parse_proc_locks_all();
        // 只确认不崩溃，结果可能为空（无锁）或非空
        let _ = result;
    }

    /// LinuxDetector::detect_file 对不存在的路径应返回错误
    #[test]
    fn detector_file_not_found() {
        let det = LinuxDetector::new();
        let result = det.detect_file(Path::new("/nonexistent_who_locks_test_file"));
        assert!(result.is_err());
    }

    /// LinuxDetector::detect_batch 空输入应返回空结果
    #[test]
    fn detector_batch_empty() {
        let det = LinuxDetector::new();
        let result = det.detect_batch(&[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    /// resolve_uid 对 root (UID 0) 应返回 "root"
    #[test]
    fn resolve_uid_root() {
        let result = resolve_uid(0);
        assert_eq!(result, Some("root".to_string()));
    }

    /// resolve_uid 对不存在的 UID 应返回数字字符串
    #[test]
    fn resolve_uid_unknown() {
        let result = resolve_uid(99999);
        assert_eq!(result, Some("99999".to_string()));
    }
}
