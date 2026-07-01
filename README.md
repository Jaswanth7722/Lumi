<div align="center">

# ⚡ Lumas

**Desktop AI Companion · Pure Rust & TypeScript**

[![Rust](https://img.shields.io/badge/rust-2024-edition?style=for-the-badge&logo=rust&logoColor=white&color=%23DEA584)](https://www.rust-lang.org)
[![TypeScript](https://img.shields.io/badge/typescript-5.7-3178C6?style=for-the-badge&logo=typescript&logoColor=white)](https://www.typescriptlang.org)
[![License](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge)](LICENSE)
[![Build](https://img.shields.io/badge/build-passing-brightgreen?style=for-the-badge)](https://github.com/lumas-platform/lumas/actions)
[![Crates.io](https://img.shields.io/badge/cargo-workspace-8A2BE2?style=for-the-badge&logo=rust)](Cargo.toml)

**An always-on, locally-first AI companion that lives on your desktop.**  
Lumas is a high-performance desktop runtime written entirely in **Rust 2024 edition** with a **TypeScript SDK** for plugin development. Built for sub-millisecond IPC, lock-free state machines, and real-time character animation.

</div>

---

## Architecture Blueprint — 50-Component Topology

The Lumas platform is decomposed into **50 components** across **10 architectural layers**. Below is the complete system map. Components marked **`✓`** are fully implemented and operational. All others are in active development.

### Layer 1: Foundation Runtime `✓`

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 1 | **Core Runtime** | `lumas-runtime` | `✓` | Boostrap lifecycle, scheduler, health monitor, graceful shutdown |
| 2 | **Configuration** | `lumas-config` | `✓` | 7-stage TOML/ENV/CLI loading, hot-reload, schema migration, `Secret<T>` values |
| 3 | **Logging** | `lumas-logging` | `✓` | Tracing layer → crossbeam pipeline → filter → redact → format → sink |
| 4 | **Error Handling** | `lumas-error` | `✓` | `LumasError` with typed categories, recovery engine, retry policies, crash reports |
| 5 | **Performance** | `lumas-performance` | `✓` | Real-time-safe counters, HDR histograms, threshold alerting, profiler integration |
| 6 | **State Machine** | `lumas-state` | `✓` | Typestate hybrid, 6-step atomic transitions, cross-machine guards, history |
| 7 | **IPC Framework** | `lumas-ipc` | `✓` | 3-tier transport (SHM/Socket/InProc), auth engine, middleware pipeline |
| 8 | **Wire Protocol** | `lumas-ipc::wire` | `✓` | Binary framing, MessagePack, zstd compression, ChaCha20-Poly1305 AEAD |

### Layer 2: Data & Storage

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 9 | **Memory Store** | `lumas-storage` | — | Vector memory with embedding, confidence scoring, TTL-based retention |
| 10 | **Storage Engine** | `lumas-storage` | — | SQLite + LMDB hybrid for structured/unstructured data |
| 11 | **Cache Layer** | `lumas-config::cache` | `✓` | Lock-free `ArcSwap`-based config cache |
| 12 | **Credential Vault** | `lumas-common` | — | OS keychain integration via `SecretStore` trait |

### Layer 3: AI Core

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 13 | **Inference Engine** | `lumas-core` | — | Provider-agnostic LLM interface (Anthropic, OpenAI, local GGUF) |
| 14 | **Context Manager** | `lumas-core` | — | Sliding window, token budgeting, context compression |
| 15 | **Planning Engine** | `lumas-common` | `✓` | DAG-based task planner with dependency resolution |
| 16 | **Tool Execution** | `lumas-core` | — | Plugin tool calling, approval gates, result routing |
| 17 | **Prompt Guard** | `lumas-common` | — | System prompt enforcement, injection detection |
| 18 | **Model Registry** | `lumas-core` | — | Provider capability discovery, auto-fallback |

### Layer 4: Voice & Audio

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 19 | **Wake Word** | `lumas-voice` | — | Local wake word detection with configurable threshold |
| 20 | **STT Engine** | `lumas-voice` | — | Whisper model integration (tiny → large-v3), language detection |
| 21 | **TTS Engine** | `lumas-voice` | — | Neural TTS with SSML processing and emotion control |
| 22 | **Lip Sync** | `lumas-common` | `✓` | Viseme extraction (14 phoneme categories), frame-accurate animation |
| 23 | **Voice VAD** | `lumas-common` | `✓` | Voice activity detection, speech start/end/silence events |

### Layer 5: Rendering & Animation

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 24 | **Render Engine** | `lumas-render` | — | GPU-accelerated (OpenGL/Vulkan/DirectX/Metal), 60+ FPS |
| 25 | **Character Renderer** | `lumas-render` | — | Spine-compatible skeletal animation, procedural motion |
| 26 | **Animation System** | `lumas-common` | — | State-driven animation blending, clip library |
| 27 | **Physics Engine** | `lumas-common` | — | 2D physics for cloth/hair, configurable gravity |
| 28 | **Workspace UI** | `lumas-render` | — | Panel system, approval dialogs, floating character window |

### Layer 6: Desktop Awareness

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 29 | **Window Tracker** | `lumas-core` | — | Active window detection (macOS/Linux/Windows) |
| 30 | **Screen Capture** | `lumas-core` | — | Privacy-gated OCR, focus mode detection |
| 31 | **Clipboard Monitor** | `lumas-common` | `✓` | Tiered clipboard access (Never/OnRequest/Always) |
| 32 | **Notification Listener** | `lumas-core` | — | OS notification interception (macOS: NSAccessibility) |

### Layer 7: Plugin System

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 33 | **Plugin Host** | `lumas-plugin-host` | — | WASM sandbox, capability-restricted execution |
| 34 | **Plugin SDK** | `lumas-sdk` | — | TypeScript SDK with full IPC bindings |
| 35 | **Plugin Registry** | `lumas-core` | — | Signed plugin verification, version management |
| 36 | **Sandbox** | `lumas-common` | `✓` | Process isolation, capability model, threat model |

### Layer 8: Security

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 37 | **IPC Auth** | `lumas-ipc::auth` | — | ECDH handshake, HMAC-SHA256, replay protection |
| 38 | **Tool Approval** | `lumas-common` | `✓` | Approval gate with pattern matching, user confirmation |
| 39 | **PII Detection** | `lumas-common` | `✓` | PII scanner (credit card, email, API keys, passwords) |
| 40 | **Crash Reporting** | `lumas-error::crash` | `✓` | Atomic crash reports with environment snapshots |
| 41 | **Audit Log** | `lumas-common` | `✓` | Security-sensitive event logging, tool audit trail |

### Layer 9: Diagnostics & Observability

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 42 | **Error History** | `lumas-error` | `✓` | Circular buffer (10K entries), multi-key indexing, full-text search |
| 43 | **Failure Patterns** | `lumas-error` | `✓` | Sliding-window pattern detection, severity escalation |
| 44 | **Recovery Engine** | `lumas-error` | `✓` | Rule-based recovery with thrash detection |
| 45 | **Metrics Dashboard** | `lumas-performance` | — | Real-time FPS, latency, memory; HDR histogram export |
| 46 | **Diagnostic Report** | `lumas-error` | `✓` | Queryable error reports, batch export, CrashReport disk writer |

### Layer 10: Platform & Interop

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 47 | **Model Context Protocol** | `lumas-ipc` | — | Anthropic MCP broker for external AI tool integration |
| 48 | **TypeScript SDK** | `lumas-sdk` | — | Full IPC client, tool definitions, plugin bootstrap |
| 49 | **macOS Integration** | `lumas-core` | — | macOS app bundle, accessibility APIs, Metal backend |
| 50 | **Windows Integration** | `lumas-core` | — | Windows named pipes, DirectX, WinRT interop |

---

## Quick Start

```bash
# Build the entire workspace
cargo build --workspace

# Run all tests
cargo test --workspace

# Start Lumas (requires full build)
cargo run -p lumas-core
```

### Prerequisites

- **Rust 2024 edition** (stable toolchain, minimum 1.84+)
- **Node.js 22+** (for TypeScript SDK and plugin development)
- **System deps**: CMake, pkg-config (for native crate builds)

---

## Workspace Architecture

```
lumas/
├── crates/                  # Internal library crates
│   ├── lumas-runtime/        # Core runtime, bootstrap, lifecycle
│   ├── lumas-config/         # Configuration system (7-stage loader)
│   ├── lumas-logging/        # Structured logging pipeline
│   ├── lumas-error/          # Error handling, recovery, crash reporting
│   ├── lumas-performance/    # Metrics, profiling, threshold alerting
│   ├── lumas-state/          # Hierarchical state machine framework
│   └── lumas-ipc/            # IPC framework + wire protocol
├── lumas-common/             # Shared types, traits, utilities
├── lumas-core/               # Main application binary
├── lumas-render/             # GPU rendering engine
├── lumas-voice/              # Audio processing pipeline
├── lumas-storage/            # Persistent storage layer
├── lumas-plugin-host/        # WASM plugin sandbox
├── lumas-sdk/                # TypeScript plugin SDK
├── tests/                   # Workspace-level integration tests
└── Cargo.toml               # Workspace root (resolver v2, 14 members)
```

### Crate Dependency Graph

```
lumas-core
  ├── lumas-runtime
  │     ├── lumas-config
  │     └── lumas-error
  ├── lumas-ipc
  │     ├── lumas-common
  │     └── lumas-error
  ├── lumas-render (external process)
  ├── lumas-voice  (external process)
  ├── lumas-storage (external process)
  └── lumas-plugin-host (external process)
```

---


## Key Technical Decisions

- **Zero Python**: Lumas is 100% Python-free. The entire stack is Rust + TypeScript, ensuring maximum performance, minimal memory footprint, and no runtime dependency conflicts.
- **Three-Tier IPC**: Shared memory (sub-100µs) for render commands/state → Unix sockets (< 1ms) for structured messages → In-process channels (nanoseconds) for internal events.
- **Wire Protocol**: Binary frame format with MessagePack serialization, zstd compression, and ChaCha20-Poly1305 AEAD encryption. Max 256KB payload per frame.
- **State Machine**: Hybrid typestate-runtime approach — compile-time enforcement for critical paths with runtime extensibility for plugins.
- **Configuration**: 7-stage pipeline (defaults → file → env → CLI → migration → validation → cache) with lock-free `ArcSwap` reads.

---

## License

MIT © 2024 Lumas Platform

---

<div align="center">
  <sub>Built with Rust 2024 · 50 components · 14 workspace crates </sub>
</div>
