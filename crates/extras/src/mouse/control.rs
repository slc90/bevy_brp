//! Queue-independent pointer status and cancellation.

use bevy::prelude::*;
use bevy_remote::BrpResult;
use bevy_remote::error_codes::INVALID_PARAMS;
use serde::Deserialize;
use serde_json::Value;

use super::{pointer, support};
use crate::constants::METHOD_POINTER_CONTROL;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    action: Action,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Action {
    Status,
    Release,
}

pub(crate) fn pointer_control_handler(
    In(params): In<Option<Value>>,
    world: &mut World,
) -> BrpResult {
    if !params.as_ref().is_some_and(|params| {
        params.is_object() && params.get("action").is_some_and(Value::is_string)
    }) {
        return Err(pointer::error(
            METHOD_POINTER_CONTROL,
            INVALID_PARAMS,
            "Expected an object with a string action (status or release)",
            None,
        ));
    }
    let request: Request = support::parse_request(params, support::EmptyParamsPolicy::Reject)
        .map_err(|error| {
            pointer::error(METHOD_POINTER_CONTROL, INVALID_PARAMS, &error.message, None)
        })?;
    if matches!(request.action, Action::Release) {
        pointer::release(world);
    }
    support::serialize_response(
        world.resource::<pointer::BrpPointerState>().snapshot(),
        METHOD_POINTER_CONTROL,
    )
}
