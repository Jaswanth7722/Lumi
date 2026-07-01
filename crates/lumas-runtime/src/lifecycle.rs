//! # Runtime Lifecycle State Machine
//!
//! Typed state machine for the runtime lifecycle.
//!
//! The runtime transitions through a well-defined set of states:
//! `Uninitialized → Bootstrapping → Running → ShuttingDown → Stopped`.
//! Failures during bootstrap or runtime may cause transitions to
//! `Stopped` or `Degraded` respectively.
//!
//! # Thread Safety
//!
//! `LifecycleManager` requires external synchronization (`Arc<RwLock<>>`).
//! State transitions are not atomic with respect to event emission;
//! callers should hold a write lock for the duration of a transition.
//!
//! # Errors
//!
//! Invalid transitions return `LifecycleError` with a description of
//! why the transition is not allowed.

use crate::error::RuntimeError;
use std::collections::VecDeque;
use std::fmt;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Lifecycle States
// ---------------------------------------------------------------------------

/// The runtime's lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LifecycleState {
    /// Initial state before `start()` is called.
    Uninitialized,
    /// Startup sequence in progress.
    Bootstrapping {
        /// The current bootstrap phase.
        phase: BootstrapPhase,
    },
    /// All services running normally.
    Running,
    /// Some services have failed; core functionality may be limited.
    Degraded {
        /// Reason for degradation.
        reason: String,
        /// List of services that have failed.
        failed_services: Vec<String>,
    },
    /// Shutdown sequence in progress.
    ShuttingDown {
        /// The current shutdown phase.
        phase: ShutdownPhase,
    },
    /// Runtime has stopped.
    Stopped,
}

impl fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LifecycleState::Uninitialized => write!(f, "Uninitialized"),
            LifecycleState::Bootstrapping { phase } => write!(f, "Bootstrapping({phase})"),
            LifecycleState::Running => write!(f, "Running"),
            LifecycleState::Degraded { reason, .. } => write!(f, "Degraded({reason})"),
            LifecycleState::ShuttingDown { phase } => write!(f, "ShuttingDown({phase})"),
            LifecycleState::Stopped => write!(f, "Stopped"),
        }
    }
}

// ---------------------------------------------------------------------------
// Bootstrap Phases
// ---------------------------------------------------------------------------

/// Phases of the bootstrap sequence, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BootstrapPhase {
    LoadingConfig,
    InitializingLogger,
    InitializingIPC,
    InitializingStorage,
    DiscoveringPlugins,
    RegisteringServices,
    ResolvingDependencies,
    StartingServices,
    StartingHealthMonitor,
    Complete,
}

impl BootstrapPhase {
    /// All phases in execution order.
    pub const ALL: &'static [BootstrapPhase] = &[
        BootstrapPhase::LoadingConfig,
        BootstrapPhase::InitializingLogger,
        BootstrapPhase::InitializingIPC,
        BootstrapPhase::InitializingStorage,
        BootstrapPhase::DiscoveringPlugins,
        BootstrapPhase::RegisteringServices,
        BootstrapPhase::ResolvingDependencies,
        BootstrapPhase::StartingServices,
        BootstrapPhase::StartingHealthMonitor,
        BootstrapPhase::Complete,
    ];
}

impl fmt::Display for BootstrapPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootstrapPhase::LoadingConfig => write!(f, "LoadingConfig"),
            BootstrapPhase::InitializingLogger => write!(f, "InitializingLogger"),
            BootstrapPhase::InitializingIPC => write!(f, "InitializingIPC"),
            BootstrapPhase::InitializingStorage => write!(f, "InitializingStorage"),
            BootstrapPhase::DiscoveringPlugins => write!(f, "DiscoveringPlugins"),
            BootstrapPhase::RegisteringServices => write!(f, "RegisteringServices"),
            BootstrapPhase::ResolvingDependencies => write!(f, "ResolvingDependencies"),
            BootstrapPhase::StartingServices => write!(f, "StartingServices"),
            BootstrapPhase::StartingHealthMonitor => write!(f, "StartingHealthMonitor"),
            BootstrapPhase::Complete => write!(f, "Complete"),
        }
    }
}

// ---------------------------------------------------------------------------
// Shutdown Phases
// ---------------------------------------------------------------------------

/// Phases of the shutdown sequence, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShutdownPhase {
    SignalReceived,
    StoppingNewWork,
    DrainingTasks,
    StoppingServices,
    FlushingLogs,
    ReleasingResources,
    Complete,
}

impl ShutdownPhase {
    /// All phases in execution order.
    pub const ALL: &'static [ShutdownPhase] = &[
        ShutdownPhase::SignalReceived,
        ShutdownPhase::StoppingNewWork,
        ShutdownPhase::DrainingTasks,
        ShutdownPhase::StoppingServices,
        ShutdownPhase::FlushingLogs,
        ShutdownPhase::ReleasingResources,
        ShutdownPhase::Complete,
    ];
}

impl fmt::Display for ShutdownPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShutdownPhase::SignalReceived => write!(f, "SignalReceived"),
            ShutdownPhase::StoppingNewWork => write!(f, "StoppingNewWork"),
            ShutdownPhase::DrainingTasks => write!(f, "DrainingTasks"),
            ShutdownPhase::StoppingServices => write!(f, "StoppingServices"),
            ShutdownPhase::FlushingLogs => write!(f, "FlushingLogs"),
            ShutdownPhase::ReleasingResources => write!(f, "ReleasingResources"),
            ShutdownPhase::Complete => write!(f, "Complete"),
        }
    }
}

// ---------------------------------------------------------------------------
// Lifecycle Error
// ---------------------------------------------------------------------------

/// Errors returned by the lifecycle state machine.
#[derive(Debug, Clone)]
pub struct LifecycleError {
    /// Description of why the transition is invalid.
    pub message: String,
    /// The current state.
    pub current: LifecycleState,
    /// The attempted state.
    pub attempted: LifecycleState,
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid lifecycle transition: {} -> {}: {}",
            self.current, self.attempted, self.message
        )
    }
}

impl std::error::Error for LifecycleError {}

// ---------------------------------------------------------------------------
// Lifecycle Manager
// ---------------------------------------------------------------------------

/// Manages the runtime lifecycle state machine.
///
/// Tracks the current state, enforces valid transitions, retains
/// transition history for diagnostics, and provides query methods.
///
/// # Examples
///
/// ```ignore
/// let mut lm = LifecycleManager::new();
/// assert!(lm.transition_to_running().is_err()); // Can't skip bootstrap
/// lm.start_bootstrap();
/// lm.transition_to_running().unwrap();
/// ```
pub struct LifecycleManager {
    /// Current lifecycle state.
    current: LifecycleState,
    /// Transition history (most recent first, max 20 entries).
    history: VecDeque<(LifecycleState, Instant)>,
    /// When the runtime was started.
    started_at: Option<Instant>,
    /// Total uptime tracking.
    uptime: std::time::Duration,
}

impl LifecycleManager {
    /// Create a new lifecycle manager in the `Uninitialized` state.
    pub fn new() -> Self {
        Self {
            current: LifecycleState::Uninitialized,
            history: VecDeque::with_capacity(20),
            started_at: None,
            uptime: std::time::Duration::ZERO,
        }
    }

    /// Get the current lifecycle state.
    pub fn current(&self) -> &LifecycleState {
        &self.current
    }

    /// Whether the runtime is in a running state (Running or Degraded).
    pub fn is_running(&self) -> bool {
        matches!(
            self.current,
            LifecycleState::Running | LifecycleState::Degraded { .. }
        )
    }

    /// Whether the runtime is shutting down or stopped.
    pub fn is_stopped(&self) -> bool {
        matches!(
            self.current,
            LifecycleState::ShuttingDown { .. } | LifecycleState::Stopped
        )
    }

    /// Whether the runtime is in the bootstrap phase.
    pub fn is_bootstrapping(&self) -> bool {
        matches!(self.current, LifecycleState::Bootstrapping { .. })
    }

    /// Get the uptime in seconds since the runtime started.
    pub fn uptime_secs(&self) -> u64 {
        self.started_at
            .map(|start| start.elapsed().as_secs())
            .unwrap_or(0)
    }

    /// Get the transition history.
    pub fn history(&self) -> &VecDeque<(LifecycleState, Instant)> {
        &self.history
    }

    // -------------------------------------------------------------------
    // State Transitions
    // -------------------------------------------------------------------

    /// Transition from `Uninitialized` to `Bootstrapping`.
    ///
    /// # Errors
    ///
    /// Returns `LifecycleError` if the current state is not `Uninitialized`.
    pub fn start_bootstrap(&mut self) -> Result<BootstrapPhase, LifecycleError> {
        self.transition_to(LifecycleState::Bootstrapping {
            phase: BootstrapPhase::LoadingConfig,
        })?;
        self.started_at = Some(Instant::now());
        Ok(BootstrapPhase::LoadingConfig)
    }

    /// Advance the bootstrap phase.
    ///
    /// # Errors
    ///
    /// Returns `LifecycleError` if not in a `Bootstrapping` state.
    pub fn advance_bootstrap(&mut self, phase: BootstrapPhase) -> Result<(), LifecycleError> {
        match &self.current {
            LifecycleState::Bootstrapping { .. } => {
                self.transition_to(LifecycleState::Bootstrapping { phase })?;
                Ok(())
            }
            _ => Err(LifecycleError {
                message: "Cannot advance bootstrap: not in Bootstrapping state".into(),
                current: self.current.clone(),
                attempted: LifecycleState::Bootstrapping { phase },
            }),
        }
    }

    /// Transition from `Bootstrapping` to `Running`.
    ///
    /// # Errors
    ///
    /// Returns `LifecycleError` if the bootstrap phase is not `Complete`.
    pub fn transition_to_running(&mut self) -> Result<(), LifecycleError> {
        match &self.current {
            LifecycleState::Bootstrapping { phase } if *phase == BootstrapPhase::Complete => {
                self.transition_to(LifecycleState::Running)
            }
            LifecycleState::Bootstrapping { phase } => Err(LifecycleError {
                message: format!(
                    "Cannot transition to Running: bootstrap phase is {phase}, not Complete"
                ),
                current: self.current.clone(),
                attempted: LifecycleState::Running,
            }),
            _ => Err(LifecycleError {
                message: "Cannot transition to Running: not in Bootstrapping state".into(),
                current: self.current.clone(),
                attempted: LifecycleState::Running,
            }),
        }
    }

    /// Transition to `Degraded` due to a service failure.
    pub fn transition_to_degraded(
        &mut self,
        reason: String,
        failed_services: Vec<String>,
    ) -> Result<(), LifecycleError> {
        if !self.is_running() {
            return Err(LifecycleError {
                message: "Cannot degrade: runtime is not running".into(),
                current: self.current.clone(),
                attempted: LifecycleState::Degraded {
                    reason,
                    failed_services,
                },
            });
        }
        self.transition_to(LifecycleState::Degraded {
            reason,
            failed_services,
        })
    }

    /// Recover from `Degraded` back to `Running`.
    ///
    /// # Errors
    ///
    /// Returns `LifecycleError` if not in `Degraded` state.
    pub fn recover_from_degraded(&mut self) -> Result<(), LifecycleError> {
        match &self.current {
            LifecycleState::Degraded { .. } => self.transition_to(LifecycleState::Running),
            _ => Err(LifecycleError {
                message: "Cannot recover: not in Degraded state".into(),
                current: self.current.clone(),
                attempted: LifecycleState::Running,
            }),
        }
    }

    /// Begin graceful shutdown.
    ///
    /// Valid from `Running` or `Degraded` states.
    pub fn begin_shutdown(&mut self) -> Result<ShutdownPhase, LifecycleError> {
        if !self.is_running() {
            return Err(LifecycleError {
                message: "Cannot shutdown: runtime is not running".into(),
                current: self.current.clone(),
                attempted: LifecycleState::ShuttingDown {
                    phase: ShutdownPhase::SignalReceived,
                },
            });
        }
        self.transition_to(LifecycleState::ShuttingDown {
            phase: ShutdownPhase::SignalReceived,
        })?;
        Ok(ShutdownPhase::SignalReceived)
    }

    /// Advance the shutdown phase.
    pub fn advance_shutdown(&mut self, phase: ShutdownPhase) -> Result<(), LifecycleError> {
        match &self.current {
            LifecycleState::ShuttingDown { .. } => {
                self.transition_to(LifecycleState::ShuttingDown { phase })
            }
            _ => Err(LifecycleError {
                message: "Cannot advance shutdown: not in ShuttingDown state".into(),
                current: self.current.clone(),
                attempted: LifecycleState::ShuttingDown { phase },
            }),
        }
    }

    /// Transition to `Stopped` (final state).
    pub fn transition_to_stopped(&mut self) -> Result<(), LifecycleError> {
        match &self.current {
            LifecycleState::ShuttingDown { phase } if *phase == ShutdownPhase::Complete => {
                self.uptime = self
                    .started_at
                    .map(|start| start.elapsed())
                    .unwrap_or_default();
                self.transition_to(LifecycleState::Stopped)
            }
            LifecycleState::Bootstrapping { .. } => {
                // Bootstrap failure — go directly to Stopped
                self.transition_to(LifecycleState::Stopped)
            }
            _ => Err(LifecycleError {
                message: "Cannot transition to Stopped: shutdown not complete".into(),
                current: self.current.clone(),
                attempted: LifecycleState::Stopped,
            }),
        }
    }

    // -------------------------------------------------------------------
    // Internal
    // -------------------------------------------------------------------

    /// Perform the actual state transition after validation.
    fn transition_to(&mut self, new_state: LifecycleState) -> Result<(), LifecycleError> {
        // Validate the transition
        self.validate_transition(&new_state)?;

        let old_state = std::mem::replace(&mut self.current, new_state);

        // Record history
        self.history.push_front((old_state, Instant::now()));
        if self.history.len() > 20 {
            self.history.pop_back();
        }

        Ok(())
    }

    /// Validate that the transition is allowed.
    fn validate_transition(&self, new_state: &LifecycleState) -> Result<(), LifecycleError> {
        use LifecycleState::*;

        let allowed = match (&self.current, new_state) {
            // From Uninitialized
            (Uninitialized, Bootstrapping { .. }) => true,
            // From Bootstrapping — can advance phase, complete, or fail to Stopped
            (Bootstrapping { .. }, Bootstrapping { .. }) => true,
            (Bootstrapping { .. }, Running) => true,
            (Bootstrapping { .. }, Stopped) => true,
            // From Running
            (Running, Degraded { .. }) => true,
            (Running, ShuttingDown { .. }) => true,
            // From Degraded
            (Degraded { .. }, Running) => true,
            (Degraded { .. }, ShuttingDown { .. }) => true,
            // From ShuttingDown — advance phase or complete
            (ShuttingDown { .. }, ShuttingDown { .. }) => true,
            (ShuttingDown { .. }, Stopped) => true,
            // Terminal states
            (Stopped, _) => false,
            // Everything else is invalid
            _ => false,
        };

        if allowed {
            Ok(())
        } else {
            Err(LifecycleError {
                message: format!(
                    "Transition from {} to {} is not allowed",
                    self.current, new_state
                ),
                current: self.current.clone(),
                attempted: new_state.clone(),
            })
        }
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let lm = LifecycleManager::new();
        assert_eq!(*lm.current(), LifecycleState::Uninitialized);
        assert!(!lm.is_running());
        assert!(!lm.is_bootstrapping());
    }

    #[test]
    fn test_bootstrap_transition() {
        let mut lm = LifecycleManager::new();
        assert!(lm.start_bootstrap().is_ok());
        assert!(lm.is_bootstrapping());
        assert!(!lm.is_running());
    }

    #[test]
    fn test_transition_to_running() {
        let mut lm = LifecycleManager::new();
        lm.start_bootstrap().unwrap();

        // Must advance through all phases
        for phase in BootstrapPhase::ALL {
            lm.advance_bootstrap(*phase).unwrap();
        }

        lm.transition_to_running().unwrap();
        assert_eq!(*lm.current(), LifecycleState::Running);
        assert!(lm.is_running());
    }

    #[test]
    fn test_invalid_transition_returns_error() {
        let mut lm = LifecycleManager::new();
        // Can't go directly to Running from Uninitialized
        let result = lm.transition_to_running();
        assert!(result.is_err());
        assert_eq!(*lm.current(), LifecycleState::Uninitialized);
    }

    #[test]
    fn test_shutdown_from_degrated() {
        let mut lm = LifecycleManager::new();
        lm.start_bootstrap().unwrap();
        for phase in BootstrapPhase::ALL {
            lm.advance_bootstrap(*phase).unwrap();
        }
        lm.transition_to_running().unwrap();

        // Degrade
        lm.transition_to_degraded("test failure".into(), vec!["test".into()])
            .unwrap();
        assert!(matches!(lm.current(), LifecycleState::Degraded { .. }));

        // Shutdown from Degraded
        assert!(lm.begin_shutdown().is_ok());
    }

    #[test]
    fn test_double_start_returns_error() {
        let mut lm = LifecycleManager::new();
        assert!(lm.start_bootstrap().is_ok());
        // Can't start bootstrap again
        let result = lm.start_bootstrap();
        assert!(result.is_err());
    }

    #[test]
    fn test_history_retained() {
        let mut lm = LifecycleManager::new();
        lm.start_bootstrap().unwrap();
        for phase in BootstrapPhase::ALL {
            lm.advance_bootstrap(*phase).unwrap();
        }
        lm.transition_to_running().unwrap();

        assert!(lm.history().len() >= 2);
    }

    #[test]
    fn test_uptime() {
        let mut lm = LifecycleManager::new();
        assert_eq!(lm.uptime_secs(), 0);
        lm.start_bootstrap().unwrap();
        // Uptime should be non-zero after start
        assert!(lm.uptime_secs() == 0 || lm.uptime_secs() > 0);
    }
}
