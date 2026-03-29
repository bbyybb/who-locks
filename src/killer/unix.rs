use crate::error::Error;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

pub struct UnixKiller;

impl UnixKiller {
    /// 将 u32 PID 安全转换为 i32，防止溢出截断
    fn to_pid(pid: u32) -> Result<Pid, Error> {
        let raw: i32 = pid.try_into().map_err(|_| Error::KillFailed {
            pid,
            reason: format!("PID {} exceeds i32::MAX", pid),
        })?;
        Ok(Pid::from_raw(raw))
    }
}

impl super::ProcessKiller for UnixKiller {
    fn kill_graceful(&self, pid: u32) -> Result<(), Error> {
        // PID 0 会向当前进程组的所有进程发送信号，必须拒绝
        if pid == 0 {
            return Err(Error::KillFailed {
                pid,
                reason: "Cannot kill PID 0 (would signal entire process group)".to_string(),
            });
        }
        let nix_pid = Self::to_pid(pid)?;
        signal::kill(nix_pid, Signal::SIGTERM).map_err(|e| Error::KillFailed {
            pid,
            reason: format!("SIGTERM failed: {}", e),
        })
    }

    fn kill_force(&self, pid: u32) -> Result<(), Error> {
        if pid == 0 {
            return Err(Error::KillFailed {
                pid,
                reason: "Cannot kill PID 0 (would signal entire process group)".to_string(),
            });
        }
        let nix_pid = Self::to_pid(pid)?;
        signal::kill(nix_pid, Signal::SIGKILL).map_err(|e| Error::KillFailed {
            pid,
            reason: format!("SIGKILL failed: {}", e),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_pid_valid() {
        let pid = UnixKiller::to_pid(1234).unwrap();
        assert_eq!(pid.as_raw(), 1234);
    }

    #[test]
    fn to_pid_max_i32() {
        let pid = UnixKiller::to_pid(i32::MAX as u32).unwrap();
        assert_eq!(pid.as_raw(), i32::MAX);
    }

    #[test]
    fn to_pid_overflow() {
        let result = UnixKiller::to_pid(u32::MAX);
        assert!(result.is_err());
    }

    #[test]
    fn to_pid_just_over_i32_max() {
        let result = UnixKiller::to_pid(i32::MAX as u32 + 1);
        assert!(result.is_err());
    }

    #[test]
    fn to_pid_zero() {
        let pid = UnixKiller::to_pid(0).unwrap();
        assert_eq!(pid.as_raw(), 0);
    }

    #[test]
    fn kill_graceful_rejects_pid_zero() {
        use crate::killer::ProcessKiller;
        let killer = UnixKiller;
        let result = killer.kill_graceful(0);
        assert!(result.is_err(), "PID 0 should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("process group"),
            "Error should explain PID 0 danger"
        );
    }

    #[test]
    fn kill_force_rejects_pid_zero() {
        use crate::killer::ProcessKiller;
        let killer = UnixKiller;
        let result = killer.kill_force(0);
        assert!(result.is_err(), "PID 0 should be rejected");
    }
}
