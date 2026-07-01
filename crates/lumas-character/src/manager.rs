//! # Character Engine — Top-Level Manager
//!
//! `CharacterEngine` is the top-level orchestrator that manages all subsystems:
//! - Identity and appearance
//! - Behavior selection and execution
//! - Emotion computation
//! - Movement intent
//! - Expression targets
//! - Interaction handling
//! - Persistence
//! - Diagnostics and metrics
//!
//! # Authority
//! Character Engine — top-level orchestration.
//!
//! # Does NOT
//! - Own the event loop or main tick loop (delegates to the runtime)
//! - Directly manipulate render state or animation
//! - Define state machine states

use crate::accessory::AccessoryRegistry;
use crate::appearance::AppearanceProfile;
use crate::behavior::{BehaviorCandidate, BehaviorContext, BehaviorSelector, register_builtin_behaviors};
use crate::config::CharacterConfig;
use crate::diagnostics::{EngineDiagnostics, build_diagnostics};
use crate::emotion::{EmotionContext, EmotionSystem};
use crate::error::{CharacterError, CharacterResult};
use crate::event::CharacterEvent;
use crate::expression::{BlinkScheduler, ExpressionTargets, LookAtTarget, compute_expression_targets};
use crate::identity::{CharacterId, CharacterIdentity, PersonalityProfile};
use crate::interaction::{InteractionEvent, InteractionKind, InteractionSystem};
use crate::lifecycle::EngineLifecycle;
use crate::metrics::CharacterMetrics;
use crate::movement::{MovementIntent, MovementPlanner, MovementReason, MovementUrgency};
use crate::observer::CharacterObserver;
use crate::persistence::{CharacterPersistence, PersistedCharacterProfile};
use crate::scheduler::BehaviorScheduler;
use crate::navigation::Navigator;
use lumas_common::ai::AIState;
use lumas_common::desktop::{DesktopEvent, DesktopSnapshot};
use lumas_common::emotion::SentimentSignal;
use lumas_state::error::{MachineId, StateId};
use lumas_state::manager::StateMachineManager;
use lumas_state::observer::TransitionEvent;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// Top-level orchestrator of the Character Engine.
///
/// Manages lifecycle, coordinates subsystems, and provides the `tick()` entry point.
pub struct CharacterEngine {
    // --- Identity & Appearance ---
    identity: RwLock<CharacterIdentity>,
    appearance: RwLock<AppearanceProfile>,
    accessory_registry: AccessoryRegistry,

    // --- Behavior ---
    behavior_selector: RwLock<BehaviorSelector>,
    navigator: RwLock<Navigator>,

    // --- Emotion & Expression ---
    emotion_system: EmotionSystem,
    blink_scheduler: RwLock<BlinkScheduler>,

    // --- Movement ---
    movement_planner: MovementPlanner,

    // --- Interaction ---
    interaction_system: RwLock<InteractionSystem>,

    // --- Engine State ---
    lifecycle: Mutex<EngineLifecycle>,
    config: CharacterConfig,
    started_at: Instant,
    tick_count: AtomicU64,

    // --- External Dependencies ---
    state_machine: Arc<StateMachineManager>,
    observer: RwLock<CharacterObserver>,
    scheduler: BehaviorScheduler,
    persistence: Arc<dyn CharacterPersistence>,

    // --- Metrics & Diagnostics ---
    metrics: Arc<CharacterMetrics>,

    // --- Event Callbacks ---
    event_callback: RwLock<Option<Box<dyn Fn(CharacterEvent) + Send + Sync>>>,
}

impl CharacterEngine {
    /// Create and initialize the character engine. Loads the profile, registers
    /// built-in behaviors and accessories, and transitions to Ready.
    pub async fn start(
        config: CharacterConfig,
        state_machine: Arc<StateMachineManager>,
        persistence: Arc<dyn CharacterPersistence>,
    ) -> CharacterResult<Arc<Self>> {
        let started_at = Instant::now();

        // Load profile (or create default)
        let profile = if persistence.profile_exists().await {
            persistence.load_profile().await?
        } else {
            let identity = CharacterIdentity::new(config.default_name.clone());
            let profile = PersistedCharacterProfile::new(identity);
            persistence.save_profile(&profile).await?;
            profile
        };

        // Create behavior selector with built-in behaviors
        let mut selector =
            BehaviorSelector::new(&config.behavior);
        register_builtin_behaviors(&mut selector);

        // Create navigator
        let navigator = Navigator::new(
            config.navigation.no_walk_zones.clone(),
            config.navigation.exploration_radius_px,
            None,
        );

        // Create emotion system
        let emotion = EmotionSystem::new(
            profile.character.personality_profile.expressiveness,
        );

        // Create blink scheduler
        let blink = BlinkScheduler::default();

        // Set up observer — subscribe to character machine transitions
        let state_receiver = state_machine.observe(MachineId::CHARACTER);
        let observer = CharacterObserver::new(state_receiver);

        // Set up scheduler — get the scheduler event sender
        let scheduler_sender = state_machine.scheduler_sender();
        let scheduler = BehaviorScheduler::new(scheduler_sender);

        // Validate personality weights
        profile.character.personality_profile.validate()?;

        // Build accessory registry
        let mut accessory_registry = AccessoryRegistry::new();
        crate::accessory::register_builtin_accessories(&mut accessory_registry);

        // Create interaction system
        let mut interaction_system = InteractionSystem::new();
        interaction_system.register(Box::new(crate::interaction::LoggingInteractionHandler));

        let engine = Arc::new(Self {
            identity: RwLock::new(profile.character.clone()),
            appearance: RwLock::new(profile.appearance),
            accessory_registry,
            behavior_selector: RwLock::new(selector),
            navigator: RwLock::new(navigator),
            emotion_system: emotion,
            blink_scheduler: RwLock::new(blink),
            movement_planner: MovementPlanner::new(),
            interaction_system: RwLock::new(interaction_system),
            lifecycle: Mutex::new(EngineLifecycle::Ready),
            config,
            started_at,
            tick_count: AtomicU64::new(0),
            state_machine,
            observer: RwLock::new(observer),
            scheduler,
            persistence,
            metrics: CharacterMetrics::new(),
            event_callback: RwLock::new(None),
        });

        // If we have a last known position, emit a movement intent to restore it
        if let Some(last_pos) = profile.last_known_position {
            // Revalidation will be done when monitor info is available
            let _ = last_pos;
        }

        // Emit profile loaded event
        engine.emit_event(CharacterEvent::ProfileLoaded {
            character_id: profile.character.id,
        });

        Ok(engine)
    }

    /// Register a behavior candidate.
    pub fn register_behavior(&self, behavior: Arc<dyn BehaviorCandidate>) {
        if let Ok(mut selector) = self.behavior_selector.write() {
            selector.register(behavior);
        }
    }

    /// Register an event callback for CharacterEngine events.
    pub fn set_event_callback(&self, callback: Box<dyn Fn(CharacterEvent) + Send + Sync>) {
        if let Ok(mut cb) = self.event_callback.write() {
            *cb = Some(callback);
        }
    }

    /// Run one tick of the character engine.
    ///
    /// This is called on a fixed schedule (default 200ms), decoupled from render FPS.
    /// Immediate re-evaluation is triggered on significant events (state transitions,
    /// user interactions, AI state changes) rather than waiting for the next tick.
    pub async fn tick(&self, ctx: &TickContext) -> CharacterResult<()> {
        let tick_start = Instant::now();

        // Check lifecycle
        match *self.lifecycle.lock().unwrap() {
            EngineLifecycle::Ready | EngineLifecycle::Degraded { .. } => {}
            _ => return Ok(()), // Not ready to tick
        }

        // 1. Process pending state machine transitions
        self.process_state_transitions().await;

        // 2. Build behavior context and select behavior
        let behavior_ctx = self.build_behavior_context(ctx);
        let mut selector = self.behavior_selector.write().map_err(|_| {
            CharacterError::Internal("behavior selector lock".into())
        })?;

        if let Some(selected_id) = selector.select(&behavior_ctx) {
            self.metrics.record_behavior_selection();
            // Fire state machine events if the behavior specifies them
            if let Some(meta) = selector.all_metadata().iter().find(|m| m.id == selected_id) {
                if let Some(event_id) = meta.fires_event_on_start {
                    let _ = self
                        .state_machine
                        .send(MachineId::CHARACTER, lumas_state::event::StateEvent::new(event_id))
                        .await;
                }
            }
        }
        drop(selector);

        // 3. Handle interaction events
        // (interactions are processed as they arrive, but we drain any queued events here)

        // 4. Compute emotion target
        let emotion_ctx = EmotionContext {
            ai_state: ctx.ai_state.clone(),
            current_state_id: Some(ctx.current_state),
            sentiment: ctx.sentiment.clone(),
            active_behavior: self.behavior_selector.read().ok().and_then(|s| s.current_behavior()),
        };
        let emotion_target = self.emotion_system.compute_target(&emotion_ctx);

        // 5. Compute expression targets
        let look_at = ctx
            .desktop
            .as_ref()
            .and_then(|d| d.active_window.bounds)
            .map(|b| LookAtTarget {
                x: (b.x + b.width * 0.5) as f32,
                y: (b.y + b.height * 0.5) as f32,
                weight: 0.3,
            });

        let mut blink = self.blink_scheduler.write().map_err(|_| {
            CharacterError::Internal("blink scheduler lock".into())
        })?;
        let expression_targets = compute_expression_targets(
            look_at,
            &mut *blink,
            &emotion_target.primary,
            0.0,
        );
        drop(blink);

        // 6. Process state changes from observer (check for transitions that affect behavior)
        let _ = (emotion_target, expression_targets);

        // 7. Auto-save if needed
        self.auto_save_if_needed(tick_start).await;

        // Record metrics
        self.metrics.record_tick(tick_start);
        self.tick_count.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Process any pending state machine transitions.
    async fn process_state_transitions(&self) {
        if let Ok(mut observer) = self.observer.write() {
            loop {
                match observer.try_recv() {
                    Ok(event) => {
                        // React to transitions
                        match event.to_state {
                            StateId(1400) => { // Sleeping
                                // Reduce behavior selection frequency
                            }
                            _ => {}
                        }
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                        break;
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                        // Skip lagged events
                        continue;
                    }
                }
            }
        }
    }

    /// Build a BehaviorContext from the TickContext and engine state.
    fn build_behavior_context(&self, ctx: &TickContext) -> BehaviorContext {
        BehaviorContext {
            current_state: ctx.current_state,
            desktop: ctx.desktop.clone(),
            ai_state: ctx.ai_state.clone(),
            session_elapsed: self.started_at.elapsed(),
            current_emotion: Some(self.emotion_system.current().primary),
            playfulness: self
                .identity
                .read()
                .map(|id| id.personality_profile.playfulness)
                .unwrap_or(0.5),
            patience: self
                .identity
                .read()
                .map(|id| id.personality_profile.patience)
                .unwrap_or(0.5),
            time_since_last_selection: None,
            selection_count: 0,
        }
    }

    /// Called when an AI state event is received from the AI Core.
    pub fn on_ai_state_event(&self, event: lumas_common::ai::AIStateEvent) {
        // The AI state change is picked up on the next tick
        self.metrics.record_emotion_change();
    }

    /// Called when a desktop event is received from the Desktop Awareness system.
    pub fn on_desktop_event(&self, _event: &DesktopEvent) {
        // Desktop context is included in TickContext
    }

    /// Called when a sentiment signal is received.
    pub fn apply_sentiment_signal(&self, signal: SentimentSignal) {
        self.emotion_system.apply_sentiment_signal(signal);
    }

    /// Handle an interaction event.
    pub async fn handle_interaction(&self, event: &InteractionEvent) -> CharacterResult<()> {
        let system = self.interaction_system.read().map_err(|_| {
            CharacterError::Internal("interaction system lock".into())
        })?;
        system.handle_event(event).await?;
        Ok(())
    }

    /// Queue a movement intent.
    pub fn set_movement_intent(&self, intent: MovementIntent) {
        self.movement_planner.set_intent(intent);
    }

    /// Get the current movement intent (consumes it).
    pub fn take_movement_intent(&self) -> Option<MovementIntent> {
        self.movement_planner.take_intent()
    }

    /// Update the navigator with no-walk zones.
    pub fn set_no_walk_zones(&self, zones: Vec<crate::config::ScreenRect>) {
        if let Ok(mut navigator) = self.navigator.write() {
            *navigator = Navigator::new(
                zones,
                self.config.navigation.exploration_radius_px,
                None,
            );
        }
    }

    /// Get the current character identity.
    pub fn identity(&self) -> CharacterIdentity {
        self.identity.read().map(|id| id.clone()).unwrap_or_else(|_| {
            CharacterIdentity::new(self.config.default_name.clone())
        })
    }

    /// Get the current appearance profile.
    pub fn appearance(&self) -> AppearanceProfile {
        self.appearance.read().map(|a| a.clone()).unwrap_or_default()
    }

    /// Get the current emotion state.
    pub fn current_emotion_state(&self) -> lumas_common::emotion::EmotionState {
        self.emotion_system.current()
    }

    /// Get the accessory registry.
    pub fn accessory_registry(&self) -> &AccessoryRegistry {
        &self.accessory_registry
    }

    /// Get metrics collector.
    pub fn metrics(&self) -> &Arc<CharacterMetrics> {
        &self.metrics
    }

    /// Build a diagnostic snapshot of the engine state.
    pub fn diagnostics(&self) -> EngineDiagnostics {
        // Use a default empty selector if the lock is poisoned
        let selector_guard = self.behavior_selector.read();
        let default_selector = BehaviorSelector::new(&self.config.behavior);
        
        build_diagnostics(
            &*self.lifecycle.lock().unwrap(),
            self.identity.read().as_deref().ok(),
            selector_guard.as_deref().unwrap_or(&default_selector),
            &self.emotion_system,
            &self.movement_planner,
            self.tick_count.load(Ordering::Relaxed),
            self.started_at.elapsed(),
        )
    }

    /// Save the character profile to persistence.
    pub async fn save_profile(&self) -> CharacterResult<()> {
        let profile = PersistedCharacterProfile {
            schema_version: PersistedCharacterProfile::CURRENT_SCHEMA_VERSION,
            character: self.identity(),
            appearance: self.appearance(),
            last_known_position: None, // Position is owned by Desktop Engine
            behavior_preferences: crate::persistence::BehaviorPreferences::default(),
            equipped_accessories: self
                .appearance()
                .equipped_accessories
                .iter()
                .map(|a| a.accessory_id.clone())
                .collect(),
        };
        self.persistence.save_profile(&profile).await
    }

    /// Shut down the character engine gracefully.
    pub async fn shutdown(&self) -> CharacterResult<()> {
        *self.lifecycle.lock().unwrap() = EngineLifecycle::ShuttingDown;
        // Save profile one last time
        self.save_profile().await?;
        *self.lifecycle.lock().unwrap() = EngineLifecycle::Stopped;
        Ok(())
    }

    /// Auto-save if the save interval has elapsed.
    async fn auto_save_if_needed(&self, _last_save: Instant) {
        // In production, this would check a timer
        // For now, we rely on explicit save calls
    }

    /// Emit a CharacterEvent through the registered callback.
    fn emit_event(&self, event: CharacterEvent) {
        if let Ok(cb) = self.event_callback.read() {
            if let Some(ref callback) = *cb {
                callback(event);
            }
        }
    }
}

/// Context provided to each tick of the character engine.
#[derive(Debug, Clone)]
pub struct TickContext {
    /// Current state machine state.
    pub current_state: StateId,
    /// Current desktop snapshot (if available).
    pub desktop: Option<DesktopSnapshot>,
    /// Current AI state (if AI Core is active).
    pub ai_state: Option<AIState>,
    /// Sentiment signal processed this tick.
    pub sentiment: Option<SentimentSignal>,
}

impl TickContext {
    /// Create a new tick context.
    pub fn new(
        current_state: StateId,
        desktop: Option<DesktopSnapshot>,
        ai_state: Option<AIState>,
    ) -> Self {
        Self {
            current_state,
            desktop,
            ai_state,
            sentiment: None,
        }
    }

    /// Add a sentiment signal to this tick context.
    pub fn with_sentiment(mut self, signal: SentimentSignal) -> Self {
        self.sentiment = Some(signal);
        self
    }
}
