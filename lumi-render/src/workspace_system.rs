//! # Workspace System — Floating UI Panels (Chapter 12)
//!
//! Manages the lifecycle and rendering of floating workspace panels
//! that appear beside Lumi during active AI operations.

use lumi_common::workspace::{
    PanelContent, PanelId, PanelState, PanelStatus, PanelType, PreferredSide, Rect, Size,
    SectionContentType, compute_panel_position, PanelSection,
};
use std::collections::HashMap;
use tracing::debug;

/// Manages the lifecycle and display of workspace panels.
pub struct WorkspaceSystem {
    /// Active panels by ID.
    panels: HashMap<PanelId, PanelInstance>,
    /// Next panel ID counter.
    next_id: u64,
    /// Lumi's current position on screen.
    lumi_bounds: Rect,
    /// The monitor's work area.
    monitor_bounds: Rect,
}

/// A single panel instance with its state and content.
pub struct PanelInstance {
    pub panel_id: PanelId,
    pub panel_type: PanelType,
    pub state: PanelState,
    pub content: PanelContent,
    pub position: (f32, f32),
    pub size: Size,
    pub preferred_side: PreferredSide,
    pub creation_time: i64,
    pub auto_dismiss_after_ms: u64,
}

impl WorkspaceSystem {
    pub fn new() -> Self {
        Self {
            panels: HashMap::new(),
            next_id: 0,
            lumi_bounds: Rect::new(500.0, 500.0, 100.0, 150.0),
            monitor_bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
        }
    }

    /// Show a workspace panel with the given type.
    pub fn show_panel(&mut self, panel_type: PanelType) -> PanelId {
        let id = PanelId::new();
        let (width, height) = self.panel_size_for_type(&panel_type);
        let panel_size = Size { width, height };

        let position = compute_panel_position(
            self.lumi_bounds,
            panel_size,
            lumi_common::workspace::MonitorInfo {
                work_area: self.monitor_bounds,
                scale_factor: 1.0,
                is_primary: true,
            },
            PreferredSide::Right,
        );

        let instance = PanelInstance {
            panel_id: id.clone(),
            panel_type: panel_type.clone(),
            state: PanelState::Appearing,
            content: PanelContent {
                title: None,
                sections: vec![],
                status: None,
                metadata: None,
            },
            position,
            size: panel_size,
            preferred_side: PreferredSide::Right,
            creation_time: chrono::Utc::now().timestamp_millis(),
            auto_dismiss_after_ms: 3000,
        };

        self.panels.insert(id.clone(), instance);
        debug!("Panel shown: {:?}", panel_type);
        id
    }

    /// Hide and dismiss a panel.
    pub fn hide_panel(&mut self, panel_id: &PanelId) {
        if let Some(panel) = self.panels.get_mut(panel_id) {
            panel.state = PanelState::Dismissing;
            // In production, after dismiss animation completes, remove from map
            self.panels.remove(panel_id);
            debug!("Panel hidden: {:?}", panel_id.0);
        }
    }

    /// Update the content of a panel.
    pub fn update_panel(&mut self, panel_id: &PanelId, content: PanelContent) {
        if let Some(panel) = self.panels.get_mut(panel_id) {
            panel.content = content;
            panel.state = PanelState::Updating;
            // After update animation, set back to visible
            panel.state = PanelState::Visible;
        }
    }

    /// Pin a panel to prevent auto-dismiss.
    pub fn pin_panel(&mut self, panel_id: &PanelId) {
        if let Some(panel) = self.panels.get_mut(panel_id) {
            panel.state = PanelState::Pinned;
            debug!("Panel pinned: {:?}", panel_id.0);
        }
    }

    /// Unpin a panel (restores auto-dismiss).
    pub fn unpin_panel(&mut self, panel_id: &PanelId) {
        if let Some(panel) = self.panels.get_mut(panel_id) {
            panel.state = PanelState::Visible;
            debug!("Panel unpinned: {:?}", panel_id.0);
        }
    }

    /// Update Lumi's position on screen (repositions panels accordingly).
    pub fn update_lumi_position(&mut self, bounds: Rect) {
        self.lumi_bounds = bounds;
        // Reposition all visible panels
        for panel in self.panels.values_mut() {
            panel.position = compute_panel_position(
                self.lumi_bounds,
                panel.size,
                lumi_common::workspace::MonitorInfo {
                    work_area: self.monitor_bounds,
                    scale_factor: 1.0,
                    is_primary: true,
                },
                panel.preferred_side,
            );
        }
    }

    /// Get all active panels.
    pub fn active_panels(&self) -> Vec<&PanelInstance> {
        self.panels.values().collect()
    }

    /// Check if there are any visible panels.
    pub fn has_visible_panels(&self) -> bool {
        self.panels
            .values()
            .any(|p| matches!(p.state, PanelState::Visible | PanelState::Pinned))
    }

    /// Get default panel dimensions by type.
    fn panel_size_for_type(&self, panel_type: &PanelType) -> (f32, f32) {
        match panel_type {
            PanelType::Plan => (320.0, 240.0),
            PanelType::Terminal => (400.0, 300.0),
            PanelType::Thinking => (200.0, 80.0),
            PanelType::Memory => (300.0, 200.0),
            PanelType::Custom(_) => (350.0, 200.0),
        }
    }

    /// Create a default plan panel content.
    pub fn create_plan_content(title: &str, steps: &[(&str, PanelStatus)]) -> PanelContent {
        let sections: Vec<PanelSection> = steps
            .iter()
            .map(|(name, _status)| PanelSection {
                heading: Some(name.to_string()),
                body: String::new(),
                content_type: SectionContentType::StepList,
            })
            .collect();

        PanelContent {
            title: Some(title.to_string()),
            sections,
            status: None,
            metadata: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_show_and_hide_panel() {
        let mut ws = WorkspaceSystem::new();
        let id = ws.show_panel(PanelType::Thinking);
        assert!(ws.active_panels().len() == 1);
        ws.hide_panel(&id);
        assert!(ws.active_panels().is_empty());
    }

    #[test]
    fn test_multiple_panels() {
        let mut ws = WorkspaceSystem::new();
        ws.show_panel(PanelType::Plan);
        ws.show_panel(PanelType::Terminal);
        assert_eq!(ws.active_panels().len(), 2);
    }

    #[test]
    fn test_panel_types_have_sizes() {
        let ws = WorkspaceSystem::new();
        assert_eq!(ws.panel_size_for_type(&PanelType::Thinking), (200.0, 80.0));
        assert_eq!(ws.panel_size_for_type(&PanelType::Plan), (320.0, 240.0));
    }

    #[test]
    fn test_pin_unpin() {
        let mut ws = WorkspaceSystem::new();
        let id = ws.show_panel(PanelType::Memory);

        ws.pin_panel(&id);
        let panel = ws.active_panels().iter().find(|p| p.panel_id == id).unwrap();
        assert_eq!(panel.state, PanelState::Pinned);

        ws.unpin_panel(&id);
        let panel = ws.active_panels().iter().find(|p| p.panel_id == id).unwrap();
        assert_eq!(panel.state, PanelState::Visible);
    }

    #[test]
    fn test_reposition_on_lumi_move() {
        let mut ws = WorkspaceSystem::new();
        let id = ws.show_panel(PanelType::Plan);
        let original_pos = ws.active_panels().iter().find(|p| p.panel_id == id).unwrap().position;

        ws.update_lumi_position(Rect::new(100.0, 100.0, 100.0, 150.0));
        let new_pos = ws.active_panels().iter().find(|p| p.panel_id == id).unwrap().position;

        assert_ne!(original_pos, new_pos);
    }
}
