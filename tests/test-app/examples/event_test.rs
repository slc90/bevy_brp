//! Minimal BRP event test example
//!
//! Tests `world.trigger_event` BRP method with triggerable events.

use bevy::ecs::observer::On;
use bevy::prelude::*;
use bevy::window::WindowPlugin;
use bevy::winit::WinitSettings;
use bevy_brp_extras::BrpExtrasActivity;
use bevy_brp_runtime::BrpRuntimePlugin;
use bevy_remote::{BrpResult, RemoteMethodSystemId, RemoteMethods};
use serde_json::{Value, json};
use std::collections::HashMap;

const EVENT_TEST_TITLE: &str = "Event Test";
const EVENT_TEST_WINDOW_HEIGHT: u32 = 300;
const EVENT_TEST_WINDOW_WIDTH: u32 = 400;

/// Test event with no payload
#[derive(Event, Reflect, Clone)]
#[reflect(Event)]
struct TestUnitEvent;

/// Test event with payload
#[derive(Event, Reflect, Clone, Default)]
#[reflect(Event)]
struct TestPayloadEvent {
    message: String,
    value: i32,
}

/// Resource to verify events were triggered
#[derive(Resource, Default, Reflect)]
#[reflect(Resource)]
struct EventTriggerTracker {
    unit_events: u32,
    last_payload_message: String,
    last_payload_value: i32,
    payload_events: u32,
}

/// Safe reflection data for protocol CRUD and generated type examples.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
struct ProtocolData {
    count: u64,
    enabled: bool,
    mode: ProtocolMode,
    optional: Option<i32>,
    nested: ProtocolNested,
    values: Vec<i32>,
    labels: HashMap<String, i32>,
}

#[derive(Default, Reflect)]
enum ProtocolMode {
    #[default]
    Idle,
    Running,
}

#[derive(Default, Reflect)]
struct ProtocolNested {
    value: f32,
}

#[derive(Resource, Default, Reflect)]
#[reflect(Resource, Default)]
struct ProtocolResource {
    value: u64,
}

#[derive(Component, Reflect)]
#[reflect(Component)]
struct ProtocolLinks {
    target: Entity,
    image: Handle<Image>,
}

#[derive(Resource, Default)]
struct ProtocolProgress {
    updates: u64,
    watch_calls: u64,
}

fn main() -> AppExit {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: EVENT_TEST_TITLE.to_string(),
            resolution: (EVENT_TEST_WINDOW_WIDTH, EVENT_TEST_WINDOW_HEIGHT).into(),
            focused: false,
            ..default()
        }),
        ..default()
    }))
    .insert_resource(WinitSettings::desktop_app())
    .add_plugins(BrpRuntimePlugin::default())
    .register_type::<ProtocolData>()
    .register_type::<ProtocolResource>()
    .register_type::<ProtocolLinks>()
    .register_type::<EventTriggerTracker>()
    .register_type::<TestUnitEvent>()
    .register_type::<TestPayloadEvent>()
    .init_resource::<EventTriggerTracker>()
    .init_resource::<ProtocolResource>()
    .init_resource::<ProtocolProgress>()
    .add_observer(on_unit_event)
    .add_observer(on_payload_event)
    .add_systems(Startup, minimize_window)
    .add_systems(Update, count_updates);
    let state = app.world_mut().register_system(protocol_state);
    let wait = app.world_mut().register_system(protocol_wait);
    let stream = app.world_mut().register_system(protocol_stream);
    let params = app.world_mut().register_system(protocol_params);
    let mut methods = app.world_mut().resource_mut::<RemoteMethods>();
    methods.insert(
        "fixture/protocol_state",
        RemoteMethodSystemId::Instant(state),
    );
    methods.insert(
        "fixture/protocol_wait",
        RemoteMethodSystemId::Watching(wait),
    );
    methods.insert(
        "fixture/protocol_stream+watch",
        RemoteMethodSystemId::Watching(stream),
    );
    methods.insert(
        "fixture/protocol_params",
        RemoteMethodSystemId::Instant(params),
    );
    app.run()
}

fn count_updates(mut progress: ResMut<ProtocolProgress>) {
    progress.updates += 1;
}

fn protocol_state(
    _: In<Option<Value>>,
    progress: Res<ProtocolProgress>,
    activity: Res<BrpExtrasActivity>,
    windows: Query<&Window>,
) -> BrpResult {
    Ok(
        json!({"updates": progress.updates, "watch_calls": progress.watch_calls,
        "active_operations": activity.state().active_count(), "focused": windows.iter().any(|window| window.focused)}),
    )
}

fn protocol_wait(
    _: In<Option<Value>>,
    mut progress: ResMut<ProtocolProgress>,
) -> BrpResult<Option<Value>> {
    progress.watch_calls += 1;
    Ok(None)
}

fn protocol_stream(
    _: In<Option<Value>>,
    mut progress: ResMut<ProtocolProgress>,
) -> BrpResult<Option<Value>> {
    progress.watch_calls += 1;
    Ok(Some(json!({"watch_calls": progress.watch_calls})))
}

fn protocol_params(In(params): In<Option<Value>>) -> BrpResult {
    Ok(json!({"present": params.is_some(), "value": params}))
}

fn on_unit_event(_unit_event: On<TestUnitEvent>, mut tracker: ResMut<EventTriggerTracker>) {
    tracker.unit_events += 1;
}

fn on_payload_event(on: On<TestPayloadEvent>, mut tracker: ResMut<EventTriggerTracker>) {
    tracker.last_payload_message.clone_from(&on.event().message);
    tracker.last_payload_value = on.event().value;
    tracker.payload_events += 1;
}

fn minimize_window(mut windows: Query<&mut Window>) {
    for mut window in &mut windows {
        window.set_minimized(true);
    }
}
