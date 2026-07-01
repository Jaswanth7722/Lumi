//! # Lumas Character Engine
//!
//! The Character Engine answers **"who is Lumi, and what does Lumas want to do right now?"**
//! It is distinct from the State Machine (which answers "what is Lumas currently doing and
//! what is it allowed to do next") and the Rendering Engine (which answers "what does that
//! look like on screen").
//!
//! ## Three-Layer Authority Model
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  lumas-state (CharacterMachine)                                  │
//! │  Authority: WHAT state Lumas is in, and what transitions are     │
//! │  legal right now. Owns timers, guards, hierarchy, history.      │
//! └───────────────────────────┬───────────────────────────────────────┘
//!                              │ fires events into / receives transitions from
//! ┌───────────────────────────▼───────────────────────────────────────┐
//! │  lumas-character (THIS CRATE)                                     │
//! │  Authority: WHO Lumas is (identity, appearance) and WHAT Lumas     │
//! │  wants to do within the current state (behavior selection,       │
//! │  movement intent, emotion target, expression target).            │
//! └───────────────────────────┬───────────────────────────────────────┘
//!                              │ emits intent / parameters to
//!         ┌────────────────────┼────────────────────┐
//!         ▼                    ▼                    ▼
//! ┌───────────────┐  ┌──────────────────┐  ┌─────────────────────┐
//! │ Desktop Engine │  │ Animation Engine │  │ lumas-render         │
//! │ Authority:     │  │ Authority:       │  │ Authority:          │
//! │ WHERE pixels   │  │ HOW the pose     │  │ HOW it's drawn      │
//! │ actually move  │  │ blends and       │  │ to the GPU          │
//! │ on screen      │  │ interpolates     │  │                     │
//! └───────────────┘  └──────────────────┘  └──────────────────────┘
//! ```
//!
//! ## Responsibility Table
//!
//! | State | Character Engine Responsibility | NOT Character Engine's Responsibility |
//! |---|---|---|
//! | `Idle.Watching` | Select idle behavior (track cursor vs. stay still), compute look-at target | Rendering the look-at IK (Animation Engine); deciding *when* to enter this state (State Machine via guards/timers) |
//! | `Idle.Exploring` | Select exploration destination via `BehaviorSelector`, compute path via `Navigator` | Actually moving pixels (Desktop Engine); timer that triggers `Watching → Exploring` transition (`lumas-state` Scheduler) |
//! | `Idle.Resting` | Suppress behavior selection, reduce update frequency | The rest timer (`lumas-state` Scheduler fires `CHAR_RESTING_TIMER`) |
//! | `Interacting.*` | Compute engagement emotion intensity, select listening/thinking micro-behaviors | State transitions between Listening/Thinking/Speaking (State Machine guards and entry actions) |
//! | `Working.Executing` | Compute "working" emotion intensity, select idle micro-behaviors during long tool calls | Executing the tool (Tool Framework); tracking plan progress (Planning Engine) |
//! | `Working.VerifyingResult` | Compute anticipation/hope emotion, prepare celebration reaction | The plan verification logic (Planning Engine) |
//! | `Sleeping` | Reduce update frequency, suppress behavior selection | The sleep timer itself (`lumas-state` Scheduler fires `CHAR_SLEEP_TIMER_EXPIRED`) |
//! | `FocusMode` | Suppress notification reactions, reduce movement | The focus mode timer (`lumas-state` Scheduler) |
//! | `Error` | Compute apologetic/concerned emotion target | Error recovery logic (AI Core, Tool Framework) |
//!
//! ## Cross-Crate Data Flow
//!
//! | Direction | Crate | Exact Call/Type | Used For |
//! |---|---|---|---|
//! | Receives | `lumas-state` | `observer.subscribe(MachineId::CHARACTER) -> broadcast::Receiver<TransitionEvent>` | Reacting to state changes |
//! | Sends | `lumas-state` | `manager.send(MachineId::CHARACTER, StateEvent::new(event_id))` | Firing behavioral triggers |
//! | Receives | AI Core (via IPC) | `AIStateEvent` | AI-driven behavior cues |
//! | Receives | Desktop Awareness | `DesktopSnapshot`, `DesktopEvent` | Environmental context for behavior selection |
//! | Sends | Desktop Engine | `PositionTarget` (from `lumas_common::position`) | Movement intent |
//! | Sends | Animation Engine | `EmotionState` (via shared state or IPC) | Emotion target |
//! | Sends/Receives | Storage | `PersistedCharacterProfile` (serde roundtrip) | Cross-session continuity |
//!
//! ## Worked Example
//!
//! ```ignore
//! // 1. Character receives an AI state event (AI Core → Character Engine)
//! let ai_event = AIStateEvent { state: AIState::Thinking, .. };
//! character_manager.on_ai_state_event(ai_event).await;
//!
//! // 2. Next tick: BehaviorSelector evaluates candidates given new context
//! let ctx = TickContext::from_state(current_state, &desktop_snapshot, &ai_state);
//! let behavior_id = character_manager.behavior_selector.select(&ctx);
//!
//! // 3. Behavior fires a state machine event (Character Engine → lumas-state)
//! //    (e.g., greet_user fires CHAR_GREETING_STARTED)
//! state_machine.send(MachineId::CHARACTER, StateEvent::new(events::CHAR_GREETING_COMPLETE)).await;
//!
//! // 4. Behavior emits movement intent + emotion target
//! //    (Character Engine → Desktop Engine via PositionTarget)
//! //    (Character Engine → Animation Engine via EmotionState)
//! let intent = MovementIntent { target: PositionTarget::NearCursor { offset_x: 20.0, offset_y: 20.0 }, .. };
//! character_manager.movement_planner.set_intent(intent);
//! let emotion = character_manager.emotion_system.compute_target(&emotion_ctx);
//! ```
//!
//! ## Why There Is No `CharacterState` Enum Here
//!
//! Behavioral state is **not** defined in this crate. The `lumas_state::CharacterMachine`
//! hierarchy (SRS Chapter 20) is the single source of truth for what Lumas is doing.
//! This crate owns behavior selection *within* that state — it is a sophisticated client
//! of the state machine, never a duplicate of it.
//!
//! ## Behavior Selection Algorithm
//!
//! Behaviors are selected via **weighted utility scoring**:
//!
//! 1. Each tick/generation, all registered `BehaviorCandidate`s are evaluated
//! 2. Each candidate returns `Some(score)` if applicable, `None` if precondition fails
//! 3. The highest-scoring applicable candidate is selected
//! 4. **Hysteresis** prevents thrashing: a new behavior must score at least
//!    `interrupt_margin` higher than the current behavior's re-evaluated score,
//!    and the current behavior must have run for at least `min_run_time`
//! 5. The selected behavior's `start()` is called, producing a `BehaviorExecution`
//!
//! To add a new behavior:
//! 1. Implement `BehaviorCandidate` trait
//! 2. Register it with `BehaviorSelector::register()`
//! 3. Declare `BehaviorMetadata::applicable_states` to scope it
//!
//! ## Tick Rate Rationale
//!
//! The Character Engine ticks at **200ms by default** (configurable). Behavior selection
//! does not need 60Hz evaluation — behavior-relevant context changes on the order of
//! seconds, not milliseconds. Decoupling from render FPS saves CPU and avoids wasted
//! re-evaluations. Immediate re-evaluation is triggered on significant events (state
//! transitions, user interactions, AI state changes) rather than waiting for the next tick.
//!
//! ## Persistence Scope
//!
//! Only identity-durable data is persisted: character profile, appearance, and
//! behavior preferences. **Runtime behavioral state (what Lumas was doing) is never
//! persisted** — on restart, Lumas always re-initializes to `Idle.Watching` and
//! re-derives behavior from current context. This avoids bugs around resuming
//! stale, possibly-invalid in-progress behaviors after a crash or update.

pub mod accessory;
pub mod appearance;
pub mod behavior;
pub mod character;
pub mod config;
pub mod customization;
pub mod diagnostics;
pub mod emotion;
pub mod error;
pub mod event;
pub mod expression;
pub mod identity;
pub mod interaction;
pub mod lifecycle;
pub mod manager;
pub mod metrics;
pub mod movement;
pub mod navigation;
pub mod observer;
pub mod persistence;
pub mod position;
pub mod scheduler;

/// Prelude — commonly needed types from the Character Engine.
pub mod prelude {
    pub use crate::behavior::{BehaviorCandidate, BehaviorId, BehaviorSelector};
    pub use crate::character::Character;
    pub use crate::config::CharacterConfig;
    pub use crate::emotion::EmotionSystem;
    pub use crate::error::{CharacterError, CharacterResult};
    pub use crate::event::CharacterEvent;
    pub use crate::identity::{CharacterId, CharacterIdentity, PersonalityProfile};
    pub use crate::lifecycle::EngineLifecycle;
    pub use crate::manager::CharacterEngine;
    pub use crate::movement::{MovementIntent, MovementReason, MovementUrgency};
    pub use crate::persistence::CharacterPersistence;
}
