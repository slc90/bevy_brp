//! Mouse input wrappers used by the `brp_extras/*_mouse` tools.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

/// Mouse button for BRP operations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub enum MouseButtonWrapper {
    /// Left mouse button
    Left,
    /// Right mouse button
    Right,
    /// Middle mouse button (wheel click)
    Middle,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_button_schema_and_decoder_support_exactly_three_buttons() {
        let schema = serde_json::to_value(schemars::schema_for!(MouseButtonWrapper)).unwrap();
        let buttons: Vec<_> = schema["oneOf"]
            .as_array()
            .expect("Documented enum variants use oneOf")
            .iter()
            .map(|variant| variant["const"].clone())
            .collect();
        assert_eq!(
            buttons,
            vec![
                serde_json::json!("Left"),
                serde_json::json!("Right"),
                serde_json::json!("Middle")
            ]
        );
        assert!(serde_json::from_value::<MouseButtonWrapper>(serde_json::json!("Back")).is_err());
        assert!(
            serde_json::from_value::<MouseButtonWrapper>(serde_json::json!("Forward")).is_err()
        );
    }
}

/// Scroll unit for BRP scroll operations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub enum ScrollUnitWrapper {
    /// Line-based scrolling
    Line,
    /// Pixel-based scrolling
    Pixel,
}
