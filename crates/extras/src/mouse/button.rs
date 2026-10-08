//! Timed Custom Pointer button input.

use bevy::ecs::system::In;
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy_remote::BrpResult;
use bevy_remote::error_codes::INVALID_PARAMS;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::constants::{DEFAULT_MOUSE_DURATION_MS, MAX_MOUSE_DURATION_MS};
use super::cursor::SimulatedCursorPosition;
use super::support::EmptyParamsPolicy;
use super::{pointer, support};
use crate::constants::METHOD_SEND_MOUSE_BUTTON;

#[derive(Deserialize)]
struct SendMouseButtonRequest {
    button: MouseButton,
    #[serde(default)]
    duration_ms: Option<u32>,
    #[serde(default)]
    window: Option<u64>,
}

#[derive(Serialize)]
struct SendMouseButtonResponse {
    button: MouseButton,
    duration_ms: u32,
}

pub(crate) fn send_mouse_button_handler(
    In(params): In<Option<Value>>,
    world: &mut World,
) -> BrpResult {
    let request: SendMouseButtonRequest =
        support::parse_request(params, EmptyParamsPolicy::Reject)?;
    let duration_ms = request.duration_ms.unwrap_or(DEFAULT_MOUSE_DURATION_MS);
    if duration_ms > MAX_MOUSE_DURATION_MS {
        return Err(pointer::error(
            METHOD_SEND_MOUSE_BUTTON,
            INVALID_PARAMS,
            "Duration exceeds maximum 60000ms",
            None,
        ));
    }
    let window = support::resolve_window(world, request.window, METHOD_SEND_MOUSE_BUTTON)?;
    let button = support::pointer_button(request.button, METHOD_SEND_MOUSE_BUTTON)?;
    let position = world
        .resource::<SimulatedCursorPosition>()
        .get_position(window);
    pointer::enqueue_hold(
        world,
        window,
        position,
        button,
        duration_ms,
        METHOD_SEND_MOUSE_BUTTON,
    )?;
    world.resource_mut::<SimulatedCursorPosition>().last_window = Some(window);
    support::serialize_response(
        SendMouseButtonResponse {
            button: request.button,
            duration_ms,
        },
        METHOD_SEND_MOUSE_BUTTON,
    )
}
