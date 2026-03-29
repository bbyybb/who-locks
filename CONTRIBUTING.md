# Contributing Guide / 贡献指南

Thank you for your interest in who-locks! Contributions are welcome.
感谢你对 who-locks 项目的关注！欢迎通过以下方式参与贡献。

## How to Contribute / 如何贡献

### Report Bugs / 报告 Bug

Use the [Bug Report](https://github.com/BBYYBB/who-locks/issues/new?template=bug_report.md) template. Please include:
使用 Bug Report 模板提交，请包含：

- OS and version / 操作系统和版本
- who-locks version (see window title or `Cargo.toml`) / who-locks 版本（见窗口标题或 Cargo.toml）
- Steps to reproduce and screenshots / 复现步骤和截图

### Feature Requests / 功能建议

Use the [Feature Request](https://github.com/BBYYBB/who-locks/issues/new?template=feature_request.md) template.
使用 Feature Request 模板提交。

### Submit Code / 提交代码

1. **Fork** the repository / Fork 本仓库
2. Create a feature branch / 创建功能分支: `git checkout -b feature/your-feature`
3. Commit changes / 提交改动: `git commit -m "feat: add your feature"`
4. Push / 推送: `git push origin feature/your-feature`
5. Create a **Pull Request** / 创建 PR

## Development Setup / 开发环境

### Prerequisites / 前置要求

- [Rust](https://rustup.rs/) 1.74+
- Windows: [Sysinternals Handle](https://learn.microsoft.com/sysinternals/downloads/handle) recommended (auto-downloaded on first run)
- Linux/macOS: No extra dependencies / 无额外要求

### Build & Run / 构建与运行

```bash
cargo build --release                      # Build / 编译
./target/release/who-locks                 # Run GUI / 运行图形界面
./target/release/who-locks /path/to/scan   # Run CLI mode / 命令行模式
cargo fmt --check                          # Format check / 格式检查
cargo clippy                               # Lint check / 代码检查
cargo test                                 # Run unit tests / 运行单元测试
```

### Testing Notes / 测试说明

- `cargo test` runs all unit tests on the current platform. Tests for other platforms are automatically skipped via `#[cfg]` / `cargo test` 运行当前平台的所有单元测试，其他平台的测试通过 `#[cfg]` 自动跳过
- Some tests (e.g., resource integrity checks) require a clean build (`cargo clean && cargo test`) / 部分测试（如资源完整性校验）需要干净构建才能通过
- Process killer tests use non-existent PIDs and don't affect your system / 进程终止器测试使用不存在的 PID，不会影响系统
- Windows-specific tests (case-insensitive exclude, handle.exe parsing) only run on Windows / Windows 特定测试仅在 Windows 上运行
- For full CI-equivalent checks, run all three: `cargo test && cargo clippy -- -D warnings && cargo fmt --check` / 完整的 CI 等效检查需运行全部三项

## Code Style / 代码规范

- Follow Rust standard style (`cargo fmt`) / 遵循 Rust 标准风格
- Use `#[cfg]` for platform-specific code / 跨平台代码使用条件编译
- New detection types need a `LockType` enum variant / 新检测类型需要添加枚举变体
- Use `ProcessInfo::new(pid, name, lock_type, cmdline, user)` to construct process info; it auto-computes the `blocking` field / 使用 `ProcessInfo::new()` 构造进程信息，blocking 字段会自动计算
- Commit format: `type: description` (`feat:` / `fix:` / `docs:` / `refactor:`)

## Architecture / 项目架构

```
assets/     -- App icons (SVG/PNG/ICO) / 应用图标
main.rs     -- Entry point (GUI or CLI) / 程序入口
cli.rs      -- CLI command-line mode / 命令行模式
error.rs    -- Error types / 错误类型
model.rs    -- Data models / 数据模型
scan.rs     -- Scan coordinator / 扫描协调器
res.rs      -- Resource integrity / 资源完整性校验
detector/   -- Platform detectors (trait LockDetector) / 平台检测器
killer/     -- Process killers (trait ProcessKiller) / 进程终止器
gui/        -- GUI layer (egui/eframe, i18n, export) / 图形界面层
```

To add a new platform / 添加新平台支持:
1. Create a new module in `detector/` / 在 detector/ 下创建新模块
2. Implement `LockDetector` trait / 实现 LockDetector trait
3. Add `#[cfg]` branch in `detector/mod.rs` `create_detector()` / 在工厂函数中添加分支

## Important Notes / 注意事项

- **Do not modify donation info** / **不要修改打赏信息**: QR codes in `docs/`, donation section in README, `.github/FUNDING.yml` are protected by `build.rs` build-time integrity checks / 受 build.rs 编译时完整性校验保护
- **Cross-platform testing** / **跨平台测试**: Detector changes should be tested on Windows/Linux/macOS when possible / 检测器改动请尽可能多平台测试
- **Security** / **安全性**: Changes involving process termination require extra caution / 涉及进程终止的改动需格外谨慎

## License / 许可证

By submitting code, you agree to release it under the [MIT License](LICENSE).
提交代码即表示你同意将代码以 MIT 许可证发布。
