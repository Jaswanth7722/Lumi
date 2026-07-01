//! # Platform Abstraction
//!
//! Cross-platform process management operations.
//!
//! Provides platform-specific implementations for:
//! - Graceful process termination (SIGTERM → SIGKILL on Unix, TerminateProcess on Windows)
//! - Process group management
//! - Resource limit configuration
//! - Job object creation (Windows)
//!
//! # Thread Safety
//!
//! All platform functions are `Send` and must be safe to call from async contexts.
//! Functions that block are wrapped with `tokio::task::spawn_blocking`.

use crate::error::ProcessError;
use crate::id::ProcessId;
use std::time::Duration;

#[cfg(unix)]
mod platform_unix;
#[cfg(windows)]
mod platform_windows;

#[cfg(unix)]
pub use platform_unix::*;
#[cfg(windows)]
pub use platform_windows::*;

/// Gracefully terminate a process by PID.
///
/// Sends a termination signal and waits up to `timeout` for the process
/// to exit. If the process does not exit within the timeout, it is force-killed.
///
/// # Platform Notes
///
/// - **Unix**: Sends SIGTERM, then SIGKILL after timeout.
/// - **Windows**: Sends WM_CLOSE, then TerminateProcess after timeout.
///
/// # Errors
///
/// Returns `ProcessError::OsError` if the operation fails.
/// Returns `ProcessError::StopTimeout` if force-kill was required.
/// Returns `ProcessError::PlatformUnsupported` on unsupported platforms.
pub async fn graceful_kill(pid: u32, timeout: Duration) -> Result<(), ProcessError> {
    #[cfg(unix)]
    {
        platform_unix::graceful_kill(pid, timeout).await
    }
    #[cfg(windows)]
    {
        platform_windows::graceful_kill(pid, timeout).await
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (pid, timeout);
        Err(ProcessError::PlatformUnsupported {
            operation: "graceful_kill",
        })
    }
}

/// Set resource limits for the current process.
///
/// # Platform Notes
///
/// - **Unix**: Uses `setrlimit` for RLIMIT_AS, RLIMIT_NOFILE, RLIMIT_NPROC.
/// - **Windows**: Uses Job Object resource limits.
///
/// # Errors
///
/// Returns `ProcessError::OsError` if `setrlimit` fails.
/// Returns `ProcessError::PlatformUnsupported` on unsupported platforms.
#[cfg(unix)]
pub fn set_process_limits(
    max_memory_bytes: Option<u64>,
    max_file_handles: Option<u32>,
    max_threads: Option<u32>,
) -> Result<(), ProcessError> {
    platform_unix::set_process_limits(max_memory_bytes, max_file_handles, max_threads)
}

/// Create a Windows Job Object and assign a process to it.
///
/// Configures kill-on-job-close so child processes are cleaned up if
/// the supervisor exits abnormally.
///
/// # Platform Notes
///
/// **Windows only.** On non-Windows platforms, this is a no-op.
///
/// # Errors
///
/// Returns `ProcessError::OsError` on Windows if Job Object creation fails.
#[cfg(windows)]
pub fn create_job_object(pid: u32) -> Result<windows::Win32::Foundation::HANDLE, ProcessError> {
    platform_windows::create_job_object(pid)
}
