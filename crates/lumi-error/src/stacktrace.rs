//! # Stack Trace Capture
//!
//! Stack trace with symbol resolution, demangling, and frame filtering.

use std::fmt;

/// A single stack frame.
#[derive(Debug, Clone)]
pub struct StackFrame {
    /// Function name (demangled).
    pub function: String,
    /// Source file path.
    pub file: Option<String>,
    /// Source line number.
    pub line: Option<u32>,
    /// Raw symbol name (before demangling).
    pub raw_symbol: String,
    /// Module path.
    pub module: Option<String>,
}

/// A captured stack trace.
#[derive(Debug, Clone)]
pub struct StackTrace {
    /// Stack frames (outermost first).
    pub frames: Vec<StackFrame>,
    /// Whether this is a truncated trace.
    pub truncated: bool,
}

impl StackTrace {
    /// Capture the current stack trace.
    #[cfg(feature = "backtrace")]
    pub fn capture() -> Self {
        let bt = backtrace::Backtrace::new();
        let frames: Vec<StackFrame> = bt
            .frames()
            .iter()
            .filter_map(|frame| {
                let symbols = frame.symbols();
                symbols.first().map(|sym| StackFrame {
                    function: sym.name().map(|n| format!("{n}")).unwrap_or_default(),
                    file: sym.filename().map(|p| p.to_string_lossy().to_string()),
                    line: sym.lineno(),
                    raw_symbol: sym.name().map(|n| format!("{n}")).unwrap_or_default(),
                    module: sym.filename().map(|p| p.to_string_lossy().to_string()),
                })
            })
            .filter(|f| !Self::is_internal_frame(&f.function))
            .collect();

        Self {
            frames,
            truncated: false,
        }
    }

    /// Capture the current stack trace (no-op when backtrace feature is disabled).
    #[cfg(not(feature = "backtrace"))]
    pub fn capture() -> Self {
        Self {
            frames: Vec::new(),
            truncated: false,
        }
    }

    /// Filter out internal frames from the trace.
    fn is_internal_frame(function: &str) -> bool {
        function.starts_with("std::")
            || function.starts_with("core::")
            || function.starts_with("alloc::")
            || function.starts_with("tokio::runtime::")
            || function.starts_with("lumi_error::")
            || function.starts_with("<")
            || function == "___rust_try"
            || function == "___rust_maybe_catch_panic"
    }

    /// Check if any frame contains sensitive keywords.
    pub fn is_sensitive(&self) -> bool {
        let keywords = ["password", "secret", "token", "key", "credential"];
        self.frames.iter().any(|f| {
            let lower = f.function.to_lowercase();
            keywords.iter().any(|k| lower.contains(k))
        })
    }

    /// Number of frames.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether the trace is empty.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

impl fmt::Display for StackTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, frame) in self.frames.iter().enumerate() {
            writeln!(f, "  {i:>3}: {func}", func = frame.function)?;
            if let (Some(file), Some(line)) = (&frame.file, frame.line) {
                writeln!(f, "        at {file}:{line}")?;
            }
        }
        Ok(())
    }
}

impl Default for StackTrace {
    fn default() -> Self {
        Self {
            frames: Vec::new(),
            truncated: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_trace_capture() {
        let trace = StackTrace::capture();
        // When backtrace feature is disabled, frames will be empty
        #[cfg(feature = "backtrace")]
        assert!(!trace.frames.is_empty());
        #[cfg(not(feature = "backtrace"))]
        assert!(trace.frames.is_empty());
    }

    #[test]
    fn test_sensitive_detection() {
        let trace = StackTrace {
            frames: vec![StackFrame {
                function: "verify_password".into(),
                file: None,
                line: None,
                raw_symbol: String::new(),
                module: None,
            }],
            truncated: false,
        };
        assert!(trace.is_sensitive());
    }
}
