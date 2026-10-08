//! Cursor position tracking and movement

use std::collections::HashMap;

use bevy::ecs::system::In;
use bevy::math::Vec2;
use bevy::prelude::*;
use bevy_remote::BrpError;
use bevy_remote::BrpResult;
use bevy_remote::error_codes::INVALID_PARAMS;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use super::support;
use super::support::EmptyParamsPolicy;
use crate::constants::METHOD_MOVE_MOUSE;

// ============================================================================
// Types
// ============================================================================

/// Request structure for `move_mouse`
#[derive(Deserialize)]
struct MoveMouseRequest {
    /// Delta movement (mutually exclusive with position)
    #[serde(default)]
    delta: Option<Vec2>,
    /// Absolute position (mutually exclusive with delta)
    #[serde(default)]
    position: Option<Vec2>,
    /// Target window entity (None = primary window)
    #[serde(default)]
    window: Option<u64>,
}

/// Response structure for `move_mouse`
#[derive(Serialize)]
struct MoveMouseResponse {
    /// New cursor position
    new_position: Vec2,
    /// Delta that was applied
    delta: Vec2,
}

// ============================================================================
// Resources
// ============================================================================

/// Tracks cursor position for delta calculation
///
/// This resource maintains the last known cursor position **per window** to enable
/// delta-based movement calculations. When moving by delta, the new position is
/// calculated relative to the stored position for that specific window.
///
/// Physical cursor input never updates this independent position history.
#[derive(Resource, Default)]
pub(super) struct SimulatedCursorPosition {
    /// Per-window cursor positions
    pub positions: HashMap<Entity, Vec2>,
    /// The last window the cursor was moved to (used as default for click/scroll operations)
    pub last_window: Option<Entity>,
}

impl SimulatedCursorPosition {
    /// Get cursor position for window, defaulting to origin if not set
    ///
    /// # Arguments
    /// * `window` - `Window` entity to get position for
    ///
    /// # Returns
    /// Current cursor position or `Vec2::ZERO` if no position stored
    pub(super) fn get_position(&self, window: Entity) -> Vec2 {
        self.positions.get(&window).copied().unwrap_or(Vec2::ZERO)
    }

    /// Update cursor position and return the delta from previous position
    ///
    /// # Arguments
    /// * `window` - `Window` entity to update
    /// * `new_pos` - New cursor position
    ///
    /// # Returns
    /// Delta from previous position (or from origin if no previous position)
    pub(super) fn update_position(&mut self, window: Entity, new_pos: Vec2) -> Vec2 {
        let old_pos = self.get_position(window);
        self.positions.insert(window, new_pos);
        new_pos - old_pos
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// Handler for `move_mouse` BRP method
pub(crate) fn move_mouse_handler(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let request: MoveMouseRequest = support::parse_request(params, EmptyParamsPolicy::Reject)?;

    // Validate that exactly one of delta or position is provided
    if request.delta.is_none() && request.position.is_none() {
        return Err(BrpError {
            code: INVALID_PARAMS,
            message: "Must provide either 'delta' or 'position'".to_string(),
            data: None,
        });
    }

    if request.delta.is_some() && request.position.is_some() {
        return Err(BrpError {
            code: INVALID_PARAMS,
            message: "Cannot provide both 'delta' and 'position'".to_string(),
            data: None,
        });
    }

    // Resolve window entity
    let window = support::resolve_window(world, request.window, METHOD_MOVE_MOUSE)?;

    // Get or create simulated cursor position resource
    if !world.contains_resource::<SimulatedCursorPosition>() {
        world.init_resource::<SimulatedCursorPosition>();
    }

    let cursor_res = world.resource::<SimulatedCursorPosition>();

    // Get current position for this window (default to origin if not set)
    let current_pos = cursor_res.get_position(window);

    // Calculate new position and delta
    let (new_position, delta) = if let Some(delta) = request.delta {
        (current_pos + delta, delta)
    } else if let Some(pos) = request.position {
        (pos, pos - current_pos)
    } else {
        // Validation above already rejects this case
        return Err(BrpError {
            code: INVALID_PARAMS,
            message: "Must provide either 'delta' or 'position'".to_string(),
            data: None,
        });
    };

    if !new_position.is_finite() || !delta.is_finite() {
        return Err(super::pointer::error(
            METHOD_MOVE_MOUSE,
            INVALID_PARAMS,
            "Position and resulting delta must be finite",
            Some(window),
        ));
    }
    super::pointer::enqueue(
        world,
        window,
        new_position,
        bevy::picking::pointer::PointerAction::Move { delta },
        METHOD_MOVE_MOUSE,
    )?;
    let mut cursor_res = world.resource_mut::<SimulatedCursorPosition>();
    cursor_res.positions.insert(window, new_position);
    cursor_res.last_window = Some(window);

    support::serialize_response(
        MoveMouseResponse {
            new_position,
            delta,
        },
        METHOD_MOVE_MOUSE,
    )
}
