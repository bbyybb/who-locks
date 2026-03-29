use crate::model::ScanResult;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum ScanPhase {
    Idle,
    Scanning,
    Done,
}

pub enum WorkerMsg {
    Progress(String),
    Completed(Box<ScanResult>),
    Cancelled,
    KillResult {
        pid: u32,
        success: bool,
        msg: String,
    },
    KillAllDone,
}

#[derive(Clone)]
pub struct ResultRow {
    pub file_path: String,
    pub pid: u32,
    pub proc_name: String,
    pub lock_type: String,
    pub cmdline: String,
    pub user: String,
    pub blocking: bool,
}

/// 排序列标识
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortColumn {
    FilePath,
    Pid,
    ProcName,
    LockType,
    CmdLine,
    User,
}

/// 排序方向
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

pub struct GuiState {
    pub paths: Vec<PathBuf>,
    pub path_input: String,
    pub include_subdirs: bool,
    pub follow_symlinks: bool,
    pub depth_input: String,
    pub exclude_input: String,

    pub phase: ScanPhase,
    pub progress_text: String,

    pub rows: Vec<ResultRow>,
    pub selected: HashSet<usize>,
    pub total_files: usize,
    pub elapsed_secs: f64,
    pub errors: Vec<String>,
    pub search_filter: String,

    pub sort_column: Option<SortColumn>,
    pub sort_order: SortOrder,

    pub cancel_flag: Option<Arc<AtomicBool>>,

    pub confirm_kill: Option<(Vec<u32>, bool)>,
    pub status_msg: Option<(String, Instant)>,
    pub show_donate: bool,
    pub donate_tab: usize, // 0=微信 1=支付宝 2=BMC
    pub show_errors: bool,
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            path_input: String::new(),
            include_subdirs: true,
            follow_symlinks: false,
            depth_input: String::new(),
            exclude_input: String::new(),

            phase: ScanPhase::Idle,
            progress_text: String::new(),

            rows: Vec::new(),
            selected: HashSet::new(),
            total_files: 0,
            elapsed_secs: 0.0,
            errors: Vec::new(),
            search_filter: String::new(),

            sort_column: None,
            sort_order: SortOrder::Ascending,

            cancel_flag: None,

            confirm_kill: None,
            status_msg: None,
            show_donate: false,
            donate_tab: 0,
            show_errors: false,
        }
    }
}

impl GuiState {
    /// 切换排序列：点击同一列切换升降序，点击不同列默认升序
    pub fn toggle_sort(&mut self, col: SortColumn) {
        if self.sort_column == Some(col) {
            self.sort_order = match self.sort_order {
                SortOrder::Ascending => SortOrder::Descending,
                SortOrder::Descending => SortOrder::Ascending,
            };
        } else {
            self.sort_column = Some(col);
            self.sort_order = SortOrder::Ascending;
        }
    }

    /// 排序指示符
    pub fn sort_indicator(&self, col: SortColumn) -> &str {
        if self.sort_column == Some(col) {
            match self.sort_order {
                SortOrder::Ascending => " ^",
                SortOrder::Descending => " v",
            }
        } else {
            ""
        }
    }

    pub fn filtered_rows(&self) -> Vec<(usize, &ResultRow)> {
        let mut result: Vec<(usize, &ResultRow)> = if self.search_filter.is_empty() {
            self.rows.iter().enumerate().collect()
        } else {
            let q = self.search_filter.to_lowercase();
            self.rows
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    r.file_path.to_lowercase().contains(&q)
                        || r.proc_name.to_lowercase().contains(&q)
                        || r.cmdline.to_lowercase().contains(&q)
                        || r.pid.to_string().contains(&q)
                        || r.lock_type.to_lowercase().contains(&q)
                })
                .collect()
        };

        // 排序
        if let Some(col) = self.sort_column {
            result.sort_by(|(_, a), (_, b)| {
                let cmp = match col {
                    SortColumn::FilePath => {
                        a.file_path.to_lowercase().cmp(&b.file_path.to_lowercase())
                    }
                    SortColumn::Pid => a.pid.cmp(&b.pid),
                    SortColumn::ProcName => {
                        a.proc_name.to_lowercase().cmp(&b.proc_name.to_lowercase())
                    }
                    SortColumn::LockType => a.lock_type.cmp(&b.lock_type),
                    SortColumn::CmdLine => a.cmdline.to_lowercase().cmp(&b.cmdline.to_lowercase()),
                    SortColumn::User => a.user.to_lowercase().cmp(&b.user.to_lowercase()),
                };
                if self.sort_order == SortOrder::Descending {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }

        result
    }

    pub fn selected_pids(&self) -> Vec<u32> {
        let filtered = self.filtered_rows();
        self.selected
            .iter()
            .filter_map(|&i| {
                filtered
                    .iter()
                    .find(|(idx, _)| *idx == i)
                    .map(|(_, r)| r.pid)
            })
            .collect::<HashSet<u32>>()
            .into_iter()
            .collect()
    }

    pub fn apply_result(&mut self, result: ScanResult) {
        self.total_files = result.total_files_scanned;
        self.elapsed_secs = result.elapsed.as_secs_f64();
        self.errors = result
            .errors
            .iter()
            .map(|e| format!("{}: {}", e.path.display(), e.reason))
            .collect();

        self.rows.clear();
        self.selected.clear();

        for file_info in &result.locked_files {
            let display_path = Self::compute_display_path(&file_info.path);

            for proc in &file_info.lockers {
                self.rows.push(ResultRow {
                    file_path: display_path.clone(),
                    pid: proc.pid,
                    proc_name: proc.name.clone(),
                    lock_type: proc.lock_type.to_string(),
                    cmdline: proc.cmdline.clone().unwrap_or_default(),
                    user: proc.user.clone().unwrap_or_default(),
                    blocking: proc.blocking,
                });
            }
        }
    }

    /// 计算显示路径（始终使用绝对路径，更直观清晰）
    fn compute_display_path(file_path: &std::path::Path) -> String {
        file_path.display().to_string()
    }

    pub fn tick_status(&mut self) {
        if let Some((_, created)) = &self.status_msg {
            if created.elapsed().as_secs() > 5 {
                self.status_msg = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_row(file_path: &str, pid: u32, blocking: bool) -> ResultRow {
        ResultRow {
            file_path: file_path.to_string(),
            pid,
            proc_name: "test.exe".to_string(),
            lock_type: "File Handle".to_string(),
            cmdline: String::new(),
            user: String::new(),
            blocking,
        }
    }

    #[test]
    fn filtered_rows_empty_filter() {
        let mut state = GuiState::default();
        state.rows = vec![make_row("a.txt", 1, true), make_row("b.txt", 2, true)];
        let filtered = state.filtered_rows();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filtered_rows_by_filename() {
        let mut state = GuiState::default();
        state.rows = vec![make_row("a.txt", 1, true), make_row("b.txt", 2, true)];
        state.search_filter = "a.txt".to_string();
        let filtered = state.filtered_rows();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1.file_path, "a.txt");
    }

    #[test]
    fn filtered_rows_by_pid() {
        let mut state = GuiState::default();
        state.rows = vec![make_row("a.txt", 123, true), make_row("b.txt", 456, true)];
        state.search_filter = "456".to_string();
        let filtered = state.filtered_rows();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1.pid, 456);
    }

    #[test]
    fn filtered_rows_case_insensitive() {
        let mut state = GuiState::default();
        state.rows = vec![make_row("README.TXT", 1, true)];
        state.search_filter = "readme".to_string();
        let filtered = state.filtered_rows();
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn selected_pids_dedup() {
        let mut state = GuiState::default();
        state.rows = vec![
            make_row("a.txt", 100, true),
            make_row("b.txt", 100, true),
            make_row("c.txt", 200, true),
        ];
        state.selected.insert(0);
        state.selected.insert(1);
        let pids = state.selected_pids();
        // PID 100 appears twice in rows but should be deduplicated
        assert!(pids.contains(&100));
        assert!(!pids.contains(&200));
    }

    #[test]
    fn compute_display_path_always_absolute() {
        let path = std::path::Path::new("/tmp/data/file.txt");
        let display = GuiState::compute_display_path(path);
        assert_eq!(display, "/tmp/data/file.txt");
    }

    #[test]
    fn compute_display_path_no_matching_base() {
        let path = std::path::Path::new("/tmp/other/file.txt");
        let display = GuiState::compute_display_path(path);
        assert_eq!(display, "/tmp/other/file.txt");
    }

    #[test]
    fn compute_display_path_exact_target() {
        let path = std::path::Path::new("/home/user/project");
        let display = GuiState::compute_display_path(path);
        assert_eq!(display, "/home/user/project");
    }
}
