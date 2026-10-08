//! Mouse scroll wheel events

use bevy::ecs::system::In;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::touch::TouchPhase;
use bevy::picking::pointer::PointerAction;
use bevy::prelude::*;
use bevy_remote::BrpResult;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use super::support;
use super::support::EmptyParamsPolicy;
use crate::constants::METHOD_SCROLL_MOUSE;

// ============================================================================
// Types
// ============================================================================

/// Request structure for `scroll_mouse`
#[derive(Deserialize)]
struct ScrollMouseRequest {
    /// Horizontal scroll amount
    x: f32,
    /// Vertical scroll amount
    y: f32,
    /// Scroll unit
    unit: MouseScrollUnit,
    /// Target window entity (None = primary window)
    #[serde(default)]
    window: Option<u64>,
}

/// Response structure for `scroll_mouse`
#[derive(Serialize)]
struct ScrollMouseResponse {
    /// Horizontal scroll amount
    x: f32,
    /// Vertical scroll amount
    y: f32,
    /// Scroll unit that was used
    unit: MouseScrollUnit,
}

// ============================================================================
// Handlers
// ============================================================================

/// Handler for `scroll_mouse` BRP method
pub(crate) fn scroll_mouse_handler(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let request: ScrollMouseRequest = support::parse_request(params, EmptyParamsPolicy::Reject)?;
    let window = support::resolve_window(world, request.window, METHOD_SCROLL_MOUSE)?;

    if !request.x.is_finite() || !request.y.is_finite() {
        return Err(super::pointer::error(
            METHOD_SCROLL_MOUSE,
            bevy_remote::error_codes::INVALID_PARAMS,
            "Scroll values must be finite",
            Some(window),
        ));
    }
    let position = world
        .resource::<super::cursor::SimulatedCursorPosition>()
        .get_position(window);
    super::pointer::enqueue(
        world,
        window,
        position,
        PointerAction::Scroll {
            unit: request.unit,
            x: request.x,
            y: request.y,
            phase: TouchPhase::Moved,
        },
        METHOD_SCROLL_MOUSE,
    )?;
    world
        .resource_mut::<super::cursor::SimulatedCursorPosition>()
        .last_window = Some(window);

    support::serialize_response(
        ScrollMouseResponse {
            x: request.x,
            y: request.y,
            unit: request.unit,
        },
        METHOD_SCROLL_MOUSE,
    )
}
