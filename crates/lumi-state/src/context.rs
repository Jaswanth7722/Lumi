//! # State Context
//!
//! The runtime context passed to guards and actions during a transition.
//!
//! `StateContext` is created per-transition and scoped to the transition thread.
//! It is passed immutably to guards (`&StateContext`) and mutably to actions
//! (`&mut StateContext`).

pub use crate::state::{
    AiContextMetadata, DesktopContextMetadata, StateContext, StateTimeout, TimeoutAction,
    VoiceContextMetadata, WorkspaceContextMetadata,
};
