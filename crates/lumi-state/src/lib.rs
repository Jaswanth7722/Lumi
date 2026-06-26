//! # Lumi Hierarchical State Machine Framework
//!
//! The single authority on what Lumi is currently doing, what it is allowed to do next,
//! and what must happen when a transition occurs. Every subsystem registers its state
//! machine here.
//!
//! ## Architecture Rationale: The Typestate Hybrid
//!
//! This framework uses a **hybrid approach** combining compile-time typestate encoding
//! with validated runtime state machines:
//!
//! - **Typestate** (feature `typestate`): Used for the innermost, highest-frequency
//!   behavioral states (Character, Animation) where correctness is most critical and
//!   states are closed/stable. Transition functions consume `Machine<Src>` and return
//!   `Machine<Dst>`. Illegal transitions are compile errors.
//!
//! - **Runtime Machines**: Used for extensible, plugin-facing, and cross-subsystem
//!   coordination (Plugin lifecycles, AI processing pipeline). Transition validation
//!   happens at runtime via guards. The two layers compose: runtime guards call into
//!   typestate-encoded subsystems to verify transitions are physically possible.
//!
//! ## Transition Atomicity (Six-Step Protocol)
//!
//! 1. **Guard Evaluation** — all guards evaluated; any failure → reject transition
//! 2. **Exit Actions** — current state's exit actions executed; failure → rollback
//! 3. **Transition Actions** — transition-level actions executed; failure → rollback
//! 4. **State Commit** — atomic state update (commit point, cannot fail)
//! 5. **Entry Actions** — new state's entry actions executed; failure → error state
//! 6. **Event Publication** — transition event published to IPC (best-effort)
//!
//! Steps 1–3 are **tentative**. Step 4 is the **commit point**: it uses an atomic
//! operation so concurrent readers see either the old or new state, never a partial
//! transition. Steps 5–6 occur **after commit**; failures here produce errors but
//! do not roll back.
//!
//! ## Migration Inventory (from Step 2)
//!
//! | Crate | File | Existing State Type | Variants | Migration Plan |
//! |---|---|---|---|---|
//! | lumi-common | src/state_machine.rs | `LumiState` | 12 variants | **Adopt**: implement MachineState |
//! | lumi-common | src/state_machine.rs | `CharacterState` | 8 variants | **Adopt**: IPC signaling state |
//! | lumi-common | src/state_machine.rs | `StateEvent` (old) | 22 variants | **Replace**: use EventId system |
//! | lumi-common | src/ai.rs | `AIState` | 13 variants | **Adopt**: AIMachine states |
//! | lumi-common | src/animation.rs | `AnimationClip` | clip library | **Extend**: add clip playback |
//! | lumi-core | src/state_machine.rs | `StateMachine` (old) | 1 struct | **Replace**: migrate to new engine |
//! | lumi-common/src | workspace.rs | `PanelState` | 6 variants | **Adopt**: workspace lifecycle |
//! | lumi-runtime | src/lifecycle.rs | `LifecycleState` | 6 variants | **Adopt**: RuntimeMachine states |
//! | lumi-common | src/character.rs | `CrystalState` | crystal system | **Extend**: add trait impls |
//!
//! ## Cross-Subsystem Invariants (from Step 3)
//!
//! | Invariant ID | When Subsystem A is in state... | Subsystem B must be... | Enforcement |
//! |---|---|---|---|
//! | INV-001 | `AIState::CallingModel` | Character != `Sleeping` | CrossMachineGuard |
//! | INV-002 | `VoiceState::Speaking` | AnimState allows LipSync | CrossMachineGuard |
//! | INV-003 | `PluginState::Unloading` | AIState != `ToolExecution` | CrossMachineGuard |
//! | INV-004 | `RenderState::Paused` | AnimState updates suspended | CrossMachineGuard |
//! | INV-005 | `RuntimeState::ShuttingDown` | All machines = finalizing | Manager check |
//!
//! # Worked Example
//!
//! ```ignore
//! use lumi_state::prelude::*;
//!
//! // 1. Define a machine
//! let mut machine = StateMachine::new(
//!     MachineId::new("my-machine"),
//!     "My Machine",
//! );
//! machine.add_state(Arc::new(MyState::Ready));
//! machine.add_state(Arc::new(MyState::Active));
//! machine.add_transition(TransitionDefinition::new(
//!     MyState::Ready.id(), MyState::Active.id(),
//!     EventId(1001),
//! ));
//!
//! // 2. Register in manager
//! let manager = StateMachineManager::start(config).await.unwrap();
//! let handle = manager.register(machine).unwrap();
//!
//! // 3. Fire an event
//! manager.send(handle.machine_id(), StateEvent::new(EventId(1001))).await;
//!
//! // 4. Observe the transition
//! let snapshot = manager.current_state(handle.machine_id()).unwrap();
//! assert_eq!(snapshot.state_id, MyState::Active.id());
//! ```

// Crate-level re-exports
pub mod action;
pub mod config;
pub mod context;
pub mod diagnostics;
pub mod error;
pub mod event;
pub mod guard;
pub mod hierarchy;
pub mod lifecycle;
pub mod machine;
pub mod manager;
pub mod metrics;
pub mod observer;
pub mod scheduler;
pub mod state;
pub mod transition;

#[cfg(feature = "typestate")]
pub mod typestate;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// Prelude — the 10 most commonly needed types.
pub mod prelude {
    pub use crate::action::Action;
    pub use crate::config::*;
    pub use crate::context::StateContext;
    pub use crate::error::*;
    pub use crate::event::*;
    pub use crate::guard::Guard;
    pub use crate::machine::*;
    pub use crate::manager::*;
    pub use crate::observer::ObserverRegistry;
    pub use crate::scheduler::Scheduler;
    pub use crate::state::*;
    pub use crate::transition::*;
}
