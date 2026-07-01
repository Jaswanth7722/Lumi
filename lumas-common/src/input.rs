//! # Input System — User Input Event Model (Chapter 21)
//!
//! Defines the unified input event model for mouse, keyboard, touch, voice,
//! and API-triggered interactions with the Lumas character.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Input Source
// ---------------------------------------------------------------------------

/// The source device or channel for an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputSource {
    Mouse,
    Keyboard,
    Touch,
    Voice,
    Api,
}

// ---------------------------------------------------------------------------
// Input Event Types
// ---------------------------------------------------------------------------

/// The specific type of input event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputEventType {
    /// Left-click on the character body.
    CharacterClick,
    /// Right-click on the character body.
    CharacterRightClick,
    /// Beginning of a character drag.
    CharacterDragStart,
    /// End of a character drag.
    CharacterDragEnd,
    /// Text submitted via the conversation input.
    TextSubmitted { text: String },
    /// Voice activation triggered.
    VoiceActivated,
    /// Global hotkey pressed.
    Hotkey { keys: String },
    /// Interaction with a workspace panel element.
    WorkspacePanelInteraction { panel_id: String, action: String },
}

/// A screen point.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// A complete input event with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputEvent {
    pub id: String,
    pub source: InputSource,
    pub event_type: InputEventType,
    pub timestamp: i64,
    pub screen_position: Option<Point>,
}

// ---------------------------------------------------------------------------
// Mouse Buttons
// ---------------------------------------------------------------------------

/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

// ---------------------------------------------------------------------------
// Drag State
// ---------------------------------------------------------------------------

/// State tracking during a character drag operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DragState {
    /// Offset from the character's origin where the drag started.
    pub offset: (f32, f32),
    /// Whether the drag is currently active.
    pub active: bool,
}

// ---------------------------------------------------------------------------
// Hotkey Definitions
// ---------------------------------------------------------------------------

/// Global hotkey mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyBinding {
    pub id: String,
    pub description: String,
    pub default_keys: String,
    pub action: HotkeyAction,
}

/// Actions that can be bound to hotkeys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HotkeyAction {
    ToggleConversation,
    ToggleVoice,
    ToggleVisibility,
    ToggleFocusMode,
    DismissPanel,
    CancelVoice,
}

/// Returns the default hotkey bindings.
pub fn default_hotkey_bindings() -> Vec<HotkeyBinding> {
    vec![
        HotkeyBinding {
            id: "toggle_conversation".into(),
            description: "Toggle conversation input".into(),
            default_keys: "Ctrl+Shift+L".into(),
            action: HotkeyAction::ToggleConversation,
        },
        HotkeyBinding {
            id: "toggle_voice".into(),
            description: "Toggle voice input".into(),
            default_keys: "Ctrl+Shift+M".into(),
            action: HotkeyAction::ToggleVoice,
        },
        HotkeyBinding {
            id: "toggle_visibility".into(),
            description: "Show or hide Lumi".into(),
            default_keys: "Ctrl+Shift+H".into(),
            action: HotkeyAction::ToggleVisibility,
        },
        HotkeyBinding {
            id: "toggle_focus_mode".into(),
            description: "Toggle focus/do-not-disturb mode".into(),
            default_keys: "Ctrl+Shift+F".into(),
            action: HotkeyAction::ToggleFocusMode,
        },
        HotkeyBinding {
            id: "dismiss_panel".into(),
            description: "Dismiss the active workspace panel".into(),
            default_keys: "Escape".into(),
            action: HotkeyAction::DismissPanel,
        },
    ]
}

impl InputEvent {
    /// Create a new input event.
    pub fn new(source: InputSource, event_type: InputEventType, position: Option<Point>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            source,
            event_type,
            timestamp: chrono::Utc::now().timestamp_millis(),
            screen_position: position,
        }
    }

    /// Create a character click event.
    pub fn character_click(x: f32, y: f32) -> Self {
        Self::new(
            InputSource::Mouse,
            InputEventType::CharacterClick,
            Some(Point { x, y }),
        )
    }

    /// Create a text submitted event.
    pub fn text_submitted(text: impl Into<String>) -> Self {
        Self::new(
            InputSource::Keyboard,
            InputEventType::TextSubmitted { text: text.into() },
            None,
        )
    }

    /// Create a hotkey event.
    pub fn hotkey(keys: impl Into<String>) -> Self {
        Self::new(
            InputSource::Keyboard,
            InputEventType::Hotkey { keys: keys.into() },
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_event_creation() {
        let event = InputEvent::character_click(100.0, 200.0);
        assert_eq!(event.source, InputSource::Mouse);
        assert!(matches!(event.event_type, InputEventType::CharacterClick));
        assert!(event.screen_position.is_some());
    }

    #[test]
    fn test_text_submitted_event() {
        let event = InputEvent::text_submitted("Hello Lumi");
        assert_eq!(event.source, InputSource::Keyboard);
        match event.event_type {
            InputEventType::TextSubmitted { ref text } => assert_eq!(text, "Hello Lumi"),
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_hotkey_bindings() {
        let bindings = default_hotkey_bindings();
        assert_eq!(bindings.len(), 5);
        assert!(
            bindings
                .iter()
                .any(|b| b.action == HotkeyAction::ToggleConversation)
        );
        assert!(
            bindings
                .iter()
                .any(|b| b.action == HotkeyAction::DismissPanel)
        );
    }

    #[test]
    fn test_drag_state() {
        let drag = DragState {
            offset: (10.0, 20.0),
            active: true,
        };
        assert!(drag.active);
        assert_eq!(drag.offset.0, 10.0);
    }
}
