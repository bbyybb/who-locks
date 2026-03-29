# Security Policy / 安全策略

## Supported Versions / 支持的版本

| Version / 版本 | Supported / 是否支持 |
|----------------|---------------------|
| 1.0.x          | Yes / 是             |
| < 1.0          | No / 否              |

## Security Considerations / 安全说明

who-locks is a system utility that / who-locks 是一个系统工具：
- **Reads process information** (PID, name, command line, user) from the OS / 从操作系统读取进程信息
- **Can terminate processes** via GUI kill button (requires appropriate permissions) / 可通过界面按钮终止进程（需要对应权限）
- **On Windows**, may invoke external tools (`handle.exe`, `powershell.exe`) / Windows 上可能调用外部工具
- **Requires elevated privileges** (admin/root) for full functionality / 完整功能需要管理员/root 权限

### Privilege Model / 权限模型

- The tool itself does not escalate privileges / 工具本身不会提升权限
- Process termination uses standard OS APIs, which respect OS permission boundaries / 进程终止使用标准系统 API，遵循系统权限边界
  - **Windows graceful kill**: Sends `WM_CLOSE` to visible GUI windows, allowing processes to save data and clean up. Falls back to `TerminateProcess` for processes without windows or on force kill / Windows 优雅终止通过 WM_CLOSE 发送关闭请求，允许进程保存数据；无窗口进程或强制终止时使用 TerminateProcess
  - **Unix**: `SIGTERM` for graceful kill, `SIGKILL` for force kill / Unix 优雅终止发送 SIGTERM，强制终止发送 SIGKILL
- Non-admin users can only see/kill their own processes / 非管理员只能查看/终止自己的进程

### Security Measures / 安全措施

**handle.exe Verification / handle.exe 验证**
- Bundled and locally found `handle.exe` / `handle64.exe` are verified via **SHA-256 hash** against a known-good list before execution / 内置及本地发现的 handle.exe 在执行前通过 SHA-256 哈希校验
- Auto-downloaded `handle64.exe` is verified via **Authenticode digital signature** (must be signed by Microsoft) / 自动下载的 handle64.exe 通过 Authenticode 数字签名验证（必须由 Microsoft 签名）
- If signature verification fails (including when PowerShell is unavailable), the downloaded file is **rejected and deleted** / 签名验证失败时（包括 PowerShell 不可用时），下载的文件会被拒绝并删除

**Command Injection Prevention / 命令注入防护**
- Paths passed to PowerShell (WMI fallback) are sanitized: `$`, `` ` ``, `[`, `]`, `*`, `?`, `'` characters are escaped to prevent variable expansion, wildcard matching, and code injection / 传入 PowerShell 的路径中的 `$`、`` ` ``、`[`、`]`、`*`、`?`、`'` 字符经过转义处理，防止变量扩展、通配符匹配和代码注入
- External commands (`handle.exe`, `powershell.exe`, `lsof`) are invoked with `CREATE_NO_WINDOW` flag on Windows to prevent visible console windows / 外部命令使用无窗口标志调用

**CSV Export Injection Protection / CSV 导出注入防护**
- Exported CSV cells starting with `=`, `+`, `@`, `\t`, `\r`, or `\n` are sanitized with a single-quote prefix to prevent formula injection attacks in spreadsheet applications (Excel, LibreOffice Calc, etc.) / 导出的 CSV 单元格若以 `=`、`+`、`@`、`\t`、`\r`、`\n` 开头，会添加单引号前缀，防止电子表格应用中的公式注入攻击
- Reference: [OWASP CSV Injection](https://owasp.org/www-community/attacks/CSV_Injection)

**Process Termination Safety / 进程终止安全**
- All process termination requires explicit user confirmation via dialog / 所有进程终止操作需用户通过对话框明确确认
- **GUI layer**: System processes (explorer.exe, Windows Defender, Finder, etc.) are marked as non-blocking and cannot be selected for termination / GUI 层：系统进程标记为非阻塞，不可被选中终止
- **Killer layer (defense-in-depth)**: Windows critical system processes (`explorer.exe`, `csrss.exe`, `lsass.exe`, `winlogon.exe`, `smss.exe`, `services.exe`, `svchost.exe`, `wininit.exe`, `dwm.exe`, `system`) are protected at the process killer level — even if the GUI check is bypassed, these processes cannot be terminated. Attempting to kill them returns an error message / 终止器层（纵深防御）：Windows 关键系统进程在终止器层面受保护，即使绕过 GUI 检查也无法终止
- PID 0 is always rejected by the process killer to prevent signaling the entire process group on Unix systems / PID 0 始终被进程终止器拒绝，防止在 Unix 系统上向整个进程组发送信号

**Directory Handle Classification / 目录句柄分类**
- Directory handles (`DirHandle`) are always classified as non-blocking — they represent shared `FILE_LIST_DIRECTORY` access and do not prevent file operations within the directory / 目录句柄始终标记为非阻塞——它们是共享的目录列表访问，不阻止目录内的文件操作
- On Windows, if a directory handle matches the process's working directory (cwd), it is automatically reclassified as `WorkingDir` (blocking) via `sysinfo` lookup / Windows 上，如果目录句柄匹配进程的工作目录，会通过 sysinfo 自动重分类为 WorkingDir（阻塞）
- On Linux and macOS, working directories are detected natively (`/proc/pid/cwd` and `lsof cwd`) and classified as `WorkingDir` from the start / Linux 和 macOS 上，工作目录通过原生机制检测并直接分类为 WorkingDir

## Reporting a Vulnerability / 报告漏洞

If you discover a security vulnerability / 如果你发现安全漏洞：

1. **DO NOT** open a public GitHub issue / **不要**公开提交 Issue
2. Use [GitHub Security Advisories](https://github.com/BBYYBB/who-locks/security/advisories/new) to report privately / 使用 GitHub 安全公告私密报告
3. Or contact the maintainer directly / 或直接联系项目维护者

We will respond within 7 days and work with you to address the issue.
我们会在 7 天内回复并与你协作解决问题。

## Acknowledgments / 致谢

We appreciate responsible disclosure and will credit reporters (with permission) in the changelog.
我们感谢负责任的漏洞披露，会在更新日志中致谢（经同意）。
