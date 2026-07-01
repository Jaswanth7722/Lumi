//! # Planning Engine — Plan and Execution Types (Chapter 10)
//!
//! Defines the plan structure, step lifecycle, dependency graph,
//! execution engine types, and error recovery strategies.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a plan (used by state_machine).
pub type PlanId = String;
/// Unique identifier for a plan step (used by state_machine).
pub type StepId = String;

// ---------------------------------------------------------------------------
// Plan Structure
// ---------------------------------------------------------------------------

/// A complete task plan with steps and execution context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub title: String,
    pub description: String,
    pub created_at: i64,
    pub status: PlanStatus,
    pub steps: Vec<PlanStep>,
    pub context: PlanContext,
    pub approval_required: bool,
    pub approved_at: Option<i64>,
}

/// A single step within a plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    pub description: String,
    pub tool: String,
    pub tool_input: serde_json::Value,
    /// IDs of steps this step depends on.
    pub depends_on: Vec<String>,
    pub status: StepStatus,
    pub result: Option<ToolResult>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub retry_count: u32,
    pub max_retries: u32,
}

/// Overall plan lifecycle status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanStatus {
    Draft,
    AwaitingApproval,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// Individual step execution status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    Ready,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// Context captured when a plan was created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanContext {
    pub original_request: String,
    pub desktop_snapshot: serde_json::Value,
    pub user_preferences: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Execution Types
// ---------------------------------------------------------------------------

/// The result of executing a single tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Overall result of executing a plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanResult {
    Success,
    Failure(Vec<ExecutionError>),
    Cancelled,
}

/// An error that occurred during plan execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionError {
    pub step_id: String,
    pub step_title: String,
    pub error: String,
}

/// Strategy for recovering from a step failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// Retry the step with exponential backoff.
    Retry { max_attempts: u8, backoff_ms: u64 },
    /// Skip this step and continue.
    SkipStep,
    /// Use an alternative tool for this step.
    AlternativeTool {
        tool: String,
        input: serde_json::Value,
    },
    /// Ask the user how to proceed.
    AskUser { message: String },
    /// Abort the entire plan.
    AbortPlan,
}

// ---------------------------------------------------------------------------
// Execution Graph (DAG)
// ---------------------------------------------------------------------------

/// A directed acyclic graph representing step dependencies for parallel execution.
#[derive(Debug, Clone)]
pub struct ExecutionGraph {
    /// All steps indexed by ID.
    pub steps: HashMap<String, PlanStep>,
    /// Adjacency list: step_id → dependents.
    pub adjacency: HashMap<String, Vec<String>>,
    /// In-degree count for each step (number of uncompleted dependencies).
    pub in_degree: HashMap<String, usize>,
}

impl ExecutionGraph {
    /// Build an execution graph from a list of plan steps.
    pub fn build(steps: Vec<PlanStep>) -> Self {
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut step_map: HashMap<String, PlanStep> = HashMap::new();

        for step in &steps {
            step_map.insert(step.id.clone(), step.clone());
            adjacency.entry(step.id.clone()).or_default();
            in_degree.entry(step.id.clone()).or_insert(0);
        }

        for step in &steps {
            for dep in &step.depends_on {
                adjacency
                    .entry(dep.clone())
                    .or_default()
                    .push(step.id.clone());
                *in_degree.entry(step.id.clone()).or_insert(0) += 1;
            }
        }

        Self {
            steps: step_map,
            adjacency,
            in_degree,
        }
    }

    /// Get steps that are ready to execute (all dependencies satisfied).
    pub fn ready_steps(&self) -> Vec<String> {
        self.in_degree
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .filter(|(id, _)| {
                self.steps
                    .get(id.as_str())
                    .is_some_and(|s| s.status == StepStatus::Pending)
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Mark a step as completed and update dependency counts.
    pub fn complete_step(&mut self, step_id: &str) {
        if let Some(dependents) = self.adjacency.get(step_id).cloned() {
            for dependent in dependents {
                if let Some(degree) = self.in_degree.get_mut(&dependent) {
                    *degree = degree.saturating_sub(1);
                }
            }
        }
        if let Some(step) = self.steps.get_mut(step_id) {
            step.status = StepStatus::Completed;
        }
    }

    /// Check if all steps are completed.
    pub fn all_completed(&self) -> bool {
        self.steps
            .values()
            .all(|s| s.status == StepStatus::Completed)
    }

    /// Check if any steps have failed.
    pub fn any_failed(&self) -> bool {
        self.steps.values().any(|s| s.status == StepStatus::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_step(id: &str, depends_on: Vec<&str>) -> PlanStep {
        PlanStep {
            id: id.to_string(),
            title: format!("Step {}", id),
            description: String::new(),
            tool: "test.tool".to_string(),
            tool_input: serde_json::json!({}),
            depends_on: depends_on.into_iter().map(String::from).collect(),
            status: StepStatus::Pending,
            result: None,
            started_at: None,
            completed_at: None,
            retry_count: 0,
            max_retries: 3,
        }
    }

    #[test]
    fn test_execution_graph_linear() {
        let steps = vec![
            make_step("step-1", vec![]),
            make_step("step-2", vec!["step-1"]),
            make_step("step-3", vec!["step-2"]),
        ];
        let graph = ExecutionGraph::build(steps);

        let ready = graph.ready_steps();
        assert_eq!(ready, vec!["step-1"]);
    }

    #[test]
    fn test_execution_graph_parallel() {
        let steps = vec![
            make_step("step-1", vec![]),
            make_step("step-2", vec![]),
            make_step("step-3", vec!["step-1", "step-2"]),
        ];
        let graph = ExecutionGraph::build(steps);

        let mut ready = graph.ready_steps();
        ready.sort();
        assert_eq!(ready, vec!["step-1", "step-2"]);
    }

    #[test]
    fn test_complete_step_updates_dependents() {
        let steps = vec![
            make_step("step-1", vec![]),
            make_step("step-2", vec!["step-1"]),
        ];
        let mut graph = ExecutionGraph::build(steps);

        assert_eq!(graph.in_degree["step-2"], 1);
        graph.complete_step("step-1");
        assert_eq!(graph.in_degree["step-2"], 0);
        assert_eq!(graph.steps["step-1"].status, StepStatus::Completed);
    }

    #[test]
    fn test_plan_status_roundtrip() {
        let statuses = vec![
            PlanStatus::Draft,
            PlanStatus::Running,
            PlanStatus::Completed,
            PlanStatus::Failed,
            PlanStatus::Cancelled,
        ];
        for status in statuses {
            let json = serde_json::to_value(&status).unwrap();
            let deserialized: PlanStatus = serde_json::from_value(json).unwrap();
            // We can't derive PartialEq on enum variants with different shapes
            // so we match on the debug string instead
            assert_eq!(format!("{status:?}"), format!("{deserialized:?}"));
        }
    }
}
