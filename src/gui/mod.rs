pub mod export;
pub mod i18n;
pub mod panels;
pub mod state;
pub mod worker;

use eframe::egui;
use i18n::{Lang, T};
use state::{GuiState, ScanPhase, WorkerMsg};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

struct WhoLocksApp {
    state: GuiState,
    rx: Option<mpsc::Receiver<WorkerMsg>>,
    lang: Lang,
    fonts_loaded: bool,
    cjk_font_ok: bool,
    admin: bool,
    pending_rescan: bool,
}

impl WhoLocksApp {
    fn new(admin: bool) -> Self {
        Self {
            state: GuiState::default(),
            rx: None,
            lang: i18n::detect_system_lang(),
            fonts_loaded: false,
            cjk_font_ok: true,
            admin,
            pending_rescan: false,
        }
    }

    fn poll_messages(&mut self, ctx: &egui::Context) {
        if let Some(ref rx) = self.rx {
            let mut drop_rx = false;
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    WorkerMsg::Progress(text) => {
                        self.state.progress_text = text;
                    }
                    WorkerMsg::Completed(result) => {
                        self.state.apply_result(*result);
                        self.state.phase = ScanPhase::Done;
                        self.state.cancel_flag = None;
                        drop_rx = true;
                    }
                    WorkerMsg::Cancelled => {
                        self.state.phase = ScanPhase::Idle;
                        self.state.cancel_flag = None;
                        self.state.progress_text.clear();
                        self.state.status_msg = Some((
                            T::scan_cancelled(self.lang).to_string(),
                            std::time::Instant::now(),
                        ));
                        drop_rx = true;
                    }
                    WorkerMsg::KillResult { pid, success, msg } => {
                        let prefix = if success { "OK" } else { "FAIL" };
                        self.state.status_msg =
                            Some((format!("{}: {}", prefix, msg), std::time::Instant::now()));
                        if success {
                            self.state.rows.retain(|r| r.pid != pid);
                            self.state.selected.clear();
                        }
                    }
                    WorkerMsg::KillAllDone => {
                        // 终止完成后自动重新扫描，验证文件锁是否已释放
                        if !self.state.paths.is_empty() {
                            self.pending_rescan = true;
                        }
                        drop_rx = true;
                    }
                }
            }
            if drop_rx {
                self.rx = None;
            }
        }

        if self.state.phase == ScanPhase::Scanning {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            if let Some(path) = file.path {
                if !self.state.paths.contains(&path) {
                    log::info!("File dropped: {}", path.display());
                    self.state.paths.push(path);
                }
            }
        }
    }

    fn start_scan(&mut self) {
        let depth: Option<usize> = self.state.depth_input.trim().parse().ok();
        let max_depth = if self.state.include_subdirs {
            depth
        } else {
            Some(1)
        };

        let exclude: Vec<String> = self
            .state
            .exclude_input
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // 丢弃旧的 channel（防止旧扫描的残留消息被新 rx 接收）
        self.rx = None;
        self.state.cancel_flag = None;

        let (tx, rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.rx = Some(rx);
        self.state.cancel_flag = Some(cancel_flag.clone());
        self.state.phase = ScanPhase::Scanning;
        self.state.progress_text = T::preparing(self.lang).to_string();
        // 彻底清空旧结果，防止闪现
        self.state.rows.clear();
        self.state.selected.clear();
        self.state.errors.clear();
        self.state.total_files = 0;
        self.state.elapsed_secs = 0.0;
        self.state.status_msg = None;

        worker::spawn_scan(
            worker::ScanRequest {
                paths: self.state.paths.clone(),
                max_depth,
                follow_symlinks: self.state.follow_symlinks,
                exclude_patterns: exclude,
                chinese: self.lang == Lang::Chinese,
                cancel_flag,
            },
            tx,
        );
    }

    fn cancel_scan(&mut self) {
        if let Some(flag) = &self.state.cancel_flag {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

impl eframe::App for WhoLocksApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.fonts_loaded {
            self.cjk_font_ok = i18n::setup_fonts(ctx);
            egui_extras::install_image_loaders(ctx);
            self.fonts_loaded = true;
        }

        self.poll_messages(ctx);

        // 终止进程后自动重新扫描
        if self.pending_rescan {
            self.pending_rescan = false;
            self.state.status_msg = Some((
                if self.lang == Lang::Chinese {
                    "正在重新扫描以验证...".to_string()
                } else {
                    "Re-scanning to verify...".to_string()
                },
                std::time::Instant::now(),
            ));
            self.start_scan();
        }

        self.state.tick_status();
        self.handle_dropped_files(ctx);

        let lang = self.lang;

        // 顶部工具栏
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            // 语言切换按钮（右上角）
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(lang.label()).clicked() {
                        self.lang = lang.toggle();
                    }
                });
            });

            let action = panels::render_toolbar(ui, &mut self.state, lang);
            match action {
                panels::ToolbarAction::StartScan => self.start_scan(),
                panels::ToolbarAction::CancelScan => self.cancel_scan(),
                panels::ToolbarAction::None => {}
            }
        });

        // 底部状态栏
        egui::TopBottomPanel::bottom("footer")
            .min_height(28.0)
            .show(ctx, |ui| {
                panels::render_footer(ui, &mut self.state, lang, self.admin, self.cjk_font_ok);
            });

        // 中央结果区（操作栏 + 搜索 + 表格 全部在这里，表格铺满剩余空间）
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some((pids, force)) = panels::render_action_bar(ui, &mut self.state, lang) {
                self.state.confirm_kill = Some((pids, force));
            }
            ui.separator();
            panels::render_results(ui, &mut self.state, lang);
        });

        // 确认对话框
        if let Some((pids, force)) = panels::render_confirm_dialog(ctx, &mut self.state, lang) {
            let (tx, rx) = mpsc::channel();
            self.rx = Some(rx);
            worker::spawn_kill(pids, force, self.lang == Lang::Chinese, tx);
        }

        // 错误详情弹窗
        panels::render_errors_dialog(ctx, &mut self.state, lang);

        // 打赏弹窗
        panels::render_donate_dialog(ctx, &mut self.state, lang);
    }
}

/// 检测当前进程是否以管理员权限运行
#[cfg(target_os = "windows")]
fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    // TokenElevation = 20, TOKEN_ELEVATION struct = { TokenIsElevated: u32 }
    const TOKEN_ELEVATION_CLASS: i32 = 20;

    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation: u32 = 0;
        let mut size: u32 = 0;
        let result = GetTokenInformation(
            token,
            TOKEN_ELEVATION_CLASS,
            &mut elevation as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
            &mut size,
        );
        CloseHandle(token);
        result != 0 && elevation != 0
    }
}

#[cfg(unix)]
fn is_elevated() -> bool {
    nix::unistd::geteuid().is_root()
}

#[cfg(not(any(target_os = "windows", unix)))]
fn is_elevated() -> bool {
    false
}

pub fn run_gui() {
    let admin = is_elevated();
    let version = env!("CARGO_PKG_VERSION");
    let title = if admin {
        format!("who-locks v{} - File Lock Detector [Admin]", version)
    } else {
        format!("who-locks v{} - File Lock Detector", version)
    };

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon.png"))
        .expect("Failed to load app icon");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_min_inner_size([640.0, 400.0])
            .with_title(title)
            .with_icon(std::sync::Arc::new(icon)),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "who-locks",
        options,
        Box::new(move |_cc| Ok(Box::new(WhoLocksApp::new(admin)))),
    );
}
