//! # State Machine Manager
//!
//! The central coordinator for all state machines. Manages registration,
//! event dispatch, transition serialization, cross-machine invariants,
//! and lifecycle coordination.

use crate::config::StateMachineConfig;
use crate::error::{EventId, MachineId, StateError, StateId, StateResult, TransitionId};
use crate::event::StateEvent;
use crate::guard::{CrossMachineGuard, Guard, StateQuery};
use crate::machine::{MachineInstance, StateMachine};
use crate::observer::{ObserverRegistry, TransitionEvent, TransitionOutcomeKind};
use crate::scheduler::ScheduledEvent;
use crate::state::StateSnapshot;
use crate::transition::{TransitionEngine, TransitionOutcome};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{broadcast, mpsc};

/// The central state machine manager.
///
/// Coordinates all registered state machines, enforces cross-machine
/// invariants, and provides observation APIs.
#[derive(Clone)]
pub struct StateMachineManager {
    inner: Arc<ManagerInner>,
}

struct ManagerInner {
    /// Registered machines (definition).
    machines: DashMap<MachineId, StateMachine>,
    /// Running machine instances.
    instances: DashMap<MachineId, Arc<RwLock<MachineInstance>>>,
    /// Per-machine event queues.
    event_queues: DashMap<MachineId, mpsc::UnboundedSender<StateEvent>>,
    /// Transition engine.
    transition_engine: TransitionEngine,
    /// Observer registry.
    observer: ObserverRegistry,
    /// Cross-machine coordinator.
    cross_machine: CrossMachineCoordinator,
    /// Configuration.
    config: StateMachineConfig,
    /// Whether the manager is shutting down.
    shutting_down: AtomicBool,
    /// Scheduler event receiver.
    scheduler_rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<ScheduledEvent>>,
    scheduler_tx: mpsc::UnboundedSender<ScheduledEvent>,
    /// Registration order (for diagnostics).
    registration_order: parking_lot::RwLock<Vec<MachineId>>,
}

impl StateMachineManager {
    /// Create and start the state machine manager.
    pub async fn start(config: StateMachineConfig) -> StateResult<Arc<Self>> {
        config.validate().map_err(|e| StateError::Internal(e))?;

        let (scheduler_tx, scheduler_rx) = mpsc::unbounded_channel();

        let inner = Arc::new(ManagerInner {
            machines: DashMap::new(),
            instances: DashMap::new(),
            event_queues: DashMap::new(),
            transition_engine: TransitionEngine::new(
                config.transition_timeout,
                config.guard_timeout,
            ),
            observer: ObserverRegistry::new(config.event_queue_capacity),
            cross_machine: CrossMachineCoordinator::new(),
            config,
            shutting_down: AtomicBool::new(false),
            scheduler_rx: tokio::sync::Mutex::new(scheduler_rx),
            scheduler_tx,
            registration_order: parking_lot::RwLock::new(Vec::new()),
        });

        let manager = Arc::new(Self { inner });

        // Spawn the scheduler processing task
        let manager_clone = manager.clone();
        tokio::spawn(async move {
            manager_clone.run_scheduler_loop().await;
        });

        Ok(manager)
    }

    /// Run the scheduler event loop, processing scheduled events.
    async fn run_scheduler_loop(&self) {
        let mut rx = self.inner.scheduler_rx.lock().await;
        while let Some(scheduled) = rx.recv().await {
            // Fire the event at the target machine (best-effort)
            let _ = self
                .fire_event_inner(scheduled.machine_id, scheduled.event)
                .await;
        }
    }

    /// Register a new state machine.
    ///
    /// Must be called before any events are processed. The runtime machine
    /// should be registered first.
    pub fn register(&self, machine: StateMachine) -> StateResult<()> {
        if self.inner.shutting_down.load(Ordering::Acquire) {
            return Err(StateError::SystemShuttingDown);
        }

        let machine_id = machine.id;

        // Validate the machine
        machine.validate()?;

        // Check for duplicate
        if self.inner.machines.contains_key(&machine_id) {
            return Err(StateError::MachineAlreadyRegistered { machine_id });
        }

        // Create instance
        let instance = MachineInstance::new(machine.initial_state);

        // Create event queue
        let (tx, mut rx) = mpsc::unbounded_channel::<StateEvent>();

        // Store
        self.inner.machines.insert(machine_id, machine);
        self.inner
            .instances
            .insert(machine_id, Arc::new(RwLock::new(instance)));
        self.inner.event_queues.insert(machine_id, tx);
        self.inner.registration_order.write().push(machine_id);

        // Spawn event processing task for this machine
        let manager_weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let Some(inner) = manager_weak.upgrade() {
                    // Process event on this machine
                    let manager = StateMachineManager { inner };
                    let _ = manager.fire_event_inner(machine_id, event).await;
                } else {
                    break;
                }
            }
        });

        Ok(())
    }

    /// Fire an event at a specific machine (fire-and-forget).
    pub async fn send(&self, machine_id: MachineId, event: StateEvent) -> StateResult<()> {
        if self.inner.shutting_down.load(Ordering::Acquire) {
            return Err(StateError::SystemShuttingDown);
        }

        let queue = self
            .inner
            .event_queues
            .get(&machine_id)
            .ok_or(StateError::MachineNotFound { machine_id })?;

        queue
            .send(event)
            .map_err(|_| StateError::MachineNotFound { machine_id })?;

        Ok(())
    }

    /// Fire an event and wait for the transition to complete.
    pub async fn send_and_wait(
        &self,
        machine_id: MachineId,
        event: StateEvent,
        timeout: Duration,
    ) -> StateResult<TransitionOutcome> {
        // For send-and-wait, we process synchronously
        let outcome = self.fire_event_inner(machine_id, event).await?;
        Ok(outcome)
    }

    /// Internal event processing — actually executes the transition.
    async fn fire_event_inner(
        &self,
        machine_id: MachineId,
        event: StateEvent,
    ) -> StateResult<TransitionOutcome> {
        let (machine_guard, instance_guard) = {
            let machine = self
                .inner
                .machines
                .get(&machine_id)
                .ok_or(StateError::MachineNotFound { machine_id })?;
            let instance = self
                .inner
                .instances
                .get(&machine_id)
                .ok_or(StateError::MachineNotFound { machine_id })?;

            (machine.value().clone(), instance.value().clone())
        };

        let machine = &machine_guard;

        // Phase 1: Read current state (brief write lock, released before any .await)
        let current_state = {
            let instance_read = instance_guard.write();
            instance_read.current_state
        };

        // Resolve transition
        let transition = match machine.transitions.resolve(current_state, event.id)? {
            Some(t) => t.clone(),
            None => {
                return Ok(TransitionOutcome::Rejected {
                    reason: crate::transition::GuardRejection::NoMatchingTransition {
                        source: current_state,
                        event: event.id,
                    },
                    evaluated_guards: Vec::new(),
                });
            }
        };

        // Check cross-machine invariants (no write lock held)
        if machine.supports_cross_machine {
            self.inner
                .cross_machine
                .check(&machine_id, &transition, &self.inner)
                .await?;
        }

        // Get source and target state objects
        let source_state = machine
            .get_state(current_state)
            .ok_or(StateError::StateNotFound {
                state_id: current_state,
            })?;
        let target_state =
            machine
                .get_state(transition.target)
                .ok_or(StateError::StateNotFound {
                    state_id: transition.target,
                })?;

        // Create context
        let mut ctx = crate::context::StateContext::new(machine_id);
        ctx.current_state = current_state;
        ctx.correlation_id = event.correlation_id;

        // Execute transition (no write lock held)
        let outcome = self
            .inner
            .transition_engine
            .execute(source_state, target_state, &transition, &event, &mut ctx)
            .await;

        // Phase 2: Write back results (brief write lock)
        {
            let mut instance_write = instance_guard.write();
            match &outcome {
                TransitionOutcome::Completed { to, .. } => {
                    instance_write.previous_state = Some(instance_write.current_state);
                    instance_write.current_state = *to;
                    instance_write.state_entered_at = Instant::now();
                    instance_write.transition_count += 1;

                    // Record history for composite states
                    instance_write.history.record_shallow(current_state, *to);
                }
                _ => {}
            }
        }

        // Publish observer event
        let observer_event = TransitionEvent {
            machine_id,
            from_state: current_state,
            to_state: transition.target,
            trigger: event.id,
            outcome: match &outcome {
                TransitionOutcome::Completed { .. } => TransitionOutcomeKind::Completed,
                TransitionOutcome::Rejected { .. } => {
                    TransitionOutcomeKind::Rejected { guard: "guard" }
                }
                TransitionOutcome::RolledBack { at_step, .. } => {
                    TransitionOutcomeKind::RolledBack {
                        at_step: match at_step {
                            crate::transition::TransitionStep::GuardEvaluation => "guard",
                            crate::transition::TransitionStep::ExitAction => "exit",
                            crate::transition::TransitionStep::TransitionAction => "transition",
                            crate::transition::TransitionStep::StateCommit => "commit",
                            crate::transition::TransitionStep::EntryAction => "entry",
                            crate::transition::TransitionStep::EventPublication => "event",
                        },
                    }
                }
            },
            duration_us: match &outcome {
                TransitionOutcome::Completed { duration, .. } => duration.as_micros() as u64,
                _ => 0,
            },
            timestamp: SystemTime::now(),
            correlation_id: event.correlation_id,
        };
        self.inner.observer.publish(observer_event);

        Ok(outcome)
    }

    /// Get the current state of a machine.
    pub fn current_state(&self, machine_id: MachineId) -> StateResult<StateSnapshot> {
        let machine = self
            .inner
            .machines
            .get(&machine_id)
            .ok_or(StateError::MachineNotFound { machine_id })?;
        let instance = self
            .inner
            .instances
            .get(&machine_id)
            .ok_or(StateError::MachineNotFound { machine_id })?;

        // We can't access async data synchronously, so this is best-effort
        // For true sync access, a different approach would be needed
        let snapshot = machine.build_snapshot(&instance.value().read());
        Ok(snapshot)
    }

    /// Subscribe to all transitions on a specific machine.
    pub fn observe(&self, machine_id: MachineId) -> broadcast::Receiver<TransitionEvent> {
        self.inner.observer.subscribe(machine_id)
    }

    /// Subscribe to all transitions across all machines.
    pub fn observe_all(&self) -> broadcast::Receiver<TransitionEvent> {
        self.inner.observer.subscribe_all()
    }

    /// Full platform behavioral snapshot.
    pub fn platform_snapshot(&self) -> PlatformStateSnapshot {
        let mut machine_states = Vec::new();
        for entry in self.inner.machines.iter() {
            let machine = entry.value();
            if let Some(instance) = self.inner.instances.get(&machine.id) {
                let snap = machine.build_snapshot(&instance.value().read());
                machine_states.push(snap);
            }
        }

        PlatformStateSnapshot {
            timestamp: SystemTime::now(),
            machine_states,
            total_registered: self.inner.machines.len(),
        }
    }

    /// Get the scheduler's event sender (for registering timers).
    pub fn scheduler_sender(&self) -> mpsc::UnboundedSender<ScheduledEvent> {
        self.inner.scheduler_tx.clone()
    }

    /// Shutdown the manager gracefully.
    pub async fn shutdown(&self) -> StateResult<()> {
        self.inner.shutting_down.store(true, Ordering::Release);
        // Clear all event queues
        self.inner.event_queues.clear();
        Ok(())
    }

    /// Check if the manager is shutting down.
    pub fn is_shutting_down(&self) -> bool {
        self.inner.shutting_down.load(Ordering::Acquire)
    }

    /// Get a registered machine by ID (cloned).
    pub fn get_machine(&self, machine_id: MachineId) -> Option<StateMachine> {
        self.inner.machines.get(&machine_id).map(|e| e.value().clone())
    }

    /// List all registered machine IDs.
    pub fn registered_machines(&self) -> Vec<MachineId> {
        self.inner.registration_order.read().clone()
    }
}

impl StateQuery for ManagerInner {
    fn current_state_for(&self, machine_id: MachineId) -> Option<StateId> {
        self.instances
            .get(&machine_id)
            .map(|instance| instance.value().read().current_state)
    }
}

impl StateQuery for StateMachineManager {
    fn current_state_for(&self, machine_id: MachineId) -> Option<StateId> {
        self.inner.current_state_for(machine_id)
    }
}

// =========================================================================
// Cross-Machine Coordinator
// =========================================================================

/// Coordinates cross-machine invariant enforcement.
#[derive(Debug)]
pub struct CrossMachineCoordinator {
    /// Registered cross-machine guards.
    guards: parking_lot::RwLock<Vec<(MachineId, CrossMachineGuard)>>,
}

impl CrossMachineCoordinator {
    pub fn new() -> Self {
        Self {
            guards: parking_lot::RwLock::new(Vec::new()),
        }
    }

    /// Register a cross-machine guard.
    pub fn add_guard(&self, source_machine: MachineId, guard: CrossMachineGuard) {
        self.guards.write().push((source_machine, guard));
    }

    /// Check all guards for a transitioning machine.
    pub async fn check(
        &self,
        machine_id: &MachineId,
        _transition: &crate::transition::TransitionDefinition,
        _manager: &ManagerInner,
    ) -> StateResult<()> {
        // Collect matching guards first (release read lock before any .await)
        let relevant_guards: Vec<CrossMachineGuard> = {
            let guards = self.guards.read();
            guards
                .iter()
                .filter(|(source, _)| *source == *machine_id)
                .map(|(_, guard)| guard.clone())
                .collect()
        };

        for guard in &relevant_guards {
            // Create a minimal context for evaluation
            let ctx = crate::context::StateContext::new(*machine_id);
            let dummy_event = StateEvent::new(EventId(0));

            let outcome = guard.evaluate(&ctx, &dummy_event).await.map_err(|e| {
                StateError::GuardError {
                    guard_name: e.guard_name,
                    cause: e.message,
                }
            })?;

            match outcome {
                crate::guard::GuardOutcome::Allow => {}
                crate::guard::GuardOutcome::Deny { reason } => {
                    return Err(StateError::TransitionRejected {
                        transition_id: TransitionId(0),
                        reason: reason.to_string(),
                    });
                }
            }
        }
        Ok(())
    }
}

impl Default for CrossMachineCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

// =========================================================================
// Public Types
// =========================================================================

/// A snapshot of all machine states on the platform.
#[derive(Debug, Clone)]
pub struct PlatformStateSnapshot {
    /// When the snapshot was taken.
    pub timestamp: SystemTime,
    /// Per-machine state snapshots.
    pub machine_states: Vec<StateSnapshot>,
    /// Total number of registered machines.
    pub total_registered: usize,
}
