use crate::error::Error;

/// 进程终止 trait
pub trait ProcessKiller: Send + Sync {
    /// 正常请求终止 (SIGTERM / 友好关闭)
    fn kill_graceful(&self, pid: u32) -> Result<(), Error>;

    /// 强制终止 (SIGKILL / TerminateProcess)
    fn kill_force(&self, pid: u32) -> Result<(), Error>;
}

/// 根据平台创建进程终止器
pub fn create_killer() -> Box<dyn ProcessKiller> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsKiller)
    }

    #[cfg(unix)]
    {
        Box::new(unix::UnixKiller)
    }

    #[cfg(not(any(target_os = "windows", unix)))]
    {
        compile_error!("who-locks only supports Windows and Unix platforms");
    }
}

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(unix)]
pub mod unix;
