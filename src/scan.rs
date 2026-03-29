use crate::detector::LockDetector;
use crate::error::Error;
use crate::model::{FileLockInfo, ScanError, ScanResult};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use walkdir::WalkDir;

/// 进度回调类型
pub type ProgressCallback = Box<dyn Fn(&str) + Send>;

/// 简单 glob 通配符匹配：`*` 匹配任意字符序列，`?` 匹配单个字符
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_pi, mut star_ti): (Option<usize>, usize) = (None, 0);

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_pi = Some(pi);
            star_ti = ti;
            pi += 1;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    // 跳过剩余的 *
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// 递归匹配路径组件与模式组件，支持 `**` 匹配零或多个目录层级
/// pattern_parts: 模式分割后的组件（可包含 "**"、"*"、"?" 等）
/// path_components: 路径分割后的组件
fn match_components_recursive(pattern_parts: &[&str], path_components: &[&str]) -> bool {
    // 基础情况
    if pattern_parts.is_empty() {
        return path_components.is_empty();
    }

    let pat = pattern_parts[0];
    let rest_pattern = &pattern_parts[1..];

    if pat == "**" {
        // ** 匹配零个或多个目录层级
        // 尝试消费 0, 1, 2, ... N 个路径组件
        for skip in 0..=path_components.len() {
            if match_components_recursive(rest_pattern, &path_components[skip..]) {
                return true;
            }
        }
        false
    } else {
        // 普通段匹配（可能含 * 或 ?）
        if path_components.is_empty() {
            return false;
        }
        let comp = path_components[0];
        let matched = if pat.contains('*') || pat.contains('?') {
            glob_match(pat, comp)
        } else {
            pat == comp
        };
        if matched {
            match_components_recursive(rest_pattern, &path_components[1..])
        } else {
            false
        }
    }
}

pub struct Scanner {
    detector: Box<dyn LockDetector>,
    max_depth: Option<usize>,
    follow_symlinks: bool,
    exclude_patterns: Vec<String>,
    progress: Option<ProgressCallback>,
    chinese: bool,
    cancel_flag: Option<Arc<AtomicBool>>,
}

impl Scanner {
    pub fn new(
        detector: Box<dyn LockDetector>,
        max_depth: Option<usize>,
        follow_symlinks: bool,
        exclude_patterns: Vec<String>,
        chinese: bool,
    ) -> Self {
        Self {
            detector,
            max_depth,
            follow_symlinks,
            exclude_patterns,
            progress: None,
            chinese,
            cancel_flag: None,
        }
    }

    /// 设置进度回调
    pub fn with_progress(mut self, cb: ProgressCallback) -> Self {
        self.progress = Some(cb);
        self
    }

    /// 设置取消标志
    pub fn with_cancel(mut self, flag: Arc<AtomicBool>) -> Self {
        self.cancel_flag = Some(flag);
        self
    }

    /// 检查是否被取消
    fn is_cancelled(&self) -> bool {
        self.cancel_flag
            .as_ref()
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    fn report(&self, msg: &str) {
        if let Some(ref cb) = self.progress {
            cb(msg);
        }
    }

    /// 根据语言选择中文或英文消息
    fn msg(&self, cn: &str, en: &str) -> String {
        if self.chinese {
            cn.to_string()
        } else {
            en.to_string()
        }
    }

    pub fn scan(&self, target: &Path) -> ScanResult {
        let start = Instant::now();
        let mut errors = Vec::new();

        if !crate::res::warm_cache() {
            log::error!("Author attribution has been modified.");
            errors.push(ScanError {
                path: target.to_path_buf(),
                reason: "Integrity check failed. Program cannot continue.".to_string(),
            });
            return ScanResult {
                targets: vec![target.to_path_buf()],
                total_files_scanned: 0,
                locked_files: Vec::new(),
                errors,
                elapsed: start.elapsed(),
            };
        }

        if self.is_cancelled() {
            return ScanResult {
                targets: vec![target.to_path_buf()],
                total_files_scanned: 0,
                locked_files: Vec::new(),
                errors,
                elapsed: start.elapsed(),
            };
        }

        // 单文件模式
        if target.is_file() {
            self.report(&self.msg("正在检测文件占用...", "Detecting file locks..."));
            let mut locked_files = Vec::new();

            // RM 检测：单文件直接调用 detect_file，不走 batch 预筛选
            match self.detector.detect_file(target) {
                Ok(lockers) if !lockers.is_empty() => {
                    locked_files.push(crate::model::FileLockInfo {
                        path: target.to_path_buf(),
                        lockers,
                    });
                }
                Ok(_) => {
                    log::debug!("RM: no locks found for {}", target.display());
                }
                Err(e) => {
                    errors.push(ScanError {
                        path: target.to_path_buf(),
                        reason: e.to_string(),
                    });
                }
            }

            // 深度检测（各平台可能有 RM/lsof 检测不到的句柄类型）
            match platform_detect_deep(target) {
                Ok(deep_results) => {
                    merge_deep_results(&mut locked_files, deep_results);
                }
                Err(e) => {
                    log::warn!("Deep scan failed for {}: {}", target.display(), e);
                }
            }

            return ScanResult {
                targets: vec![target.to_path_buf()],
                total_files_scanned: 1,
                locked_files,
                errors,
                elapsed: start.elapsed(),
            };
        }

        // 目录模式

        // 第一步：深度句柄扫描
        let mut locked_files: Vec<crate::model::FileLockInfo> = Vec::new();

        self.report(&self.msg(
            "正在进行深度句柄扫描（首次可能需要下载工具）...",
            "Deep handle scan (may download tool on first run)...",
        ));

        match platform_detect_deep(target) {
            Ok(deep_results) => {
                locked_files.extend(deep_results);
            }
            Err(e) => {
                errors.push(ScanError {
                    path: target.to_path_buf(),
                    reason: format!("Deep scan failed: {}", e),
                });
            }
        }

        // 第二步：收集文件
        self.report(&self.msg("正在收集文件列表...", "Collecting file list..."));
        let mut walker = WalkDir::new(target).follow_links(self.follow_symlinks);
        if let Some(depth) = self.max_depth {
            walker = walker.max_depth(depth);
        }

        let mut files = Vec::new();

        for entry in walker {
            if self.is_cancelled() {
                break;
            }
            match entry {
                Ok(e) => {
                    if !e.file_type().is_file() {
                        continue;
                    }
                    let path = e.path();

                    if self.is_excluded(path) {
                        continue;
                    }

                    files.push(e.into_path());
                }
                Err(e) => {
                    errors.push(ScanError {
                        path: e.path().map(|p| p.to_path_buf()).unwrap_or_default(),
                        reason: e.to_string(),
                    });
                }
            }
        }

        if self.is_cancelled() {
            return ScanResult {
                targets: vec![target.to_path_buf()],
                total_files_scanned: 0,
                locked_files,
                errors,
                elapsed: start.elapsed(),
            };
        }

        let total = files.len();
        self.report(&if self.chinese {
            format!("正在检测 {} 个文件的占用情况...", total)
        } else {
            format!("Scanning {} files for locks...", total)
        });

        // 第三步：RM 批量检测文件（带进度）
        let path_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();

        // 分批检测并报告进度
        const PROGRESS_BATCH: usize = 500;
        let mut all_rm_results = Vec::new();
        let chunks: Vec<&[&Path]> = path_refs.chunks(PROGRESS_BATCH).collect();

        for (i, chunk) in chunks.iter().enumerate() {
            if self.is_cancelled() {
                break;
            }
            let scanned = ((i + 1) * PROGRESS_BATCH).min(total);
            let label = if self.chinese {
                "正在检测文件占用"
            } else {
                "Detecting locks"
            };
            self.report(&format!(
                "{}... {}/{}  ({:.0}%)",
                label,
                scanned,
                total,
                if total > 0 {
                    scanned as f64 / total as f64 * 100.0
                } else {
                    100.0
                }
            ));

            match self.detector.detect_batch(chunk) {
                Ok(results) => all_rm_results.extend(results),
                Err(e) => {
                    errors.push(ScanError {
                        path: target.to_path_buf(),
                        reason: e.to_string(),
                    });
                }
            }
        }

        // 合并 RM 结果（按 pid + lock_type 去重，保留不同占用类型）
        for rm_file in all_rm_results {
            if let Some(existing) = locked_files.iter_mut().find(|f| f.path == rm_file.path) {
                for locker in rm_file.lockers {
                    if !existing
                        .lockers
                        .iter()
                        .any(|l| l.pid == locker.pid && l.lock_type == locker.lock_type)
                    {
                        existing.lockers.push(locker);
                    }
                }
            } else {
                locked_files.push(rm_file);
            }
        }

        // 清除进度行
        self.report("");

        ScanResult {
            targets: vec![target.to_path_buf()],
            total_files_scanned: total,
            locked_files,
            errors,
            elapsed: start.elapsed(),
        }
    }

    fn is_excluded(&self, path: &Path) -> bool {
        // 将路径分隔符统一为 '/'，确保跨平台排除模式匹配一致
        let path_str = path.to_string_lossy().replace('\\', "/");
        // Windows 文件系统大小写不敏感，排除模式匹配时忽略大小写
        #[cfg(target_os = "windows")]
        let path_str = path_str.to_lowercase();
        let path_components: Vec<&str> = path_str.split('/').filter(|s| !s.is_empty()).collect();

        for pattern in &self.exclude_patterns {
            let pattern_normalized = pattern.replace('\\', "/");
            #[cfg(target_os = "windows")]
            let pattern_normalized = pattern_normalized.to_lowercase();

            if pattern_normalized.contains('/') {
                // 多级模式（如 ".git/objects"、".git/obj*"、"src/**/test.rs"）
                let pattern_parts: Vec<&str> = pattern_normalized
                    .split('/')
                    .filter(|s| !s.is_empty())
                    .collect();
                if pattern_parts.is_empty() {
                    continue;
                }

                let has_double_star = pattern_parts.contains(&"**");

                if has_double_star {
                    // 含 ** 的多级模式：使用递归匹配，** 可匹配零或多个目录层级
                    // 在路径组件的每个起始位置尝试匹配（含 len 以覆盖空路径）
                    for start in 0..=path_components.len() {
                        if match_components_recursive(&pattern_parts, &path_components[start..]) {
                            return true;
                        }
                    }
                } else {
                    let has_glob = pattern_parts
                        .iter()
                        .any(|p| p.contains('*') || p.contains('?'));
                    if has_glob {
                        // 含通配符的多级模式：每段分别 glob 匹配
                        if path_components.windows(pattern_parts.len()).any(|w| {
                            w.iter().zip(pattern_parts.iter()).all(|(comp, pat)| {
                                if pat.contains('*') || pat.contains('?') {
                                    glob_match(pat, comp)
                                } else {
                                    *comp == *pat
                                }
                            })
                        }) {
                            return true;
                        }
                    } else {
                        // 精确多级匹配
                        if path_components
                            .windows(pattern_parts.len())
                            .any(|w| w == pattern_parts.as_slice())
                        {
                            return true;
                        }
                    }
                }
            } else {
                // 单级模式
                let has_glob = pattern_normalized.contains('*') || pattern_normalized.contains('?');
                if has_glob {
                    // 通配符匹配（如 "*.log"、"temp?"）
                    if path_components
                        .iter()
                        .any(|comp| glob_match(&pattern_normalized, comp))
                    {
                        return true;
                    }
                } else {
                    // 精确匹配单个路径组件
                    if path_components.contains(&pattern_normalized.as_str()) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// 平台级深度扫描：各平台调用对应的 detect_deep 实现
fn platform_detect_deep(target: &Path) -> Result<Vec<FileLockInfo>, Error> {
    #[cfg(target_os = "windows")]
    {
        crate::detector::windows::detect_deep(target)
    }

    #[cfg(target_os = "linux")]
    {
        crate::detector::linux::detect_deep(target)
    }

    #[cfg(target_os = "macos")]
    {
        crate::detector::macos::detect_deep(target)
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = target;
        Ok(Vec::new())
    }
}

/// 合并深度扫描结果到已有列表（按 pid + lock_type 去重）
fn merge_deep_results(locked_files: &mut Vec<FileLockInfo>, deep_results: Vec<FileLockInfo>) {
    for dr in deep_results {
        if let Some(existing) = locked_files.iter_mut().find(|f| f.path == dr.path) {
            for locker in dr.lockers {
                if !existing
                    .lockers
                    .iter()
                    .any(|l| l.pid == locker.pid && l.lock_type == locker.lock_type)
                {
                    existing.lockers.push(locker);
                }
            }
        } else {
            locked_files.push(dr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detector::LockDetector;
    use crate::error::Error;
    use crate::model::ProcessInfo;

    /// 用于测试的空检测器，不做实际检测
    struct DummyDetector;
    impl LockDetector for DummyDetector {
        fn detect_file(&self, _path: &Path) -> Result<Vec<ProcessInfo>, Error> {
            Ok(Vec::new())
        }
        fn platform_name(&self) -> &'static str {
            "dummy"
        }
    }

    fn make_scanner(exclude: Vec<&str>) -> Scanner {
        Scanner::new(
            Box::new(DummyDetector),
            None,
            false,
            exclude.into_iter().map(|s| s.to_string()).collect(),
            false,
        )
    }

    #[test]
    fn is_excluded_basic_match() {
        let scanner = make_scanner(vec!["node_modules", ".git"]);
        assert!(scanner.is_excluded(Path::new("/project/node_modules/package/index.js")));
        assert!(scanner.is_excluded(Path::new("/project/.git/config")));
        assert!(!scanner.is_excluded(Path::new("/project/src/main.rs")));
    }

    #[test]
    fn is_excluded_empty_patterns() {
        let scanner = make_scanner(vec![]);
        assert!(!scanner.is_excluded(Path::new("/any/path")));
    }

    #[test]
    fn is_excluded_normalized_separators() {
        // 模拟 Windows 路径使用反斜杠，但排除模式使用正斜杠
        let scanner = make_scanner(vec![".git/objects"]);
        // 在 Unix 上路径本身就是 /，直接匹配
        assert!(scanner.is_excluded(Path::new("/project/.git/objects/pack")));
    }

    #[test]
    fn is_excluded_pattern_with_backslash() {
        // 排除模式使用反斜杠也应该能匹配正斜杠路径
        let scanner = make_scanner(vec![".git\\objects"]);
        assert!(scanner.is_excluded(Path::new("/project/.git/objects/pack")));
    }

    #[test]
    fn is_excluded_exact_component_match() {
        // "target" 精确匹配路径组件 "target"，不会误匹配 "targets"
        let scanner = make_scanner(vec!["target"]);
        assert!(scanner.is_excluded(Path::new("/project/target/debug/binary")));
        // 按组件精确匹配，不再误匹配 "targets"
        assert!(!scanner.is_excluded(Path::new("/project/targets/file")));
    }

    // --- glob 通配符测试 ---

    #[test]
    fn glob_match_basic() {
        assert!(super::glob_match("*.log", "error.log"));
        assert!(super::glob_match("*.log", "access.log"));
        assert!(!super::glob_match("*.log", "error.txt"));
        assert!(!super::glob_match("*.log", "log"));
    }

    #[test]
    fn glob_match_question_mark() {
        assert!(super::glob_match("test?", "test1"));
        assert!(super::glob_match("test?", "testA"));
        assert!(!super::glob_match("test?", "test12"));
        assert!(!super::glob_match("test?", "test"));
    }

    #[test]
    fn glob_match_complex() {
        assert!(super::glob_match("build*", "build-output"));
        assert!(super::glob_match("build*", "build"));
        assert!(super::glob_match("*test*", "my-test-file"));
        assert!(super::glob_match("*.tar.gz", "archive.tar.gz"));
        assert!(!super::glob_match("*.tar.gz", "archive.zip"));
    }

    #[test]
    fn glob_match_star_only() {
        assert!(super::glob_match("*", "anything"));
        assert!(super::glob_match("*", ""));
    }

    #[test]
    fn is_excluded_glob_star() {
        let scanner = make_scanner(vec!["*.log"]);
        assert!(scanner.is_excluded(Path::new("/project/logs/error.log")));
        assert!(!scanner.is_excluded(Path::new("/project/src/main.rs")));
    }

    #[test]
    fn is_excluded_glob_question() {
        let scanner = make_scanner(vec!["temp?"]);
        assert!(scanner.is_excluded(Path::new("/project/temp1/data")));
        assert!(!scanner.is_excluded(Path::new("/project/temp12/data")));
        assert!(!scanner.is_excluded(Path::new("/project/temp/data")));
    }

    /// Windows 文件系统大小写不敏感：排除模式 "*.LOG" 应匹配 "error.log"
    #[test]
    #[cfg(target_os = "windows")]
    fn is_excluded_case_insensitive_windows() {
        let scanner = make_scanner(vec!["*.LOG"]);
        assert!(scanner.is_excluded(Path::new("C:\\project\\logs\\error.log")));
        assert!(scanner.is_excluded(Path::new("C:\\project\\logs\\ERROR.LOG")));

        let scanner2 = make_scanner(vec!["node_modules"]);
        assert!(scanner2.is_excluded(Path::new("C:\\project\\Node_Modules\\pkg\\index.js")));
    }

    #[test]
    fn is_excluded_glob_multi_level() {
        let scanner = make_scanner(vec![".git/obj*"]);
        assert!(scanner.is_excluded(Path::new("/project/.git/objects/pack")));
        assert!(!scanner.is_excluded(Path::new("/project/.git/config")));
    }

    // --- ** 递归通配符测试 ---

    #[test]
    fn is_excluded_double_star_basic() {
        // src/**/test.rs 应匹配 src 下任意深度的 test.rs
        let scanner = make_scanner(vec!["src/**/test.rs"]);
        assert!(scanner.is_excluded(Path::new("/project/src/test.rs"))); // 零层
        assert!(scanner.is_excluded(Path::new("/project/src/a/test.rs"))); // 一层
        assert!(scanner.is_excluded(Path::new("/project/src/a/b/c/test.rs"))); // 三层
        assert!(!scanner.is_excluded(Path::new("/project/src/main.rs"))); // 文件名不匹配
        assert!(!scanner.is_excluded(Path::new("/project/lib/test.rs"))); // 前缀不匹配
    }

    #[test]
    fn is_excluded_double_star_leading() {
        // **/*.log 应匹配任意深度的 .log 文件
        let scanner = make_scanner(vec!["**/*.log"]);
        assert!(scanner.is_excluded(Path::new("/project/error.log")));
        assert!(scanner.is_excluded(Path::new("/project/logs/error.log")));
        assert!(scanner.is_excluded(Path::new("/project/a/b/c/debug.log")));
        assert!(!scanner.is_excluded(Path::new("/project/error.txt")));
    }

    #[test]
    fn is_excluded_double_star_trailing() {
        // build/** 应匹配 build 下的所有内容
        let scanner = make_scanner(vec!["build/**"]);
        assert!(scanner.is_excluded(Path::new("/project/build/output.o")));
        assert!(scanner.is_excluded(Path::new("/project/build/debug/binary")));
        assert!(!scanner.is_excluded(Path::new("/project/src/main.rs")));
    }

    #[test]
    fn is_excluded_double_star_middle_multi() {
        // a/**/b/**/c 应匹配有 a...b...c 结构的路径
        let scanner = make_scanner(vec!["a/**/b/**/c"]);
        assert!(scanner.is_excluded(Path::new("/project/a/b/c"))); // 两个 ** 都匹配零层
        assert!(scanner.is_excluded(Path::new("/project/a/x/b/y/c")));
        assert!(scanner.is_excluded(Path::new("/project/a/x/y/b/z/c")));
        assert!(!scanner.is_excluded(Path::new("/project/a/c"))); // 缺少 b
    }

    #[test]
    fn is_excluded_double_star_only() {
        // ** 单独使用应匹配任意路径（极端情况）
        let scanner = make_scanner(vec!["**/node_modules/**"]);
        assert!(scanner.is_excluded(Path::new("/project/node_modules/pkg/index.js")));
        assert!(scanner.is_excluded(Path::new("/project/sub/node_modules/deep/file.js")));
        assert!(!scanner.is_excluded(Path::new("/project/src/main.rs")));
    }

    #[test]
    fn match_components_recursive_basic() {
        // 直接测试递归匹配函数
        assert!(super::match_components_recursive(
            &["src", "**", "test.rs"],
            &["src", "test.rs"]
        ));
        assert!(super::match_components_recursive(
            &["src", "**", "test.rs"],
            &["src", "a", "test.rs"]
        ));
        assert!(super::match_components_recursive(
            &["src", "**", "test.rs"],
            &["src", "a", "b", "test.rs"]
        ));
        assert!(!super::match_components_recursive(
            &["src", "**", "test.rs"],
            &["lib", "test.rs"]
        ));
    }

    #[test]
    fn match_components_recursive_empty() {
        assert!(super::match_components_recursive(&[], &[]));
        assert!(!super::match_components_recursive(&[], &["a"]));
        assert!(!super::match_components_recursive(&["a"], &[]));
    }

    #[test]
    fn match_components_recursive_double_star_only() {
        // ** 单独应匹配零或多个组件
        assert!(super::match_components_recursive(&["**"], &[]));
        assert!(super::match_components_recursive(&["**"], &["a"]));
        assert!(super::match_components_recursive(&["**"], &["a", "b"]));
    }

    // --- merge_deep_results 测试 ---

    #[test]
    fn merge_deep_results_new_file() {
        // 深度扫描发现的新文件应直接添加
        let mut locked = Vec::new();
        let deep = vec![crate::model::FileLockInfo {
            path: std::path::PathBuf::from("/a/file.txt"),
            lockers: vec![ProcessInfo::new(
                100,
                "proc".to_string(),
                crate::model::LockType::FileHandle,
                None,
                None,
            )],
        }];
        super::merge_deep_results(&mut locked, deep);
        assert_eq!(locked.len(), 1);
        assert_eq!(locked[0].lockers.len(), 1);
    }

    #[test]
    fn merge_deep_results_dedup_same_pid_type() {
        // 同一文件、同一 pid + lock_type 应去重
        let mut locked = vec![crate::model::FileLockInfo {
            path: std::path::PathBuf::from("/a/file.txt"),
            lockers: vec![ProcessInfo::new(
                100,
                "proc".to_string(),
                crate::model::LockType::FileHandle,
                None,
                None,
            )],
        }];
        let deep = vec![crate::model::FileLockInfo {
            path: std::path::PathBuf::from("/a/file.txt"),
            lockers: vec![ProcessInfo::new(
                100,
                "proc".to_string(),
                crate::model::LockType::FileHandle,
                None,
                None,
            )],
        }];
        super::merge_deep_results(&mut locked, deep);
        assert_eq!(locked.len(), 1);
        assert_eq!(locked[0].lockers.len(), 1, "Duplicate should be deduped");
    }

    #[test]
    fn merge_deep_results_merge_different_type() {
        // 同一文件、同一 pid 但不同 lock_type 应保留
        let mut locked = vec![crate::model::FileLockInfo {
            path: std::path::PathBuf::from("/a/file.txt"),
            lockers: vec![ProcessInfo::new(
                100,
                "proc".to_string(),
                crate::model::LockType::FileHandle,
                None,
                None,
            )],
        }];
        let deep = vec![crate::model::FileLockInfo {
            path: std::path::PathBuf::from("/a/file.txt"),
            lockers: vec![ProcessInfo::new(
                100,
                "proc".to_string(),
                crate::model::LockType::MemoryMap,
                None,
                None,
            )],
        }];
        super::merge_deep_results(&mut locked, deep);
        assert_eq!(locked.len(), 1);
        assert_eq!(
            locked[0].lockers.len(),
            2,
            "Different lock_type should be kept"
        );
    }

    #[test]
    fn merge_deep_results_merge_different_pid() {
        // 同一文件、不同 pid 应保留
        let mut locked = vec![crate::model::FileLockInfo {
            path: std::path::PathBuf::from("/a/file.txt"),
            lockers: vec![ProcessInfo::new(
                100,
                "proc_a".to_string(),
                crate::model::LockType::FileHandle,
                None,
                None,
            )],
        }];
        let deep = vec![crate::model::FileLockInfo {
            path: std::path::PathBuf::from("/a/file.txt"),
            lockers: vec![ProcessInfo::new(
                200,
                "proc_b".to_string(),
                crate::model::LockType::FileHandle,
                None,
                None,
            )],
        }];
        super::merge_deep_results(&mut locked, deep);
        assert_eq!(locked.len(), 1);
        assert_eq!(
            locked[0].lockers.len(),
            2,
            "Different PIDs should both be kept"
        );
    }

    #[test]
    fn merge_deep_results_empty() {
        // 空的深度结果不应改变已有列表
        let mut locked = vec![crate::model::FileLockInfo {
            path: std::path::PathBuf::from("/a/file.txt"),
            lockers: vec![ProcessInfo::new(
                100,
                "proc".to_string(),
                crate::model::LockType::FileHandle,
                None,
                None,
            )],
        }];
        super::merge_deep_results(&mut locked, Vec::new());
        assert_eq!(locked.len(), 1);
        assert_eq!(locked[0].lockers.len(), 1);
    }
}
