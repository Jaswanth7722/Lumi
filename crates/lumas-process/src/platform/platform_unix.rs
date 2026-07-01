//! # Unix Platform Operations
//!
//! Process management implementations for Unix-like systems (Linux, macOS).
//!
//! Uses `nix` crate for POSIX signal handling, process groups, and resource limits.

use crate::error::ProcessError;
use crate::id::ProcessId;
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::time::{Duration, Instant};
use tokio::time;

/// Send SIGTERM to a process and wait up to `timeout` for it to exit.
///
/// If the process is still running after the timeout, sends SIGKILL.
///
/// # Errors
///
/// Returns `ProcessError::OsError` if the kill syscall fails.
/// Returns `ProcessError::StopTimeout` if force-kill was required.
///
/// # Platform Notes
///
/// Uses `nix::sys::signal::kill` for signal delivery and
/// `nix::sys::wait::waitpid` with WNOHANG for exit polling.
pub async fn graceful_kill(pid: u32, timeout: Duration) -> Result<(), ProcessError> {
    let pid = Pid::from_raw(pid as i32);

    // Send SIGTERM
    kill(pid, Signal::SIGTERM).map_err(|e| ProcessError::OsError {
        id: ProcessId::root(),
        source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
    })?;

    // Wait for process to exit within the timeout
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() >= deadline {
            // Timeout — send SIGKILL
            let _ = kill(pid, Signal::SIGKILL);
            return Err(ProcessError::StopTimeout {
                id: ProcessId::root(),
                timeout_ms: timeout.as_millis() as u64,
            });
        }

        match waitpid(Some(pid), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(_, _)) => return Ok(()),
            Ok(WaitStatus::Signaled(_, _, _)) => return Ok(()),
            Ok(WaitStatus::StillAlive) => {
                time::sleep(Duration::from_millis(50)).await;
            }
            Ok(_) => return Ok(()),
            Err(e) => {
                // ESRCH means the process no longer exists
                if e == nix::errno::Errno::ESRCH {
                    return Ok(());
                }
                return Err(ProcessError::OsError {
                    id: ProcessId::root(),
                    source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                });
            }
        }
    }
}

/// Set resource limits for the current process using `setrlimit`.
///
/// # Parameters
///
/// * `max_memory_bytes` — Maximum address space (RLIMIT_AS).
/// * `max_file_handles` — Maximum open file descriptors (RLIMIT_NOFILE).
/// * `max_threads` — Maximum number of processes/threads (RLIMIT_NPROC).
///
/// # Errors
///
/// Returns `ProcessError::OsError` if any `setrlimit` call fails.
pub fn set_process_limits(
    max_memory_bytes: Option<u64>,
    max_file_handles: Option<u32>,
    max_threads: Option<u32>,
) -> Result<(), ProcessError> {
    use nix::sys::resource::{setrlimit, Resource};

    if let Some(max_mem) = max_memory_bytes {
        setrlimit(Resource::RLIMIT_AS, max_mem, max_mem).map_err(|e| {
            ProcessError::OsError {
                id: ProcessId::root(),
                source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            }
        })?;
    }

    if let Some(max_fds) = max_file_handles {
        setrlimit(
            Resource::RLIMIT_NOFILE,
            max_fds as u64,
            max_fds as u64,
        )
        .map_err(|e| ProcessError::OsError {
            id: ProcessId::root(),
            source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
        })?;
    }

    if let Some(max_thr) = max_threads {
        setrlimit(Resource::RLIMIT_NPROC, max_thr as u64, max_thr as u64).map_err(|e| {
            ProcessError::OsError {
                id: ProcessId::root(),
                source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            }
        })?;
    }

    Ok(())
}
