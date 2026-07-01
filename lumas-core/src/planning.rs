//! # Planning Engine — Task Decomposition and Execution (Chapter 10)
//!
//! Manages plan creation, dependency graph resolution, step execution,
//! error recovery, and user approval workflows.

use lumas_common::plan::{
    ExecutionGraph, Plan, PlanContext, PlanStatus, PlanStep,
    RecoveryStrategy, StepStatus, ToolResult,
};
use std::collections::HashMap;
use tracing::{debug, info};

/// Configuration for the planning engine.
pub struct PlanningConfig {
    /// Maximum number of tools that can run concurrently.
    pub max_concurrent_tools: usize,
    /// Default number of retries for transient failures.
    pub default_max_retries: u32,
    /// Default backoff in milliseconds between retries.
    pub default_backoff_ms: u64,
}

impl Default for PlanningConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tools: 3,
            default_max_retries: 3,
            default_backoff_ms: 1000,
        }
    }
}

/// The Planning Engine orchestrates multi-step task execution.
pub struct PlanningEngine {
    /// Active plans by ID.
    active_plans: HashMap<String, Plan>,
    /// Completed plan history.
    plan_history: Vec<Plan>,
    /// Execution graph for the current plan.
    current_graph: Option<ExecutionGraph>,
    /// Engine configuration.
    config: PlanningConfig,
}

impl PlanningEngine {
    pub fn new() -> Self {
        Self {
            active_plans: HashMap::new(),
            plan_history: Vec::new(),
            current_graph: None,
            config: PlanningConfig::default(),
        }
    }

    /// Create a new plan from a user request.
    pub fn create_plan(
        &mut self,
        title: &str,
        description: &str,
        steps: Vec<PlanStep>,
        approval_required: bool,
    ) -> Plan {
        let plan = Plan {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            description: description.to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            status: PlanStatus::Draft,
            steps,
            context: PlanContext {
                original_request: String::new(),
                desktop_snapshot: serde_json::json!({}),
                user_preferences: HashMap::new(),
            },
            approval_required,
            approved_at: None,
        };

        info!(
            "Plan created: {} ({} steps, approval: {})",
            plan.title,
            plan.steps.len(),
            plan.approval_required
        );

        self.active_plans.insert(plan.id.clone(), plan.clone());
        plan
    }

    /// Build the execution graph for a plan.
    pub fn build_execution_graph(&mut self, plan_id: &str) {
        let plan = self.active_plans.get_mut(plan_id).expect("plan not found");
        let graph = ExecutionGraph::build(plan.steps.clone());
        plan.status = PlanStatus::Running;
        self.current_graph = Some(graph);
        debug!("Execution graph built for plan {}", plan_id);
    }

    /// Get the next set of ready-to-execute steps.
    pub fn ready_steps(&self) -> Vec<String> {
        self.current_graph
            .as_ref()
            .map(|g| g.ready_steps())
            .unwrap_or_default()
    }

    /// Mark a step as completed in the execution graph.
    pub fn complete_step(&mut self, step_id: &str) {
        if let Some(graph) = &mut self.current_graph {
            graph.complete_step(step_id);
        }
    }

    /// Execute a single step.
    pub async fn execute_step(
        &mut self,
        plan_id: &str,
        step_id: &str,
    ) -> Result<ToolResult, String> {
        let plan = self
            .active_plans
            .get_mut(plan_id)
            .ok_or_else(|| format!("Plan {plan_id} not found"))?;

        let step = plan
            .steps
            .iter_mut()
            .find(|s| s.id == step_id)
            .ok_or_else(|| format!("Step {step_id} not found"))?;

        step.status = StepStatus::Running;
        step.started_at = Some(chrono::Utc::now().timestamp_millis());

        debug!("Executing step: {} ({})", step.title, step.tool);

        // In a real implementation, this would invoke the Tool Framework.
        // For the skeleton, return a mock success.
        let result = ToolResult {
            success: true,
            output: serde_json::json!({"status": "executed"}),
            error: None,
            duration_ms: 0,
        };

        step.status = StepStatus::Completed;
        step.result = Some(result.clone());
        step.completed_at = Some(chrono::Utc::now().timestamp_millis());

        self.complete_step(step_id);

        // Check if plan is complete
        if self.is_plan_complete(plan_id) {
            if let Some(p) = self.active_plans.get_mut(plan_id) {
                p.status = PlanStatus::Completed;
                info!("Plan {} completed successfully", plan_id);
                self.plan_history.push(p.clone());
                self.active_plans.remove(plan_id);
            }
        }

        Ok(result)
    }

    /// Determine recovery strategy for a failed step.
    pub fn determine_recovery(&self, step: &PlanStep, error: &str) -> RecoveryStrategy {
        if error.contains("transient") || error.contains("timeout") {
            RecoveryStrategy::Retry {
                max_attempts: 3,
                backoff_ms: 1000,
            }
        } else if error.contains("permission") || error.contains("denied") {
            RecoveryStrategy::AskUser {
                message: format!(
                    "Lumas needs permission to {}. Please grant access.",
                    step.description
                ),
            }
        } else if error.contains("not found") || error.contains("missing") {
            RecoveryStrategy::AlternativeTool {
                tool: String::new(),
                input: step.tool_input.clone(),
            }
        } else {
            RecoveryStrategy::AbortPlan
        }
    }

    /// Approve a plan for execution.
    pub fn approve_plan(&mut self, plan_id: &str) -> Option<&mut Plan> {
        let plan = self.active_plans.get_mut(plan_id)?;
        plan.status = PlanStatus::Running;
        plan.approved_at = Some(chrono::Utc::now().timestamp_millis());
        info!("Plan {} approved", plan_id);
        Some(plan)
    }

    /// Cancel a plan.
    pub fn cancel_plan(&mut self, plan_id: &str) {
        if let Some(mut plan) = self.active_plans.remove(plan_id) {
            plan.status = PlanStatus::Cancelled;
            self.plan_history.push(plan);
            info!("Plan {} cancelled", plan_id);
        }
    }

    /// Check if a plan is fully completed.
    pub fn is_plan_complete(&self, _plan_id: &str) -> bool {
        self.current_graph
            .as_ref()
            .map(|g| g.all_completed())
            .unwrap_or(false)
    }

    /// Get an active plan by ID.
    pub fn get_plan(&self, plan_id: &str) -> Option<&Plan> {
        self.active_plans.get(plan_id)
    }

    /// Get all active plans.
    pub fn active_plans(&self) -> &HashMap<String, Plan> {
        &self.active_plans
    }

    /// Get plan execution history.
    pub fn plan_history(&self) -> &[Plan] {
        &self.plan_history
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumas_common::plan::PlanStep;

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
    fn test_create_plan() {
        let mut engine = PlanningEngine::new();
        let steps = vec![make_step("step-1", vec![])];
        let plan = engine.create_plan("Test Plan", "A test plan", steps, false);
        assert_eq!(plan.status, PlanStatus::Draft);
        assert_eq!(plan.title, "Test Plan");
    }

    #[test]
    fn test_execution_graph() {
        let mut engine = PlanningEngine::new();
        let steps = vec![
            make_step("step-1", vec![]),
            make_step("step-2", vec!["step-1"]),
        ];
        let plan = engine.create_plan("Test", "", steps, false);
        let plan_id = plan.id.clone();
        engine.build_execution_graph(&plan_id);
        let ready = engine.ready_steps();
        assert_eq!(ready, vec!["step-1"]);
    }

    #[test]
    fn test_recovery_strategies() {
        let engine = PlanningEngine::new();
        let step = make_step("test", vec![]);

        match engine.determine_recovery(&step, "transient error") {
            RecoveryStrategy::Retry { .. } => {}
            _ => panic!("Expected Retry strategy"),
        }

        match engine.determine_recovery(&step, "permission denied") {
            RecoveryStrategy::AskUser { .. } => {}
            _ => panic!("Expected AskUser strategy"),
        }

        match engine.determine_recovery(&step, "fatal error") {
            RecoveryStrategy::AbortPlan => {}
            _ => panic!("Expected AbortPlan strategy"),
        }
    }

    #[tokio::test]
    async fn test_execute_step_completes_plan() {
        let mut engine = PlanningEngine::new();
        let steps = vec![make_step("step-1", vec![])];
        let plan = engine.create_plan("Test", "", steps, false);

        let plan_id = plan.id.clone();

        engine.build_execution_graph(&plan_id);

        let result = engine.execute_step(&plan_id, "step-1").await;
        assert!(result.is_ok());
        assert!(engine.is_plan_complete(&plan_id));
    }
}
