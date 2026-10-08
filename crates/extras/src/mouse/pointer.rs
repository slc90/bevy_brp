//! App-owned pointer input and handover lifecycle.

use std::collections::{HashSet, VecDeque};

use bevy::camera::RenderTarget;
use bevy::ecs::message::MessageCursor;
use bevy::picking::backend::HitData;
use bevy::picking::events::{Cancel, DragEnd, DragStart, Pointer, Press, Release};
use bevy::picking::hover::{HoverMap, PreviousHoverMap};
use bevy::picking::pointer::{
    Location, PointerAction, PointerButton, PointerId, PointerInput, PointerLocation, PointerMap,
    PointerPress,
};
use bevy::picking::{PickingSettings, PickingSystems};
use bevy::prelude::*;
use bevy::window::{WindowEvent, WindowRef};
use bevy_remote::BrpError;
use bevy_remote::error_codes::{INTERNAL_ERROR, INVALID_PARAMS};
use serde::Serialize;

use crate::activity::{self, BrpExtrasActivityGuard};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Phase {
    #[default]
    Inactive,
    Active,
    Draining,
}

#[derive(Clone, Serialize)]
pub(super) struct PointerError {
    pub generation: u64,
    pub method: String,
    pub window: Option<u64>,
    pub code: i16,
    pub message: String,
}

struct QueuedInput {
    generation: u64,
    window: Entity,
    position: Vec2,
    action: PointerAction,
    method: String,
}

struct CancelResponsibility {
    generation: u64,
    target: Entity,
    button: PointerButton,
    location: Location,
    hit: HitData,
}

#[derive(Default)]
enum Drain {
    #[default]
    None,
    Pending,
    Emitted,
    Converging,
}

#[derive(Resource, Default)]
pub(super) struct BrpPointerState {
    pub phase: Phase,
    pub id: Option<PointerId>,
    pub entity: Option<Entity>,
    pub window: Option<Entity>,
    pub generation: u64,
    pub pressed: Vec<PointerButton>,
    pub last_error: Option<PointerError>,
    queue: VecDeque<QueuedInput>,
    activity: Option<BrpExtrasActivityGuard>,
    drain: Drain,
    cancelled_generation: u64,
    physical_location: Option<Location>,
    left_windows: HashSet<Entity>,
    input_reader: MessageCursor<PointerInput>,
    window_reader: MessageCursor<WindowEvent>,
    press_reader: MessageCursor<Pointer<Press>>,
    release_reader: MessageCursor<Pointer<Release>>,
    drag_reader: MessageCursor<Pointer<DragStart>>,
    drag_end_reader: MessageCursor<Pointer<DragEnd>>,
    cancel_reader: MessageCursor<Pointer<Cancel>>,
    responsibilities: Vec<CancelResponsibility>,
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<BrpPointerState>()
        .add_systems(
            First,
            produce
                .in_set(PickingSystems::Input)
                .after(bevy::picking::input::mouse_pick_events),
        )
        .add_systems(
            PreUpdate,
            suppress_physical
                .after(PointerInput::receive)
                .before(bevy::picking::backend::ray::RayMap::repopulate)
                .before(PickingSystems::Backend),
        )
        .add_systems(
            PreUpdate,
            prepare_physical_out
                .after(PickingSystems::Backend)
                .before(PickingSystems::Hover),
        )
        .add_systems(
            PreUpdate,
            finish_cycle
                .after(PickingSystems::Hover)
                .in_set(PickingSystems::Last),
        );
}

fn location(window: Entity, position: Vec2) -> Option<Location> {
    RenderTarget::Window(WindowRef::Entity(window))
        .normalize(None)
        .map(|target| Location { target, position })
}

pub(super) fn error(method: &str, code: i16, message: &str, window: Option<Entity>) -> BrpError {
    BrpError {
        code,
        message: message.to_owned(),
        data: Some(
            serde_json::json!({"method": format!("brp_extras/{method}"), "window": window.map(Entity::to_bits)}),
        ),
    }
}

fn capability(world: &World, method: &str, window: Entity) -> Result<(), BrpError> {
    let enabled = world
        .get_resource::<PickingSettings>()
        .is_some_and(|settings| {
            settings.is_enabled && settings.is_input_enabled && settings.is_hover_enabled
        });
    if !enabled
        || !world.contains_resource::<PointerMap>()
        || !world.contains_resource::<HoverMap>()
        || !world.contains_resource::<PreviousHoverMap>()
        || !world.contains_resource::<Messages<Pointer<Press>>>()
        || !world.contains_resource::<Messages<PointerInput>>()
    {
        return Err(error(
            method,
            INTERNAL_ERROR,
            "Pointer input requires enabled PickingPlugin and InteractionPlugin; the host must supply a picking backend",
            Some(window),
        ));
    }
    Ok(())
}

pub(super) fn enqueue(
    world: &mut World,
    window: Entity,
    position: Vec2,
    action: PointerAction,
    method: &str,
) -> Result<(), BrpError> {
    capability(world, method, window)?;
    if world.get::<Window>(window).is_none() || !position.is_finite() {
        return Err(error(
            method,
            INVALID_PARAMS,
            "Expected a live Window and finite logical coordinates",
            Some(window),
        ));
    }
    world.init_resource::<BrpPointerState>();
    world.resource_scope(|world, mut state: Mut<BrpPointerState>| {
        if state.phase == Phase::Draining {
            return Err(error(
                method,
                INVALID_PARAMS,
                "Pointer is draining; wait for inactive before submitting input",
                Some(window),
            ));
        }
        if state.phase == Phase::Inactive {
            let mut physical = world.query::<(&PointerId, &PointerLocation, &PointerPress)>();
            for (id, location, press) in physical.iter(world) {
                if *id == PointerId::Mouse {
                    if press.is_any_pressed() {
                        return Err(error(
                            method,
                            INVALID_PARAMS,
                            "Physical mouse still owns a pressed button",
                            Some(window),
                        ));
                    }
                    state.physical_location = location.location.clone();
                }
            }
            let id = *state
                .id
                .get_or_insert_with(|| PointerId::Custom(uuid::Uuid::new_v4()));
            if state
                .entity
                .is_none_or(|entity| world.get::<PointerId>(entity).is_none())
            {
                state.entity = Some(world.spawn(id).id());
            }
            state.generation = state.generation.wrapping_add(1);
            state.phase = Phase::Active;
            state.last_error = None;
        }
        state.window = Some(window);
        let generation = state.generation;
        state.queue.push_back(QueuedInput {
            generation,
            window,
            position,
            action,
            method: method.to_owned(),
        });
        if state.activity.is_none() {
            state.activity = Some(activity::begin(world));
        }
        Ok(())
    })
}

#[cfg(test)]
fn release(world: &mut World) {
    world.resource_scope(|world, mut state: Mut<BrpPointerState>| begin_drain(world, &mut state));
}

fn begin_drain(world: &mut World, state: &mut BrpPointerState) {
    if state.phase != Phase::Active {
        return;
    }
    state.cancelled_generation = state.generation;
    state.generation = state.generation.wrapping_add(1);
    state.queue.clear();
    state.phase = Phase::Draining;
    state.drain = Drain::Pending;
    if state.activity.is_none() {
        state.activity = Some(activity::begin(world));
    }
}

fn valid_location(world: &World, location: &Location, left_windows: &HashSet<Entity>) -> bool {
    match location.target {
        bevy::camera::NormalizedRenderTarget::Window(window) => {
            world.get::<Window>(window.entity()).is_some()
                && !left_windows.contains(&window.entity())
        }
        _ => false,
    }
}

fn restore_physical(world: &mut World, state: &BrpPointerState, read_native: bool) {
    let saved = state
        .physical_location
        .as_ref()
        .filter(|location| valid_location(world, location, &state.left_windows));
    let restored = saved.map(|saved| {
        let bevy::camera::NormalizedRenderTarget::Window(window) = saved.target else {
            return saved.clone();
        };
        let mut restored = saved.clone();
        if read_native
            && let Some(position) = world
                .get::<Window>(window.entity())
                .and_then(Window::cursor_position)
        {
            restored.position = position;
        }
        restored
    });
    let mut query = world.query::<(&PointerId, &mut PointerLocation)>();
    for (id, mut location) in query.iter_mut(world) {
        if *id == PointerId::Mouse {
            location.location = restored.clone();
        }
    }
}

fn produce(world: &mut World) {
    if !world.contains_resource::<Messages<PointerInput>>() {
        return;
    }
    world.resource_scope(|world, mut state: Mut<BrpPointerState>| {
        let mut exited_this_cycle = HashSet::new();
        if let Some(messages) = world.get_resource::<Messages<WindowEvent>>() {
            let events: Vec<_> = state.window_reader.read(messages).cloned().collect();
            for event in events {
                match event {
                    WindowEvent::CursorLeft(event) => { state.left_windows.insert(event.window); exited_this_cycle.insert(event.window); }
                    WindowEvent::CursorMoved(event) => { state.left_windows.remove(&event.window); exited_this_cycle.remove(&event.window); }
                    _ => {}
                }
            }
        }
        let inputs: Vec<_> = state.input_reader.read(world.resource::<Messages<PointerInput>>()).filter(|input| input.pointer_id == PointerId::Mouse).cloned().collect();
        let mut handover = false;
        for input in inputs {
            if matches!(input.action, PointerAction::Move { .. } | PointerAction::Press(_) | PointerAction::Scroll { .. }) {
                if let bevy::camera::NormalizedRenderTarget::Window(window) = input.location.target && !exited_this_cycle.contains(&window.entity()) { state.left_windows.remove(&window.entity()); }
                state.physical_location = Some(input.location);
                handover = true;
            }
        }
        if handover && state.phase == Phase::Active { begin_drain(world, &mut state); }
        if handover || (state.phase == Phase::Draining && matches!(state.drain, Drain::Pending)) { restore_physical(world, &state, !handover); }
        if state.phase == Phase::Active && state.window.is_some_and(|window| world.get::<Window>(window).is_none()) {
            let method = state.queue.front().map_or("move_mouse", |input| input.method.as_str()).to_owned();
            state.last_error = Some(PointerError { generation: state.generation, method: format!("brp_extras/{method}"), window: state.window.map(Entity::to_bits), code: INVALID_PARAMS, message: "Target window was destroyed".to_owned() });
            warn!(method, window = ?state.window, "BRP pointer target window was destroyed");
            begin_drain(world, &mut state);
            restore_physical(world, &state, true);
        }
        if state.phase == Phase::Draining {
            if matches!(state.drain, Drain::Pending) {
                if let (Some(id), Some(entity)) = (state.id, state.entity)
                    && let Some(location) = world.get::<PointerLocation>(entity).and_then(|location| location.location.clone()) {
                    world.write_message(PointerInput::new(id, location, PointerAction::Cancel));
                }
                state.drain = Drain::Emitted;
            }
            return;
        }
        if let Some(input) = state.queue.pop_front() {
            if input.generation != state.generation { return; }
            if world.get::<Window>(input.window).is_none() {
                state.last_error = Some(PointerError { generation: input.generation, method: format!("brp_extras/{}", input.method), window: Some(input.window.to_bits()), code: INVALID_PARAMS, message: "Queued target window was destroyed".to_owned() });
                warn!(method = input.method, window = %input.window, "BRP queued pointer target was destroyed");
                begin_drain(world, &mut state);
                return;
            }
            if let Some(id) = state.id {
                match input.action {
                    PointerAction::Press(button) => { if !state.pressed.contains(&button) { state.pressed.push(button); } }
                    PointerAction::Release(button) => state.pressed.retain(|pressed| *pressed != button),
                    _ => {}
                }
                if let Some(location) = location(input.window, input.position) {
                    world.write_message(PointerInput::new(id, location, input.action));
                } else {
                    error!(window = %input.window, "Explicit pointer window could not normalize");
                    begin_drain(world, &mut state);
                }
            }
        }
    });
}

fn suppress_physical(world: &mut World) {
    let state = world.resource::<BrpPointerState>();
    let active = state.phase == Phase::Active;
    let left_windows = state.left_windows.clone();
    let invalid: HashSet<_> = {
        let mut query = world.query::<(&PointerId, &PointerLocation)>();
        query
            .iter(world)
            .filter_map(|(id, location)| {
                (*id == PointerId::Mouse
                    && location
                        .location
                        .as_ref()
                        .is_some_and(|location| !valid_location(world, location, &left_windows)))
                .then_some(*id)
            })
            .collect()
    };
    let mut query = world.query::<(&PointerId, &mut PointerLocation)>();
    for (id, mut location) in query.iter_mut(world) {
        if *id == PointerId::Mouse && (active || invalid.contains(id)) {
            location.location = None;
        }
    }
}

fn prepare_physical_out(world: &mut World) {
    // Backends have seen an inactive Mouse; event dispatch still needs its old location for Out.
    world.resource_scope(|world, state: Mut<BrpPointerState>| {
        if state.phase == Phase::Active {
            restore_physical(world, &state, false);
        }
    });
}

fn finish_cycle(world: &mut World) {
    if !world.contains_resource::<Messages<Pointer<Press>>>() {
        return;
    }
    world.resource_scope(|world, mut state: Mut<BrpPointerState>| {
        let Some(id) = state.id else {
            return;
        };
        let generation = if state.phase == Phase::Draining {
            state.cancelled_generation
        } else {
            state.generation
        };
        let presses: Vec<_> = state
            .press_reader
            .read(world.resource::<Messages<Pointer<Press>>>())
            .filter(|event| event.pointer_id == id)
            .cloned()
            .collect();
        for event in presses {
            state.responsibilities.push(CancelResponsibility {
                generation,
                target: event.entity,
                button: event.button,
                hit: event.hit.clone(),
                location: event.pointer_location,
            });
        }
        let drags: Vec<_> = state
            .drag_reader
            .read(world.resource::<Messages<Pointer<DragStart>>>())
            .filter(|event| event.pointer_id == id)
            .cloned()
            .collect();
        for event in drags {
            state.responsibilities.push(CancelResponsibility {
                generation,
                target: event.entity,
                button: event.button,
                hit: event.hit.clone(),
                location: event.pointer_location,
            });
        }
        let releases: Vec<_> = state
            .release_reader
            .read(world.resource::<Messages<Pointer<Release>>>())
            .filter(|event| event.pointer_id == id)
            .map(|event| event.button)
            .collect();
        let ends: Vec<_> = state
            .drag_end_reader
            .read(world.resource::<Messages<Pointer<DragEnd>>>())
            .filter(|event| event.pointer_id == id)
            .map(|event| event.button)
            .collect();
        state.responsibilities.retain(|responsibility| {
            !releases.contains(&responsibility.button) && !ends.contains(&responsibility.button)
        });
        // Releases outside every hit do not produce Pointer<Release>, but still end ownership.
        if state.phase == Phase::Active {
            let pressed = state.pressed.clone();
            state
                .responsibilities
                .retain(|responsibility| pressed.contains(&responsibility.button));
        }
        let notified: HashSet<_> = state
            .cancel_reader
            .read(world.resource::<Messages<Pointer<Cancel>>>())
            .filter(|event| event.pointer_id == id)
            .map(|event| event.entity)
            .collect();
        match state.drain {
            Drain::Emitted => {
                let mut notified = notified;
                for responsibility in state.responsibilities.drain(..) {
                    if responsibility.generation == generation
                        && world.get_entity(responsibility.target).is_ok()
                        && notified.insert(responsibility.target)
                    {
                        let cancel = Pointer::new(
                            id,
                            responsibility.location,
                            Cancel {
                                hit: responsibility.hit,
                            },
                            responsibility.target,
                        );
                        world.write_message(cancel.clone());
                        world.trigger(cancel);
                    }
                }
                if let Some(entity) = state.entity {
                    if let Some(mut press) = world.get_mut::<PointerPress>(entity) {
                        *press = PointerPress::default();
                    }
                    if let Some(mut location) = world.get_mut::<PointerLocation>(entity) {
                        location.location = None;
                    }
                }
                state.pressed.clear();
                state.drain = Drain::Converging;
            }
            Drain::Converging => {
                state.phase = Phase::Inactive;
                state.window = None;
                state.drain = Drain::None;
                state.activity = None;
            }
            Drain::None if state.queue.is_empty() => state.activity = None,
            _ => {}
        }
    });
    // Complete observer commands before the next frame can finish draining.
    world.flush();
    suppress_physical(world);
}

#[cfg(test)]
mod tests;
