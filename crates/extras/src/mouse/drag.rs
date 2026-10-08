//! Ordinary widget drag gestures through Custom Pointer Picking input.

use bevy::ecs::system::In;
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy_remote::BrpResult;
use bevy_remote::error_codes::INVALID_PARAMS;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::constants::MIN_DRAG_FRAMES;
use super::cursor::SimulatedCursorPosition;
use super::support::EmptyParamsPolicy;
use super::{pointer, support};
use crate::constants::METHOD_DRAG_MOUSE;

#[derive(Deserialize)]
struct DragMouseRequest {
    button: MouseButton,
    start: Vec2,
    end: Vec2,
    frames: u32,
    #[serde(default)]
    window: Option<u64>,
}

#[derive(Serialize)]
struct DragMouseResponse {
    button: MouseButton,
    start: Vec2,
    end: Vec2,
    frames: u32,
}

pub(crate) fn drag_mouse_handler(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let request: DragMouseRequest = support::parse_request(params, EmptyParamsPolicy::Reject)?;
    if request.frames < MIN_DRAG_FRAMES {
        return Err(pointer::error(
            METHOD_DRAG_MOUSE,
            INVALID_PARAMS,
            "Frames must be greater than 0",
            None,
        ));
    }
    let window = support::resolve_window(world, request.window, METHOD_DRAG_MOUSE)?;
    let button = support::pointer_button(request.button, METHOD_DRAG_MOUSE)?;
    pointer::enqueue_drag(
        world,
        window,
        request.start,
        request.end,
        button,
        request.frames,
        METHOD_DRAG_MOUSE,
    )?;
    let mut positions = world.resource_mut::<SimulatedCursorPosition>();
    positions.positions.insert(window, request.end);
    positions.last_window = Some(window);
    support::serialize_response(
        DragMouseResponse {
            button: request.button,
            start: request.start,
            end: request.end,
            frames: request.frames,
        },
        METHOD_DRAG_MOUSE,
    )
}
