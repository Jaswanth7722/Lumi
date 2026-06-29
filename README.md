<div align="center">

# ⚡ Lumi

**Desktop AI Companion · Pure Rust & TypeScript · Zero Python**

[![Rust](https://img.shields.io/badge/rust-2024-edition?style=for-the-badge&logo=rust&logoColor=white&color=%23DEA584)](https://www.rust-lang.org)
[![TypeScript](https://img.shields.io/badge/typescript-5.7-3178C6?style=for-the-badge&logo=typescript&logoColor=white)](https://www.typescriptlang.org)
[![License](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge)](LICENSE)
[![Build](https://img.shields.io/badge/build-passing-brightgreen?style=for-the-badge)](https://github.com/lumi-platform/lumi/actions)
[![Crates.io](https://img.shields.io/badge/cargo-workspace-8A2BE2?style=for-the-badge&logo=rust)](Cargo.toml)

**An always-on, locally-first AI companion that lives on your desktop.**  
Lumi is a high-performance desktop runtime written entirely in **Rust 2024 edition** with a **TypeScript SDK** for plugin development. Built for sub-millisecond IPC, lock-free state machines, and real-time character animation.

</div>

---

## Architecture Blueprint — 50-Component Topology

The Lumi platform is decomposed into **50 components** across **10 architectural layers**. Below is the complete system map. Components marked **`✓`** are fully implemented and operational. All others are in active development.

### Layer 1: Foundation Runtime `✓`

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 1 | **Core Runtime** | `lumi-runtime` | `✓` | Boostrap lifecycle, scheduler, health monitor, graceful shutdown |
| 2 | **Configuration** | `lumi-config` | `✓` | 7-stage TOML/ENV/CLI loading, hot-reload, schema migration, `Secret<T>` values |
| 3 | **Logging** | `lumi-logging` | `✓` | Tracing layer → crossbeam pipeline → filter → redact → format → sink |
| 4 | **Error Handling** | `lumi-error` | `✓` | `LumiError` with typed categories, recovery engine, retry policies, crash reports |
| 5 | **Performance** | `lumi-performance` | `✓` | Real-time-safe counters, HDR histograms, threshold alerting, profiler integration |
| 6 | **State Machine** | `lumi-state` | `✓` | Typestate hybrid, 6-step atomic transitions, cross-machine guards, history |
| 7 | **IPC Framework** | `lumi-ipc` | `✓` | 3-tier transport (SHM/Socket/InProc), auth engine, middleware pipeline |
| 8 | **Wire Protocol** | `lumi-ipc::wire` | `✓` | Binary framing, MessagePack, zstd compression, ChaCha20-Poly1305 AEAD |

### Layer 2: Data & Storage

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 9 | **Memory Store** | `lumi-storage` | — | Vector memory with embedding, confidence scoring, TTL-based retention |
| 10 | **Storage Engine** | `lumi-storage` | — | SQLite + LMDB hybrid for structured/unstructured data |
| 11 | **Cache Layer** | `lumi-config::cache` | `✓` | Lock-free `ArcSwap`-based config cache |
| 12 | **Credential Vault** | `lumi-common` | — | OS keychain integration via `SecretStore` trait |

### Layer 3: AI Core

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 13 | **Inference Engine** | `lumi-core` | — | Provider-agnostic LLM interface (Anthropic, OpenAI, local GGUF) |
| 14 | **Context Manager** | `lumi-core` | — | Sliding window, token budgeting, context compression |
| 15 | **Planning Engine** | `lumi-common` | `✓` | DAG-based task planner with dependency resolution |
| 16 | **Tool Execution** | `lumi-core` | — | Plugin tool calling, approval gates, result routing |
| 17 | **Prompt Guard** | `lumi-common` | — | System prompt enforcement, injection detection |
| 18 | **Model Registry** | `lumi-core` | — | Provider capability discovery, auto-fallback |

### Layer 4: Voice & Audio

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 19 | **Wake Word** | `lumi-voice` | — | Local wake word detection with configurable threshold |
| 20 | **STT Engine** | `lumi-voice` | — | Whisper model integration (tiny → large-v3), language detection |
| 21 | **TTS Engine** | `lumi-voice` | — | Neural TTS with SSML processing and emotion control |
| 22 | **Lip Sync** | `lumi-common` | `✓` | Viseme extraction (14 phoneme categories), frame-accurate animation |
| 23 | **Voice VAD** | `lumi-common` | `✓` | Voice activity detection, speech start/end/silence events |

### Layer 5: Rendering & Animation

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 24 | **Render Engine** | `lumi-render` | — | GPU-accelerated (OpenGL/Vulkan/DirectX/Metal), 60+ FPS |
| 25 | **Character Renderer** | `lumi-render` | — | Spine-compatible skeletal animation, procedural motion |
| 26 | **Animation System** | `lumi-common` | — | State-driven animation blending, clip library |
| 27 | **Physics Engine** | `lumi-common` | — | 2D physics for cloth/hair, configurable gravity |
| 28 | **Workspace UI** | `lumi-render` | — | Panel system, approval dialogs, floating character window |

### Layer 6: Desktop Awareness

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 29 | **Window Tracker** | `lumi-core` | — | Active window detection (macOS/Linux/Windows) |
| 30 | **Screen Capture** | `lumi-core` | — | Privacy-gated OCR, focus mode detection |
| 31 | **Clipboard Monitor** | `lumi-common` | `✓` | Tiered clipboard access (Never/OnRequest/Always) |
| 32 | **Notification Listener** | `lumi-core` | — | OS notification interception (macOS: NSAccessibility) |

### Layer 7: Plugin System

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 33 | **Plugin Host** | `lumi-plugin-host` | — | WASM sandbox, capability-restricted execution |
| 34 | **Plugin SDK** | `lumi-sdk` | — | TypeScript SDK with full IPC bindings |
| 35 | **Plugin Registry** | `lumi-core` | — | Signed plugin verification, version management |
| 36 | **Sandbox** | `lumi-common` | `✓` | Process isolation, capability model, threat model |

### Layer 8: Security

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 37 | **IPC Auth** | `lumi-ipc::auth` | — | ECDH handshake, HMAC-SHA256, replay protection |
| 38 | **Tool Approval** | `lumi-common` | `✓` | Approval gate with pattern matching, user confirmation |
| 39 | **PII Detection** | `lumi-common` | `✓` | PII scanner (credit card, email, API keys, passwords) |
| 40 | **Crash Reporting** | `lumi-error::crash` | `✓` | Atomic crash reports with environment snapshots |
| 41 | **Audit Log** | `lumi-common` | `✓` | Security-sensitive event logging, tool audit trail |

### Layer 9: Diagnostics & Observability

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 42 | **Error History** | `lumi-error` | `✓` | Circular buffer (10K entries), multi-key indexing, full-text search |
| 43 | **Failure Patterns** | `lumi-error` | `✓` | Sliding-window pattern detection, severity escalation |
| 44 | **Recovery Engine** | `lumi-error` | `✓` | Rule-based recovery with thrash detection |
| 45 | **Metrics Dashboard** | `lumi-performance` | — | Real-time FPS, latency, memory; HDR histogram export |
| 46 | **Diagnostic Report** | `lumi-error` | `✓` | Queryable error reports, batch export, CrashReport disk writer |

### Layer 10: Platform & Interop

| # | Component | Crate | Status | Description |
|---|-----------|-------|--------|-------------|
| 47 | **Model Context Protocol** | `lumi-ipc` | — | Anthropic MCP broker for external AI tool integration |
| 48 | **TypeScript SDK** | `lumi-sdk` | — | Full IPC client, tool definitions, plugin bootstrap |
| 49 | **macOS Integration** | `lumi-core` | — | macOS app bundle, accessibility APIs, Metal backend |
| 50 | **Windows Integration** | `lumi-core` | — | Windows named pipes, DirectX, WinRT interop |

---

## Quick Start

```bash
# Build the entire workspace
cargo build --workspace

# Run all tests
cargo test --workspace

# Start Lumi (requires full build)
cargo run -p lumi-core
```

### Prerequisites

- **Rust 2024 edition** (stable toolchain, minimum 1.84+)
- **Node.js 22+** (for TypeScript SDK and plugin development)
- **System deps**: CMake, pkg-config (for native crate builds)

---

## Workspace Architecture

```
lumi/
├── crates/                  # Internal library crates
│   ├── lumi-runtime/        # Core runtime, bootstrap, lifecycle
│   ├── lumi-config/         # Configuration system (7-stage loader)
│   ├── lumi-logging/        # Structured logging pipeline
│   ├── lumi-error/          # Error handling, recovery, crash reporting
│   ├── lumi-performance/    # Metrics, profiling, threshold alerting
│   ├── lumi-state/          # Hierarchical state machine framework
│   └── lumi-ipc/            # IPC framework + wire protocol
├── lumi-common/             # Shared types, traits, utilities
├── lumi-core/               # Main application binary
├── lumi-render/             # GPU rendering engine
├── lumi-voice/              # Audio processing pipeline
├── lumi-storage/            # Persistent storage layer
├── lumi-plugin-host/        # WASM plugin sandbox
├── lumi-sdk/                # TypeScript plugin SDK
├── tests/                   # Workspace-level integration tests
└── Cargo.toml               # Workspace root (resolver v2, 14 members)
```

### Crate Dependency Graph

```
lumi-core
  ├── lumi-runtime
  │     ├── lumi-config
  │     └── lumi-error
  ├── lumi-ipc
  │     ├── lumi-common
  │     └── lumi-error
  ├── lumi-render (external process)
  ├── lumi-voice  (external process)
  ├── lumi-storage (external process)
  └── lumi-plugin-host (external process)
```

---


## Key Technical Decisions

- **Zero Python**: Lumi is 100% Python-free. The entire stack is Rust + TypeScript, ensuring maximum performance, minimal memory footprint, and no runtime dependency conflicts.
- **Three-Tier IPC**: Shared memory (sub-100µs) for render commands/state → Unix sockets (< 1ms) for structured messages → In-process channels (nanoseconds) for internal events.
- **Wire Protocol**: Binary frame format with MessagePack serialization, zstd compression, and ChaCha20-Poly1305 AEAD encryption. Max 256KB payload per frame.
- **State Machine**: Hybrid typestate-runtime approach — compile-time enforcement for critical paths with runtime extensibility for plugins.
- **Configuration**: 7-stage pipeline (defaults → file → env → CLI → migration → validation → cache) with lock-free `ArcSwap` reads.

---

## License

MIT © 2024 Lumi Platform

---

<div align="center">
  <sub>Built with Rust 2024 · 50 components · 14 workspace crates </sub>
</div>
