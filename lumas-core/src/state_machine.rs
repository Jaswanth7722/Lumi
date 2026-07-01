//! # State Machine — Behavioral State Coordinator (Chapter 20)
//!
//! Receives signals from AI Core, Desktop Awareness, Voice System,
//! and Input System, and produces authoritative behavioral commands
//! consumed by downstream systems.

use lumas_common::state_machine::{
    LumiState, StateAction, StateCommand, StateEvent, StatePattern,
    TransitionRule, Trigger,
};
use std::collections::VecDeque;
use std::time::Instant;
use tracing::{debug, info};

/// Context provided when evaluating transition guards.
pub struct TransitionContext {
    pub idle_seconds: u64,
    pub focus_mode: bool,
}

/// The State Machine is the single source of truth for Lumi's behavioral state.
pub struct StateMachine {
    current_state: LumiState,
    history: VecDeque<(LumiState, Instant)>,
    transition_rules: Vec<TransitionRule>,
    pending_commands: VecDeque<StateCommand>,
}

impl StateMachine {
    pub fn new(rules: Vec<TransitionRule>) -> Self {
        Self {
            current_state: LumiState::Initializing,
            history: VecDeque::with_capacity(20),
            transition_rules: rules,
            pending_commands: VecDeque::new(),
        }
    }

    pub fn initialize(&mut self) {
        info!("State Machine initializing");
        self.current_state = LumiState::Initializing;
    }

    pub fn handle_event(&mut self, event: StateEvent) {
        for rule in &self.transition_rules {
            if self.pattern_matches(&rule.from, &self.current_state)
                && self.trigger_matches(&rule.trigger, &event)
            {
                let new_state = rule.to.clone();
                let actions = rule.actions.clone();
                self.transition_to(new_state, &actions);
                return;
            }
        }
        debug!(
            "Unhandled event {:?} in state {:?}",
            event, self.current_state
        );
    }

    fn transition_to(&mut self, new_state: LumiState, actions: &[StateAction]) {
        let old_state = std::mem::replace(&mut self.current_state, new_state.clone());

        self.history.push_front((old_state.clone(), Instant::now()));
        if self.history.len() > 20 {
            self.history.pop_back();
        }

        debug!("State transition: {:?} → {:?}", old_state, new_state);

        self.pending_commands.push_back(StateCommand::StateChanged {
            from: old_state,
            to: new_state,
        });

        for action in actions {
            match action {
                StateAction::EmitAIState(_) => {
                    debug!("Emitting AI state");
                }
                StateAction::SetCrystalState(state) => {
                    self.pending_commands
                        .push_back(StateCommand::SetCrystalState(state.clone()));
                }
                StateAction::UpdateWorkspace => {
                    debug!("Updating workspace panel");
                }
                StateAction::LogTransition => {
                    debug!("Logging transition");
                }
            }
        }
    }

    fn pattern_matches(&self, pattern: &StatePattern, state: &LumiState) -> bool {
        match pattern {
            StatePattern::Exact(_target) => true,
            StatePattern::Any => true,
            StatePattern::IdleAny => matches!(state, LumiState::Idle(_)),
            StatePattern::AnyOf(states) => states
                .iter()
                .any(|s| std::mem::discriminant(s) == std::mem::discriminant(state)),
            StatePattern::Not(states) => !states
                .iter()
                .any(|s| std::mem::discriminant(s) == std::mem::discriminant(state)),
        }
    }

    fn trigger_matches(&self, trigger: &Trigger, event: &StateEvent) -> bool {
        match trigger {
            Trigger::Event(_target) => true,
            Trigger::AnyEvent => true,
            Trigger::AIState(_) => matches!(event, StateEvent::AIStateChanged(_)),
            Trigger::Timeout { .. } => false,
        }
    }

    pub fn drain_commands(&mut self) -> Vec<StateCommand> {
        self.pending_commands.drain(..).collect()
    }

    pub fn current_state(&self) -> &LumiState {
        &self.current_state
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.current_state, LumiState::Idle(_))
    }

    pub fn is_processing(&self) -> bool {
        matches!(
            self.current_state,
            LumiState::Processing { .. }
                | LumiState::Planning { .. }
                | LumiState::Executing { .. }
                | LumiState::Responding { .. }
        )
    }

    pub fn transition_count(&self) -> usize {
        self.history.len()
    }

    #[allow(dead_code)]
    pub fn set_state(&mut self, state: LumiState) {
        self.current_state = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumas_common::state_machine::default_transition_rules;

    #[test]
    fn test_initial_state() {
        let sm = StateMachine::new(default_transition_rules());
        assert_eq!(*sm.current_state(), LumiState::Initializing);
    }

    #[test]
    fn test_startup_transition() {
        let mut sm = StateMachine::new(default_transition_rules());
        sm.handle_event(StateEvent::StartupComplete);
        assert_eq!(
            *sm.current_state(),
            LumiState::Greeting {
                phase: GreetingPhase::Starting
            }
        );
    }

    #[test]
    fn test_idle_is_idle() {
        let mut sm = StateMachine::new(default_transition_rules());
        sm.set_state(LumiState::Idle(IdleSubState::Watching));
        assert!(sm.is_idle());
        assert!(!sm.is_processing());
    }

    #[test]
    fn test_processing_state() {
        let mut sm = StateMachine::new(default_transition_rules());
        sm.set_state(LumiState::Processing { intent: None });
        assert!(!sm.is_idle());
        assert!(sm.is_processing());
    }

    #[test]
    fn test_drain_commands() {
        let mut sm = StateMachine::new(default_transition_rules());
        sm.set_state(LumiState::Idle(IdleSubState::Watching));

        // Directly set state and test drain
        sm.set_state(LumiState::FocusMode);
        let commands = sm.drain_commands();
        assert!(commands.is_empty()); // set_state doesn't emit commands
    }

    #[test]
    fn test_transition_history() {
        let mut sm = StateMachine::new(default_transition_rules());
        assert_eq!(sm.transition_count(), 0);
        sm.set_state(LumiState::Idle(IdleSubState::Watching));
        assert_eq!(sm.transition_count(), 0); // set_state doesn't add to history
    }
}
