//! # Structural Test: No Duplicate State Machine
//!
//! Zero tolerance for a parallel behavioral state enum in this crate.
//! Behavioral state belongs in `lumas_state::CharacterMachine`, not here.

use std::fs;

/// State-like variant names that belong in `lumas_state::CharacterMachine`,
/// **not** in this crate.
const STATE_LIKE_VARIANTS: &[&str] = &[
    "Idle", "Watching", "Exploring", "Resting", "Interacting",
    "Listening", "Thinking", "Speaking", "AwaitingInput",
    "Working", "Preparing", "Executing", "VerifyingResult",
    "Sleeping", "FocusMode",
];

fn collect_source_files(dir: &str) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_source_files(&path.to_string_lossy()));
            } else if path.extension().map_or(false, |e| e == "rs") {
                files.push(path.to_string_lossy().to_string());
            }
        }
    }
    files
}

#[test]
fn test_no_parallel_state_enum_in_src() {
    let src_dir = "crates/lumas-character/src";
    let files = collect_source_files(src_dir);

    for file_path in &files {
        let content = fs::read_to_string(file_path)
            .unwrap_or_else(|_| panic!("Failed to read {}", file_path));

        let lines: Vec<&str> = content.lines().collect();

        for variant in STATE_LIKE_VARIANTS {
            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim();

                // Skip comments and string literals
                if trimmed.starts_with("//") || trimmed.starts_with("\"") {
                    continue;
                }

                // Look for a line containing the variant name in enum context
                if trimmed.contains(variant) {
                    // Check if there's an `enum` keyword within 15 lines above
                    let start = if i > 15 { i - 15 } else { 0 };
                    let has_enum_nearby = lines[start..=i].iter().any(|pl| {
                        let pt = pl.trim();
                        pt.starts_with("pub enum") || pt.starts_with("enum ")
                    });

                    if has_enum_nearby {
                        panic!(
                            "\n❌ FAIL: {}:{} contains state-like variant '{}' in enum context.\n\
                             Do NOT define behavioral state enums in lumas-character!\n\
                             Use lumas_state::CharacterMachine instead.\n\
                             Line content: {}",
                            file_path, i + 1, variant, trimmed
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn test_crate_does_not_export_state_enum() {
    let lib_rs = fs::read_to_string("crates/lumas-character/src/lib.rs")
        .expect("Failed to read lib.rs");

    assert!(
        lib_rs.contains("lumas_state"),
        "lib.rs must reference lumas_state (the authoritative state machine)"
    );

    assert!(
        !lib_rs.contains("pub enum CharacterState"),
        "Do NOT define CharacterState in this crate"
    );
    assert!(
        !lib_rs.contains("pub enum StateMachine"),
        "Do NOT define StateMachine in this crate"
    );
}
