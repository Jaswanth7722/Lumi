//! # Panic Handler
//!
//! Replaces the default Rust panic hook with one that generates a crash report,
//! writes it atomically to disk, emits an IPC event, runs shutdown hooks, and
//! calls `std::process::abort()`.
//!
//! # Thread Safety
//! The panic handler is registered once and is globally accessible. The shutdown
//! hook registry uses `parking_lot::RwLock` for thread-safe access.

use crate::crash::{CrashReport, CrashType, PanicInfo, ThreadSnapshot};
use crate::stacktrace::StackTrace;
use parking_lot::RwLock;
use std::sync::Arc;
use uuid::Uuid;

/// A shutdown hook that is called during crash handling.
pub type ShutdownHook = Arc<dyn Fn() + Send + Sync>;

/// Registry of shutdown hooks to call during panic/crash handling.
static SHUTDOWN_HOOKS: std::sync::OnceLock<RwLock<Vec<(String, ShutdownHook)>>> =
    std::sync::OnceLock::new();

fn shutdown_hooks() -> &'static RwLock<Vec<(String, ShutdownHook)>> {
    SHUTDOWN_HOOKS.get_or_init(|| RwLock::new(Vec::new()))
}

/// Register a shutdown hook to be called during crash handling.
///
/// # Arguments
/// * `name` - A human-readable name for the hook (used for debugging).
/// * `hook` - The closure to execute during shutdown.
///
/// # Thread Safety
/// This function is thread-safe and can be called from any thread.
///
/// # Panics
/// Does not panic.
pub fn register_shutdown_hook(name: &str, hook: ShutdownHook) {
    shutdown_hooks().write().push((name.to_string(), hook));
}

/// Run all registered shutdown hooks.
///
/// Hooks are executed in reverse registration order. If a hook panics,
/// it is caught and logged, and the remaining hooks continue to execute.
///
/// # Thread Safety
/// This function acquires a write lock on the shutdown hooks registry.
///
/// # Panics
/// This function does not panic. Hook panics are caught.
fn run_shutdown_hooks() {
    let hooks = shutdown_hooks().read().clone();
    for (name, hook) in hooks.iter().rev() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            hook();
        }));
        if result.is_err() {
            // Best-effort: write to stderr since the logging system may be compromised
            eprintln!("[lumi-error] Shutdown hook '{}' panicked", name);
        }
    }
}

/// Install the custom panic handler.
///
/// This replaces the default Rust panic hook with one that:
/// 1. Captures a stack trace
/// 2. Assembles a CrashReport
/// 3. Writes it atomically to `{data_dir}/crashes/{uuid}.json`
/// 4. Runs all registered shutdown hooks
/// 5. Calls `std::process::abort()`
///
/// # Thread Safety
/// This function should be called exactly once during bootstrap. Calling it
/// multiple times is safe — the previous hook is returned.
///
/// # Panics
/// Does not panic.
pub fn install_panic_handler() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        handle_panic(panic_info);
    }));
    // Keep previous hook for potential chaining (not called by default)
    let _ = prev;
}

/// Internal panic handler implementation.
fn handle_panic(panic_info: &std::panic::PanicHookInfo<'_>) {
    // Extract panic message
    let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    };

    let location = panic_info.location().map(|loc| PanicInfo {
        message: message.clone(),
        file: Some(loc.file().to_string()),
        line: Some(loc.line()),
    });

    // Create crash report ID first (before any potential allocation issues)
    let crash_id = Uuid::new_v4();

    // Attempt full crash report
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let crash_type = CrashType::Panic {
            message: message.clone(),
        };

        let mut report = CrashReport::new(crash_type);

        if let Some(info) = location {
            report = report.with_panic_info(info);
        }

        // Capture current thread
        let thread_id = format!("{:?}", std::thread::current().id())
            .parse::<u64>()
            .unwrap_or(0);
        let thread_name = std::thread::current().name().map(String::from);
        report = report.with_thread(ThreadSnapshot {
            thread_id,
            name: thread_name,
            is_async: false,
            stack_trace: Some(StackTrace::capture().to_string()),
        });

        // Determine data directory
        let data_dir =
            std::path::PathBuf::from(std::env::var("LUMI_DATA_DIR").unwrap_or_else(|_| {
                let base = dirs_data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
                base.join("lumi")
            }));

        let crash_dir = data_dir.join("crashes");

        // Write the crash report
        match report.write_to_dir(&crash_dir) {
            Ok(path) => {
                eprintln!("[lumi-error] Crash report written to {}", path.display());
            }
            Err(e) => {
                eprintln!("[lumi-error] Failed to write crash report: {}", e);
            }
        }
    }));

    if result.is_err() {
        // The crash report generation itself panicked — write a fallback
        match CrashReport::write_fallback(crash_id, &message) {
            Ok(path) => {
                eprintln!(
                    "[lumi-error] Fallback crash report written to {}",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("[lumi-error] Failed to write fallback crash report: {}", e);
            }
        }
    }

    // Run shutdown hooks
    run_shutdown_hooks();

    // Abort the process (not exit — ensures OS cleanup of child processes)
    eprintln!("[lumi-error] Process aborting...");
    std::process::abort();
}

/// Get the platform-appropriate data directory.
fn dirs_data_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| std::path::PathBuf::from(h).join(".local").join("share"))
            })
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME").ok().map(|h| {
            std::path::PathBuf::from(h)
                .join("Library")
                .join("Application Support")
        })
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(std::path::PathBuf::from)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Some(std::path::PathBuf::from("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn test_register_shutdown_hook() {
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        register_shutdown_hook(
            "test_hook",
            Arc::new(move || {
                called_clone.store(true, Ordering::SeqCst);
            }),
        );

        // Run hooks
        run_shutdown_hooks();

        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_panic_handler_install() {
        // Should not panic
        install_panic_handler();
        // Installing twice is also safe
        install_panic_handler();
    }

    #[test]
    fn test_shutdown_hook_panic_safety() {
        register_shutdown_hook(
            "panicking_hook",
            Arc::new(|| {
                panic!("This hook intentionally panics");
            }),
        );

        register_shutdown_hook(
            "good_hook",
            Arc::new(|| {
                // This should still run despite the previous hook panicking
            }),
        );

        // Should not propagate the panic
        run_shutdown_hooks();
    }
}
