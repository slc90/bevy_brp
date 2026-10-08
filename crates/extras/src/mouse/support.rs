//! Shared helper functions for mouse input simulation

use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_remote::BrpError;
use bevy_remote::BrpResult;
use bevy_remote::error_codes::INTERNAL_ERROR;
use bevy_remote::error_codes::INVALID_PARAMS;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

use super::cursor::SimulatedCursorPosition;
use crate::constants::MISSING_REQUEST_PARAMETERS_MESSAGE;

/// Whether `parse_request` should accept `None` params by treating them as an empty object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EmptyParamsPolicy {
    Allow,
    Reject,
}

/// Parse BRP request parameters into strongly typed request struct
///
/// Handles parameter extraction, validation, and error conversion for all handlers.
/// Provides consistent error messages across the module.
///
/// # Arguments
/// * `params` - Optional JSON value from BRP request
/// * `empty_params_policy` - `Allow` permits None params (creates empty object for
///   deserialization); `Reject` returns an error when params is None
///
/// # Returns
/// Parsed request struct or BRP error with `INVALID_PARAMS` code
pub(super) fn parse_request<T: serde::de::DeserializeOwned>(
    params: Option<Value>,
    empty_params_policy: EmptyParamsPolicy,
) -> Result<T, BrpError> {
    if matches!(empty_params_policy, EmptyParamsPolicy::Allow) && params.is_none() {
        // For requests with no required fields (e.g., `DoubleTapGestureRequest`)
        return serde_json::from_value(Value::Object(Map::default())).map_err(|e| BrpError {
            code: INVALID_PARAMS,
            message: format!("Failed to parse parameters: {e}"),
            data: None,
        });
    }

    let params = params.ok_or_else(|| BrpError {
        code: INVALID_PARAMS,
        message: MISSING_REQUEST_PARAMETERS_MESSAGE.to_string(),
        data: None,
    })?;

    serde_json::from_value(params).map_err(|e| BrpError {
        code: INVALID_PARAMS,
        message: format!("Failed to parse parameters: {e}"),
        data: None,
    })
}

/// Serialize BRP response with standardized error handling
///
/// Provides consistent serialization error handling and logging across all handlers.
///
/// # Arguments
/// * `response` - Response struct to serialize
/// * `handler_name` - Name of the handler (for logging)
///
/// # Returns
/// Serialized JSON value or BRP error with `INTERNAL_ERROR` code
pub(super) fn serialize_response<T: Serialize>(response: T, handler_name: &str) -> BrpResult {
    serde_json::to_value(response).map_err(|e| {
        warn!("Failed to serialize {handler_name} response: {e}");
        BrpError {
            code: INTERNAL_ERROR,
            message: format!("Failed to serialize response: {e}"),
            data: None,
        }
    })
}

/// Resolve window entity from optional u64 ID
///
/// Resolution order when `window_id` is None:
/// 1. Last window the cursor was moved to (from `SimulatedCursorPosition`)
/// 2. Primary window (fallback)
pub(super) fn resolve_window(
    world: &mut World,
    window_id: Option<u64>,
    method: &str,
) -> Result<Entity, BrpError> {
    if let Some(id) = window_id {
        let entity = Entity::try_from_bits(id).ok_or_else(|| {
            super::pointer::error(method, INVALID_PARAMS, "Invalid window entity bits", None)
        })?;
        // Verify entity exists and is a window
        if world.get::<Window>(entity).is_none() {
            return Err(super::pointer::error(
                method,
                INVALID_PARAMS,
                &format!("Invalid window entity: {id}"),
                Some(entity),
            ));
        }
        return Ok(entity);
    }

    // Default to the last window the cursor was moved to
    if let Some(cursor_pos) = world.get_resource::<SimulatedCursorPosition>()
        && let Some(last_window) = cursor_pos.last_window
    {
        if world.get::<Window>(last_window).is_none() {
            return Err(super::pointer::error(
                method,
                INVALID_PARAMS,
                "Last BRP target window was destroyed",
                Some(last_window),
            ));
        }
        return Ok(last_window);
    }

    // Fall back to primary window
    let entity = {
        let mut query = world.query_filtered::<Entity, (With<PrimaryWindow>, With<Window>)>();
        let mut iter = query.iter(world);
        iter.next()
    };

    entity.ok_or_else(|| {
        super::pointer::error(method, INVALID_PARAMS, "No primary window found", None)
    })
}

pub(super) fn pointer_button(
    button: MouseButton,
    method: &str,
) -> Result<bevy::picking::pointer::PointerButton, BrpError> {
    use bevy::picking::pointer::PointerButton;
    match button {
        MouseButton::Left => Ok(PointerButton::Primary),
        MouseButton::Right => Ok(PointerButton::Secondary),
        MouseButton::Middle => Ok(PointerButton::Middle),
        _ => Err(super::pointer::error(
            method,
            INVALID_PARAMS,
            "Custom Pointer supports only Left, Right and Middle",
            None,
        )),
    }
}
