use crate::detector;
use crate::gui::state::WorkerMsg;
use crate::killer;
use crate::scan::{self, Scanner};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

pub struct ScanRequest {
    pub paths: Vec<PathBuf>,
    pub max_depth: Option<usize>,
    pub follow_symlinks: bool,
    pub exclude_patterns: Vec<String>,
    pub chinese: bool,
    pub cancel_flag: Arc<AtomicBool>,
}

pub fn spawn_scan(req: ScanRequest, tx: mpsc::Sender<WorkerMsg>) {
    std::thread::spawn(move || {
        let mut all_locked = Vec::new();
        let mut total_scanned = 0;
        let mut all_errors = Vec::new();
        let start = std::time::Instant::now();

        // 在循环外创建检测器和扫描器，复用进程信息缓存（Windows sys_cache）
        let det = detector::create_detector();
        let tx_p = tx.clone();
        let progress: scan::ProgressCallback = Box::new(move |msg: &str| {
            let _ = tx_p.send(WorkerMsg::Progress(msg.to_string()));
        });
        let scanner = Scanner::new(
            det,
            req.max_depth,
            req.follow_symlinks,
            req.exclude_patterns.clone(),
            req.chinese,
        )
        .with_progress(progress)
        .with_cancel(req.cancel_flag.clone());

        for path in &req.paths {
            if req.cancel_flag.load(Ordering::Relaxed) {
                break;
            }

            let result = scanner.scan(path);
            total_scanned += result.total_files_scanned;
            all_errors.extend(result.errors);
            all_locked.extend(result.locked_files);
        }

        if req.cancel_flag.load(Ordering::Relaxed) {
            let _ = tx.send(WorkerMsg::Cancelled);
            return;
        }

        let merged = crate::model::ScanResult {
            targets: req.paths.clone(),
            total_files_scanned: total_scanned,
            locked_files: all_locked,
            errors: all_errors,
            elapsed: start.elapsed(),
        };

        let _ = tx.send(WorkerMsg::Completed(Box::new(merged)));
    });
}

pub fn spawn_kill(pids: Vec<u32>, force: bool, chinese: bool, tx: mpsc::Sender<WorkerMsg>) {
    std::thread::spawn(move || {
        let k = killer::create_killer();
        for pid in pids {
            let result = if force {
                k.kill_force(pid)
            } else {
                k.kill_graceful(pid)
            };
            let (success, msg) = match result {
                Ok(()) => (
                    true,
                    if chinese {
                        format!("PID {} 已终止", pid)
                    } else {
                        format!("PID {} terminated", pid)
                    },
                ),
                Err(e) => (
                    false,
                    if chinese {
                        format!("PID {} 终止失败: {}", pid, e)
                    } else {
                        format!("PID {} failed: {}", pid, e)
                    },
                ),
            };
            let _ = tx.send(WorkerMsg::KillResult { pid, success, msg });
        }
        let _ = tx.send(WorkerMsg::KillAllDone);
    });
}
