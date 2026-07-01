//! # Resource Tracking
//!
//! Tracks resource limits and usage across subsystems.
//!
//! # Thread Safety
//! Uses `DashMap` for concurrent access.

use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Resource type identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    /// Memory (bytes).
    Memory,
    /// CPU time (microseconds).
    CpuTime,
    /// Network bandwidth (bytes/sec).
    NetworkBandwidth,
    /// Disk I/O (bytes/sec).
    DiskIo,
    /// GPU memory (bytes).
    GpuMemory,
}

/// Resource usage at a point in time.
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    /// Resource type.
    pub resource: ResourceType,
    /// Current usage.
    pub current: u64,
    /// Maximum observed.
    pub max: u64,
    /// Configured limit.
    pub limit: u64,
    /// Usage percentage (0–100).
    pub percent: f32,
}

/// Resource limit configuration.
#[derive(Debug, Clone)]
pub struct ResourceLimit {
    /// Resource type.
    pub resource: ResourceType,
    /// Hard limit.
    pub hard_limit: u64,
    /// Soft limit (warning at this level).
    pub soft_limit: u64,
}

/// Resource tracker for monitoring subsystem resource usage.
#[derive(Debug)]
pub struct ResourceTracker {
    limits: Vec<ResourceLimit>,
    usage: Arc<DashMap<String, Vec<(ResourceType, u64)>>>,
}

impl ResourceTracker {
    /// Create a new resource tracker.
    pub fn new(limits: Vec<ResourceLimit>) -> Self {
        Self {
            limits,
            usage: Arc::new(DashMap::new()),
        }
    }

    /// Record resource usage for a subsystem.
    pub fn record(&self, subsystem: &str, resource: ResourceType, value: u64) {
        self.usage
            .entry(subsystem.to_string())
            .or_insert_with(Vec::new)
            .push((resource, value));
    }

    /// Get current usage for a subsystem and resource type.
    pub fn get_usage(&self, subsystem: &str, resource: ResourceType) -> ResourceUsage {
        let current = self
            .usage
            .get(subsystem)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|(r, _)| *r == resource)
                    .map(|(_, v)| *v)
                    .sum()
            })
            .unwrap_or(0);

        let limit = self
            .limits
            .iter()
            .find(|l| l.resource == resource)
            .map(|l| l.hard_limit)
            .unwrap_or(u64::MAX);

        ResourceUsage {
            resource,
            current,
            max: current,
            limit,
            percent: if limit > 0 {
                (current as f32 / limit as f32) * 100.0
            } else {
                0.0
            },
        }
    }

    /// Check if any resource is over its limit.
    pub fn check_limits(&self) -> Vec<ResourceUsage> {
        let mut violations = Vec::new();
        for limit in &self.limits {
            // Check across all subsystems
            for entry in self.usage.iter() {
                for (resource, value) in entry.value() {
                    if *resource == limit.resource && *value > limit.hard_limit {
                        violations.push(ResourceUsage {
                            resource: limit.resource,
                            current: *value,
                            max: *value,
                            limit: limit.hard_limit,
                            percent: (*value as f32 / limit.hard_limit as f32) * 100.0,
                        });
                    }
                }
            }
        }
        violations
    }
}

impl Default for ResourceTracker {
    fn default() -> Self {
        Self::new(vec![
            ResourceLimit {
                resource: ResourceType::Memory,
                hard_limit: 1_073_741_824, // 1GB
                soft_limit: 536_870_912,   // 512MB
            },
            ResourceLimit {
                resource: ResourceType::CpuTime,
                hard_limit: 50_000_000, // 50s
                soft_limit: 25_000_000, // 25s
            },
            ResourceLimit {
                resource: ResourceType::GpuMemory,
                hard_limit: 2_147_483_648, // 2GB
                soft_limit: 1_073_741_824, // 1GB
            },
        ])
    }
}
