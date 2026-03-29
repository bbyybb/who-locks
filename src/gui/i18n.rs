use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lang {
    Chinese,
    English,
}

/// 根据系统区域设置自动检测应使用的界面语言
pub fn detect_system_lang() -> Lang {
    // 1. 检查常见的区域环境变量（跨平台）
    for var in ["LANG", "LC_ALL", "LC_MESSAGES", "LANGUAGE"] {
        if let Ok(val) = std::env::var(var) {
            if val.starts_with("zh") {
                return Lang::Chinese;
            }
            if !val.is_empty() && val != "C" && val != "POSIX" {
                return Lang::English;
            }
        }
    }

    // 2. Windows: 通过系统 API 检测 UI 语言
    #[cfg(target_os = "windows")]
    {
        unsafe {
            use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
            let lang_id = GetUserDefaultUILanguage();
            // 主语言 ID 0x04 = LANG_CHINESE（含 zh-CN、zh-TW 等）
            let primary = lang_id & 0x3FF;
            if primary == 0x04 {
                return Lang::Chinese;
            }
        }
        Lang::English
    }

    #[cfg(not(target_os = "windows"))]
    Lang::English
}

impl Lang {
    pub fn toggle(&self) -> Self {
        match self {
            Lang::Chinese => Lang::English,
            Lang::English => Lang::Chinese,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Lang::Chinese => "EN",
            Lang::English => "中文",
        }
    }
}

pub struct T;

impl T {
    pub fn path(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "路径",
            Lang::English => "Path",
        }
    }
    pub fn select_file(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "选择文件",
            Lang::English => "Files",
        }
    }
    pub fn select_folder(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "选择目录",
            Lang::English => "Folder",
        }
    }
    pub fn selected(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "已选",
            Lang::English => "Selected",
        }
    }
    pub fn include_subdirs(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "包含子目录",
            Lang::English => "Subdirs",
        }
    }
    pub fn follow_symlinks(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "跟随符号链接",
            Lang::English => "Follow symlinks",
        }
    }
    pub fn depth(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "深度",
            Lang::English => "Depth",
        }
    }
    pub fn exclude(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "排除",
            Lang::English => "Exclude",
        }
    }
    pub fn scan(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "  开始扫描  ",
            Lang::English => "  Scan  ",
        }
    }
    pub fn refresh(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "刷新",
            Lang::English => "Refresh",
        }
    }
    pub fn search(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "搜索",
            Lang::English => "Search",
        }
    }
    pub fn clear(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "清除",
            Lang::English => "Clear",
        }
    }
    pub fn file_path(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "文件路径",
            Lang::English => "File Path",
        }
    }
    pub fn pid(_l: Lang) -> &'static str {
        "PID"
    }
    pub fn proc_name(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "进程名",
            Lang::English => "Process",
        }
    }
    pub fn lock_type(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "占用类型",
            Lang::English => "Lock Type",
        }
    }
    pub fn cmdline(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "命令行",
            Lang::English => "Command",
        }
    }
    pub fn user(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "用户",
            Lang::English => "User",
        }
    }
    pub fn kill(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "终止进程",
            Lang::English => "Kill",
        }
    }
    pub fn force_kill(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "强制终止",
            Lang::English => "Force Kill",
        }
    }
    pub fn export_json(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "导出 JSON",
            Lang::English => "Export JSON",
        }
    }
    pub fn export_csv(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "导出 CSV",
            Lang::English => "Export CSV",
        }
    }
    pub fn support(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "打赏支持",
            Lang::English => "Support",
        }
    }
    pub fn no_results(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "没有发现文件被占用",
            Lang::English => "No locked files found",
        }
    }
    pub fn select_hint(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "选择文件或目录，然后点击「开始扫描」",
            Lang::English => "Select files or folders, then click Scan",
        }
    }
    pub fn input_hint(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "输入路径或点击右侧按钮选择...",
            Lang::English => "Enter path or click buttons...",
        }
    }
    pub fn preparing(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "准备扫描...",
            Lang::English => "Preparing...",
        }
    }
    pub fn please_select(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "请先选择文件或目录",
            Lang::English => "Please select files or folders first",
        }
    }
    pub fn n_selected(l: Lang, n: usize) -> String {
        match l {
            Lang::Chinese => format!("已选 {} 项", n),
            Lang::English => format!("{} selected", n),
        }
    }
    pub fn n_errors(l: Lang, n: usize) -> String {
        match l {
            Lang::Chinese => format!("{} 个错误", n),
            Lang::English => format!("{} errors", n),
        }
    }
    pub fn stats(l: Lang, files: usize, locks: usize, secs: f64) -> String {
        match l {
            Lang::Chinese => format!("{} 个文件 | {} 个占用 | {:.2}s", files, locks, secs),
            Lang::English => format!("{} files | {} locks | {:.2}s", files, locks, secs),
        }
    }
    pub fn confirm_title(l: Lang, force: bool) -> String {
        match (l, force) {
            (Lang::Chinese, false) => "确认终止".to_string(),
            (Lang::Chinese, true) => "确认强制终止".to_string(),
            (Lang::English, false) => "Confirm Kill".to_string(),
            (Lang::English, true) => "Confirm Force Kill".to_string(),
        }
    }
    pub fn confirm_msg(l: Lang, n: usize) -> String {
        match l {
            Lang::Chinese => format!("将要终止 {} 个进程:", n),
            Lang::English => format!("About to kill {} processes:", n),
        }
    }
    pub fn confirm(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "确认",
            Lang::English => "Confirm",
        }
    }
    pub fn cancel(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "取消",
            Lang::English => "Cancel",
        }
    }
    pub fn exclude_hint(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "node_modules, .git, *.log, src/**/test, **/*.tmp",
            Lang::English => "node_modules, .git, *.log, src/**/test, **/*.tmp",
        }
    }
    pub fn depth_hint(_l: Lang) -> &'static str {
        "∞"
    }
    pub fn drop_hint(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "拖拽文件或目录到此处",
            Lang::English => "Drop files or folders here",
        }
    }
    #[allow(dead_code)]
    pub fn downloading_handle(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "正在下载 handle64.exe...",
            Lang::English => "Downloading handle64.exe...",
        }
    }
    pub fn kill_graceful_hint(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "先尝试发送关闭请求（WM_CLOSE / SIGTERM），允许进程正常退出",
            Lang::English => "Sends close request (WM_CLOSE / SIGTERM), allows graceful exit",
        }
    }
    pub fn cancel_scan(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "取消扫描",
            Lang::English => "Cancel",
        }
    }
    pub fn scan_cancelled(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "扫描已取消",
            Lang::English => "Scan cancelled",
        }
    }
    pub fn click_to_view_errors(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "点击查看错误详情",
            Lang::English => "Click to view error details",
        }
    }
    pub fn copy(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "复制",
            Lang::English => "Copy",
        }
    }
    pub fn copied(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "已复制到剪贴板",
            Lang::English => "Copied to clipboard",
        }
    }
    pub fn cjk_font_missing(l: Lang) -> &'static str {
        match l {
            Lang::Chinese => "未找到中文字体，中文可能显示异常。请安装 Noto CJK 或文泉驿字体",
            Lang::English => {
                "CJK font not found. Install noto-cjk or wqy-microhei for Chinese support"
            }
        }
    }

    /// 将英文 LockType 名称翻译为当前语言
    pub fn lock_type_label(l: Lang, english: &str) -> String {
        if l == Lang::English {
            return english.to_string();
        }
        match english {
            "File Handle" => "文件句柄".to_string(),
            "Working Dir" => "工作目录".to_string(),
            "Executable" => "可执行文件".to_string(),
            "Memory Map" => "内存映射".to_string(),
            "File Lock" => "文件锁".to_string(),
            "Dir Handle" => "目录句柄".to_string(),
            _ => english.to_string(),
        }
    }
}

/// 设置字体。返回 true 表示 CJK 字体加载成功，false 表示未找到 CJK 字体。
pub fn setup_fonts(ctx: &egui::Context) -> bool {
    let mut fonts = egui::FontDefinitions::default();

    let font_paths: &[&str] = if cfg!(target_os = "windows") {
        &[
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyhbd.ttc",
            "C:\\Windows\\Fonts\\simhei.ttf",
            "C:\\Windows\\Fonts\\simsun.ttc",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/Library/Fonts/Arial Unicode.ttf",
        ]
    } else {
        &[
            // Ubuntu / Debian
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            // Arch Linux
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            // Fedora / RHEL / CentOS
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
            // openSUSE
            "/usr/share/fonts/truetype/NotoSansCJK-Regular.ttc",
            // Fallback: Droid / WenQuanYi
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
            "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        ]
    };

    let mut loaded = false;
    for path in font_paths {
        if let Ok(data) = std::fs::read(path) {
            let mut font_data = egui::FontData::from_owned(data);
            // .ttc 文件包含多个字体，取第一个
            font_data.tweak.scale = 1.0;

            fonts.font_data.insert("cjk".to_string(), font_data);

            // 插入到 Proportional 和 Monospace 的靠前位置（在默认英文字体之后）
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.insert(1, "cjk".to_string());
            }
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.insert(1, "cjk".to_string());
            }

            loaded = true;
            break;
        }
    }

    if !loaded {
        log::warn!("No CJK font found, Chinese characters may not display correctly");
    }

    ctx.set_fonts(fonts);
    loaded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_toggle() {
        assert_eq!(Lang::Chinese.toggle(), Lang::English);
        assert_eq!(Lang::English.toggle(), Lang::Chinese);
    }

    #[test]
    fn lang_label() {
        assert_eq!(Lang::Chinese.label(), "EN");
        assert_eq!(Lang::English.label(), "中文");
    }

    #[test]
    fn lock_type_label_chinese() {
        assert_eq!(T::lock_type_label(Lang::Chinese, "File Handle"), "文件句柄");
        assert_eq!(T::lock_type_label(Lang::Chinese, "Working Dir"), "工作目录");
        assert_eq!(
            T::lock_type_label(Lang::Chinese, "Executable"),
            "可执行文件"
        );
        assert_eq!(T::lock_type_label(Lang::Chinese, "Memory Map"), "内存映射");
        assert_eq!(T::lock_type_label(Lang::Chinese, "File Lock"), "文件锁");
        assert_eq!(T::lock_type_label(Lang::Chinese, "Dir Handle"), "目录句柄");
    }

    #[test]
    fn lock_type_label_english() {
        assert_eq!(
            T::lock_type_label(Lang::English, "File Handle"),
            "File Handle"
        );
        assert_eq!(
            T::lock_type_label(Lang::English, "Working Dir"),
            "Working Dir"
        );
    }

    #[test]
    fn lock_type_label_unknown_passthrough() {
        assert_eq!(T::lock_type_label(Lang::Chinese, "WMI"), "WMI");
        assert_eq!(T::lock_type_label(Lang::English, "WMI"), "WMI");
    }

    #[test]
    fn translations_not_empty() {
        // Verify key translations are non-empty for both languages
        for lang in [Lang::Chinese, Lang::English] {
            assert!(!T::scan(lang).is_empty());
            assert!(!T::kill(lang).is_empty());
            assert!(!T::export_json(lang).is_empty());
            assert!(!T::no_results(lang).is_empty());
            assert!(!T::select_hint(lang).is_empty());
            assert!(!T::drop_hint(lang).is_empty());
            assert!(!T::copy(lang).is_empty());
            assert!(!T::copied(lang).is_empty());
        }
    }

    #[test]
    fn detect_system_lang_returns_valid() {
        // detect_system_lang 应返回一个有效的 Lang 值
        let lang = super::detect_system_lang();
        assert!(lang == Lang::Chinese || lang == Lang::English);
    }

    #[test]
    fn stats_format() {
        let s = T::stats(Lang::Chinese, 100, 5, 1.23);
        assert!(s.contains("100"));
        assert!(s.contains("5"));
        assert!(s.contains("1.23"));

        let s = T::stats(Lang::English, 100, 5, 1.23);
        assert!(s.contains("files"));
        assert!(s.contains("locks"));
    }
}
