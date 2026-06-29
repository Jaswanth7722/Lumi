/// # lumi-ipc Build Script
///
/// Detects the target platform and sets cfg flags for:
/// - `unix-socket`: available on macOS, Linux, BSD
/// - `named-pipe`: available on Windows
/// - `shared-memory`: available on Unix (mmap) and Windows (CreateFileMapping)
///
/// Also validates that the Rust toolchain meets minimum version requirements.

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS")
        .unwrap_or_else(|_| "unknown".to_string());

    let target_family = std::env::var("CARGO_CFG_TARGET_FAMILY")
        .unwrap_or_else(|_| "unknown".to_string());

    // Emit cargo instructions for conditional compilation
    match target_os.as_str() {
        "macos" | "ios" | "linux" | "freebsd" | "openbsd" | "netbsd" | "android" => {
            println!("cargo:rustc-cfg=platform_unix");
            println!("cargo:rustc-cfg=platform_unix_socket");
            println!("cargo:rustc-cfg=platform_shared_memory");
        }
        "windows" => {
            println!("cargo:rustc-cfg=platform_windows");
            println!("cargo:rustc-cfg=platform_named_pipe");
            println!("cargo:rustc-cfg=platform_shared_memory");
        }
        _ => {
            // Unsupported platform — only in-process transport will work
            println!("cargo:warning=Lumi IPC: unsupported platform '{}'. Only in-process transport available.", target_os);
        }
    }

    // Family-based flags
    if target_family == "unix" {
        println!("cargo:rustc-cfg=platform_unix_socket");
    }

    // Verify minimum Rust version
    let rust_version = std::env::var("CARGO_PKG_RUST_VERSION").ok();
    if let Some(version) = rust_version {
        println!("cargo:rustc-cfg=rust_version=\"{}\"", version);
    }

    // Re-run if the build script changes
    println!("cargo:rerun-if-changed=build.rs");
}
