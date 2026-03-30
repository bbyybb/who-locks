use crate::gui::i18n::{Lang, T};
use crate::gui::state::{GuiState, ResultRow, ScanPhase, SortColumn};
use eframe::egui;

pub enum ToolbarAction {
    None,
    StartScan,
    CancelScan,
}

pub fn render_toolbar(ui: &mut egui::Ui, state: &mut GuiState, lang: Lang) -> ToolbarAction {
    let mut action = ToolbarAction::None;

    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(format!("{}:", T::path(lang)));
        let resp = ui.add_sized(
            [ui.available_width() - 240.0, 22.0],
            egui::TextEdit::singleline(&mut state.path_input).hint_text(T::input_hint(lang)),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let p = std::path::PathBuf::from(state.path_input.trim());
            if p.exists() && !state.paths.contains(&p) {
                state.paths.push(p);
                state.path_input.clear();
            }
        }

        if ui.button(T::select_file(lang)).clicked() {
            if let Some(files) = rfd::FileDialog::new().pick_files() {
                for f in files {
                    if !state.paths.contains(&f) {
                        state.paths.push(f);
                    }
                }
            }
        }
        if ui.button(T::select_folder(lang)).clicked() {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                if !state.paths.contains(&dir) {
                    state.paths.push(dir);
                }
            }
        }
    });

    if !state.paths.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{}:", T::selected(lang)));
            let mut to_remove = None;
            for (i, p) in state.paths.iter().enumerate() {
                ui.label(
                    egui::RichText::new(p.display().to_string())
                        .color(egui::Color32::from_rgb(100, 180, 255))
                        .small(),
                );
                if ui.small_button("x").clicked() {
                    to_remove = Some(i);
                }
                ui.label(" ");
            }
            if let Some(i) = to_remove {
                state.paths.remove(i);
            }
        });
    }

    ui.horizontal(|ui| {
        ui.checkbox(&mut state.include_subdirs, T::include_subdirs(lang));
        ui.checkbox(&mut state.follow_symlinks, T::follow_symlinks(lang));
        ui.label(format!("{}:", T::depth(lang)));
        ui.add_sized(
            [40.0, 18.0],
            egui::TextEdit::singleline(&mut state.depth_input).hint_text(T::depth_hint(lang)),
        );
        ui.label(format!("{}:", T::exclude(lang)));
        ui.add_sized(
            [ui.available_width() - 20.0, 18.0],
            egui::TextEdit::singleline(&mut state.exclude_input).hint_text(T::exclude_hint(lang)),
        );
    });

    ui.horizontal(|ui| {
        let scanning = state.phase == ScanPhase::Scanning;
        let has_paths = !state.paths.is_empty();

        ui.add_enabled_ui(!scanning && has_paths, |ui| {
            if ui
                .button(egui::RichText::new(T::scan(lang)).size(15.0))
                .clicked()
            {
                action = ToolbarAction::StartScan;
            }
        });

        ui.add_enabled_ui(!scanning && state.phase == ScanPhase::Done, |ui| {
            if ui.button(T::refresh(lang)).clicked() {
                action = ToolbarAction::StartScan;
            }
        });

        if scanning {
            if ui
                .button(
                    egui::RichText::new(T::cancel_scan(lang))
                        .color(egui::Color32::from_rgb(220, 80, 80)),
                )
                .clicked()
            {
                action = ToolbarAction::CancelScan;
            }
            ui.spinner();
            ui.label(
                egui::RichText::new(&state.progress_text)
                    .color(egui::Color32::from_rgb(150, 150, 150)),
            );
        }

        if !has_paths && !scanning {
            ui.label(
                egui::RichText::new(T::please_select(lang))
                    .color(egui::Color32::from_rgb(200, 150, 50)),
            );
        }
    });

    ui.add_space(2.0);
    ui.separator();

    action
}

/// 渲染可排序的列标题按钮
fn sort_header_btn(ui: &mut egui::Ui, label: String, col: SortColumn, state: &mut GuiState) {
    if ui
        .add_sized(
            ui.available_size(),
            egui::Button::new(egui::RichText::new(label).strong()).frame(false),
        )
        .clicked()
    {
        state.toggle_sort(col);
    }
}

pub fn render_results(ui: &mut egui::Ui, state: &mut GuiState, lang: Lang) {
    ui.horizontal(|ui| {
        ui.label(format!("{}:", T::search(lang)));
        ui.add_sized(
            [300.0, 18.0],
            egui::TextEdit::singleline(&mut state.search_filter).hint_text("..."),
        );
        if !state.search_filter.is_empty() && ui.small_button(T::clear(lang)).clicked() {
            state.search_filter.clear();
        }
    });

    ui.add_space(4.0);

    if state.phase == ScanPhase::Idle && state.rows.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(T::select_hint(lang))
                        .size(16.0)
                        .color(egui::Color32::from_rgb(140, 140, 140)),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(T::drop_hint(lang))
                        .size(13.0)
                        .color(egui::Color32::from_rgb(110, 110, 110)),
                );
            });
        });
        return;
    }

    if state.phase == ScanPhase::Done && state.rows.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(T::no_results(lang))
                    .size(16.0)
                    .color(egui::Color32::from_rgb(80, 200, 80)),
            );
        });
        return;
    }

    let text_height = 18.0;
    let filtered_data: Vec<(usize, ResultRow)> = state
        .filtered_rows()
        .into_iter()
        .map(|(i, r)| (i, r.clone()))
        .collect();
    let num_rows = filtered_data.len();

    egui_extras::TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .vscroll(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(egui_extras::Column::exact(30.0)) // 勾选框
        .column(egui_extras::Column::remainder().at_least(120.0).clip(true)) // 文件路径
        .column(egui_extras::Column::initial(70.0).at_least(50.0).clip(true)) // PID
        .column(
            egui_extras::Column::initial(120.0)
                .at_least(60.0)
                .clip(true),
        ) // 进程名
        .column(egui_extras::Column::initial(80.0).at_least(50.0).clip(true)) // 占用类型
        .column(
            egui_extras::Column::initial(200.0)
                .at_least(60.0)
                .clip(true),
        ) // 命令行
        .column(
            egui_extras::Column::initial(100.0)
                .at_least(50.0)
                .clip(true),
        ) // 用户
        .header(text_height, |mut header| {
            header.col(|ui| {
                if ui.small_button("All").clicked() {
                    // 只全选阻塞性占用
                    let blocking_indices: std::collections::HashSet<usize> = filtered_data
                        .iter()
                        .filter(|(_, r)| r.blocking)
                        .map(|(i, _)| *i)
                        .collect();
                    if state.selected == blocking_indices {
                        state.selected.clear();
                    } else {
                        state.selected = blocking_indices;
                    }
                }
            });
            header.col(|ui| {
                sort_header_btn(
                    ui,
                    format!(
                        "{}{}",
                        T::file_path(lang),
                        state.sort_indicator(SortColumn::FilePath)
                    ),
                    SortColumn::FilePath,
                    state,
                );
            });
            header.col(|ui| {
                sort_header_btn(
                    ui,
                    format!("{}{}", T::pid(lang), state.sort_indicator(SortColumn::Pid)),
                    SortColumn::Pid,
                    state,
                );
            });
            header.col(|ui| {
                sort_header_btn(
                    ui,
                    format!(
                        "{}{}",
                        T::proc_name(lang),
                        state.sort_indicator(SortColumn::ProcName)
                    ),
                    SortColumn::ProcName,
                    state,
                );
            });
            header.col(|ui| {
                sort_header_btn(
                    ui,
                    format!(
                        "{}{}",
                        T::lock_type(lang),
                        state.sort_indicator(SortColumn::LockType)
                    ),
                    SortColumn::LockType,
                    state,
                );
            });
            header.col(|ui| {
                sort_header_btn(
                    ui,
                    format!(
                        "{}{}",
                        T::cmdline(lang),
                        state.sort_indicator(SortColumn::CmdLine)
                    ),
                    SortColumn::CmdLine,
                    state,
                );
            });
            header.col(|ui| {
                sort_header_btn(
                    ui,
                    format!(
                        "{}{}",
                        T::user(lang),
                        state.sort_indicator(SortColumn::User)
                    ),
                    SortColumn::User,
                    state,
                );
            });
        })
        .body(|body| {
            body.rows(text_height, num_rows, |mut row| {
                let idx = row.index();
                if let Some((orig_idx, r)) = filtered_data.get(idx) {
                    let orig_idx = *orig_idx;
                    let is_sel = state.selected.contains(&orig_idx);
                    let dim = egui::Color32::from_rgb(130, 130, 130);

                    // 非阻塞占用：灰色显示，不可勾选
                    row.col(|ui| {
                        if r.blocking {
                            let mut checked = is_sel;
                            if ui.checkbox(&mut checked, "").clicked() {
                                if checked {
                                    state.selected.insert(orig_idx);
                                } else {
                                    state.selected.remove(&orig_idx);
                                }
                            }
                        } else {
                            ui.label(egui::RichText::new("  -").color(dim));
                        }
                    });
                    row.col(|ui| {
                        let text = egui::RichText::new(&r.file_path);
                        let text = if r.blocking { text } else { text.color(dim) };
                        // clip(true) 裁剪时 egui 自动显示 tooltip，无需手动添加
                        ui.label(text);
                    });
                    row.col(|ui| {
                        let c = if r.blocking {
                            egui::Color32::from_rgb(220, 180, 50)
                        } else {
                            dim
                        };
                        ui.label(egui::RichText::new(r.pid.to_string()).color(c));
                    });
                    row.col(|ui| {
                        let c = if r.blocking {
                            egui::Color32::from_rgb(80, 200, 120)
                        } else {
                            dim
                        };
                        let label = if r.blocking {
                            r.proc_name.clone()
                        } else {
                            let tag = match lang {
                                Lang::Chinese => "非阻塞",
                                Lang::English => "non-blocking",
                            };
                            format!("{} ({})", r.proc_name, tag)
                        };
                        ui.label(egui::RichText::new(label).color(c));
                    });
                    row.col(|ui| {
                        let c = if r.blocking {
                            egui::Color32::from_rgb(180, 120, 220)
                        } else {
                            dim
                        };
                        let translated = T::lock_type_label(lang, &r.lock_type);
                        ui.label(egui::RichText::new(translated).color(c));
                    });
                    row.col(|ui| {
                        let cmd = if r.cmdline.is_empty() {
                            "-".to_string()
                        } else {
                            r.cmdline.clone()
                        };
                        let text = egui::RichText::new(&cmd);
                        // clip(true) 裁剪时 egui 自动显示 tooltip，无需手动添加
                        ui.label(if r.blocking { text } else { text.color(dim) });
                    });
                    row.col(|ui| {
                        let s = if r.user.is_empty() {
                            "-".to_string()
                        } else {
                            r.user.clone()
                        };
                        let text = egui::RichText::new(&s);
                        // clip(true) 裁剪时 egui 自动显示 tooltip，无需手动添加
                        ui.label(if r.blocking { text } else { text.color(dim) });
                    });
                }
            });
        });
}

pub fn render_action_bar(
    ui: &mut egui::Ui,
    state: &mut GuiState,
    lang: Lang,
) -> Option<(Vec<u32>, bool)> {
    let mut kill_request = None;

    ui.horizontal(|ui| {
        let sel_count = state.selected.len();
        let pids = state.selected_pids();

        if sel_count > 0 {
            ui.label(T::n_selected(lang, sel_count));
        }

        ui.add_enabled_ui(!pids.is_empty(), |ui| {
            let kill_btn = ui
                .button(T::kill(lang))
                .on_hover_text(T::kill_graceful_hint(lang));
            if kill_btn.clicked() {
                kill_request = Some((pids.clone(), false));
            }
            if ui.button(T::force_kill(lang)).clicked() {
                kill_request = Some((pids.clone(), true));
            }
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // 复制按钮：将选中行（或全部可见行）的信息复制到剪贴板
            if ui.button(T::copy(lang)).clicked() {
                let filtered = state.filtered_rows();
                let rows_to_copy: Vec<&ResultRow> = if state.selected.is_empty() {
                    filtered.iter().map(|(_, r)| *r).collect()
                } else {
                    filtered
                        .iter()
                        .filter(|(i, _)| state.selected.contains(i))
                        .map(|(_, r)| *r)
                        .collect()
                };
                if !rows_to_copy.is_empty() {
                    let mut text =
                        String::from("File Path\tPID\tProcess\tLock Type\tCommand\tUser\n");
                    for r in &rows_to_copy {
                        text.push_str(&format!(
                            "{}\t{}\t{}\t{}\t{}\t{}\n",
                            r.file_path,
                            r.pid,
                            r.proc_name,
                            r.lock_type,
                            if r.cmdline.is_empty() {
                                "-"
                            } else {
                                &r.cmdline
                            },
                            if r.user.is_empty() { "-" } else { &r.user },
                        ));
                    }
                    ui.output_mut(|o| o.copied_text = text);
                    state.status_msg =
                        Some((T::copied(lang).to_string(), std::time::Instant::now()));
                }
            }

            if ui.button(T::export_csv(lang)).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name("who-locks-result.csv")
                    .save_file()
                {
                    let msg = match crate::gui::export::export_csv(&state.rows, &path) {
                        Ok(()) => match lang {
                            Lang::Chinese => "CSV 导出成功".to_string(),
                            Lang::English => "CSV exported".to_string(),
                        },
                        Err(e) => match lang {
                            Lang::Chinese => format!("导出失败: {}", e),
                            Lang::English => format!("Export failed: {}", e),
                        },
                    };
                    state.status_msg = Some((msg, std::time::Instant::now()));
                }
            }
            if ui.button(T::export_json(lang)).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .set_file_name("who-locks-result.json")
                    .save_file()
                {
                    let msg = match crate::gui::export::export_json(&state.rows, &path) {
                        Ok(()) => match lang {
                            Lang::Chinese => "JSON 导出成功".to_string(),
                            Lang::English => "JSON exported".to_string(),
                        },
                        Err(e) => match lang {
                            Lang::Chinese => format!("导出失败: {}", e),
                            Lang::English => format!("Export failed: {}", e),
                        },
                    };
                    state.status_msg = Some((msg, std::time::Instant::now()));
                }
            }
        });
    });

    kill_request
}

pub fn render_footer(
    ui: &mut egui::Ui,
    state: &mut GuiState,
    lang: Lang,
    admin: bool,
    cjk_font_ok: bool,
) {
    ui.horizontal(|ui| {
        if !cjk_font_ok {
            ui.label(
                egui::RichText::new(T::cjk_font_missing(lang))
                    .small()
                    .color(egui::Color32::from_rgb(220, 130, 50)),
            );
            ui.separator();
        } else if !admin {
            let hint = match lang {
                Lang::Chinese => "建议以管理员身份运行（更快速、更完整的检测结果）",
                Lang::English => "Run as Admin for faster & more complete results",
            };
            ui.label(
                egui::RichText::new(hint)
                    .small()
                    .color(egui::Color32::from_rgb(200, 150, 50)),
            );
            ui.separator();
        }

        if state.phase == ScanPhase::Done {
            ui.label(T::stats(
                lang,
                state.total_files,
                state.rows.len(),
                state.elapsed_secs,
            ));
        }

        if let Some((msg, _)) = &state.status_msg {
            ui.label(egui::RichText::new(msg).color(egui::Color32::from_rgb(220, 180, 50)));
        }

        if !state.errors.is_empty()
            && ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(T::n_errors(lang, state.errors.len()))
                            .color(egui::Color32::from_rgb(220, 80, 80)),
                    )
                    .frame(false),
                )
                .on_hover_text(T::click_to_view_errors(lang))
                .clicked()
        {
            state.show_errors = true;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button(T::support(lang)).clicked() {
                state.show_donate = true;
            }
            ui.label(
                egui::RichText::new(crate::res::footer_line())
                    .small()
                    .color(egui::Color32::from_rgb(120, 120, 120)),
            );
        });
    });
}

const WECHAT_IMG: &[u8] = include_bytes!("../../docs/wechat_pay.jpg");
const ALIPAY_IMG: &[u8] = include_bytes!("../../docs/alipay.jpg");
const BMC_IMG: &[u8] = include_bytes!("../../docs/bmc_qr.png");

pub fn render_donate_dialog(ctx: &egui::Context, state: &mut GuiState, lang: Lang) {
    if !state.show_donate {
        return;
    }

    let title = match lang {
        Lang::Chinese => "打赏支持 / Support",
        Lang::English => "Support the Author",
    };

    let mut open = state.show_donate;

    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .fixed_size([300.0, 380.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let msg = match lang {
                Lang::Chinese => "如果这个工具对你有帮助，欢迎请作者喝杯咖啡 :)",
                Lang::English => "If this tool helps you, buy the author a coffee :)",
            };
            ui.vertical_centered(|ui| {
                ui.label(msg);
            });

            ui.add_space(8.0);

            // Tab 标签
            let tab_labels = [
                match lang {
                    Lang::Chinese => "微信支付",
                    Lang::English => "WeChat",
                },
                match lang {
                    Lang::Chinese => "支付宝",
                    Lang::English => "Alipay",
                },
                "Buy Me a Coffee",
            ];

            ui.horizontal(|ui| {
                for (i, label) in tab_labels.iter().enumerate() {
                    let selected = state.donate_tab == i;
                    let btn =
                        egui::Button::new(egui::RichText::new(*label).strong()).selected(selected);
                    if ui.add(btn).clicked() {
                        state.donate_tab = i;
                    }
                }
            });

            ui.add_space(8.0);

            // 显示当前 tab 的图片
            let uri = match state.donate_tab {
                0 => "bytes://wechat_pay",
                1 => "bytes://alipay",
                _ => "bytes://bmc",
            };

            let img_bytes: &[u8] = match state.donate_tab {
                0 => WECHAT_IMG,
                1 => ALIPAY_IMG,
                _ => BMC_IMG,
            };

            ui.vertical_centered(|ui| {
                ui.add(
                    egui::Image::new(egui::ImageSource::Bytes {
                        uri: uri.into(),
                        bytes: egui::load::Bytes::Static(img_bytes),
                    })
                    .fit_to_exact_size(egui::vec2(220.0, 220.0))
                    .rounding(8.0),
                );
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    if ui.link("buymeacoffee.com/bbyybb").clicked() {
                        let _ = open::that("https://www.buymeacoffee.com/bbyybb");
                    }
                    ui.label(" | ");
                    if ui.link("GitHub Sponsors").clicked() {
                        let _ = open::that("https://github.com/sponsors/bbyybb/");
                    }
                });
            });
        });

    state.show_donate = open;
}

pub fn render_confirm_dialog(
    ctx: &egui::Context,
    state: &mut GuiState,
    lang: Lang,
) -> Option<(Vec<u32>, bool)> {
    let mut result = None;

    if let Some((ref pids, force)) = state.confirm_kill.clone() {
        egui::Window::new(T::confirm_title(lang, force))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(T::confirm_msg(lang, pids.len()));
                for pid in pids {
                    ui.label(format!("  PID {}", pid));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(
                            egui::RichText::new(T::confirm(lang))
                                .color(egui::Color32::from_rgb(220, 80, 80)),
                        )
                        .clicked()
                    {
                        result = Some((pids.clone(), force));
                        state.confirm_kill = None;
                    }
                    if ui.button(T::cancel(lang)).clicked() {
                        state.confirm_kill = None;
                    }
                });
            });
    }

    result
}

pub fn render_errors_dialog(ctx: &egui::Context, state: &mut GuiState, lang: Lang) {
    if !state.show_errors || state.errors.is_empty() {
        state.show_errors = false;
        return;
    }

    let title = match lang {
        Lang::Chinese => format!("扫描错误 ({} 个)", state.errors.len()),
        Lang::English => format!("Scan Errors ({})", state.errors.len()),
    };

    let mut open = state.show_errors;

    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([500.0, 300.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(250.0)
                .show(ui, |ui| {
                    for (i, err) in state.errors.iter().enumerate() {
                        ui.label(
                            egui::RichText::new(format!("{}. {}", i + 1, err))
                                .small()
                                .color(egui::Color32::from_rgb(200, 100, 100)),
                        );
                    }
                });
        });

    state.show_errors = open;
}
