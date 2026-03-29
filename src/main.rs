#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod detector;
mod error;
mod gui;
mod killer;
mod model;
mod res;
mod scan;

fn main() {
    // 初始化日志系统（通过 RUST_LOG 环境变量控制级别，如 RUST_LOG=debug）
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    if !res::init_res_table() {
        // 有命令行参数时用 stderr 输出错误，无参数时弹 GUI 对话框
        if std::env::args().len() > 1 {
            #[cfg(target_os = "windows")]
            cli::attach_console();
            eprintln!("Error: Author attribution has been modified. Program cannot start.");
        } else {
            let _ = rfd::MessageDialog::new()
                .set_title("Error")
                .set_description("Author attribution has been modified.\nProgram cannot start.\n\nPlease restore the original files.")
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
        std::process::exit(78);
    }

    // 有命令行参数时进入 CLI 模式，否则启动 GUI
    if std::env::args().len() > 1 {
        #[cfg(target_os = "windows")]
        cli::attach_console();
        cli::run_cli();
    } else {
        gui::run_gui();
    }
}
