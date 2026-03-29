use crate::error::Error;
use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, TerminateProcess, PROCESS_TERMINATE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
};

/// 检查进程是否仍在运行
fn is_process_alive(pid: u32) -> bool {
    // PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
    const PROCESS_QUERY_LIMITED: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED, 0, pid);
        if handle.is_null() {
            return false; // 无法打开 → 进程已不存在
        }
        let mut exit_code: u32 = 0;
        let ret = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        ret != 0 && exit_code == STILL_ACTIVE
    }
}

/// 系统关键进程保护名单：终止这些进程会导致桌面崩溃、系统不稳定等严重后果
/// 在终止器层面拦截，即使 GUI 层（is_blocking）被绕过也能保证安全
const PROTECTED_PROCESSES: &[&str] = &[
    "explorer.exe", // 桌面和任务栏，终止后桌面消失
    "csrss.exe",    // 客户端/服务器运行时，终止会蓝屏
    "wininit.exe",  // Windows 初始化进程
    "winlogon.exe", // 登录进程，终止会蓝屏
    "smss.exe",     // 会话管理器
    "services.exe", // 服务控制管理器
    "lsass.exe",    // 本地安全认证，终止会蓝屏
    "svchost.exe",  // 服务宿主进程
    "dwm.exe",      // 桌面窗口管理器，终止后界面异常
    "system",       // 系统进程
];

/// 检查 PID 对应的进程是否为受保护的系统关键进程
fn is_protected_process(pid: u32) -> bool {
    let mut sys = sysinfo::System::new();
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::new(),
    );

    if let Some(proc) = sys.process(sysinfo::Pid::from_u32(pid)) {
        let name = proc.name().to_string_lossy().to_lowercase();
        return PROTECTED_PROCESSES.iter().any(|&p| name == p);
    }

    false
}

pub struct WindowsKiller;

impl super::ProcessKiller for WindowsKiller {
    fn kill_graceful(&self, pid: u32) -> Result<(), Error> {
        if is_protected_process(pid) {
            return Err(Error::KillFailed {
                pid,
                reason: "This is a protected system process. Terminating it would crash the desktop or cause a blue screen.".to_string(),
            });
        }

        // 尝试通过 WM_CLOSE 消息优雅关闭 GUI 窗口
        // 如果进程没有可见窗口，则回退到 TerminateProcess
        let windows = find_process_windows(pid);
        if windows.is_empty() {
            log::debug!(
                "PID {} has no visible windows, falling back to TerminateProcess",
                pid
            );
            return self.kill_force(pid);
        }

        let mut sent = false;
        for hwnd in &windows {
            unsafe {
                if PostMessageW(*hwnd, WM_CLOSE, 0, 0) != 0 {
                    sent = true;
                }
            }
        }

        if !sent {
            log::debug!(
                "WM_CLOSE failed for PID {}, falling back to TerminateProcess",
                pid
            );
            return self.kill_force(pid);
        }

        log::debug!(
            "Sent WM_CLOSE to {} window(s) of PID {}, waiting for exit...",
            windows.len(),
            pid
        );

        // 等待进程退出（WM_CLOSE 可能触发保存对话框，需要一定时间）
        std::thread::sleep(std::time::Duration::from_millis(800));

        if is_process_alive(pid) {
            Err(Error::KillFailed {
                pid,
                reason: "Process did not exit after close request (WM_CLOSE). \
                         It may be showing a save dialog. Try Force Kill."
                    .to_string(),
            })
        } else {
            Ok(())
        }
    }

    fn kill_force(&self, pid: u32) -> Result<(), Error> {
        if is_protected_process(pid) {
            return Err(Error::KillFailed {
                pid,
                reason: "This is a protected system process. Terminating it would crash the desktop or cause a blue screen.".to_string(),
            });
        }

        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if handle.is_null() {
                return Err(Error::KillFailed {
                    pid,
                    reason: format!(
                        "Cannot open process (error {})",
                        std::io::Error::last_os_error()
                    ),
                });
            }

            let ret = TerminateProcess(handle, 1);
            CloseHandle(handle);

            if ret == 0 {
                return Err(Error::KillFailed {
                    pid,
                    reason: format!(
                        "TerminateProcess failed (error {})",
                        std::io::Error::last_os_error()
                    ),
                });
            }

            Ok(())
        }
    }
}

/// 查找属于指定 PID 的所有可见顶层窗口
fn find_process_windows(target_pid: u32) -> Vec<HWND> {
    struct EnumCtx {
        target_pid: u32,
        windows: Vec<HWND>,
    }

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam as *mut EnumCtx);
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == ctx.target_pid && IsWindowVisible(hwnd) != 0 {
            ctx.windows.push(hwnd);
        }
        1 // continue enumeration
    }

    let mut ctx = EnumCtx {
        target_pid,
        windows: Vec::new(),
    };

    unsafe {
        EnumWindows(Some(enum_callback), &mut ctx as *mut EnumCtx as LPARAM);
    }

    ctx.windows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::killer::ProcessKiller;

    #[test]
    fn kill_force_nonexistent_pid() {
        let killer = WindowsKiller;
        // PID 0 是 System Idle Process，无法终止；用一个不存在的 PID
        let result = killer.kill_force(99999999);
        assert!(result.is_err(), "Should fail for non-existent PID");
    }

    #[test]
    fn kill_graceful_nonexistent_pid() {
        let killer = WindowsKiller;
        // 不存在的进程没有窗口，回退到 TerminateProcess，也应失败
        let result = killer.kill_graceful(99999999);
        assert!(result.is_err(), "Should fail for non-existent PID");
    }

    #[test]
    fn find_process_windows_nonexistent() {
        let windows = find_process_windows(99999999);
        assert!(
            windows.is_empty(),
            "Non-existent PID should have no windows"
        );
    }
}
