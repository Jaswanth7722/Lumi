//! # Windows Platform Operations
//!
//! Process management implementations for Windows.
//!
//! Uses the `windows` crate for Win32 API access including
//! Job Objects, TerminateProcess, and OpenProcess.

use crate::error::ProcessError;
use crate::id::ProcessId;
use std::time::Duration;
use tokio::time;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
    JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Threading::{
    OpenProcess, TerminateProcess, PROCESS_TERMINATE,
};

/// Terminate a Windows process gracefully by opening a handle, waiting,
/// then calling `TerminateProcess` if it doesn't exit.
///
/// # Errors
///
/// Returns `ProcessError::OsError` if the process handle cannot be opened
/// or if `TerminateProcess` fails.
///
/// # Platform Notes
///
/// Windows does not have SIGTERM in the same sense as Unix. This function
/// waits briefly then calls `TerminateProcess` for forceful termination.
pub async fn graceful_kill(pid: u32, timeout: Duration) -> Result<(), ProcessError> {
    // SAFETY: OpenProcess is safe to call with valid parameters.
    // The handle is closed via CloseHandle when dropped.
    let handle = unsafe {
        OpenProcess(PROCESS_TERMINATE, false, pid).map_err(|e| ProcessError::OsError {
            id: ProcessId::root(),
            source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
        })?
    };

    // Wait for the timeout
    time::sleep(timeout).await;

    // Terminate the process
    // SAFETY: handle is a valid handle to the target process.
    let result = unsafe { TerminateProcess(handle, 1) };

    // Close the handle
    // SAFETY: handle is a valid open handle.
    unsafe {
        let _ = CloseHandle(handle);
    }

    if result.is_err() {
        return Err(ProcessError::OsError {
            id: ProcessId::root(),
            source: std::io::Error::new(std::io::ErrorKind::Other, "TerminateProcess failed"),
        });
    }

    Ok(())
}

/// Create a Windows Job Object and assign a process to it.
///
/// Configures kill-on-job-close so child processes are cleaned up if
/// the supervisor exits abnormally.
///
/// # Errors
///
/// Returns `ProcessError::OsError` if Job Object creation or process
/// assignment fails.
///
/// # Returns
///
/// The `HANDLE` to the created Job Object. The caller must keep this
/// handle alive for the lifetime of the job.
pub fn create_job_object(pid: u32) -> Result<HANDLE, ProcessError> {
    // SAFETY: CreateJobObjectW with null name creates an unnamed job object.
    let job_handle = unsafe { CreateJobObjectW(None, None) }.map_err(|e| {
        ProcessError::OsError {
            id: ProcessId::root(),
            source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
        }
    })?;

    // Configure kill-on-job-close
    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

    // SAFETY: SetInformationJobObject is safe with a valid handle and initialized struct.
    unsafe {
        SetInformationJobObject(
            job_handle,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    }
    .map_err(|e| ProcessError::OsError {
        id: ProcessId::root(),
        source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
    })?;

    // Open the target process
    // SAFETY: OpenProcess is safe with PROCESS_TERMINATE access.
    let process_handle = unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }.map_err(|e| {
        ProcessError::OsError {
            id: ProcessId::root(),
            source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
        }
    })?;

    // Assign process to job
    // SAFETY: Both handles are valid.
    unsafe { AssignProcessToJobObject(job_handle, process_handle) }.map_err(|e| {
        ProcessError::OsError {
            id: ProcessId::root(),
            source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
        }
    })?;

    // Close the process handle (the job keeps a reference)
    // SAFETY: The process handle is no longer needed.
    unsafe {
        let _ = CloseHandle(process_handle);
    }

    Ok(job_handle)
}
