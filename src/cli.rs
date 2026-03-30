use clap::Parser;
use std::path::PathBuf;

use crate::detector;
use crate::scan::Scanner;

/// 跨平台文件占用检测工具 / Cross-platform file lock detector
#[derive(Parser)]
#[command(name = "who-locks", version, about)]
pub struct CliArgs {
    /// 要扫描的文件或目录路径 / Paths to scan
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// 不递归扫描子目录 / Do not recurse into subdirectories
    #[arg(short = 'n', long = "no-recursive")]
    pub no_recursive: bool,

    /// 最大扫描深度 / Maximum scan depth
    #[arg(short, long)]
    pub depth: Option<usize>,

    /// 排除模式（逗号分隔，支持 *、? 和 ** 通配符）/ Exclude patterns (comma-separated, supports *, ? and ** wildcards)
    #[arg(short, long)]
    pub exclude: Option<String>,

    /// 输出格式：text 或 json / Output format: text or json
    #[arg(short, long, default_value = "text")]
    pub format: String,
}

/// Windows: 重新连接到父进程控制台，使 stdout/stderr 输出可见
/// 因为 windows_subsystem = "windows" 在 release 构建中隐藏了控制台
#[cfg(target_os = "windows")]
pub fn attach_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_single_path() {
        let args = CliArgs::try_parse_from(["who-locks", "/tmp/file.txt"]).unwrap();
        assert_eq!(args.paths, vec![PathBuf::from("/tmp/file.txt")]);
        assert!(!args.no_recursive);
        assert!(args.depth.is_none());
        assert!(args.exclude.is_none());
        assert_eq!(args.format, "text");
    }

    #[test]
    fn parse_multiple_paths() {
        let args = CliArgs::try_parse_from(["who-locks", "/a", "/b", "/c"]).unwrap();
        assert_eq!(args.paths.len(), 3);
        assert_eq!(args.paths[0], PathBuf::from("/a"));
        assert_eq!(args.paths[2], PathBuf::from("/c"));
    }

    #[test]
    fn parse_no_recursive_short() {
        let args = CliArgs::try_parse_from(["who-locks", "/tmp", "-n"]).unwrap();
        assert!(args.no_recursive);
    }

    #[test]
    fn parse_no_recursive_long() {
        let args = CliArgs::try_parse_from(["who-locks", "/tmp", "--no-recursive"]).unwrap();
        assert!(args.no_recursive);
    }

    #[test]
    fn parse_depth_option() {
        let args = CliArgs::try_parse_from(["who-locks", "/tmp", "-d", "5"]).unwrap();
        assert_eq!(args.depth, Some(5));
    }

    #[test]
    fn parse_depth_long() {
        let args = CliArgs::try_parse_from(["who-locks", "/tmp", "--depth", "3"]).unwrap();
        assert_eq!(args.depth, Some(3));
    }

    #[test]
    fn parse_exclude_option() {
        let args =
            CliArgs::try_parse_from(["who-locks", "/tmp", "-e", "node_modules,*.log"]).unwrap();
        assert_eq!(args.exclude, Some("node_modules,*.log".to_string()));
    }

    #[test]
    fn parse_format_json() {
        let args = CliArgs::try_parse_from(["who-locks", "/tmp", "-f", "json"]).unwrap();
        assert_eq!(args.format, "json");
    }

    #[test]
    fn parse_format_default_is_text() {
        let args = CliArgs::try_parse_from(["who-locks", "/tmp"]).unwrap();
        assert_eq!(args.format, "text");
    }

    #[test]
    fn parse_all_options_combined() {
        let args = CliArgs::try_parse_from([
            "who-locks",
            "/project",
            "/other",
            "-n",
            "-d",
            "2",
            "-e",
            ".git,target",
            "-f",
            "json",
        ])
        .unwrap();
        assert_eq!(args.paths.len(), 2);
        assert!(args.no_recursive);
        assert_eq!(args.depth, Some(2));
        assert_eq!(args.exclude, Some(".git,target".to_string()));
        assert_eq!(args.format, "json");
    }

    #[test]
    fn parse_missing_paths_fails() {
        let result = CliArgs::try_parse_from(["who-locks"]);
        assert!(result.is_err(), "Should fail without required paths");
    }

    #[test]
    fn parse_windows_path() {
        let args = CliArgs::try_parse_from(["who-locks", "C:\\Users\\test\\file.txt"]).unwrap();
        assert_eq!(args.paths, vec![PathBuf::from("C:\\Users\\test\\file.txt")]);
    }

    #[test]
    fn parse_path_with_spaces() {
        let args = CliArgs::try_parse_from(["who-locks", "/path/with spaces/file.txt"]).unwrap();
        assert_eq!(
            args.paths,
            vec![PathBuf::from("/path/with spaces/file.txt")]
        );
    }
}

pub fn run_cli() {
    let args = CliArgs::parse();

    let exclude: Vec<String> = args
        .exclude
        .map(|e| {
            e.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let max_depth = if args.no_recursive {
        Some(1)
    } else {
        args.depth
    };

    let det = detector::create_detector();
    let is_tty = std::io::IsTerminal::is_terminal(&std::io::stderr());
    let scanner = Scanner::new(det, max_depth, false, exclude, false).with_progress(Box::new(
        move |msg: &str| {
            if is_tty && !msg.is_empty() {
                eprint!("\r\x1b[K{}", msg);
            }
        },
    ));

    let mut all_locked = Vec::new();
    let mut total_scanned = 0;
    let mut has_errors = false;

    for path in &args.paths {
        if !path.exists() {
            eprintln!("Error: path not found: {}", path.display());
            has_errors = true;
            continue;
        }

        let result = scanner.scan(path);
        total_scanned += result.total_files_scanned;

        for err in &result.errors {
            eprintln!("Warning: {}: {}", err.path.display(), err.reason);
        }

        all_locked.extend(result.locked_files);
    }

    // 清除进度行（仅在终端环境输出 ANSI 转义码，管道/重定向时跳过）
    if is_tty {
        eprint!("\r\x1b[K");
    }

    if args.format == "json" {
        let items: Vec<serde_json::Value> = all_locked
            .iter()
            .flat_map(|info| {
                info.lockers.iter().map(move |proc| {
                    serde_json::json!({
                        "file_path": info.path.display().to_string(),
                        "pid": proc.pid,
                        "process_name": proc.name,
                        "lock_type": proc.lock_type.to_string(),
                        "command_line": proc.cmdline,
                        "user": proc.user,
                        "blocking": proc.blocking,
                    })
                })
            })
            .collect();

        match serde_json::to_string_pretty(&items) {
            Ok(json) => println!("{}", json),
            Err(e) => eprintln!("Error serializing JSON: {}", e),
        }
    } else {
        if all_locked.is_empty() {
            println!("No locked files found. ({} files scanned)", total_scanned);
        } else {
            for info in &all_locked {
                println!("{}", info.path.display());
                for proc in &info.lockers {
                    let blocking_tag = if proc.blocking { "" } else { " (non-blocking)" };
                    println!(
                        "  PID: {}  Process: {}  Type: {}{}",
                        proc.pid, proc.name, proc.lock_type, blocking_tag
                    );
                    if let Some(cmd) = &proc.cmdline {
                        if !cmd.is_empty() {
                            println!("    Command: {}", cmd);
                        }
                    }
                    if let Some(user) = &proc.user {
                        if !user.is_empty() {
                            println!("    User: {}", user);
                        }
                    }
                }
            }
            let lock_count: usize = all_locked
                .iter()
                .map(|f| f.lockers.iter().filter(|p| p.blocking).count())
                .sum();
            println!(
                "\n{} locked file(s), {} blocking process(es) ({} files scanned)",
                all_locked.len(),
                lock_count,
                total_scanned
            );
        }
    }

    if has_errors {
        std::process::exit(1);
    }
}
