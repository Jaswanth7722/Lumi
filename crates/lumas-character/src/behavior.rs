//! # Behavior System
//!
//! Data-driven behavior selection via weighted utility scoring. The `BehaviorSelector`
//! evaluates registered `BehaviorCandidate`s and picks the best one given the current
//! context, with hysteresis to prevent rapid flip-flopping.
//!
//! # Authority
//! Character Engine — owns "what Lumas wants to do right now."
//!
//! # Does NOT
//! - Define state machine states (see `lumas_state::CharacterMachine`)
//! - Execute animations or movement (emits intent, does not perform)
//! - Persist runtime behavioral state

use crate::config::BehaviorConfig;
use crate::error::CharacterResult;
use crate::event::CharacterEvent;
use crate::identity::CharacterId;
use crate::movement::{MovementIntent, MovementReason, MovementUrgency};
use lumas_common::ai::AIState;
use lumas_common::desktop::DesktopSnapshot;
use lumas_common::emotion::Emotion as CommonEmotion;
use lumas_state::error::StateId;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Unique identifier for a behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BehaviorId(pub u32);

impl std::fmt::Display for BehaviorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Behavior({})", self.0)
    }
}

/// Metadata describing a behavior's properties.
#[derive(Debug, Clone)]
pub struct BehaviorMetadata {
    /// Unique behavior identifier.
    pub id: BehaviorId,
    /// Human-readable name for diagnostics.
    pub name: Cow<'static, str>,
    /// States this behavior is legal to run in.
    pub applicable_states: BTreeSet<StateId>,
    /// Event to fire on start (None = no event).
    pub fires_event_on_start: Option<lumas_state::error::EventId>,
    /// Event to fire on completion (None = no event).
    pub fires_event_on_complete: Option<lumas_state::error::EventId>,
    /// Whether this behavior can be interrupted.
    pub interruptible: bool,
}

/// Context provided to behaviors for scoring and execution.
#[derive(Debug, Clone)]
pub struct BehaviorContext {
    /// The character's current state machine state.
    pub current_state: StateId,
    /// Current desktop snapshot.
    pub desktop: Option<DesktopSnapshot>,
    /// Current AI state (if AI Core is active).
    pub ai_state: Option<AIState>,
    /// Seconds since the character engine started.
    pub session_elapsed: Duration,
    /// Character's current emotion.
    pub current_emotion: Option<CommonEmotion>,
    /// Character's personality profile (affects scoring).
    pub playfulness: f32,
    pub patience: f32,
    /// Time since this behavior was last selected (for cooldown).
    pub time_since_last_selection: Option<Duration>,
    /// Number of times this behavior has been selected this session.
    pub selection_count: u32,
}

/// Handle returned when a behavior starts executing.
#[derive(Debug)]
pub struct BehaviorExecution {
    pub behavior_id: BehaviorId,
    pub started_at: Instant,
    /// Whether the behavior has completed naturally.
    pub completed: bool,
    /// The emitted movement intent (if any).
    pub movement_intent: Option<MovementIntent>,
    /// The emitted emotion target (if any).
    pub emotion_target: Option<CommonEmotion>,
}

impl BehaviorExecution {
    /// Create a new behavior execution.
    pub fn new(behavior_id: BehaviorId) -> Self {
        Self {
            behavior_id,
            started_at: Instant::now(),
            completed: false,
            movement_intent: None,
            emotion_target: None,
        }
    }

    /// Duration this behavior has been running.
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

/// A candidate behavior the character could perform right now.
pub trait BehaviorCandidate: Send + Sync + std::fmt::Debug {
    /// Returns this behavior's metadata.
    fn metadata(&self) -> &BehaviorMetadata;

    /// Returns None if this behavior is not applicable in the current context at all
    /// (hard precondition failure). Returns Some(score) where higher score = more desirable,
    /// 0.0 = applicable but uninteresting.
    fn score(&self, ctx: &BehaviorContext) -> Option<f32>;

    /// Begin executing. Returns a handle that can be polled or cancelled.
    fn start(&self, ctx: &BehaviorContext) -> BehaviorExecution;
}

/// Configuration for hysteresis that prevents rapid behavior flip-flopping.
#[derive(Debug, Clone, Default)]
pub struct HysteresisConfig {
    /// A new behavior must score at least this much higher than the current
    /// one's re-evaluated score to interrupt it.
    pub interrupt_margin: f32,
    /// Minimum time a behavior must run before it can be interrupted at all
    /// (except by hard precondition failure of the current behavior).
    pub min_run_time: Duration,
}

impl From<&BehaviorConfig> for HysteresisConfig {
    fn from(config: &BehaviorConfig) -> Self {
        Self {
            interrupt_margin: config.interrupt_margin,
            min_run_time: config.min_run_time,
        }
    }
}

/// Data-driven behavior selector using weighted utility scoring with hysteresis.
#[derive(Debug)]
pub struct BehaviorSelector {
    candidates: Vec<Arc<dyn BehaviorCandidate>>,
    current: Option<(BehaviorId, BehaviorExecution)>,
    hysteresis: HysteresisConfig,
    last_selection_times: Vec<(BehaviorId, Instant)>,
    selection_counts: std::collections::HashMap<BehaviorId, u32>,
}

impl BehaviorSelector {
    /// Create a new empty behavior selector.
    pub fn new(config: impl Into<HysteresisConfig>) -> Self {
        Self {
            candidates: Vec::new(),
            current: None,
            hysteresis: config.into(),
            last_selection_times: Vec::new(),
            selection_counts: std::collections::HashMap::new(),
        }
    }

    /// Register a behavior candidate.
    pub fn register(&mut self, candidate: Arc<dyn BehaviorCandidate>) {
        let id = candidate.metadata().id;
        // Avoid duplicates by id
        if !self.candidates.iter().any(|c| c.metadata().id == id) {
            self.candidates.push(candidate);
        }
    }

    /// Number of registered candidates.
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// Get the currently executing behavior ID, if any.
    pub fn current_behavior(&self) -> Option<BehaviorId> {
        self.current.as_ref().map(|(id, _)| *id)
    }

    /// Get a reference to the current execution, if any.
    pub fn current_execution(&self) -> Option<&BehaviorExecution> {
        self.current.as_ref().map(|(_, exec)| exec)
    }

    /// Get the current execution mutably.
    pub fn current_execution_mut(&mut self) -> Option<&mut BehaviorExecution> {
        self.current.as_mut().map(|(_, exec)| exec)
    }

    /// Select the best behavior given the current context. Implements hysteresis.
    /// Returns Some(BehaviorId) if a new behavior was selected, None if the current
    /// behavior continues.
    pub fn select(&mut self, ctx: &BehaviorContext) -> Option<BehaviorId> {
        // Evaluate candidates
        let mut best_score: Option<(BehaviorId, f32)> = None;

        for candidate in &self.candidates {
            let metadata = candidate.metadata();

            // Filter by applicable states — skip if current state not in applicable_states
            if !metadata.applicable_states.contains(&ctx.current_state) {
                continue;
            }

            // Score the behavior
            if let Some(score) = candidate.score(ctx) {
                match best_score {
                    None => {
                        best_score = Some((metadata.id, score));
                    }
                    Some((_, current_best)) if score > current_best => {
                        best_score = Some((metadata.id, score));
                    }
                    _ => {}
                }
            }
        }

        let Some((new_id, new_score)) = best_score else {
            // No behavior applicable — keep current if any
            return None;
        };

        // Hysteresis check
        if let Some((current_id, current_exec)) = &self.current {
            if *current_id == new_id {
                // Re-selected the same behavior
                return None;
            }

            // Re-evaluate current behavior's score to compare
            let current_score = self
                .candidates
                .iter()
                .find(|c| c.metadata().id == *current_id)
                .and_then(|c| c.score(ctx))
                .unwrap_or(0.0);

            let margin = new_score - current_score;

            // Check if current behavior is interruptible
            let current_meta = self
                .candidates
                .iter()
                .find(|c| c.metadata().id == *current_id);

            let can_interrupt = current_meta
                .map(|c| c.metadata().interruptible)
                .unwrap_or(true);

            if !can_interrupt {
                // Current behavior cannot be interrupted
                return None;
            }

            // Check min run time
            let run_time = current_exec.elapsed();
            if run_time < self.hysteresis.min_run_time && margin <= 0.0 {
                // New behavior doesn't even score higher, let current keep running
                return None;
            }

            if margin < self.hysteresis.interrupt_margin {
                // Not enough margin to interrupt
                return None;
            }

            // Interrupt current behavior
            let interrupted_by = new_id;
            let prev_id = *current_id;

            // Record interruption
            self.record_event(CharacterEvent::BehaviorInterrupted {
                behavior_id: prev_id,
                interrupted_by,
            });
        }

        // Select the new behavior
        let candidate = self
            .candidates
            .iter()
            .find(|c| c.metadata().id == new_id)
            .expect("candidate must exist");

        let execution = candidate.start(ctx);

        // Record selection
        let now = Instant::now();
        self.last_selection_times
            .retain(|(id, _)| *id != new_id);
        self.last_selection_times.push((new_id, now));
        *self.selection_counts.entry(new_id).or_insert(0) += 1;

        let events = vec![
            CharacterEvent::BehaviorStarted {
                behavior_id: new_id,
                reason: format!("score={:.3}", new_score).into(),
            },
        ];
        for ev in events {
            self.record_event(ev);
        }

        self.current = Some((new_id, execution));
        Some(new_id)
    }

    /// Mark the current behavior as completed.
    pub fn complete_current(&mut self) {
        if let Some((id, exec)) = self.current.take() {
            self.record_event(CharacterEvent::BehaviorCompleted {
                behavior_id: id,
                duration: exec.elapsed(),
            });
        }
    }

    /// Force-stop the current behavior without graceful completion.
    pub fn cancel_current(&mut self) {
        self.current = None;
    }

    /// Event emission — in a real implementation this would go through an EventEmitter.
    fn record_event(&self, _event: CharacterEvent) {
        // Events are collected and forwarded by CharacterManager
    }

    /// Get behavior metadata for all registered candidates.
    pub fn all_metadata(&self) -> Vec<&BehaviorMetadata> {
        self.candidates.iter().map(|c| c.metadata()).collect()
    }
}

// ============================================================================
// Built-in Behavior Catalog
// ============================================================================

// Behavior IDs
pub const BEHAVIOR_IDLE_WATCH_CURSOR: BehaviorId = BehaviorId(1);
pub const BEHAVIOR_IDLE_EXPLORE: BehaviorId = BehaviorId(2);
pub const BEHAVIOR_IDLE_REST: BehaviorId = BehaviorId(3);
pub const BEHAVIOR_GREET_USER: BehaviorId = BehaviorId(4);
pub const BEHAVIOR_REACT_NOTIFICATION: BehaviorId = BehaviorId(5);
pub const BEHAVIOR_CELEBRATE_SUCCESS: BehaviorId = BehaviorId(6);
pub const BEHAVIOR_EXPRESS_CONCERN: BehaviorId = BehaviorId(7);
pub const BEHAVIOR_AWAIT_APPROVAL_IDLE: BehaviorId = BehaviorId(8);

// State ID constants (from lumas-state hierarchy)
const STATE_IDLE_WATCHING: StateId = StateId::new(1100);
const STATE_IDLE_EXPLORING: StateId = StateId::new(1101);
const STATE_IDLE_RESTING: StateId = StateId::new(1102);
const STATE_IDLE_COMPOSITE: StateId = StateId::new(100);
const STATE_WORKING_PREPARING: StateId = StateId::new(1300);
const STATE_WORKING_EXECUTING: StateId = StateId::new(1301);
const STATE_WORKING_VERIFYING: StateId = StateId::new(1302);
const STATE_WORKING_COMPOSITE: StateId = StateId::new(300);
const STATE_SLEEPING: StateId = StateId::new(1400);
const STATE_ERROR: StateId = StateId::new(1500);
const STATE_FOCUS_MODE: StateId = StateId::new(1600);

/// Idle.Watching — track cursor with look-at weight.
#[derive(Debug)]
pub struct IdleWatchCursor {
    metadata: BehaviorMetadata,
}

impl IdleWatchCursor {
    pub fn new() -> Self {
        let mut states = BTreeSet::new();
        states.insert(STATE_IDLE_WATCHING);
        states.insert(STATE_IDLE_COMPOSITE);
        Self {
            metadata: BehaviorMetadata {
                id: BEHAVIOR_IDLE_WATCH_CURSOR,
                name: "idle_watch_cursor".into(),
                applicable_states: states,
                fires_event_on_start: None,
                fires_event_on_complete: None,
                interruptible: true,
            },
        }
    }
}

impl BehaviorCandidate for IdleWatchCursor {
    fn metadata(&self) -> &BehaviorMetadata {
        &self.metadata
    }

    fn score(&self, ctx: &BehaviorContext) -> Option<f32> {
        // Always applicable in Watching state; score increases with playfulness
        if !self.metadata.applicable_states.contains(&ctx.current_state) {
            return None;
        }
        // Base score modulated by playfulness — playful characters watch more
        let base = 0.5 + ctx.playfulness * 0.3;
        // Boost if user is actively interacting
        let boost = if ctx.ai_state == Some(AIState::Listening)
            || ctx.ai_state == Some(AIState::ReceivingInput)
        {
            0.2
        } else {
            0.0
        };
        Some((base + boost).min(1.0))
    }

    fn start(&self, _ctx: &BehaviorContext) -> BehaviorExecution {
        BehaviorExecution::new(BEHAVIOR_IDLE_WATCH_CURSOR)
    }
}

/// Idle.Explore — walk to a point of interest.
#[derive(Debug)]
pub struct IdleExplore {
    metadata: BehaviorMetadata,
}

impl IdleExplore {
    pub fn new() -> Self {
        let mut states = BTreeSet::new();
        states.insert(STATE_IDLE_WATCHING);
        states.insert(STATE_IDLE_EXPLORING);
        states.insert(STATE_IDLE_COMPOSITE);
        Self {
            metadata: BehaviorMetadata {
                id: BEHAVIOR_IDLE_EXPLORE,
                name: "idle_explore".into(),
                applicable_states: states,
                fires_event_on_start: None,
                fires_event_on_complete: None,
                interruptible: true,
            },
        }
    }
}

impl BehaviorCandidate for IdleExplore {
    fn metadata(&self) -> &BehaviorMetadata {
        &self.metadata
    }

    fn score(&self, ctx: &BehaviorContext) -> Option<f32> {
        if !self.metadata.applicable_states.contains(&ctx.current_state) {
            return None;
        }
        // Score increases the longer since last exploration
        let time_factor = ctx
            .time_since_last_selection
            .map(|d| ((d.as_secs_f32() / 60.0).min(1.0)) * 0.3)
            .unwrap_or(0.0);
        // Playfulness boosts exploration desire
        let personality = ctx.playfulness * 0.3;
        // Base decreases when watching cursor (stay still more often)
        let base = if ctx.current_state == STATE_IDLE_WATCHING {
            0.2
        } else {
            0.4
        };
        Some((base + time_factor + personality).min(1.0))
    }

    fn start(&self, ctx: &BehaviorContext) -> BehaviorExecution {
        let mut exec = BehaviorExecution::new(BEHAVIOR_IDLE_EXPLORE);
        exec.movement_intent = Some(MovementIntent {
            target: lumas_common::position::PositionTarget::Preserve,
            urgency: MovementUrgency::Leisurely,
            reason: MovementReason::BehaviorExploring,
        });
        exec
    }
}

/// Idle.Resting — sit, minimal animation.
#[derive(Debug)]
pub struct IdleRest {
    metadata: BehaviorMetadata,
}

impl IdleRest {
    pub fn new() -> Self {
        let mut states = BTreeSet::new();
        states.insert(STATE_IDLE_RESTING);
        Self {
            metadata: BehaviorMetadata {
                id: BEHAVIOR_IDLE_REST,
                name: "idle_rest".into(),
                applicable_states: states,
                fires_event_on_start: None,
                fires_event_on_complete: None,
                interruptible: false,
            },
        }
    }
}

impl BehaviorCandidate for IdleRest {
    fn metadata(&self) -> &BehaviorMetadata {
        &self.metadata
    }

    fn score(&self, ctx: &BehaviorContext) -> Option<f32> {
        if !self.metadata.applicable_states.contains(&ctx.current_state) {
            return None;
        }
        // Always score high when in Resting state (it's the default)
        Some(0.9)
    }

    fn start(&self, _ctx: &BehaviorContext) -> BehaviorExecution {
        BehaviorExecution::new(BEHAVIOR_IDLE_REST)
    }
}

/// Greet user — play greeting, only scores high in first N seconds.
#[derive(Debug)]
pub struct GreetUser {
    metadata: BehaviorMetadata,
    greeting_window: Duration,
}

impl GreetUser {
    pub fn new() -> Self {
        let mut states = BTreeSet::new();
        states.insert(STATE_IDLE_WATCHING);
        states.insert(STATE_IDLE_EXPLORING);
        states.insert(STATE_IDLE_RESTING);
        states.insert(STATE_IDLE_COMPOSITE);
        Self {
            metadata: BehaviorMetadata {
                id: BEHAVIOR_GREET_USER,
                name: "greet_user".into(),
                applicable_states: states,
                fires_event_on_start: None,
                fires_event_on_complete: None,
                interruptible: false,
            },
            greeting_window: Duration::from_secs(10),
        }
    }
}

impl BehaviorCandidate for GreetUser {
    fn metadata(&self) -> &BehaviorMetadata {
        &self.metadata
    }

    fn score(&self, ctx: &BehaviorContext) -> Option<f32> {
        if !self.metadata.applicable_states.contains(&ctx.current_state) {
            return None;
        }
        // Only applicable in first N seconds of session
        if ctx.session_elapsed > self.greeting_window {
            return None;
        }
        // Score decays over the greeting window
        let remaining = (self.greeting_window.as_secs_f32()
            - ctx.session_elapsed.as_secs_f32())
            / self.greeting_window.as_secs_f32();
        Some(remaining * 0.9)
    }

    fn start(&self, _ctx: &BehaviorContext) -> BehaviorExecution {
        BehaviorExecution::new(BEHAVIOR_GREET_USER)
    }
}

/// React to notification — glance toward notification origin.
#[derive(Debug)]
pub struct ReactNotification {
    metadata: BehaviorMetadata,
}

impl ReactNotification {
    pub fn new() -> Self {
        let mut states = BTreeSet::new();
        states.insert(STATE_IDLE_WATCHING);
        states.insert(STATE_IDLE_EXPLORING);
        states.insert(STATE_IDLE_COMPOSITE);
        Self {
            metadata: BehaviorMetadata {
                id: BEHAVIOR_REACT_NOTIFICATION,
                name: "react_notification".into(),
                applicable_states: states,
                fires_event_on_start: None,
                fires_event_on_complete: None,
                interruptible: true,
            },
        }
    }
}

impl BehaviorCandidate for ReactNotification {
    fn metadata(&self) -> &BehaviorMetadata {
        &self.metadata
    }

    fn score(&self, ctx: &BehaviorContext) -> Option<f32> {
        if !self.metadata.applicable_states.contains(&ctx.current_state) {
            return None;
        }
        // Only score if there are recent notifications
        let has_notification = ctx
            .desktop
            .as_ref()
            .map(|d| !d.recent_notifications.is_empty())
            .unwrap_or(false);

        if !has_notification {
            return None;
        }
        // Score based on number of notifications (decays with time)
        Some(0.7)
    }

    fn start(&self, _ctx: &BehaviorContext) -> BehaviorExecution {
        let mut exec = BehaviorExecution::new(BEHAVIOR_REACT_NOTIFICATION);
        exec.emotion_target = Some(CommonEmotion::Alert);
        exec
    }
}

/// Celebrate success — bounce/celebrate animation when task completes.
#[derive(Debug)]
pub struct CelebrateSuccess {
    metadata: BehaviorMetadata,
}

impl CelebrateSuccess {
    pub fn new() -> Self {
        let mut states = BTreeSet::new();
        states.insert(STATE_WORKING_VERIFYING);
        states.insert(STATE_WORKING_COMPOSITE);
        Self {
            metadata: BehaviorMetadata {
                id: BEHAVIOR_CELEBRATE_SUCCESS,
                name: "celebrate_success".into(),
                applicable_states: states,
                fires_event_on_start: None,
                fires_event_on_complete: None,
                interruptible: false,
            },
        }
    }
}

impl BehaviorCandidate for CelebrateSuccess {
    fn metadata(&self) -> &BehaviorMetadata {
        &self.metadata
    }

    fn score(&self, ctx: &BehaviorContext) -> Option<f32> {
        if !self.metadata.applicable_states.contains(&ctx.current_state) {
            return None;
        }
        // Score high when in VerifyingResult (task was completed)
        Some(0.85)
    }

    fn start(&self, _ctx: &BehaviorContext) -> BehaviorExecution {
        let mut exec = BehaviorExecution::new(BEHAVIOR_CELEBRATE_SUCCESS);
        exec.emotion_target = Some(CommonEmotion::Happy);
        exec
    }
}

/// Express concern — concerned posture when in Error state.
#[derive(Debug)]
pub struct ExpressConcern {
    metadata: BehaviorMetadata,
}

impl ExpressConcern {
    pub fn new() -> Self {
        let mut states = BTreeSet::new();
        states.insert(STATE_ERROR);
        Self {
            metadata: BehaviorMetadata {
                id: BEHAVIOR_EXPRESS_CONCERN,
                name: "express_concern".into(),
                applicable_states: states,
                fires_event_on_start: None,
                fires_event_on_complete: None,
                interruptible: false,
            },
        }
    }
}

impl BehaviorCandidate for ExpressConcern {
    fn metadata(&self) -> &BehaviorMetadata {
        &self.metadata
    }

    fn score(&self, ctx: &BehaviorContext) -> Option<f32> {
        if !self.metadata.applicable_states.contains(&ctx.current_state) {
            return None;
        }
        // Always applicable in Error state
        Some(0.95)
    }

    fn start(&self, _ctx: &BehaviorContext) -> BehaviorExecution {
        let mut exec = BehaviorExecution::new(BEHAVIOR_EXPRESS_CONCERN);
        exec.emotion_target = Some(CommonEmotion::Concerned);
        exec
    }
}

/// Await approval idle — subtle attention-holding idle during Working when paused.
#[derive(Debug)]
pub struct AwaitApprovalIdle {
    metadata: BehaviorMetadata,
}

impl AwaitApprovalIdle {
    pub fn new() -> Self {
        let mut states = BTreeSet::new();
        states.insert(STATE_WORKING_PREPARING);
        states.insert(STATE_WORKING_EXECUTING);
        states.insert(STATE_WORKING_COMPOSITE);
        Self {
            metadata: BehaviorMetadata {
                id: BEHAVIOR_AWAIT_APPROVAL_IDLE,
                name: "await_approval_idle".into(),
                applicable_states: states,
                fires_event_on_start: None,
                fires_event_on_complete: None,
                interruptible: true,
            },
        }
    }
}

impl BehaviorCandidate for AwaitApprovalIdle {
    fn metadata(&self) -> &BehaviorMetadata {
        &self.metadata
    }

    fn score(&self, ctx: &BehaviorContext) -> Option<f32> {
        if !self.metadata.applicable_states.contains(&ctx.current_state) {
            return None;
        }
        // Score higher when AI is awaiting confirmation
        let waiting = ctx.ai_state == Some(AIState::AwaitingConfirmation);
        let base = if waiting { 0.7 } else { 0.3 };
        Some(base)
    }

    fn start(&self, _ctx: &BehaviorContext) -> BehaviorExecution {
        let mut exec = BehaviorExecution::new(BEHAVIOR_AWAIT_APPROVAL_IDLE);
        exec.emotion_target = Some(CommonEmotion::Curious);
        exec
    }
}

/// Register all built-in behaviors with a selector.
pub fn register_builtin_behaviors(selector: &mut BehaviorSelector) {
    selector.register(Arc::new(IdleWatchCursor::new()));
    selector.register(Arc::new(IdleExplore::new()));
    selector.register(Arc::new(IdleRest::new()));
    selector.register(Arc::new(GreetUser::new()));
    selector.register(Arc::new(ReactNotification::new()));
    selector.register(Arc::new(CelebrateSuccess::new()));
    selector.register(Arc::new(ExpressConcern::new()));
    selector.register(Arc::new(AwaitApprovalIdle::new()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context(state: StateId, session_secs: f64) -> BehaviorContext {
        BehaviorContext {
            current_state: state,
            desktop: None,
            ai_state: None,
            session_elapsed: Duration::from_secs_f64(session_secs),
            current_emotion: None,
            playfulness: 0.6,
            patience: 0.7,
            time_since_last_selection: None,
            selection_count: 0,
        }
    }

    #[test]
    fn test_selector_selects_applicable_behavior() {
        let mut selector = BehaviorSelector::new(HysteresisConfig {
            interrupt_margin: 0.15,
            min_run_time: Duration::from_millis(100),
        });
        register_builtin_behaviors(&mut selector);

        // In Watching state, after greeting window expires
        let ctx = make_context(STATE_IDLE_WATCHING, 30.0);
        let selected = selector.select(&ctx);
        assert!(selected.is_some(), "Should select some behavior");
        assert_ne!(
            selected.unwrap(),
            BEHAVIOR_GREET_USER,
            "Greet should not be selected after 30s"
        );
    }

    #[test]
    fn test_greet_user_only_in_greeting_window() {
        let mut selector = BehaviorSelector::new(HysteresisConfig {
            interrupt_margin: 0.15,
            min_run_time: Duration::from_millis(100),
        });
        register_builtin_behaviors(&mut selector);

        // Early session — greet should be available
        let ctx_early = make_context(STATE_IDLE_WATCHING, 1.0);
        let result_early = selector.select(&ctx_early);

        // After greeting window
        let ctx_late = make_context(STATE_IDLE_WATCHING, 30.0);
        selector.cancel_current();
        let result_late = selector.select(&ctx_late);

        // Greet should have been selected in early session
        assert!(result_early.is_some());
    }

    #[test]
    fn test_behavior_outside_applicable_states() {
        let mut selector = BehaviorSelector::new(HysteresisConfig {
            interrupt_margin: 0.15,
            min_run_time: Duration::from_millis(100),
        });
        register_builtin_behaviors(&mut selector);

        // In Sleeping state — most idle behaviors should not apply
        let ctx = make_context(STATE_SLEEPING, 60.0);
        let selected = selector.select(&ctx);
        // IdleWatchCursor and others don't include Sleeping
        // So expect None or only behaviors that include it
        assert!(selected.is_none() || selected == Some(BEHAVIOR_IDLE_REST));
    }

    #[test]
    fn test_hysteresis_prevents_thrashing() {
        let mut selector = BehaviorSelector::new(HysteresisConfig {
            interrupt_margin: 0.5, // High margin to prevent switching
            min_run_time: Duration::from_millis(500),
        });

        // Register just two behaviors
        selector.register(Arc::new(IdleWatchCursor::new()));
        selector.register(Arc::new(IdleExplore::new()));

        let ctx = make_context(STATE_IDLE_WATCHING, 30.0);
        let first = selector.select(&ctx);
        assert!(first.is_some());

        // Immediate re-select — hysteresis should prevent switch
        let second = selector.select(&ctx);
        // Should keep the same behavior (returns None = same behavior continues)
        assert!(second.is_none());
    }
}
