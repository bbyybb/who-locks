use crate::error::Error;
use crate::model::{FileLockInfo, LockType, ProcessInfo};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use sysinfo::System;

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
use windows_sys::Win32::System::RestartManager::{
    RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY,
    RM_PROCESS_INFO,
};

pub struct WindowsDetector {
    /// 缓存的进程信息，避免在同一扫描会话中重复创建 System 对象
    sys_cache: Mutex<Option<System>>,
}

impl WindowsDetector {
    pub fn new() -> Self {
        Self {
            sys_cache: Mutex::new(None),
        }
    }

    /// 使批量扫描结束后释放缓存的进程信息，减少内存占用
    fn clear_sys_cache(&self) {
        if let Ok(mut cache) = self.sys_cache.lock() {
            *cache = None;
        }
    }

    /// 获取缓存的 System 的进程信息查询结果
    /// 返回 (cmdline, user) 对
    fn lookup_process_info(&self, pid: u32) -> (Option<String>, Option<String>) {
        let mut cache = match self.sys_cache.lock() {
            Ok(c) => c,
            Err(_) => return (None, None),
        };
        if cache.is_none() {
            let mut sys = System::new();
            sys.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::All,
                true,
                sysinfo::ProcessRefreshKind::everything(),
            );
            *cache = Some(sys);
        }
        let sys = cache.as_ref().unwrap();
        if let Some(proc) = sys.process(sysinfo::Pid::from_u32(pid)) {
            let cmd: Vec<String> = proc
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_string())
                .collect();
            let cmd_str = cmd.join(" ");
            let cmdline = if cmd_str.is_empty() {
                None
            } else {
                Some(cmd_str)
            };
            let user = proc.user_id().map(|uid| uid.to_string());
            (cmdline, user)
        } else {
            (None, None)
        }
    }

    /// 使用 Restart Manager 检测单个文件的占用进程
    fn detect_with_rm(&self, path: &Path) -> Result<Vec<ProcessInfo>, Error> {
        let path_wide = to_wide_string(path);

        unsafe {
            let mut session: u32 = 0;
            let mut session_key = [0u16; (CCH_RM_SESSION_KEY as usize) + 1];
            let ret = RmStartSession(&mut session, 0, session_key.as_mut_ptr());
            if ret != ERROR_SUCCESS {
                return Err(Error::PlatformApi {
                    api: "RmStartSession",
                    code: ret as i64,
                    detail: "Failed to create Restart Manager session".to_string(),
                });
            }

            let _guard = RmSessionGuard(session);

            let file_ptr = path_wide.as_ptr();
            let files = [file_ptr];
            let ret = RmRegisterResources(
                session,
                1,
                files.as_ptr(),
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
            );
            if ret != ERROR_SUCCESS {
                return Err(Error::PlatformApi {
                    api: "RmRegisterResources",
                    code: ret as i64,
                    detail: format!("Failed to register file resource: {}", path.display()),
                });
            }

            let mut n_proc_needed: u32 = 0;
            let mut n_proc: u32 = 0;
            let mut reason: u32 = 0;

            let ret = RmGetList(
                session,
                &mut n_proc_needed,
                &mut n_proc,
                std::ptr::null_mut(),
                &mut reason,
            );

            if ret != ERROR_MORE_DATA && ret != ERROR_SUCCESS {
                log::debug!("RmGetList returned {} for {}", ret, path.display());
                return Ok(Vec::new());
            }

            if n_proc_needed == 0 {
                return Ok(Vec::new());
            }

            let mut proc_infos: Vec<RM_PROCESS_INFO> =
                vec![std::mem::zeroed(); n_proc_needed as usize];
            n_proc = n_proc_needed;

            let ret = RmGetList(
                session,
                &mut n_proc_needed,
                &mut n_proc,
                proc_infos.as_mut_ptr(),
                &mut reason,
            );

            if ret != ERROR_SUCCESS {
                return Ok(Vec::new());
            }

            let mut results = Vec::new();
            for info in proc_infos.iter().take(n_proc as usize) {
                let pid = info.Process.dwProcessId;
                let app_name = wide_to_string(&info.strAppName);

                let (cmdline, user) = self.lookup_process_info(pid);

                results.push(ProcessInfo::new(
                    pid,
                    app_name,
                    LockType::FileHandle,
                    cmdline,
                    user,
                ));
            }

            Ok(results)
        }
    }

    /// 批量预筛选：检测一批文件中是否有任何文件被占用
    fn batch_has_locks(&self, paths: &[&Path]) -> bool {
        unsafe {
            let mut session: u32 = 0;
            let mut session_key = [0u16; (CCH_RM_SESSION_KEY as usize) + 1];
            if RmStartSession(&mut session, 0, session_key.as_mut_ptr()) != ERROR_SUCCESS {
                return true;
            }
            let _guard = RmSessionGuard(session);

            let wide_paths: Vec<Vec<u16>> = paths
                .iter()
                .filter_map(|p| p.canonicalize().ok())
                .map(|p| to_wide_string(&strip_extended_prefix(&p)))
                .collect();
            let ptrs: Vec<*const u16> = wide_paths.iter().map(|w| w.as_ptr()).collect();

            if ptrs.is_empty() {
                return false;
            }

            if RmRegisterResources(
                session,
                ptrs.len() as u32,
                ptrs.as_ptr(),
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
            ) != ERROR_SUCCESS
            {
                return true;
            }

            let mut n_proc_needed: u32 = 0;
            let mut n_proc: u32 = 0;
            let mut reason: u32 = 0;
            RmGetList(
                session,
                &mut n_proc_needed,
                &mut n_proc,
                std::ptr::null_mut(),
                &mut reason,
            );

            n_proc_needed > 0
        }
    }
}

impl super::LockDetector for WindowsDetector {
    fn detect_file(&self, path: &Path) -> Result<Vec<ProcessInfo>, Error> {
        // canonicalize 在 Windows 返回 \\?\ 前缀，RM 不认，需要去掉
        let canonical = path.canonicalize().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::PathNotFound(path.to_path_buf())
            } else {
                Error::Io(e)
            }
        })?;
        let clean = strip_extended_prefix(&canonical);
        self.detect_with_rm(&clean)
    }

    fn detect_batch(&self, paths: &[&Path]) -> Result<Vec<FileLockInfo>, Error> {
        const BATCH_SIZE: usize = 128;
        let mut all_results = Vec::new();

        for chunk in paths.chunks(BATCH_SIZE) {
            if !self.batch_has_locks(chunk) {
                continue;
            }

            for path in chunk {
                match self.detect_file(path) {
                    Ok(lockers) if !lockers.is_empty() => {
                        all_results.push(FileLockInfo {
                            path: path.to_path_buf(),
                            lockers,
                        });
                    }
                    _ => {}
                }
            }
        }

        // 批量操作结束后释放缓存，减少内存占用
        self.clear_sys_cache();

        Ok(all_results)
    }

    fn platform_name(&self) -> &'static str {
        "Windows (Restart Manager + Handle deep scan)"
    }
}

/// 深度句柄扫描：使用 Sysinternals handle.exe 检测所有句柄类型
/// 包括目录句柄、Section 映射等 Restart Manager 检测不到的类型
pub fn detect_deep(target: &Path) -> Result<Vec<FileLockInfo>, Error> {
    let handle_exe = match find_handle_exe() {
        Some(p) => p,
        None => return detect_with_powershell(&target.to_string_lossy()),
    };

    let target_str = target.to_string_lossy().to_string();
    detect_with_handle_exe(&handle_exe, &target_str, target)
}

/// handle.exe 解析结果：(pid, proc_name, lock_type, cmdline, user, actual_path)
struct HandleEntry {
    pid: u32,
    name: String,
    lock_type: LockType,
    cmdline: Option<String>,
    user: Option<String>,
    actual_path: Option<String>,
}

/// 对单个路径调用 handle.exe，返回持有句柄的进程列表（含实际文件路径）
fn handle_query_pids(handle_exe: &Path, target: &str, sys: &System) -> Vec<HandleEntry> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let output = match Command::new(handle_exe)
        .args(["-accepteula", "-nobanner", target])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let stdout = decode_system_output(&output.stdout);
    if stdout.contains("No matching handles found") || stdout.trim().is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(pid_pos) = line.find("pid:") {
            let proc_name = line[..pid_pos].trim().to_string();
            let after_pid = line[pid_pos + 4..].trim_start();
            let pid_end = after_pid
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after_pid.len());
            if let Ok(pid) = after_pid[..pid_end].parse::<u32>() {
                let lock_type = if line.contains("type: Section") {
                    LockType::MemoryMap
                } else if line.contains("type: Directory") {
                    LockType::DirHandle
                } else {
                    LockType::FileHandle
                };

                // 提取实际文件路径：在 "type: XXX" 之后，跳过句柄号 "HEX: "
                // handle.exe 输出格式: processname  pid: N  type: File  1C8: C:\actual\path
                let actual_path = extract_handle_path(line);

                let (cmdline, user) = if let Some(proc) = sys.process(sysinfo::Pid::from_u32(pid)) {
                    let cmd: Vec<String> = proc
                        .cmd()
                        .iter()
                        .map(|s| s.to_string_lossy().to_string())
                        .collect();
                    let cmd_str = cmd.join(" ");
                    (
                        if cmd_str.is_empty() {
                            None
                        } else {
                            Some(cmd_str)
                        },
                        proc.user_id().map(|uid| uid.to_string()),
                    )
                } else {
                    (None, None)
                };

                results.push(HandleEntry {
                    pid,
                    name: proc_name,
                    lock_type,
                    cmdline,
                    user,
                    actual_path,
                });
            }
        }
    }

    results
}

/// 从 handle.exe 输出行中提取实际文件路径
/// 格式: "processname  pid: N  type: File  1C8: C:\actual\path\file.txt"
fn extract_handle_path(line: &str) -> Option<String> {
    let type_pos = line.find("type: ")?;
    let after_type = &line[type_pos + 6..];
    let type_end = after_type.find(char::is_whitespace)?;
    let after_type_name = after_type[type_end..].trim_start();

    if let Some(colon_pos) = after_type_name.find(':') {
        let hex_part = &after_type_name[..colon_pos];
        if !hex_part.is_empty() && hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            let path = after_type_name[colon_pos + 1..].trim_start();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }

    None
}

/// handle.exe 管道输出无法正确编码非 ASCII 字符（中文变为 ?）
/// 通过遍历文件系统，将含 ? 的乱码路径还原为真实路径
/// 返回 None 表示无法还原（调用者应跳过该条目，避免显示错误的目录路径）
fn resolve_garbled_path(garbled: &str) -> Option<PathBuf> {
    if !garbled.contains('?') {
        return Some(PathBuf::from(garbled));
    }

    // 按路径分隔符拆分，逐段还原
    let mut current = PathBuf::new();

    for component in std::path::Path::new(garbled).components() {
        use std::path::Component;
        match component {
            Component::Prefix(p) => {
                current.push(p.as_os_str());
            }
            Component::RootDir => {
                current.push(std::path::MAIN_SEPARATOR_STR);
            }
            Component::Normal(seg) => {
                let seg_str = seg.to_string_lossy();
                if seg_str.contains('?') {
                    if let Some(resolved) = match_dir_entry(&current, &seg_str) {
                        current.push(resolved);
                    } else {
                        log::debug!(
                            "Could not resolve garbled segment '{}' in {}",
                            seg_str,
                            current.display()
                        );
                        return None;
                    }
                } else {
                    current.push(seg);
                }
            }
            _ => {
                current.push(component.as_os_str());
            }
        }
    }

    if current.exists() {
        Some(current)
    } else {
        log::debug!("Resolved path does not exist: {}", current.display());
        None
    }
}

/// 在 parent 目录下找到与 pattern 匹配的条目
/// 支持两种匹配策略：
///   1. 精确字符数匹配（? 作为单字符通配符）
///   2. 模糊匹配（处理 handle.exe 多字节编码乱码，如 GBK 下一个中文字变为两个 ?）
fn match_dir_entry(parent: &std::path::Path, pattern: &str) -> Option<std::ffi::OsString> {
    let entries: Vec<_> = std::fs::read_dir(parent).ok()?.flatten().collect();
    let pattern_chars: Vec<char> = pattern.chars().collect();

    // Pass 1: 精确字符数匹配（? 作为单字符通配符）
    for entry in &entries {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let name_chars: Vec<char> = name_str.chars().collect();

        if name_chars.len() == pattern_chars.len()
            && name_chars
                .iter()
                .zip(pattern_chars.iter())
                .all(|(n, p)| *p == '?' || *n == *p)
        {
            return Some(name);
        }
    }

    // Pass 2: 模糊匹配，处理多字节编码乱码
    // handle.exe 管道输出通过系统代码页（如中文 Windows 的 GBK），
    // 非 ASCII 字符的每个字节被替换为 '?'。
    // 一个 CJK 字符在 GBK 中 = 2 字节 = 2 个 '?'，导致字符数不匹配。
    // 策略：用非 '?' 的前缀和后缀作为锚点匹配。
    let suffix: String = pattern_chars
        .iter()
        .rev()
        .take_while(|&&c| c != '?')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let prefix: String = pattern_chars.iter().take_while(|&&c| c != '?').collect();

    if !suffix.is_empty() || !prefix.is_empty() {
        let mut candidates = Vec::new();
        for entry in &entries {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let ok_prefix = prefix.is_empty() || name_str.starts_with(&prefix);
            let ok_suffix = suffix.is_empty() || name_str.ends_with(&suffix);
            if ok_prefix && ok_suffix {
                candidates.push(name);
            }
        }
        if candidates.len() == 1 {
            return Some(candidates.into_iter().next().unwrap());
        }
    }

    // Pass 3: GBK 字节数估算匹配（用于全 '?' 模式，如纯中文目录名）
    // 估算规则：ASCII 字符 = 1 字节 = 1 个 '?'，CJK 字符 = 2 字节 = 2 个 '?'
    if prefix.is_empty() && suffix.is_empty() {
        let q_count = pattern_chars.len();
        let mut candidates = Vec::new();
        for entry in &entries {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let estimated_gbk_bytes: usize = name_str
                .chars()
                .map(|c| if c.is_ascii() { 1 } else { 2 })
                .sum();
            if estimated_gbk_bytes == q_count {
                candidates.push(name);
            }
        }
        if candidates.len() == 1 {
            return Some(candidates.into_iter().next().unwrap());
        }
    }

    None
}

/// 递归搜索还原：当逐段解析失败时，在目标目录下递归搜索匹配乱码文件名的文件/目录
/// 提取乱码路径的最后一段（文件名或目录名），与目标目录树中的条目进行多策略匹配
/// 仅当唯一匹配时返回结果，避免误匹配
fn resolve_by_recursive_search(garbled: &str, target_dir: &Path) -> Option<PathBuf> {
    if !target_dir.is_dir() {
        return None;
    }

    // 提取乱码路径的文件名部分（最后一段）
    let garbled_path = std::path::Path::new(garbled);
    let garbled_name = garbled_path.file_name()?.to_string_lossy();
    if !garbled_name.contains('?') {
        return None;
    }

    let pattern_chars: Vec<char> = garbled_name.chars().collect();
    let suffix: String = pattern_chars
        .iter()
        .rev()
        .take_while(|&&c| c != '?')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let prefix: String = pattern_chars.iter().take_while(|&&c| c != '?').collect();
    let q_count = pattern_chars.len();
    let has_anchors = !suffix.is_empty() || !prefix.is_empty();

    let mut candidates = Vec::new();
    const MAX_ENTRIES: usize = 10000; // 防止大目录遍历过慢
    let mut count = 0;

    for entry in walkdir::WalkDir::new(target_dir)
        .max_depth(10)
        .into_iter()
        .flatten()
    {
        count += 1;
        if count > MAX_ENTRIES {
            log::debug!(
                "Recursive search aborted: exceeded {} entries in {}",
                MAX_ENTRIES,
                target_dir.display()
            );
            break;
        }

        let name = entry.file_name().to_string_lossy();
        let name_chars: Vec<char> = name.chars().collect();

        // 策略 1: 精确字符数 + ? 通配符匹配
        if name_chars.len() == pattern_chars.len()
            && name_chars
                .iter()
                .zip(pattern_chars.iter())
                .all(|(n, p)| *p == '?' || *n == *p)
        {
            candidates.push(entry.into_path());
            continue;
        }

        // 策略 2: 前缀/后缀锚点匹配（如 .txt 扩展名）
        if has_anchors {
            let ok_p = prefix.is_empty() || name.starts_with(&prefix);
            let ok_s = suffix.is_empty() || name.ends_with(&suffix);
            if ok_p && ok_s {
                candidates.push(entry.into_path());
                continue;
            }
        }

        // 策略 3: GBK 字节数估算（全 ? 模式，如纯中文名）
        if !has_anchors {
            let est: usize = name.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum();
            if est == q_count {
                candidates.push(entry.into_path());
            }
        }
    }

    if candidates.len() == 1 {
        let found = candidates.into_iter().next().unwrap();
        log::debug!(
            "Recursive search resolved '{}' → {}",
            garbled_name,
            found.display()
        );
        Some(found)
    } else {
        log::debug!(
            "Recursive search for '{}' under {} found {} candidates (need exactly 1)",
            garbled_name,
            target_dir.display(),
            candidates.len()
        );
        None
    }
}

fn detect_with_handle_exe(
    handle_exe: &Path,
    target: &str,
    target_path: &Path,
) -> Result<Vec<FileLockInfo>, Error> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::everything(),
    );

    let entries = handle_query_pids(handle_exe, target, &sys);
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    // 按实际文件路径分组，对含 ? 的乱码路径尝试还原
    let mut path_map: std::collections::HashMap<PathBuf, Vec<ProcessInfo>> =
        std::collections::HashMap::new();

    for entry in entries {
        let file_path = match entry.actual_path.as_deref() {
            Some(p) if p.contains('?') => match resolve_garbled_path(p) {
                Some(resolved) => resolved,
                None => {
                    // 逐段解析失败，尝试在目标目录下递归搜索匹配
                    match resolve_by_recursive_search(p, target_path) {
                        Some(found) => found,
                        None => {
                            if target_path.is_file() {
                                target_path.to_path_buf()
                            } else {
                                log::debug!(
                                    "Skipping unresolvable garbled path: {} (pid={} name={})",
                                    p,
                                    entry.pid,
                                    entry.name
                                );
                                continue;
                            }
                        }
                    }
                }
            },
            Some(p) => PathBuf::from(p),
            None => {
                if target_path.is_file() {
                    // 单文件目标：锁一定在这个文件上
                    target_path.to_path_buf()
                } else {
                    // 目录目标：无法确定具体被锁文件，跳过以避免显示不明确的目录路径
                    log::debug!(
                        "Skipping handle entry without path for directory target: pid={} name={}",
                        entry.pid,
                        entry.name
                    );
                    continue;
                }
            }
        };

        // 如果路径实际是目录，修正 lock_type 为 DirHandle
        // （handle.exe 对目录也报告 type: File，需要纠正）
        let lock_type = if file_path.is_dir() && entry.lock_type == LockType::FileHandle {
            LockType::DirHandle
        } else {
            entry.lock_type
        };

        // DirHandle 进一步区分：检查是否为进程的工作目录（cwd）
        // cwd 会阻止删除/重命名该目录（WorkingDir 类型，标记为阻塞）
        // 普通 DirHandle 只是共享目录访问（如文件浏览、文件监控），不会阻塞
        let lock_type = if lock_type == LockType::DirHandle {
            let is_cwd = sys
                .process(sysinfo::Pid::from_u32(entry.pid))
                .and_then(|p| p.cwd())
                .is_some_and(|cwd| {
                    cwd.to_string_lossy()
                        .eq_ignore_ascii_case(&file_path.to_string_lossy())
                });
            if is_cwd {
                LockType::WorkingDir
            } else {
                LockType::DirHandle
            }
        } else {
            lock_type
        };

        let locker = ProcessInfo::new(entry.pid, entry.name, lock_type, entry.cmdline, entry.user);

        let lockers = path_map.entry(file_path).or_default();
        if !lockers.iter().any(|l| l.pid == locker.pid) {
            lockers.push(locker);
        }
    }

    Ok(path_map
        .into_iter()
        .map(|(path, lockers)| FileLockInfo { path, lockers })
        .collect())
}

/// 没有 handle.exe 时回退到 PowerShell WMI 查询
/// 只能检测到命令行中包含目标路径的进程（如 cwd 或参数中引用了该路径）
fn detect_with_powershell(target: &str) -> Result<Vec<FileLockInfo>, Error> {
    // 转义 PowerShell 特殊字符，防止注入：
    //   ' → ''   (PowerShell 单引号字符串内转义)
    //   $ → `$   (防止变量扩展，如 $env:USERPROFILE)
    //   ` → ``   (防止转义序列，如 `n `t)
    //   [ ] → `[ `]  (防止通配符字符类)
    let escaped = target
        .replace('`', "``")
        .replace('$', "`$")
        .replace('[', "`[")
        .replace(']', "`]")
        .replace('*', "`*")
        .replace('?', "`?")
        .replace('\'', "''");
    let script = format!(
        "Get-CimInstance Win32_Process | Where-Object {{ $_.CommandLine -like '*{}*' -and $_.ProcessId -ne $PID }} | Select-Object ProcessId, Name, CommandLine | ConvertTo-Json -Compress",
        escaped
    );

    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let output = Command::new("powershell")
        .args(["-NoProfile", "-OutputEncoding", "UTF8", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(Error::Io)?;

    let stdout = decode_system_output(&output.stdout);
    if stdout.trim().is_empty() || stdout.trim() == "null" {
        return Ok(Vec::new());
    }

    // 解析 JSON 结果
    let json_val: serde_json::Value = match serde_json::from_str(stdout.trim()) {
        Ok(v) => v,
        Err(_) => return Ok(Vec::new()),
    };

    let target_path = PathBuf::from(target);
    let mut lockers = Vec::new();

    // WMI 结果可能是单个对象或数组
    let items = match &json_val {
        serde_json::Value::Array(arr) => arr.clone(),
        obj @ serde_json::Value::Object(_) => vec![obj.clone()],
        _ => return Ok(Vec::new()),
    };

    // 预处理目标路径，用于后续边界验证
    let target_lower = target.to_lowercase().replace('/', "\\");
    let target_with_sep = format!("{}\\", target_lower.trim_end_matches('\\'));

    for item in &items {
        let pid = item["ProcessId"].as_u64().unwrap_or(0) as u32;
        let name = item["Name"].as_str().unwrap_or("unknown").to_string();
        let cmdline = item["CommandLine"].as_str().map(|s| s.to_string());

        if pid == 0 {
            continue;
        }

        // 路径边界验证：确认命令行确实引用了目标路径，
        // 而非碰巧包含相似子串（如 "C:\project" 不应匹配 "C:\projects"）
        if let Some(ref cmd) = cmdline {
            let cmd_lower = cmd.to_lowercase().replace('/', "\\");
            let has_boundary = cmd_lower.contains(&target_with_sep)
                || cmd_lower.ends_with(&target_lower)
                || cmd_lower.contains(&format!("{}\"", target_lower))
                || cmd_lower.contains(&format!("{} ", target_lower));
            if !has_boundary {
                log::debug!(
                    "WMI: skipping PID {} ({}) — cmdline doesn't match path boundary",
                    pid,
                    name
                );
                continue;
            }
        }

        lockers.push(ProcessInfo::new(
            pid,
            name,
            LockType::Other("WMI".to_string()),
            cmdline,
            None,
        ));
    }

    if lockers.is_empty() {
        return Ok(Vec::new());
    }

    Ok(vec![FileLockInfo {
        path: target_path,
        lockers,
    }])
}

/// 内置 handle.exe 的已知 SHA-256 哈希值
/// 用于运行时校验，防止内置的 handle.exe 被替换为恶意文件
const KNOWN_HANDLE_HASHES: &[&str] = &[
    // handle.exe v5.0 (bundled)
    "84c22579ca09f4fd8a8d9f56a6348c4ad2a92d4722c9f1213dd73c2f68a381e3",
];

/// 校验 handle.exe 文件哈希是否为已知安全值
/// 对于内置在 exe 同目录或工作目录的 handle.exe 进行校验
/// PATH 中和自动下载的版本通过 Authenticode 签名验证
fn verify_handle_hash(path: &Path) -> bool {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return false,
    };
    let hash = crate::res::_hash_hex(&data);
    if KNOWN_HANDLE_HASHES.contains(&hash.as_str()) {
        return true;
    }
    // 如果不在已知哈希列表中，回退到 Authenticode 签名验证
    verify_authenticode_signature(path)
}

/// 查找 handle.exe 的位置，找不到时自动下载
pub fn find_handle_exe() -> Option<PathBuf> {
    // 1. 检查当前可执行文件同目录
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            let handle_path = dir.join("handle.exe");
            if handle_path.exists() && verify_handle_hash(&handle_path) {
                return Some(handle_path);
            }
            let handle64_path = dir.join("handle64.exe");
            if handle64_path.exists() && verify_handle_hash(&handle64_path) {
                return Some(handle64_path);
            }
        }
    }

    // 2. 检查当前工作目录
    if let Ok(cwd) = std::env::current_dir() {
        let handle_path = cwd.join("handle.exe");
        if handle_path.exists() && verify_handle_hash(&handle_path) {
            return Some(handle_path);
        }
    }

    // 3. 检查 PATH
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW_F: u32 = 0x08000000;
    if let Ok(output) = Command::new("where")
        .arg("handle.exe")
        .creation_flags(CREATE_NO_WINDOW_F)
        .output()
    {
        let stdout = decode_system_output(&output.stdout);
        let first_line = stdout.lines().next().unwrap_or("").trim();
        if !first_line.is_empty() {
            let path = PathBuf::from(first_line);
            if path.exists() && verify_authenticode_signature(&path) {
                return Some(path);
            }
        }
    }

    // 4. 自动下载 handle64.exe 到可执行文件同目录
    if let Some(path) = download_handle_exe() {
        return Some(path);
    }

    None
}

/// 从 Sysinternals Live 下载 handle64.exe
fn download_handle_exe() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const MAX_RETRIES: u32 = 3;

    let target_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let target_path = target_dir.join("handle64.exe");

    // 使用 PowerShell 下载（无需额外依赖）
    let script = format!(
        "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
         Invoke-WebRequest -Uri 'https://live.sysinternals.com/handle64.exe' \
         -OutFile '{}' -UseBasicParsing -TimeoutSec 10",
        target_path.to_string_lossy().replace('\'', "''")
    );

    for attempt in 1..=MAX_RETRIES {
        log::info!(
            "Downloading handle64.exe from Sysinternals Live (attempt {}/{})...",
            attempt,
            MAX_RETRIES
        );

        // 清理上次下载可能残留的不完整文件
        let _ = std::fs::remove_file(&target_path);

        let result = Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        match result {
            Ok(output) if output.status.success() && target_path.exists() => {
                // 验证下载文件的 Authenticode 数字签名（确保来自 Microsoft/Sysinternals）
                if verify_authenticode_signature(&target_path) {
                    log::info!("Successfully downloaded and verified handle64.exe");
                    return Some(target_path);
                }
                log::warn!(
                    "handle64.exe signature verification failed (attempt {})",
                    attempt
                );
                let _ = std::fs::remove_file(&target_path);
            }
            Ok(output) => {
                let stderr = decode_system_output(&output.stderr);
                log::warn!(
                    "Failed to download handle64.exe (attempt {}): {}",
                    attempt,
                    stderr.trim()
                );
            }
            Err(e) => {
                log::warn!(
                    "Failed to run PowerShell for download (attempt {}): {}",
                    attempt,
                    e
                );
            }
        }

        if attempt < MAX_RETRIES {
            log::info!("Retrying in 1 second...");
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    log::warn!(
        "All {} download attempts failed for handle64.exe",
        MAX_RETRIES
    );
    None
}

/// 验证可执行文件的 Authenticode 数字签名
/// 确保文件由 Microsoft 签名，防止 MITM 攻击
fn verify_authenticode_signature(path: &Path) -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let script = format!(
        "$sig = Get-AuthenticodeSignature -FilePath '{}'; \
         if ($sig.Status -eq 'Valid' -and $sig.SignerCertificate.Subject -like '*Microsoft*') {{ \
             Write-Output 'VALID' \
         }} else {{ \
             Write-Output \"INVALID: Status=$($sig.Status), Subject=$($sig.SignerCertificate.Subject)\" \
         }}",
        path.to_string_lossy().replace('\'', "''")
    );

    match Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(output) => {
            let stdout = decode_system_output(&output.stdout);
            let result = stdout.trim();
            if result == "VALID" {
                log::debug!("Authenticode signature verified for {}", path.display());
                true
            } else {
                log::warn!("Authenticode verification: {}", result);
                false
            }
        }
        Err(e) => {
            log::warn!(
                "Failed to verify signature (PowerShell unavailable?): {}",
                e
            );
            // 无法验证签名时拒绝使用，防止未经验证的可执行文件被执行
            // 用户可手动从 https://learn.microsoft.com/sysinternals/downloads/handle 下载
            false
        }
    }
}

/// RM 会话的 RAII 守卫
struct RmSessionGuard(u32);

impl Drop for RmSessionGuard {
    fn drop(&mut self) {
        unsafe {
            RmEndSession(self.0);
        }
    }
}

/// 去掉 Windows canonicalize 返回的 \\?\ 前缀（RM 和许多 API 不认这个前缀）
fn strip_extended_prefix(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix("\\\\?\\") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

fn to_wide_string(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn wide_to_string(wide: &[u16]) -> String {
    let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len])
}

fn decode_system_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    // 全 ASCII 无歧义
    if bytes.iter().all(|&b| b < 0x80) {
        return String::from_utf8_lossy(bytes).to_string();
    }

    unsafe {
        use windows_sys::Win32::Globalization::MultiByteToWideChar;
        const MB_ERR_INVALID_CHARS: u32 = 0x8;

        // 自动检测：依次尝试 UTF-8 → GBK(936) → 系统ANSI → 系统OEM
        // 用严格模式测试哪个编码能完整解码
        for codepage in [65001u32, 936, 0, 1] {
            let len = MultiByteToWideChar(
                codepage,
                MB_ERR_INVALID_CHARS,
                bytes.as_ptr(),
                bytes.len() as i32,
                std::ptr::null_mut(),
                0,
            );
            if len > 0 {
                // 该编码通过严格校验，用它解码
                let mut wide = vec![0u16; len as usize];
                MultiByteToWideChar(
                    codepage,
                    0,
                    bytes.as_ptr(),
                    bytes.len() as i32,
                    wide.as_mut_ptr(),
                    len,
                );
                return String::from_utf16_lossy(&wide[..len as usize]);
            }
        }

        // 全部失败，宽松解码
        String::from_utf8_lossy(bytes).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_handle_path_normal() {
        let line = "notepad.exe        pid: 1234  type: File  1C8: C:\\Users\\test\\file.txt";
        assert_eq!(
            extract_handle_path(line),
            Some("C:\\Users\\test\\file.txt".to_string())
        );
    }

    #[test]
    fn extract_handle_path_directory() {
        let line = "explorer.exe       pid: 5678  type: Directory  AB0: C:\\Projects";
        assert_eq!(extract_handle_path(line), Some("C:\\Projects".to_string()));
    }

    #[test]
    fn extract_handle_path_section() {
        let line =
            "code.exe           pid: 9012  type: Section  FF0: C:\\Windows\\System32\\ntdll.dll";
        assert_eq!(
            extract_handle_path(line),
            Some("C:\\Windows\\System32\\ntdll.dll".to_string())
        );
    }

    #[test]
    fn extract_handle_path_no_type() {
        let line = "some random line without type info";
        assert_eq!(extract_handle_path(line), None);
    }

    #[test]
    fn extract_handle_path_no_hex_handle() {
        let line = "proc.exe           pid: 1  type: File  invalid: C:\\path";
        // "invalid" contains non-hex chars, so should fail
        assert_eq!(extract_handle_path(line), None);
    }

    #[test]
    fn strip_extended_prefix_with_prefix() {
        let path = PathBuf::from("\\\\?\\C:\\Users\\test");
        let result = strip_extended_prefix(&path);
        assert_eq!(result, PathBuf::from("C:\\Users\\test"));
    }

    #[test]
    fn strip_extended_prefix_without_prefix() {
        let path = PathBuf::from("C:\\Users\\test");
        let result = strip_extended_prefix(&path);
        assert_eq!(result, PathBuf::from("C:\\Users\\test"));
    }

    #[test]
    fn wide_string_roundtrip() {
        let path = PathBuf::from("C:\\test\\file.txt");
        let wide = to_wide_string(&path);
        let result = wide_to_string(&wide);
        assert_eq!(result, "C:\\test\\file.txt");
    }

    #[test]
    fn wide_to_string_with_null_terminator() {
        let wide = vec![0x0048, 0x0069, 0x0000, 0x0058]; // "Hi\0X"
        assert_eq!(wide_to_string(&wide), "Hi");
    }

    #[test]
    fn decode_system_output_ascii() {
        let bytes = b"Hello, World!";
        assert_eq!(decode_system_output(bytes), "Hello, World!");
    }

    #[test]
    fn decode_system_output_empty() {
        assert_eq!(decode_system_output(b""), "");
    }

    #[test]
    fn decode_system_output_utf8() {
        let bytes = "测试文件".as_bytes();
        assert_eq!(decode_system_output(bytes), "测试文件");
    }

    #[test]
    fn match_dir_entry_exact_match() {
        // 无通配符时，需要完全匹配字符数和非通配位置
        assert!(match_dir_entry_pattern("hello", "hello"));
        assert!(!match_dir_entry_pattern("hello", "world"));
    }

    #[test]
    fn match_dir_entry_wildcard() {
        assert!(match_dir_entry_pattern("hello", "hel?o"));
        assert!(match_dir_entry_pattern("hello", "?ello"));
        assert!(!match_dir_entry_pattern("hello", "?llo")); // 长度不同
    }

    /// 辅助函数：模拟 match_dir_entry 的通配符匹配逻辑（不依赖文件系统）
    fn match_dir_entry_pattern(name: &str, pattern: &str) -> bool {
        let name_chars: Vec<char> = name.chars().collect();
        let pattern_chars: Vec<char> = pattern.chars().collect();
        name_chars.len() == pattern_chars.len()
            && name_chars
                .iter()
                .zip(pattern_chars.iter())
                .all(|(n, p)| *p == '?' || *n == *p)
    }

    // --- 额外的 handle.exe 解析测试 ---

    #[test]
    fn extract_handle_path_with_spaces() {
        let line = "notepad.exe        pid: 1234  type: File  1C8: C:\\Users\\My User\\My File.txt";
        assert_eq!(
            extract_handle_path(line),
            Some("C:\\Users\\My User\\My File.txt".to_string())
        );
    }

    #[test]
    fn extract_handle_path_empty_after_hex() {
        // hex 后面只有冒号和空白，没有实际路径
        let line = "proc.exe           pid: 1  type: File  ABC:   ";
        let result = extract_handle_path(line);
        assert!(result.is_none(), "Empty path after hex should return None");
    }

    #[test]
    fn extract_handle_path_long_hex() {
        let line = "proc.exe           pid: 1  type: File  ABCDEF12: C:\\long\\hex\\handle";
        assert_eq!(
            extract_handle_path(line),
            Some("C:\\long\\hex\\handle".to_string())
        );
    }

    // --- decode_system_output 额外测试 ---

    #[test]
    fn decode_system_output_mixed_ascii_and_newlines() {
        let bytes = b"Line 1\r\nLine 2\r\n";
        let result = decode_system_output(bytes);
        assert!(result.contains("Line 1"));
        assert!(result.contains("Line 2"));
    }

    // --- strip_extended_prefix 额外测试 ---

    #[test]
    fn strip_extended_prefix_unc() {
        // UNC 路径不应被误剥离（\\server\share 不同于 \\?\）
        let path = PathBuf::from("\\\\server\\share\\file.txt");
        let result = strip_extended_prefix(&path);
        assert_eq!(result, PathBuf::from("\\\\server\\share\\file.txt"));
    }

    #[test]
    fn strip_extended_prefix_extended_unc() {
        // \\?\UNC\server\share 形式
        let path = PathBuf::from("\\\\?\\C:\\Windows\\System32");
        let result = strip_extended_prefix(&path);
        assert_eq!(result, PathBuf::from("C:\\Windows\\System32"));
    }

    // --- wide_string 额外测试 ---

    #[test]
    fn wide_to_string_empty() {
        let wide: Vec<u16> = vec![0x0000];
        assert_eq!(wide_to_string(&wide), "");
    }

    #[test]
    fn wide_to_string_no_null() {
        let wide = vec![0x0041, 0x0042, 0x0043]; // "ABC" without null terminator
        assert_eq!(wide_to_string(&wide), "ABC");
    }

    #[test]
    fn to_wide_string_null_terminated() {
        let path = PathBuf::from("test");
        let wide = to_wide_string(&path);
        assert_eq!(*wide.last().unwrap(), 0, "Should be null terminated");
    }

    // --- match_dir_entry_pattern 额外测试 ---

    #[test]
    fn match_dir_entry_all_wildcards() {
        assert!(match_dir_entry_pattern("abc", "???"));
        assert!(!match_dir_entry_pattern("abcd", "???"));
        assert!(!match_dir_entry_pattern("ab", "???"));
    }

    #[test]
    fn match_dir_entry_mixed_wildcards() {
        assert!(match_dir_entry_pattern("test.txt", "te?t.txt"));
        assert!(match_dir_entry_pattern("test.txt", "????.txt"));
        assert!(!match_dir_entry_pattern("test.txt", "???.txt"));
    }

    #[test]
    fn match_dir_entry_chinese_name_pattern() {
        // 模拟中文文件名与 ? 通配符模式匹配
        assert!(match_dir_entry_pattern("文件.txt", "??.txt"));
        assert!(!match_dir_entry_pattern("文件名.txt", "??.txt"));
    }
}
