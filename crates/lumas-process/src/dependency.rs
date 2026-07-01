//! # Dependency Graph
//!
//! Directed acyclic graph (DAG) of process dependencies.
//!
//! Built on top of `petgraph` for cycle detection, topological sort,
//! and transitive dependency queries. The graph is validated at
//! registration time — no process may start if its dependencies
//! contain a cycle or are missing.
//!
//! # Thread Safety
//!
//! `DependencyGraph` requires external synchronization via
//! `parking_lot::RwLock`. The graph is mutated during registration
//! and read during startup order computation.
//!
//! # Design
//!
//! - Each process is a node identified by `ProcessId`.
//! - Each edge represents a dependency, annotated with version
//!   requirement, whether it's required vs optional, and whether
//!   it affects startup ordering.
//! - Cycle detection uses petgraph's `is_cyclic_directed()`.
//! - Topological sort uses Kahn's algorithm via petgraph.

use crate::error::ProcessError;
use crate::id::ProcessId;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// ProcessDependency
// ---------------------------------------------------------------------------

/// Declares a dependency on another process.
///
/// Dependencies can be required or optional, and may or may not
/// affect startup ordering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessDependency {
    /// The ID of the dependency process.
    pub id: ProcessId,
    /// Semantic version requirement.
    pub version_req: semver::VersionReq,
    /// If `true`, the dependency must be present at registration time.
    pub required: bool,
    /// If `true`, the dependency must be `Ready` before this process starts.
    pub startup_order: bool,
}

impl ProcessDependency {
    /// Create a new required dependency with startup ordering.
    pub fn required(id: ProcessId, version_req: semver::VersionReq) -> Self {
        Self {
            id,
            version_req,
            required: true,
            startup_order: true,
        }
    }

    /// Create a new optional dependency.
    pub fn optional(id: ProcessId, version_req: semver::VersionReq) -> Self {
        Self {
            id,
            version_req,
            required: false,
            startup_order: false,
        }
    }
}

// ---------------------------------------------------------------------------
// DependencyEdge
// ----------------------------------------------------------------------------

/// Metadata for a dependency edge in the graph.
#[derive(Debug, Clone)]
pub struct DependencyEdge {
    /// Semantic version requirement.
    pub version_req: semver::VersionReq,
    /// Whether this is a required dependency.
    pub required: bool,
    /// Whether this affects startup ordering.
    pub startup_order: bool,
}

// ---------------------------------------------------------------------------
// DependencyGraph
// ---------------------------------------------------------------------------

/// Directed acyclic graph of process dependencies.
///
/// Built with petgraph for efficient cycle detection and topological
/// sort. The graph is populated during process registration and used
/// during startup/shutdown to determine execution order.
///
/// # Thread Safety
///
/// Requires external `RwLock` synchronization for mutation.
///
/// # Examples
///
/// ```ignore
/// let mut graph = DependencyGraph::new();
/// graph.add_process(a.clone()).unwrap();
/// graph.add_process(b.clone()).unwrap();
/// graph.add_dependency(a.clone(), b.clone(), edge).unwrap();
/// let order = graph.startup_order().unwrap();
/// ```
pub struct DependencyGraph {
    /// The underlying petgraph directed graph.
    graph: DiGraph<ProcessId, DependencyEdge>,
    /// Maps ProcessId to its NodeIndex for O(1) lookups.
    node_index: HashMap<ProcessId, NodeIndex>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph.
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_index: HashMap::new(),
        }
    }

    /// Add a process node to the graph.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::AlreadyRegistered` if the process
    /// is already in the graph.
    pub fn add_process(&mut self, id: ProcessId) -> Result<(), ProcessError> {
        if self.node_index.contains_key(&id) {
            return Err(ProcessError::AlreadyRegistered { id });
        }
        let idx = self.graph.add_node(id.clone());
        self.node_index.insert(id, idx);
        Ok(())
    }

    /// Check if a process is registered in the graph.
    pub fn contains(&self, id: &ProcessId) -> bool {
        self.node_index.contains_key(id)
    }

    /// Add a dependency edge from `dependent` → `dependency`.
    ///
    /// Immediately checks for cycles. If a cycle is detected, the edge
    /// is removed and `ProcessError::DependencyCycle` is returned.
    ///
    /// # Errors
    ///
    /// - `ProcessError::NotFound` if either node is not in the graph.
    /// - `ProcessError::DependencyCycle` if adding the edge creates a cycle.
    pub fn add_dependency(
        &mut self,
        dependent: ProcessId,
        dependency: ProcessId,
        edge: DependencyEdge,
    ) -> Result<(), ProcessError> {
        let dep_idx = self
            .node_index
            .get(&dependency)
            .ok_or_else(|| ProcessError::NotFound {
                id: dependency.clone(),
            })?;
        let dep_on_idx = self
            .node_index
            .get(&dependent)
            .ok_or_else(|| ProcessError::NotFound {
                id: dependent.clone(),
            })?;

        let edge_idx = self.graph.add_edge(*dep_idx, *dep_on_idx, edge);

        // Check for cycles immediately.
        if petgraph::algo::is_cyclic_directed(&self.graph) {
            self.graph.remove_edge(edge_idx);

            let cycle = self.find_cycle_path(&dependent);
            return Err(ProcessError::DependencyCycle { cycle });
        }

        Ok(())
    }

    /// Find a cycle path that involves the given node.
    fn find_cycle_path(&self, start: &ProcessId) -> Vec<ProcessId> {
        let start_idx = match self.node_index.get(start) {
            Some(idx) => *idx,
            None => return vec![start.clone()],
        };

        // Use DFS to find a cycle
        let mut visited = HashMap::new();
        let mut path = Vec::new();

        if self.dfs_cycle(start_idx, start_idx, &mut visited, &mut path) {
            // Path now contains the cycle nodes
            let cycle_start = path.iter().position(|n| n == start).unwrap_or(0);
            let cycle: Vec<ProcessId> = path[cycle_start..]
                .iter()
                .chain(std::iter::once(start))
                .cloned()
                .collect();
            return cycle;
        }

        vec![start.clone()]
    }

    /// DFS helper for cycle detection.
    fn dfs_cycle(
        &self,
        current: NodeIndex,
        start: NodeIndex,
        visited: &mut HashMap<NodeIndex, bool>,
        path: &mut Vec<ProcessId>,
    ) -> bool {
        if current == start && !path.is_empty() {
            return true;
        }
        if visited.contains_key(&current) {
            return false;
        }

        visited.insert(current, true);
        if let Some(node) = self.graph.node_weight(current) {
            path.push(node.clone());
        }

        for neighbor in self.graph.neighbors(current) {
            if self.dfs_cycle(neighbor, start, visited, path) {
                return true;
            }
        }

        path.pop();
        false
    }

    /// Returns processes in startup order (dependencies before dependents).
    ///
    /// Uses petgraph's `toposort`. Returns `ProcessError::DependencyCycle`
    /// if the graph contains a cycle.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::DependencyCycle` if the graph is cyclic.
    pub fn startup_order(&self) -> Result<Vec<ProcessId>, ProcessError> {
        match petgraph::algo::toposort(&self.graph, None) {
            Ok(order) => {
                let ids: Vec<ProcessId> = order
                    .into_iter()
                    .filter_map(|idx| self.graph.node_weight(idx).cloned())
                    .collect();
                Ok(ids)
            }
            Err(cycle) => {
                let cycle_node = self
                    .graph
                    .node_weight(cycle.node_id())
                    .cloned()
                    .unwrap_or_else(|| ProcessId::new("unknown"));
                let path = self.find_cycle_path(&cycle_node);
                Err(ProcessError::DependencyCycle { cycle: path })
            }
        }
    }

    /// Returns processes in shutdown order (reverse of startup order).
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::DependencyCycle` if the graph is cyclic.
    pub fn shutdown_order(&self) -> Result<Vec<ProcessId>, ProcessError> {
        let mut order = self.startup_order()?;
        order.reverse();
        Ok(order)
    }

    /// Returns all processes that directly depend on `id` (immediate dependents).
    pub fn dependents_of(&self, id: &ProcessId) -> Vec<ProcessId> {
        let idx = match self.node_index.get(id) {
            Some(idx) => *idx,
            None => return Vec::new(),
        };

        self.graph
            .neighbors_directed(idx, Direction::Incoming)
            .filter_map(|n| self.graph.node_weight(n).cloned())
            .collect()
    }

    /// Returns all transitive dependents of `id` (direct and indirect).
    pub fn transitive_dependents_of(&self, id: &ProcessId) -> Vec<ProcessId> {
        let idx = match self.node_index.get(id) {
            Some(idx) => *idx,
            None => return Vec::new(),
        };

        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![idx];

        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }

            for neighbor in self.graph.neighbors_directed(current, Direction::Incoming) {
                if let Some(node) = self.graph.node_weight(neighbor).cloned() {
                    result.push(node);
                }
                stack.push(neighbor);
            }
        }

        result
    }

    /// Validate the graph against a set of registered process IDs.
    ///
    /// Checks for missing required dependencies and version incompatibilities.
    /// Returns all validation errors, not just the first one.
    pub fn validate(
        &self,
        versions: &HashMap<ProcessId, semver::Version>,
    ) -> Vec<ProcessError> {
        let mut errors = Vec::new();

        for edge_idx in self.graph.edge_indices() {
            let (dep_idx, dep_on_idx) = self
                .graph
                .edge_endpoints(edge_idx)
                .expect("edge endpoints should exist");

            let dep = self.graph.node_weight(dep_idx).expect("node should exist");
            let dep_on = self
                .graph
                .node_weight(dep_on_idx)
                .expect("node should exist");
            let edge = self
                .graph
                .edge_weight(edge_idx)
                .expect("edge weight should exist");

            // Check required dependencies exist.
            if edge.required && !self.node_index.contains_key(dep_on) {
                errors.push(ProcessError::MissingDependency {
                    dep: dep_on.clone(),
                    requirer: dep.clone(),
                });
                continue;
            }

            // Check version compatibility.
            if let Some(version) = versions.get(dep_on) {
                if !edge.version_req.matches(version) {
                    errors.push(ProcessError::VersionIncompatible {
                        id: dep.clone(),
                        dep: dep_on.clone(),
                        required: edge.version_req.clone(),
                        found: version.clone(),
                    });
                }
            }
        }

        errors
    }

    /// Export the dependency graph as a Mermaid diagram string.
    ///
    /// Useful for diagnostics and documentation.
    pub fn to_mermaid(&self) -> String {
        let mut mermaid = String::from("graph TD;\n");

        for edge_idx in self.graph.edge_indices() {
            let (from_idx, to_idx) = self
                .graph
                .edge_endpoints(edge_idx)
                .expect("edge endpoints should exist");

            let from = self
                .graph
                .node_weight(from_idx)
                .map(|id| id.short_name().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let to = self
                .graph
                .node_weight(to_idx)
                .map(|id| id.short_name().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let edge = self
                .graph
                .edge_weight(edge_idx)
                .expect("edge weight should exist");

            let style = if edge.required { "-->|req|" } else { "-.->|opt|" };
            mermaid.push_str(&format!("    {}{}{};\n", sanitize_mermaid(&from), style, sanitize_mermaid(&to)));
        }

        mermaid
    }

    /// Number of processes in the graph.
    pub fn len(&self) -> usize {
        self.graph.node_count()
    }

    /// Whether the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.graph.node_count() == 0
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Sanitize a node name for Mermaid syntax (replace special chars).
fn sanitize_mermaid(name: &str) -> String {
    name.replace(['.', ':', '-', '#'], "_")
}

impl std::fmt::Debug for DependencyGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DependencyGraph")
            .field("node_count", &self.graph.node_count())
            .field("edge_count", &self.graph.edge_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_id(name: &str) -> ProcessId {
        ProcessId::new(name)
    }

    #[test]
    fn test_empty_graph() {
        let graph = DependencyGraph::new();
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
    }

    #[test]
    fn test_add_process() {
        let mut graph = DependencyGraph::new();
        let id = make_id("test");
        graph.add_process(id.clone()).unwrap();
        assert!(graph.contains(&id));
        assert_eq!(graph.len(), 1);
    }

    #[test]
    fn test_add_duplicate_process() {
        let mut graph = DependencyGraph::new();
        let id = make_id("test");
        graph.add_process(id.clone()).unwrap();
        let result = graph.add_process(id);
        assert!(result.is_err());
    }

    #[test]
    fn test_startup_order_respects_dependencies() {
        let mut graph = DependencyGraph::new();
        let a = make_id("a");
        let b = make_id("b");
        let c = make_id("c");

        graph.add_process(a.clone()).unwrap();
        graph.add_process(b.clone()).unwrap();
        graph.add_process(c.clone()).unwrap();

        // a depends on b, b depends on c
        graph
            .add_dependency(
                a.clone(),
                b.clone(),
                DependencyEdge {
                    version_req: semver::VersionReq::STAR,
                    required: true,
                    startup_order: true,
                },
            )
            .unwrap();

        graph
            .add_dependency(
                b.clone(),
                c.clone(),
                DependencyEdge {
                    version_req: semver::VersionReq::STAR,
                    required: true,
                    startup_order: true,
                },
            )
            .unwrap();

        let order = graph.startup_order().unwrap();
        // c (dependency) should come before b, which comes before a
        let pos_c = order.iter().position(|id| id.path() == "c").unwrap();
        let pos_b = order.iter().position(|id| id.path() == "b").unwrap();
        let pos_a = order.iter().position(|id| id.path() == "a").unwrap();

        assert!(pos_c < pos_b);
        assert!(pos_b < pos_a);
    }

    #[test]
    fn test_cycle_detection_returns_full_cycle_path() {
        let mut graph = DependencyGraph::new();
        let a = make_id("a");
        let b = make_id("b");
        let c = make_id("c");

        graph.add_process(a.clone()).unwrap();
        graph.add_process(b.clone()).unwrap();
        graph.add_process(c.clone()).unwrap();

        // a -> b -> c -> a (cycle)
        graph
            .add_dependency(a.clone(), b.clone(), {
                DependencyEdge {
                    version_req: semver::VersionReq::STAR,
                    required: true,
                    startup_order: true,
                }
            })
            .unwrap();

        graph
            .add_dependency(b.clone(), c.clone(), {
                DependencyEdge {
                    version_req: semver::VersionReq::STAR,
                    required: true,
                    startup_order: true,
                }
            })
            .unwrap();

        let result = graph.add_dependency(c.clone(), a.clone(), {
            DependencyEdge {
                version_req: semver::VersionReq::STAR,
                required: true,
                startup_order: true,
            }
        });

        assert!(result.is_err());
        match result {
            Err(ProcessError::DependencyCycle { cycle }) => {
                assert!(cycle.len() >= 3, "Cycle path should include at least a,b,c");
            }
            _ => panic!("Expected DependencyCycle error"),
        }
    }

    #[test]
    fn test_shutdown_order_is_reverse_of_startup() {
        let mut graph = DependencyGraph::new();
        let a = make_id("a");
        let b = make_id("b");

        graph.add_process(a.clone()).unwrap();
        graph.add_process(b.clone()).unwrap();

        graph
            .add_dependency(
                a.clone(),
                b.clone(),
                DependencyEdge {
                    version_req: semver::VersionReq::STAR,
                    required: true,
                    startup_order: true,
                },
            )
            .unwrap();

        let startup = graph.startup_order().unwrap();
        let shutdown = graph.shutdown_order().unwrap();

        assert_eq!(startup.len(), shutdown.len());
        let mut reversed_startup = startup.clone();
        reversed_startup.reverse();
        assert_eq!(shutdown, reversed_startup);
    }

    #[test]
    fn test_mermaid_export() {
        let mut graph = DependencyGraph::new();
        let a = make_id("lumi.render");
        let b = make_id("lumi.core");

        graph.add_process(a.clone()).unwrap();
        graph.add_process(b.clone()).unwrap();

        graph
            .add_dependency(
                a.clone(),
                b.clone(),
                DependencyEdge {
                    version_req: semver::VersionReq::STAR,
                    required: true,
                    startup_order: true,
                },
            )
            .unwrap();

        let mermaid = graph.to_mermaid();
        assert!(mermaid.starts_with("graph TD;"));
        // The sanitized names should appear (dots replaced with underscores)
        assert!(mermaid.contains("lumas_render") || mermaid.contains("lumi_core"));
    }

    #[test]
    fn test_dependents_of() {
        let mut graph = DependencyGraph::new();
        let a = make_id("a");
        let b = make_id("b");

        graph.add_process(a.clone()).unwrap();
        graph.add_process(b.clone()).unwrap();

        graph
            .add_dependency(
                a.clone(),
                b.clone(),
                DependencyEdge {
                    version_req: semver::VersionReq::STAR,
                    required: true,
                    startup_order: true,
                },
            )
            .unwrap();

        let deps = graph.dependents_of(&b);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].path(), "a");
    }

    #[test]
    fn test_optional_dependency_still_appears() {
        let mut graph = DependencyGraph::new();
        let a = make_id("a");
        let b = make_id("b");

        graph.add_process(a.clone()).unwrap();
        graph.add_process(b.clone()).unwrap();

        graph
            .add_dependency(
                a.clone(),
                b.clone(),
                DependencyEdge {
                    version_req: semver::VersionReq::STAR,
                    required: false,
                    startup_order: false,
                },
            )
            .unwrap();

        // Optional dependency should still be in the startup order
        let order = graph.startup_order().unwrap();
        assert!(order.iter().any(|id| id.path() == "b"));
    }
}
