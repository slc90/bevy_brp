//! Keyboard event creation utilities

use bevy::input::ButtonState;
use bevy::input::keyboard::Key;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::keyboard::NativeKey;
use bevy::prelude::Entity;
use bevy::prelude::With;
use bevy::prelude::World;
use bevy::window::PrimaryWindow;
use bevy::window::Window;
use bevy_remote::BrpError;
use bevy_remote::error_codes::INVALID_PARAMS;
use serde_json::json;

use super::key_code::KeyCodeWrapper;

/// The default window injected keyboard events name: the primary window when
/// the app has one, `Entity::PLACEHOLDER` otherwise.
///
/// Real `winit` events carry their window, and text editors compare it
/// against the window that owns the focused field, so an event with a
/// placeholder window is dropped as text even though `ButtonInput` sees it.
pub(super) fn primary_window_entity(world: &mut World) -> Entity {
    world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap_or(Entity::PLACEHOLDER)
}

/// Resolve the request's window before emitting input or creating pending work.
pub(super) fn resolve_window(
    world: &mut World,
    requested: Option<u64>,
) -> Result<Entity, BrpError> {
    let Some(bits) = requested else {
        return Ok(primary_window_entity(world));
    };
    Entity::try_from_bits(bits)
        .filter(|entity| world.get::<Window>(*entity).is_some())
        .ok_or_else(|| BrpError {
            code: INVALID_PARAMS,
            message: format!("Invalid window {bits}: target does not have a live Window component"),
            data: Some(json!({"window": bits})),
        })
}

/// Create keyboard events from validated key code wrappers, addressed to `window`.
///
/// Uses Bevy logical values for named keys and the default characters for text keys.
pub(super) fn create_keyboard_events(
    wrappers: &[KeyCodeWrapper],
    button_state: ButtonState,
    window: Entity,
) -> Vec<KeyboardInput> {
    create_keyboard_events_with_text(wrappers, button_state, None, window)
}

/// Create keyboard events with an optional target character override.
///
/// An explicit typing character supplies the logical value of the final
/// non-modifier key and its text on press. This preserves shifted characters
/// and literal characters such as a tab requested through `type_text`.
pub(super) fn create_keyboard_events_with_text(
    wrappers: &[KeyCodeWrapper],
    button_state: ButtonState,
    target_char: Option<char>,
    window: Entity,
) -> Vec<KeyboardInput> {
    // Find the last non-modifier key index (that's where we set the text)
    let last_non_modifier_idx = wrappers.iter().rposition(|w| {
        !matches!(
            w,
            KeyCodeWrapper::ShiftLeft
                | KeyCodeWrapper::ShiftRight
                | KeyCodeWrapper::ControlLeft
                | KeyCodeWrapper::ControlRight
                | KeyCodeWrapper::AltLeft
                | KeyCodeWrapper::AltRight
                | KeyCodeWrapper::SuperLeft
                | KeyCodeWrapper::SuperRight
        )
    });

    wrappers
        .iter()
        .enumerate()
        .map(|(idx, &wrapper)| {
            let key_code = wrapper.to_key_code();
            let is_target_key = Some(idx) == last_non_modifier_idx;

            let character_override = target_char.filter(|_| is_target_key);
            let character = character_override
                .or_else(|| wrapper.to_char())
                .map(|c| c.to_string().into());
            let named_key = if character_override.is_none() {
                wrapper.named_key()
            } else {
                None
            };
            let logical_key = named_key
                .or_else(|| character.as_ref().cloned().map(Key::Character))
                .unwrap_or(Key::Unidentified(NativeKey::Unidentified));
            let text = if button_state == ButtonState::Pressed && is_target_key {
                character
            } else {
                None
            };

            KeyboardInput {
                state: button_state,
                key_code,
                logical_key,
                window,
                repeat: false,
                text,
            }
        })
        .collect()
}
