//! # Compile-Time Typestate Encoding (Feature Gate)
//!
//! This module is only available with feature = "typestate" (default on).
//!
//! Provides compile-time typestate encoding for the highest-frequency
//! behavioral states (Character, Animation). When enabled, transition
//! functions consume `Machine<Src>` and return `Machine<Dst>`, making
//! illegal transitions a compile error.
//!
//! This module is currently a placeholder. Full typestate implementations
//! for Character and Animation states will be added in a follow-up.
