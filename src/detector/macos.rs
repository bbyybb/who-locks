use crate::error::Error;
use crate::model::{FileLockInfo, LockType, ProcessInfo};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct MacosDetector;

impl MacosDetector {
    pub fn new() -> Self {
        Self
    }

    /// 使用 lsof 检测文件占用
    /// -F pcuLft: 机器可解析格式, 含进程/命令/用户/fd类型/类型字段
    /// -w: 抑制警告信息
    ///
    /// 注意：单文件模式使用 "pcuLft"（不含 n），因为只有一个文件无需区分路径；
    /// 批量模式 detect_batch() 使用 "pcuLftn"（含 n），需要通过文件名字段区分各文件的结果。
    fn detect_with_lsof(&self, path: &Path) -> Result<Vec<ProcessInfo>, Error> {
        let output = Command::new("lsof")
            .args(["-w", "-F", "pcuLft"])
            .arg(path)
            .output()
            .map_err(Error::Io)?;

        if !output.status.success() && output.stdout.is_empty() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_lsof_output(&stdout)
    }
}

impl super::LockDetector for MacosDetector {
    fn detect_file(&self, path: &Path) -> Result<Vec<ProcessInfo>, Error> {
        let canonical = path.canonicalize().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::PathNotFound(path.to_path_buf())
            } else {
                Error::Io(e)
            }
        })?;
        self.detect_with_lsof(&canonical)
    }

    fn detect_batch(&self, paths: &[&Path]) -> Result<Vec<FileLockInfo>, Error> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }

        // 对路径进行规范化
        let canonical_paths: Vec<PathBuf> =
            paths.iter().filter_map(|p| p.canonicalize().ok()).collect();

        if canonical_paths.is_empty() {
            return Ok(Vec::new());
        }

        // lsof 支持一次传入多个文件路径
        let mut args = vec!["-w".to_string(), "-F".to_string(), "pcuLftn".to_string()];
        for p in &canonical_paths {
            args.push(p.to_string_lossy().to_string());
        }

        let output = Command::new("lsof")
            .args(&args)
            .output()
            .map_err(Error::Io)?;

        if !output.status.success() && output.stdout.is_empty() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_lsof_batch_output(&stdout)
    }

    fn platform_name(&self) -> &'static str {
        "macOS (lsof: fd + cwd + txt + mmap)"
    }
}

/// 目录级深度扫描：使用 lsof +D 递归查找目录下所有被打开的文件
/// 类似 Windows 的 handle.exe 目录扫描
pub fn detect_deep(target: &Path) -> Result<Vec<FileLockInfo>, Error> {
    let canonical = target.canonicalize().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::PathNotFound(target.to_path_buf())
        } else {
            Error::Io(e)
        }
    })?;

    // 单文件直接用普通检测
    if canonical.is_file() {
        let det = MacosDetector::new();
        let lockers = det.detect_with_lsof(&canonical)?;
        if lockers.is_empty() {
            return Ok(Vec::new());
        }
        return Ok(vec![FileLockInfo {
            path: canonical,
            lockers,
        }]);
    }

    // 目录：使用 lsof +D 递归扫描
    let output = Command::new("lsof")
        .args(["+D"])
        .arg(&canonical)
        .args(["-w", "-F", "pcuLftn"])
        .output()
        .map_err(Error::Io)?;

    if !output.status.success() && output.stdout.is_empty() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_lsof_batch_output(&stdout)
}

/// 从 lsof 的 fd 类型字段推断 LockType
fn fd_type_to_lock_type(fd_field: &str) -> LockType {
    match fd_field {
        "cwd" => LockType::WorkingDir,
        "txt" => LockType::Executable,
        "mem" => LockType::MemoryMap,
        "rtd" => LockType::DirHandle, // root directory
        s if s.starts_with("DEL") => LockType::MemoryMap, // deleted mmap
        s if s
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false) =>
        {
            // 数字开头的是普通 fd (如 "0r", "1w", "3u")
            LockType::FileHandle
        }
        _ => LockType::Other(fd_field.to_string()),
    }
}

/// 解析 lsof -F 输出格式（单文件模式，不含 n 字段）
fn parse_lsof_output(output: &str) -> Result<Vec<ProcessInfo>, Error> {
    let mut results = Vec::new();
    let mut current_pid: Option<u32> = None;
    let mut current_name: Option<String> = None;
    let mut current_user: Option<String> = None;
    let mut current_fd: Option<String> = None;

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }

        let (prefix, value) = line.split_at(1);
        match prefix {
            "p" => {
                // 保存前一个进程（如果有 fd 信息）
                flush_process(
                    &mut results,
                    &current_pid,
                    &current_name,
                    &current_user,
                    &current_fd,
                );
                current_pid = value.parse().ok();
                current_name = None;
                current_user = None;
                current_fd = None;
            }
            "c" => current_name = Some(value.to_string()),
            "L" => current_user = Some(value.to_string()),
            "u" => {
                if current_user.is_none() {
                    current_user = Some(value.to_string());
                }
            }
            "f" => {
                // 新的 fd 条目：先保存前一个
                flush_process(
                    &mut results,
                    &current_pid,
                    &current_name,
                    &current_user,
                    &current_fd,
                );
                current_fd = Some(value.to_string());
            }
            _ => {}
        }
    }

    // 保存最后一个
    flush_process(
        &mut results,
        &current_pid,
        &current_name,
        &current_user,
        &current_fd,
    );

    // 去重（同一个 PID 可能出现多次）
    results.sort_by(|a, b| a.pid.cmp(&b.pid));
    results.dedup_by(|a, b| a.pid == b.pid && a.lock_type == b.lock_type);

    Ok(results)
}

fn flush_process(
    results: &mut Vec<ProcessInfo>,
    pid: &Option<u32>,
    name: &Option<String>,
    user: &Option<String>,
    fd: &Option<String>,
) {
    if let (Some(pid), Some(name)) = (pid, name) {
        let lock_type = fd
            .as_deref()
            .map(fd_type_to_lock_type)
            .unwrap_or(LockType::FileHandle);
        results.push(ProcessInfo::new(
            *pid,
            name.clone(),
            lock_type,
            None,
            user.clone(),
        ));
    }
}

/// 解析 lsof -F 批量输出（带文件名字段 n）
fn parse_lsof_batch_output(output: &str) -> Result<Vec<FileLockInfo>, Error> {
    let mut file_map: HashMap<PathBuf, Vec<ProcessInfo>> = HashMap::new();

    let mut current_pid: Option<u32> = None;
    let mut current_name: Option<String> = None;
    let mut current_user: Option<String> = None;
    let mut current_fd: Option<String> = None;

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }

        let (prefix, value) = line.split_at(1);
        match prefix {
            "p" => {
                current_pid = value.parse().ok();
                current_name = None;
                current_user = None;
                current_fd = None;
            }
            "c" => current_name = Some(value.to_string()),
            "L" => current_user = Some(value.to_string()),
            "u" => {
                if current_user.is_none() {
                    current_user = Some(value.to_string());
                }
            }
            "f" => current_fd = Some(value.to_string()),
            "n" => {
                if let (Some(pid), Some(ref name)) = (current_pid, &current_name) {
                    let mut lock_type = current_fd
                        .as_deref()
                        .map(fd_type_to_lock_type)
                        .unwrap_or(LockType::FileHandle);

                    let path = PathBuf::from(value);

                    // 数字 fd 指向目录时修正为 DirHandle（与 Linux 检测器行为一致）
                    if lock_type == LockType::FileHandle && path.is_dir() {
                        lock_type = LockType::DirHandle;
                    }

                    file_map.entry(path).or_default().push(ProcessInfo::new(
                        pid,
                        name.clone(),
                        lock_type,
                        None,
                        current_user.clone(),
                    ));
                }
            }
            _ => {}
        }
    }

    // 去重
    for lockers in file_map.values_mut() {
        lockers.sort_by(|a, b| a.pid.cmp(&b.pid));
        lockers.dedup_by(|a, b| a.pid == b.pid && a.lock_type == b.lock_type);
    }

    Ok(file_map
        .into_iter()
        .map(|(path, lockers)| FileLockInfo { path, lockers })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fd_type_to_lock_type_mappings() {
        assert_eq!(fd_type_to_lock_type("cwd"), LockType::WorkingDir);
        assert_eq!(fd_type_to_lock_type("txt"), LockType::Executable);
        assert_eq!(fd_type_to_lock_type("mem"), LockType::MemoryMap);
        assert_eq!(fd_type_to_lock_type("rtd"), LockType::DirHandle);
        assert_eq!(fd_type_to_lock_type("DEL"), LockType::MemoryMap);
        assert_eq!(fd_type_to_lock_type("3r"), LockType::FileHandle);
        assert_eq!(fd_type_to_lock_type("0w"), LockType::FileHandle);
    }

    #[test]
    fn fd_type_to_lock_type_unknown() {
        assert_eq!(
            fd_type_to_lock_type("NOFD"),
            LockType::Other("NOFD".to_string())
        );
    }

    #[test]
    fn parse_lsof_output_basic() {
        // 模拟 lsof -F pcuLft 输出
        let output = "p1234\ncVim\nLuser1\nu501\nfcwd\n";
        let result = parse_lsof_output(output).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pid, 1234);
        assert_eq!(result[0].name, "Vim");
        assert_eq!(result[0].user, Some("user1".to_string()));
        assert_eq!(result[0].lock_type, LockType::WorkingDir);
    }

    #[test]
    fn parse_lsof_output_multiple_fds() {
        // 同一进程有多个 fd
        let output = "p100\ncbash\nLroot\nfcwd\nf3r\n";
        let result = parse_lsof_output(output).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].lock_type, LockType::WorkingDir);
        assert_eq!(result[1].lock_type, LockType::FileHandle);
    }

    #[test]
    fn parse_lsof_output_empty() {
        let result = parse_lsof_output("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_lsof_output_dedup() {
        // 同一 PID 同一 lock_type 应该去重
        let output = "p100\ncbash\nfcwd\np100\ncbash\nfcwd\n";
        let result = parse_lsof_output(output).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn parse_lsof_batch_output_basic() {
        // 模拟 lsof -F pcuLftn 输出（带文件名）
        let output = "p1234\ncnotepad\nLuser1\nf3r\nn/tmp/test.txt\n";
        let result = parse_lsof_batch_output(output).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, PathBuf::from("/tmp/test.txt"));
        assert_eq!(result[0].lockers.len(), 1);
        assert_eq!(result[0].lockers[0].pid, 1234);
    }

    #[test]
    fn parse_lsof_batch_output_multiple_files() {
        let output = "p100\ncvim\nf3r\nn/tmp/a.txt\nf4w\nn/tmp/b.txt\n";
        let result = parse_lsof_batch_output(output).unwrap();
        assert_eq!(result.len(), 2);
        let paths: Vec<PathBuf> = result.iter().map(|f| f.path.clone()).collect();
        assert!(paths.contains(&PathBuf::from("/tmp/a.txt")));
        assert!(paths.contains(&PathBuf::from("/tmp/b.txt")));
    }

    #[test]
    fn parse_lsof_batch_output_dedup() {
        // 同一进程对同一文件的相同 lock_type 应去重
        let output = "p100\ncbash\nfcwd\nn/tmp/dir\np100\ncbash\nfcwd\nn/tmp/dir\n";
        let result = parse_lsof_batch_output(output).unwrap();
        let dir_info = result
            .iter()
            .find(|f| f.path == PathBuf::from("/tmp/dir"))
            .unwrap();
        assert_eq!(dir_info.lockers.len(), 1);
    }

    /// detect_deep 对不存在的路径应返回 PathNotFound 错误
    #[test]
    #[cfg(target_os = "macos")]
    fn detect_deep_nonexistent_path() {
        let result = detect_deep(Path::new("/nonexistent_path_who_locks_test"));
        assert!(result.is_err(), "Should fail for non-existent path");
    }

    /// detect_deep 对单文件应回退到普通 lsof 检测
    #[test]
    #[cfg(target_os = "macos")]
    fn detect_deep_file_returns_ok() {
        // /etc/hosts 是 macOS 上确定存在的文件
        let result = detect_deep(Path::new("/etc/hosts"));
        assert!(result.is_ok(), "Should succeed for existing file");
    }
}
