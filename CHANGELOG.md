# Changelog / 更新日志

## [Unreleased]

### Added / 新增

- **Integration tests**: Add 8 CLI integration tests in `tests/cli_integration.rs` — end-to-end testing via compiled binary covering help output, error handling, JSON/text format, recursive/depth/exclude/multi-path options / 新增 8 个 CLI 集成测试，通过编译后的二进制进行端到端测试，覆盖帮助输出、错误处理、JSON/文本格式、递归/深度/排除/多路径选项
- **Unit tests**: Add tests for `error.rs` (6 tests: Display format, From conversion, Debug trait) and `killer/mod.rs` (2 tests: factory function validation), bringing total from 183 to 199 / 新增 error.rs（6 个）和 killer/mod.rs（2 个）单元测试，总数从 183 增至 199
- **`.editorconfig`**: Add editor configuration for consistent coding style across contributors (UTF-8, LF, indent settings per file type) / 新增编辑器配置文件，统一贡献者的编码风格
- **`.github/ISSUE_TEMPLATE/config.yml`**: Disable blank issues; add contact links for Discussions and Security Advisories / 禁用空白 issue，添加讨论区和安全公告链接

### Changed / 变更

- **CI: Release gate**: Release job now depends on both `build` and `security-audit` (previously only `build`), preventing releases with known dependency vulnerabilities / Release job 现在同时依赖 build 和 security-audit，防止有已知漏洞的版本被发布

### Fixed / 修复

- **`scripts/generate-screenshots.sh`**: Add `set -e` for consistent error handling with other project scripts / 添加 `set -e`，与其他脚本保持一致的错误处理
- **`scripts/update-hashes.sh`**: Add python3 availability pre-check at script start with clear warning message / 脚本开头预检测 python3 可用性并给出明确提示
- **`docs/RELEASE_GUIDE.md`**: Mark First Release Checklist as completed (v1.0.0 was released on 2026-03-29) / 将首次发布检查清单标记为已完成

## [1.1.0] - 2026-03-30

### Added / 新增

- **Unit tests**: Expand test suite from 87 to 183 tests (+96), covering CLI argument parsing, Scanner main flow (scan/cancel/exclude/depth), model serialization & all platform non-blocking processes, GUI state (sort/filter/apply_result), export edge cases (empty data/JSON structure), i18n translation completeness, detector default batch implementation, and platform-aware blocking tests / 单元测试从 87 扩展到 183 个（+96），覆盖 CLI 参数解析、Scanner 主流程、模型序列化及全平台非阻塞进程判断、GUI 状态、导出边界、国际化翻译完整性、检测器默认批量实现、平台感知阻塞测试

### Fixed / 修复

- **macOS/Linux blocking detection**: On Unix systems, all lock types except `FileLock` (flock/fcntl) are now correctly marked as non-blocking — Unix allows unlink/rename/move even while files are open; only advisory file locks may actually block operations. Previously, `FileHandle`, `WorkingDir`, `Executable`, and `MemoryMap` were incorrectly shown as blocking on macOS/Linux / 修复 macOS/Linux 上的阻塞检测：Unix 系统下除 `FileLock`（flock/fcntl）外，所有锁类型现在正确标记为非阻塞。此前 `FileHandle`、`WorkingDir`、`Executable`、`MemoryMap` 在 macOS/Linux 上被错误地显示为阻塞
- **Scan progress**: Add real-time file count during directory collection phase; reduce batch size from 500 to 100 for smoother progress updates — previously Windows showed no visible progress for small directories / 扫描进度优化：收集文件阶段实时显示文件数量；批量检测批次从 500 缩小到 100 使进度更平滑——此前 Windows 上小目录扫描看不到进度
- **Duplicate tooltips**: Remove redundant manual `on_hover_text()` calls on table cells — `clip(true)` columns already auto-show tooltips when text is truncated, causing double tooltips on hover / 修复鼠标悬浮时重复显示 tooltip：移除手动 `on_hover_text()` 调用，列裁剪时 egui 已自动显示 tooltip

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
- 183+ unit tests covering CLI parsing, scan flow, exclusion patterns, `**` glob matching, deep scan merge, handle.exe parsing, lsof parsing, path utilities, process killer, model serialization, GUI state management, export formats, resource integrity, SHA-256, i18n completeness, detector batch, platform-aware blocking / 183+ 单元测试
- `cargo clippy -D warnings` strict mode + `cargo fmt` CI checks / clippy 严格模式 + 格式检查
- 5-platform CI: Windows x64, Linux x64/ARM64, macOS Intel/Apple Silicon / 5 平台自动构建
- Dependency security audit (rustsec/audit-check) / 依赖安全审计
- Custom app icon (SVG/PNG/ICO) embedded in Windows .exe / 自定义应用图标

[Unreleased]: https://github.com/BBYYBB/who-locks/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/BBYYBB/who-locks/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/BBYYBB/who-locks/releases/tag/v1.0.0
