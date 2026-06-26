//! # Lumi Common — Shared Types and Structures
//!
//! This crate contains all shared type definitions used across the Lumi platform.
//! Every process crate depends on this crate for its type definitions.
//!
//! ## Module Organization
//!
//! Each module corresponds to a chapter from the SRS:
//!
//! - `ipc` — Chapter 5: IPC message types and channel definitions
//! - `position` — Chapter 6: Desktop Engine positioning types
//! - `character` — Chapter 7: Character Engine, crystal state, materials
//! - `ai` — Chapter 8: AI Core inference types and provider abstraction
//! - `conversation` — Chapter 9: Conversation message types and intent detection
//! - `plan` — Chapter 10: Planning Engine types and execution graph
//! - `tool` — Chapter 11: Tool Framework definitions and capabilities
//! - `workspace` — Chapter 12: Workspace panel types
//! - `voice` — Chapter 13: Voice system types
//! - `memory` — Chapter 14: Memory system types and schemas
//! - `desktop` — Chapter 15: Desktop awareness types
//! - `animation` — Chapter 16: Animation engine types
//! - `render` — Chapter 17: Rendering pipeline types
//! - `physics` — Chapter 18: Physics and movement types
//! - `emotion` — Chapter 19: Emotion system types
//! - `state_machine` — Chapter 20: State machine types

pub mod ipc;
pub mod position;
pub mod character;
pub mod ai;
pub mod conversation;
pub mod plan;
pub mod tool;
pub mod workspace;
pub mod voice;
pub mod memory;
pub mod desktop;
pub mod animation;
pub mod render;
pub mod physics;
pub mod emotion;
pub mod state_machine;
