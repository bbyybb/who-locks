# Changelog / 更新日志

## [Unreleased]

### Fixed / 修复

- **macOS lsof parser**: Fix `parse_lsof_output` generating spurious `FileHandle` entries — the first `f` field per process incorrectly flushed a `None` fd, producing duplicate records and breaking dedup / 修复 macOS lsof 解析器在每个进程的首个 fd 字段时错误 flush 产生虚假 `FileHandle` 条目的问题
- **Linux test compilation**: Add missing `use crate::detector::LockDetector` import in `detector/linux.rs` test module — tests failed to compile on Linux due to trait methods not being in scope / 修复 Linux 检测器测试模块缺少 `LockDetector` trait 导入导致编译失败的问题
- **CI: aarch64 cross-compile**: Fix ARM64 cross-compilation on Ubuntu 24.04 runners — adapt apt source configuration for deb822 format (`ubuntu.sources`) instead of legacy `sources.list`; add `security` pocket to ARM64 sources / 修复 Ubuntu 24.04 上 aarch64 交叉编译失败：适配 deb822 格式 apt 源配置，新增 security 源
- **CI: Security audit permissions**: Add `checks: write` permission to security-audit job — `rustsec/audit-check@v2` requires this to create GitHub Check Runs / 修复安全审计 CI 权限不足（缺少 `checks: write`）
- **Dependencies**: Update `unicode-segmentation` from yanked v1.13.1 to v1.13.2 / 更新被撤回的 `unicode-segmentation` v1.13.1 到 v1.13.2

## [1.0.0] - 2026-03-29

首次公开发布 / Initial public release.

### Features / 功能

#### GUI 图形界面
- **Native GUI**: egui/eframe native graphical interface, double-click to run / 原生图形界面，双击即可运行
- **File/folder picker**: Native system dialog, supports selecting multiple files and directories / 原生系统对话框，支持多文件和目录选择
- **Drag and drop**: Drag files or folders into the window / 拖拽文件或目录到窗口
- **Results table**: Sortable columns (click headers), search filter by PID/name/path, row selection / 结果表格支持排序、搜索过滤、行选中
- **Kill processes**: Normal kill (WM_CLOSE / SIGTERM) and force kill (TerminateProcess / SIGKILL) with confirmation dialog / 普通终止和强制终止，带确认对话框
- **Auto re-scan after kill**: Automatically re-scans to verify file locks are released after termination / 终止进程后自动重新扫描验证
- **Export**: Export results to JSON or CSV (both with UTF-8 BOM for Excel compatibility, CSV with injection protection) / 导出 JSON/CSV（均含 UTF-8 BOM，CSV 含注入防护）
- **Copy to clipboard**: Copy selected or all visible rows as tab-separated text / 复制选中或全部可见行到剪贴板
- **Chinese/English toggle**: One-click language switch with auto system language detection / 中英文一键切换，自动检测系统语言
- **DPI-adaptive scaling**: Automatically matches system display settings / DPI 自适应缩放
- **Scan cancellation**: Cancel button to abort in-progress scan / 随时取消扫描
- **Error details dialog**: Click error count in footer for full error list / 点击底栏错误数量查看错误详情
- **CJK font auto-loading**: Loads system Chinese fonts (Microsoft YaHei / PingFang / Noto CJK) / 自动加载系统中文字体
- **Non-blocking detection**: System processes (explorer.exe, Finder, etc.) greyed out and non-selectable / 系统进程灰色显示不可选中
- **Donation dialog**: In-app donation support with QR codes / 应用内打赏支持
- **Version display**: Version number shown in both window title and footer bar / 窗口标题和底栏均显示版本号

#### CLI 命令行模式
- **CLI mode**: Pass path arguments to scan without GUI (`who-locks path [options]`) / 传入路径参数进入命令行模式
- **Options**: `-n/--no-recursive`, `-d/--depth`, `-e/--exclude` (glob wildcards `*` `?` `**`), `-f/--format` (text/json) / 支持递归、深度、排除通配符、输出格式选项
- **Terminal detection**: ANSI escape codes only output to real terminals, safe for pipes/redirects / 终端检测，管道/重定向时不输出 ANSI 转义码

#### Detection / 检测（7 种占用类型）
- **Windows**: Restart Manager API (batch-optimized) + Sysinternals handle.exe (deep scan) + PowerShell WMI (fallback) / RM 批量优化 + handle.exe 深度扫描 + PowerShell WMI 回退
- **Linux**: `/proc` single-pass traversal with inverted index — fd, cwd, exe, mmap (map_files), flock (/proc/locks); directory-level deep scan via prefix matching / /proc 一次遍历反转索引 + 目录级深度扫描
- **macOS**: `lsof -F` machine-readable format with auto fd type detection; directory-level deep scan via `lsof +D` / lsof 机器解析格式 + 目录级深度扫描
- **Lock types**: File Handle, Dir Handle, Working Dir, Executable, Memory Map, File Lock, Section Mapping

#### Scan Options / 扫描选项
- Recursive directory scan with depth limit / 递归目录扫描，支持深度限制
- Exclude patterns with `*`, `?` and `**` glob wildcards (case-insensitive on Windows) / 排除模式支持通配符（Windows 大小写不敏感）
- Follow symlinks option / 跟随符号链接选项
- Multi-path simultaneous scan / 多路径同时扫描

#### Security / 安全
- **handle.exe verification**: Authenticode digital signature + SHA-256 hash dual verification; auto-download with 3-retry mechanism / Authenticode 数字签名 + SHA-256 哈希双重验证；自动下载支持 3 次重试
- **PowerShell injection prevention**: 7 special characters escaped (`$`, `` ` ``, `[`, `]`, `*`, `?`, `'`) / PowerShell 注入防护（7 种特殊字符转义）
- **WMI path boundary validation**: Post-filter WMI results to prevent substring false matches / WMI 结果路径边界验证，防止子串误匹配
- **CSV injection protection**: Sanitize cells starting with formula characters (`=`, `+`, `@`, etc.) / CSV 公式注入防护
- **PID 0 protection**: Unix killer rejects PID 0 to prevent signaling entire process group / PID 0 终止防护
- **Kill confirmation**: All process termination requires explicit user confirmation / 所有终止操作需用户确认
- **Graceful kill timeout**: WM_CLOSE sends close request, waits 800ms and verifies process exit / 优雅终止发送 WM_CLOSE 后等待并验证进程退出
- **Protected system processes**: Windows critical processes (explorer.exe, csrss.exe, lsass.exe, etc.) cannot be terminated even if GUI checks are bypassed — defense-in-depth at the killer layer / Windows 关键系统进程在终止器层面受保护，纵深防御
- **DirHandle non-blocking**: Directory handles are always non-blocking (shared FILE_LIST_DIRECTORY access); cwd is detected separately as WorkingDir (blocking) via sysinfo on Windows, /proc/pid/cwd on Linux, lsof cwd on macOS / 目录句柄始终非阻塞；工作目录通过各平台原生机制检测为 WorkingDir（阻塞）

#### Chinese Path Support / 中文路径支持
- **Multi-strategy path resolution**: handle.exe garbled CJK paths resolved via 4-layer fallback — segment-by-segment matching → prefix/suffix anchor matching → GBK byte-count estimation → recursive directory search / handle.exe 中文乱码路径四层还原：逐段匹配 → 前缀/后缀锚点 → GBK 字节数估算 → 递归目录搜索
- **Encoding auto-detection**: Pipe output decoded via UTF-8 → GBK → system ANSI → OEM fallback chain / 管道输出编码自动检测（UTF-8 → GBK → 系统 ANSI → OEM）

#### Quality / 质量
- 87+ unit tests covering scan exclusion, `**` glob matching, deep scan merge, handle.exe parsing, lsof parsing, path utilities, process killer, resource integrity, SHA-256, i18n, export / 87+ 单元测试
- `cargo clippy -D warnings` strict mode + `cargo fmt` CI checks / clippy 严格模式 + 格式检查
- 5-platform CI: Windows x64, Linux x64/ARM64, macOS Intel/Apple Silicon / 5 平台自动构建
- Dependency security audit (rustsec/audit-check) / 依赖安全审计
- Custom app icon (SVG/PNG/ICO) embedded in Windows .exe / 自定义应用图标

[Unreleased]: https://github.com/BBYYBB/who-locks/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/BBYYBB/who-locks/releases/tag/v1.0.0
