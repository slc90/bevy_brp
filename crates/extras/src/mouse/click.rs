//! Click gestures sharing the Custom Pointer FIFO.

use bevy::ecs::system::In;
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy_remote::BrpResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::constants::DEFAULT_DOUBLE_CLICK_DELAY_MS;
use super::cursor::SimulatedCursorPosition;
use super::support::EmptyParamsPolicy;
use super::{pointer, support};
use crate::constants::{METHOD_CLICK_MOUSE, METHOD_DOUBLE_CLICK_MOUSE};

#[derive(Deserialize)]
struct ClickMouseRequest {
    button: MouseButton,
    #[serde(default)]
    window: Option<u64>,
}

#[derive(Serialize)]
struct ClickMouseResponse {
    button: MouseButton,
}

#[derive(Deserialize)]
struct DoubleClickMouseRequest {
    button: MouseButton,
    #[serde(default)]
    delay_ms: Option<u32>,
    #[serde(default)]
    window: Option<u64>,
}

#[derive(Serialize)]
struct DoubleClickMouseResponse {
    button: MouseButton,
    delay_ms: u32,
}

pub(crate) fn click_mouse_handler(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let request: ClickMouseRequest = support::parse_request(params, EmptyParamsPolicy::Reject)?;
    let window = support::resolve_window(world, request.window, METHOD_CLICK_MOUSE)?;
    let button = support::pointer_button(request.button, METHOD_CLICK_MOUSE)?;
    let position = world
        .resource::<SimulatedCursorPosition>()
        .get_position(window);
    pointer::enqueue_click(world, window, position, button, None, METHOD_CLICK_MOUSE)?;
    world.resource_mut::<SimulatedCursorPosition>().last_window = Some(window);
    support::serialize_response(
        ClickMouseResponse {
            button: request.button,
        },
        METHOD_CLICK_MOUSE,
    )
}

pub(crate) fn double_click_mouse_handler(
    In(params): In<Option<Value>>,
    world: &mut World,
) -> BrpResult {
    let request: DoubleClickMouseRequest =
        support::parse_request(params, EmptyParamsPolicy::Reject)?;
    let delay_ms = request.delay_ms.unwrap_or(DEFAULT_DOUBLE_CLICK_DELAY_MS);
    let window = support::resolve_window(world, request.window, METHOD_DOUBLE_CLICK_MOUSE)?;
    let button = support::pointer_button(request.button, METHOD_DOUBLE_CLICK_MOUSE)?;
    let position = world
        .resource::<SimulatedCursorPosition>()
        .get_position(window);
    pointer::enqueue_click(
        world,
        window,
        position,
        button,
        Some(delay_ms),
        METHOD_DOUBLE_CLICK_MOUSE,
    )?;
    world.resource_mut::<SimulatedCursorPosition>().last_window = Some(window);
    support::serialize_response(
        DoubleClickMouseResponse {
            button: request.button,
            delay_ms,
        },
        METHOD_DOUBLE_CLICK_MOUSE,
    )
}
