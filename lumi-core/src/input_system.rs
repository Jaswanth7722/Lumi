//! # Input System — User Input Handling (Chapter 21)
//!
//! Receives, classifies, and routes all user input directed at Lumi.

use lumi_common::input::{
    DragState, HotkeyAction, HotkeyBinding, InputEvent, InputEventType, InputSource, MouseButton, Point,
    default_hotkey_bindings,
};
use lumi_common::position::PositionTarget;
use lumi_common::state_machine::StateEvent;
use std::collections::HashMap;
use tracing::debug;

/// Configuration for the Input System.
pub struct InputConfig {
    pub hit_threshold: u8,
    pub drag_enabled: bool,
    pub hotkeys_enabled: bool,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            hit_threshold: 64,
            drag_enabled: true,
            hotkeys_enabled: true,
        }
    }
}

/// The Input System processes all user interactions with Lumi.
pub struct InputSystem {
    drag_state: Option<DragState>,
    config: InputConfig,
    current_position: Point,
    last_click_position: Option<Point>,
    hotkey_bindings: HashMap<String, HotkeyBinding>,
}

impl InputSystem {
    pub fn new() -> Self {
        let mut bindings = HashMap::new();
        for binding in default_hotkey_bindings() {
            bindings.insert(binding.id.clone(), binding);
        }
        Self {
            drag_state: None,
            config: InputConfig::default(),
            current_position: Point { x: 0.0, y: 0.0 },
            last_click_position: None,
            hotkey_bindings: bindings,
        }
    }

    pub fn handle_click(&mut self, screen_x: i32, screen_y: i32, _button: MouseButton) -> Option<StateEvent> {
        if !self.test_hit(screen_x, screen_y) {
            return None;
        }
        self.last_click_position = Some(Point { x: screen_x as f32, y: screen_y as f32 });
        Some(StateEvent::UserClick)
    }

    pub fn handle_drag_start(&mut self, x: f32, y: f32) -> bool {
        if !self.test_hit(x as i32, y as i32) || !self.config.drag_enabled {
            return false;
        }
        let offset = (x - self.current_position.x, y - self.current_position.y);
        self.drag_state = Some(DragState { offset, active: true });
        debug!("Drag started at ({}, {})", x, y);
        true
    }

    pub fn handle_drag_move(&mut self, x: f32, y: f32) -> Option<PositionTarget> {
        if let Some(drag) = &self.drag_state {
            if !drag.active {
                return None;
            }
            let new_x = x - drag.offset.0;
            let new_y = y - drag.offset.1;
            self.current_position = Point { x: new_x, y: new_y };
            Some(PositionTarget::Absolute { x: new_x, y: new_y })
        } else {
            None
        }
    }

    pub fn handle_drag_end(&mut self) -> Option<PositionTarget> {
        if let Some(drag) = &self.drag_state {
            if !drag.active {
                return None;
            }
            self.drag_state = None;
            Some(PositionTarget::Preserve)
        } else {
            None
        }
    }

    pub fn handle_hotkey(&self, key: &str) -> Option<HotkeyAction> {
        for binding in self.hotkey_bindings.values() {
            if binding.default_keys == key {
                return Some(binding.action.clone());
            }
        }
        None
    }

    pub fn handle_text_input(&self, text: &str) -> StateEvent {
        StateEvent::UserInput {
            source: lumi_common::state_machine::InputSource::Keyboard,
            content: text.to_string(),
        }
    }

    pub fn handle_voice_activation(&self, confidence: f32) -> StateEvent {
        StateEvent::WakeWord { confidence }
    }

    pub fn update_position(&mut self, x: f32, y: f32) {
        self.current_position = Point { x, y };
    }

    pub fn is_dragging(&self) -> bool {
        self.drag_state.as_ref().map_or(false, |d| d.active)
    }

    pub fn last_click_position(&self) -> Option<Point> {
        self.last_click_position
    }

    pub fn registered_hotkeys(&self) -> &HashMap<String, HotkeyBinding> {
        &self.hotkey_bindings
    }

    fn test_hit(&self, screen_x: i32, screen_y: i32) -> bool {
        let cx = self.current_position.x as i32;
        let cy = self.current_position.y as i32;
        screen_x >= cx && screen_x <= cx + 100 && screen_y >= cy && screen_y <= cy + 150
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_detection() {
        let system = InputSystem::new();
        assert!(system.test_hit(50, 50));
        assert!(!system.test_hit(200, 200));
    }

    #[test]
    fn test_click() {
        let mut system = InputSystem::new();
        assert!(system.handle_click(50, 50, MouseButton::Left).is_some());
        assert!(system.handle_click(500, 500, MouseButton::Left).is_none());
    }

    #[test]
    fn test_drag() {
        let mut system = InputSystem::new();
        assert!(system.handle_drag_start(50.0, 50.0));
        assert!(system.is_dragging());
        assert!(system.handle_drag_move(100.0, 100.0).is_some());
        assert!(system.handle_drag_end().is_some());
        assert!(!system.is_dragging());
    }

    #[test]
    fn test_hotkeys() {
        let system = InputSystem::new();
        // Look up by action ID instead of key binding string
        let bindings = system.registered_hotkeys();
        assert!(bindings.contains_key("toggle_conversation"));
        assert!(bindings.contains_key("toggle_voice"));
    }
}
