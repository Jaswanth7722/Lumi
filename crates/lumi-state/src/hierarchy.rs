//! # Hierarchical State Machine Support
//!
//! Implements full UML statechart hierarchy model with composite states,
//! history (shallow and deep), and concurrent regions.
//!
//! The LCA (Lowest Common Ancestor) algorithm determines which exit/entry
//! actions fire during transitions between states at different hierarchy levels.

use crate::error::StateId;
use crate::state::MachineState;
use indexmap::IndexMap;
use std::sync::Arc;

/// A composite state containing substates.
///
/// When a composite state is active, exactly one of its substates is also active.
/// Exiting a composite state exits all active substates first (bottom-up).
#[derive(Debug)]
pub struct CompositeState {
    /// State ID of this composite.
    pub id: StateId,
    /// Human-readable name.
    pub name: &'static str,
    /// Machine this state belongs to.
    pub machine_id: crate::error::MachineId,
    /// Substate map.
    pub substates: IndexMap<StateId, Arc<dyn MachineState>>,
    /// The initial substate entered when this composite is first entered.
    pub initial_substate: StateId,
    /// History behavior for re-entering this composite.
    pub history: HistoryKind,
    /// Concurrent regions (for orthogonal states).
    pub concurrent_regions: Vec<Region>,
    /// Optional timeout.
    pub timeout: Option<crate::state::StateTimeout>,
}

impl CompositeState {
    /// Create a new composite state.
    pub fn new(
        id: StateId,
        name: &'static str,
        machine_id: crate::error::MachineId,
        initial_substate: StateId,
    ) -> Self {
        Self {
            id,
            name,
            machine_id,
            substates: IndexMap::new(),
            initial_substate,
            history: HistoryKind::None,
            concurrent_regions: Vec::new(),
            timeout: None,
        }
    }

    /// Add a substate.
    pub fn add_substate(mut self, state: Arc<dyn MachineState>) -> Self {
        self.substates.insert(state.id(), state);
        self
    }

    /// Set history kind.
    pub fn with_history(mut self, history: HistoryKind) -> Self {
        self.history = history;
        self
    }

    /// Set timeout.
    pub fn with_timeout(mut self, timeout: crate::state::StateTimeout) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Get a substate by ID.
    pub fn get_substate(&self, id: StateId) -> Option<&Arc<dyn MachineState>> {
        self.substates.get(&id)
    }
}

/// History behavior for composite states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryKind {
    /// Always enters the initial substate.
    None,
    /// Resumes the last active direct substate.
    Shallow,
    /// Resumes the last active substate at all depths.
    Deep,
}

/// A concurrent region within a composite state.
///
/// Regions within the same composite state are active simultaneously.
/// Each region executes its own state machine independently.
#[derive(Debug)]
pub struct Region {
    /// Region ID (unique within the parent composite).
    pub id: String,
    /// Human-readable name.
    pub name: &'static str,
    /// Initial state for this region.
    pub initial_state: StateId,
    /// States in this region.
    pub states: IndexMap<StateId, Arc<dyn MachineState>>,
}

// =========================================================================
// LCA (Lowest Common Ancestor) Algorithm
// =========================================================================

/// Result of computing the LCA for a transition between two states.
#[derive(Debug, Clone)]
pub struct LcaResult {
    /// The LCA state ID (common ancestor of source and target).
    pub lca: Option<StateId>,
    /// States to exit (from source up to but not including LCA), outermost first.
    pub exit_path: Vec<StateId>,
    /// States to enter (from LCA down to target), outermost first.
    pub enter_path: Vec<StateId>,
}

/// Compute the LCA of two states in a hierarchy tree.
///
/// The hierarchy is represented as a parent map: `hierarchy[state_id] = parent_id`.
/// The LCA is used to determine which states need to be exited and entered
/// during a transition.
pub fn compute_lca(
    source: StateId,
    target: StateId,
    hierarchy: &std::collections::HashMap<StateId, StateId>,
) -> LcaResult {
    // Build ancestry paths
    let source_ancestors = get_ancestors(source, hierarchy);
    let target_ancestors = get_ancestors(target, hierarchy);

    // Find the LCA
    let lca = source_ancestors
        .iter()
        .rev()
        .find(|id| target_ancestors.contains(id))
        .copied();

    // States to exit: source ancestors up to (but not including) LCA
    let exit_path: Vec<StateId> = source_ancestors
        .iter()
        .take_while(|id| Some(**id) != lca)
        .copied()
        .collect();

    // States to enter: from LCA child down to target
    let enter_path: Vec<StateId> = target_ancestors
        .iter()
        .rev()
        .skip_while(|id| Some(**id) != lca)
        .skip(if lca.is_some() { 1 } else { 0 })
        .chain(std::iter::once(&target))
        .copied()
        .collect();

    LcaResult {
        lca,
        exit_path,
        enter_path,
    }
}

/// Get all ancestors of a state (including the state itself at the front).
fn get_ancestors(
    state: StateId,
    hierarchy: &std::collections::HashMap<StateId, StateId>,
) -> Vec<StateId> {
    let mut ancestors = Vec::new();
    let mut current = Some(state);

    while let Some(id) = current {
        ancestors.push(id);
        current = hierarchy.get(&id).copied();
    }

    ancestors
}

/// Build a parent hierarchy map from a list of composite states.
///
/// Returns `(hierarchy_map, all_state_ids)` where `hierarchy_map[state_id] = parent_id`.
pub fn build_hierarchy(
    composites: &[&CompositeState],
) -> std::collections::HashMap<StateId, StateId> {
    let mut hierarchy = std::collections::HashMap::new();

    for composite in composites {
        // The composite itself has no parent in this scope (its parent is set separately)
        for (substate_id, _) in &composite.substates {
            hierarchy.insert(*substate_id, composite.id);
        }
        for region in &composite.concurrent_regions {
            for (state_id, _) in &region.states {
                hierarchy.insert(*state_id, composite.id);
            }
        }
    }

    hierarchy
}

/// Walk the hierarchy to find all ancestor composite states.
pub fn find_ancestor_composites(
    state_id: StateId,
    hierarchy: &std::collections::HashMap<StateId, StateId>,
    composites: &std::collections::HashMap<StateId, &CompositeState>,
) -> Vec<&CompositeState> {
    let ancestors = get_ancestors(state_id, hierarchy);
    ancestors
        .iter()
        .filter_map(|id| composites.get(id).copied())
        .collect()
}

// =========================================================================
// Pre-defined Character State Hierarchy
// =========================================================================

/// Build the complete Character state hierarchy.
pub fn character_state_hierarchy() -> Vec<CompositeState> {
    use crate::event::events;
    use crate::state::StateTimeout;
    use std::time::Duration;

    // Leaf states
    let watching = Arc::new(crate::state::LeafState::new(
        StateId(1100),
        "Watching",
        MachineId::CHARACTER,
    ));
    let exploring = Arc::new(crate::state::LeafState::new(
        StateId(1101),
        "Exploring",
        MachineId::CHARACTER,
    ));
    let resting = Arc::new(crate::state::LeafState::new(
        StateId(1102),
        "Resting",
        MachineId::CHARACTER,
    ));
    let listening = Arc::new(crate::state::LeafState::new(
        StateId(1200),
        "Listening",
        MachineId::CHARACTER,
    ));
    let thinking = Arc::new(crate::state::LeafState::new(
        StateId(1201),
        "Thinking",
        MachineId::CHARACTER,
    ));
    let speaking = Arc::new(crate::state::LeafState::new(
        StateId(1202),
        "Speaking",
        MachineId::CHARACTER,
    ));
    let awaiting_input = Arc::new(crate::state::LeafState::new(
        StateId(1203),
        "AwaitingInput",
        MachineId::CHARACTER,
    ));
    let preparing = Arc::new(crate::state::LeafState::new(
        StateId(1300),
        "Preparing",
        MachineId::CHARACTER,
    ));
    let executing = Arc::new(crate::state::LeafState::new(
        StateId(1301),
        "Executing",
        MachineId::CHARACTER,
    ));
    let verifying_result = Arc::new(crate::state::LeafState::new(
        StateId(1302),
        "VerifyingResult",
        MachineId::CHARACTER,
    ));
    let sleeping = Arc::new(
        crate::state::LeafState::new(StateId(1400), "Sleeping", MachineId::CHARACTER).with_timeout(
            StateTimeout {
                duration: Duration::from_secs(3600),
                on_timeout: crate::state::TimeoutAction::Transition(
                    events::CHAR_IDLE_TIMER_EXPIRED,
                ),
            },
        ),
    );
    let focus_mode = Arc::new(crate::state::LeafState::new(
        StateId(1500),
        "FocusMode",
        MachineId::CHARACTER,
    ));

    // Composite states
    let idle = CompositeState::new(StateId(100), "Idle", MachineId::CHARACTER, watching.id())
        .add_substate(watching)
        .add_substate(exploring)
        .add_substate(resting)
        .with_history(HistoryKind::None);

    let interacting = CompositeState::new(
        StateId(200),
        "Interacting",
        MachineId::CHARACTER,
        listening.id(),
    )
    .add_substate(listening)
    .add_substate(thinking)
    .add_substate(speaking)
    .add_substate(awaiting_input)
    .with_history(HistoryKind::None);

    let working = CompositeState::new(
        StateId(300),
        "Working",
        MachineId::CHARACTER,
        preparing.id(),
    )
    .add_substate(preparing)
    .add_substate(executing)
    .add_substate(verifying_result)
    .with_history(HistoryKind::Shallow);

    vec![idle, interacting, working]
}

/// Build the complete AI state hierarchy.
pub fn ai_state_hierarchy() -> Vec<CompositeState> {
    // Leaf states
    let ready = Arc::new(crate::state::LeafState::new(
        StateId(2100),
        "Ready",
        MachineId::AI,
    ));
    let building_context = Arc::new(crate::state::LeafState::new(
        StateId(2101),
        "BuildingContext",
        MachineId::AI,
    ));
    let retrieving_memory = Arc::new(crate::state::LeafState::new(
        StateId(2102),
        "RetrievingMemory",
        MachineId::AI,
    ));
    let waiting_first_token = Arc::new(crate::state::LeafState::new(
        StateId(2103),
        "WaitingFirstToken",
        MachineId::AI,
    ));
    let streaming = Arc::new(crate::state::LeafState::new(
        StateId(2104),
        "Streaming",
        MachineId::AI,
    ));
    let reflecting = Arc::new(crate::state::LeafState::new(
        StateId(2105),
        "Reflecting",
        MachineId::AI,
    ));
    let generating_plan = Arc::new(crate::state::LeafState::new(
        StateId(2106),
        "GeneratingPlan",
        MachineId::AI,
    ));
    let awaiting_approval = Arc::new(crate::state::LeafState::new(
        StateId(2107),
        "AwaitingApproval",
        MachineId::AI,
    ));
    let starting_tool = Arc::new(crate::state::LeafState::new(
        StateId(2108),
        "StartingTool",
        MachineId::AI,
    ));
    let running_tool = Arc::new(crate::state::LeafState::new(
        StateId(2109),
        "RunningTool",
        MachineId::AI,
    ));
    let collecting_result = Arc::new(crate::state::LeafState::new(
        StateId(2110),
        "CollectingResult",
        MachineId::AI,
    ));
    let cancelled = Arc::new(crate::state::LeafState::new(
        StateId(2111),
        "Cancelled",
        MachineId::AI,
    ));
    let failed = Arc::new(crate::state::LeafState::new(
        StateId(2112),
        "Failed",
        MachineId::AI,
    ));

    // Inferring is composite within Processing
    let inferring = CompositeState::new(
        StateId(2201),
        "Inferring",
        MachineId::AI,
        waiting_first_token.id(),
    )
    .add_substate(waiting_first_token)
    .add_substate(streaming);

    let processing = CompositeState::new(
        StateId(200),
        "Processing",
        MachineId::AI,
        building_context.id(),
    )
    .add_substate(building_context)
    .add_substate(retrieving_memory)
    .add_substate(Arc::new(inferring))
    .add_substate(reflecting);

    let planning = CompositeState::new(
        StateId(300),
        "Planning",
        MachineId::AI,
        generating_plan.id(),
    )
    .add_substate(generating_plan)
    .add_substate(awaiting_approval);

    let executing =
        CompositeState::new(StateId(400), "Executing", MachineId::AI, starting_tool.id())
            .add_substate(starting_tool)
            .add_substate(running_tool)
            .add_substate(collecting_result)
            .with_history(HistoryKind::Deep);

    vec![processing, planning, executing]
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_compute_lca_same_state() {
        let mut hierarchy = HashMap::new();
        hierarchy.insert(StateId(1), StateId(0));
        hierarchy.insert(StateId(2), StateId(1));

        let result = compute_lca(StateId(2), StateId(2), &hierarchy);
        assert_eq!(result.lca, Some(StateId(2)));
        assert!(result.exit_path.is_empty());
        assert!(result.enter_path.is_empty());
    }

    #[test]
    fn test_compute_lca_siblings() {
        let mut hierarchy = HashMap::new();
        hierarchy.insert(StateId(1), StateId(0));
        hierarchy.insert(StateId(2), StateId(0));

        let result = compute_lca(StateId(1), StateId(2), &hierarchy);
        assert_eq!(result.lca, Some(StateId(0)));
        assert_eq!(result.exit_path, vec![StateId(1)]);
        assert_eq!(result.enter_path, vec![StateId(2)]);
    }

    #[test]
    fn test_character_state_hierarchy_built() {
        let composites = character_state_hierarchy();
        assert_eq!(composites.len(), 3); // Idle, Interacting, Working
    }

    #[test]
    fn test_ai_state_hierarchy_built() {
        let composites = ai_state_hierarchy();
        assert_eq!(composites.len(), 3); // Processing, Planning, Executing
    }
}
