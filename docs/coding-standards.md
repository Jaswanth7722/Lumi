# Coding Standards

## Rust Standards

```toml
# Enforced via clippy config
[workspace.lints.clippy]
unwrap_used = "deny"
panic = "deny"
dbg_macro = "warn"
todo = "warn"
```

- All public APIs must have documentation comments
- All error types implement `std::error::Error` + `Display`
- No `unsafe` blocks without a safety comment explaining invariants
- All async functions must be cancel-safe unless documented otherwise
- Functions > 50 lines are flagged for review

## Naming Conventions

| Category | Convention | Example |
|---|---|---|
| Modules | `snake_case` | `memory_system` |
| Types / Traits | `PascalCase` | `MemoryEntry` |
| Functions | `snake_case` | `retrieve_memories` |
| Constants | `SCREAMING_SNAKE_CASE` | `MAX_CONTEXT_TOKENS` |
| Channels | `domain.action` | `ai.state`, `render.command` |
| Tool names | `namespace.verb` | `fs.read_file`, `terminal.execute` |

## Error Handling

All errors must be typed, contextual, and actionable:

```rust
// Good
pub enum MemoryError {
    DatabaseCorrupted { path: PathBuf, reason: String },
    VectorIndexMissing,
    EmbeddingModelNotLoaded,
    StorageFull { available_bytes: u64, required_bytes: u64 },
}

// Bad
pub enum MemoryError {
    Error(String),
}
```

## Testing Requirements

- Unit tests co-located with implementation (`#[cfg(test)]` modules)
- Integration tests in `/tests/`
- Minimum test coverage: 80% line coverage on `lumas-core`
- All new features require: one happy-path test, one error-path test, one edge-case test
