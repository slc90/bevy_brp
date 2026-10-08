//! Real UI Picking fixture for ordinary Custom Pointer input, without OS focus or cursor writes.

use bevy::camera::RenderTarget;
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::{Activate, Button};
use bevy::window::{CursorMoved, PrimaryWindow, WindowCloseRequested, WindowRef};
use bevy::winit::WinitSettings;
use bevy_brp_extras::BrpExtrasActivity;
use bevy_brp_runtime::BrpRuntimePlugin;
use bevy_remote::{BrpError, BrpResult, RemoteMethodSystemId, RemoteMethods};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Component, Default, Clone, Serialize)]
struct Evidence {
    activations: u32,
    click_counts: Vec<u8>,
    drag_starts: u32,
    drag_ends: u32,
    drops: u32,
    drag_x: f32,
    scroll_y: f32,
    scroll_units: Vec<String>,
    cancels: u32,
    outs: u32,
}

#[derive(Component)]
struct Owner(Entity);

#[derive(Component)]
struct ButtonView;

#[derive(Component)]
struct ScrollView;

#[derive(Resource, Default, Serialize)]
struct Raw {
    motion: usize,
    buttons: usize,
    wheels: usize,
    cursor: usize,
    window_events: usize,
}

#[derive(Resource, Default)]
struct Updates(u64);

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(window("Primary Pointer fixture", 50)),
        ..default()
    }))
    .insert_resource(WinitSettings::desktop_app())
    .init_resource::<Raw>()
    .init_resource::<Updates>()
    .add_plugins(BrpRuntimePlugin::default())
    .add_systems(Startup, setup)
    .add_systems(Last, record_raw);
    let state = app.world_mut().register_system(state);
    let close = app.world_mut().register_system(close_window);
    let mut methods = app.world_mut().resource_mut::<RemoteMethods>();
    methods.insert("fixture/state", RemoteMethodSystemId::Instant(state));
    methods.insert("fixture/close_window", RemoteMethodSystemId::Instant(close));
    app.run();
}

fn window(title: &str, x: i32) -> Window {
    // The regression chooses a row away from the read-only OS cursor baseline.
    let y = match std::env::var("BRP_POINTER_FIXTURE_Y") {
        Ok(value) => match value.parse::<i32>() {
            Ok(y) => y,
            Err(error) => {
                warn!(%error, "Invalid pointer fixture row; using 50");
                50
            }
        },
        Err(std::env::VarError::NotPresent) => 50,
        Err(error) => {
            warn!(%error, "Could not read pointer fixture row; using 50");
            50
        }
    };
    Window {
        title: title.to_owned(),
        resolution: bevy::window::WindowResolution::new(480, 360).with_scale_factor_override(1.0),
        position: WindowPosition::At(IVec2::new(x, y)),
        // Winit creates an inactive native window; we never forge a focused event/state.
        focused: false,
        ..default()
    }
}

fn node(x: f32, y: f32, width: f32, height: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(x),
        top: px(y),
        width: px(width),
        height: px(height),
        ..default()
    }
}

fn label(text: &str) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        Pickable::IGNORE,
    )
}

fn setup(mut commands: Commands, primary: Single<Entity, With<PrimaryWindow>>) {
    let primary = *primary;
    commands
        .entity(primary)
        .insert((Name::new("PrimaryWindow"), Evidence::default()));
    let secondary = commands
        .spawn((
            window("Secondary Pointer fixture", 560),
            Name::new("SecondaryWindow"),
            Evidence::default(),
        ))
        .id();
    for (window, camera_name, prefix) in [
        (primary, "PrimaryCamera", "Primary"),
        (secondary, "SecondaryCamera", "Secondary"),
    ] {
        let camera = commands
            .spawn((
                Camera2d,
                RenderTarget::Window(WindowRef::Entity(window)),
                Name::new(camera_name),
            ))
            .id();
        commands
            .spawn((
                node(0.0, 0.0, 480.0, 360.0),
                UiTargetCamera(camera),
                BackgroundColor(Color::srgb(0.08, 0.12, 0.2)),
                Pickable::IGNORE,
            ))
            .with_children(|parent| {
                parent
                    .spawn((
                        node(20.0, 20.0, 180.0, 50.0),
                        Button,
                        ButtonView,
                        Owner(window),
                        Name::new(format!("{prefix}Button")),
                        BackgroundColor(Color::srgb(0.1, 0.45, 0.65)),
                    ))
                    .observe(on_activate)
                    .observe(on_click)
                    .observe(on_cancel)
                    .observe(on_out)
                    .with_children(|button| {
                        button.spawn(label("Click / Cancel"));
                    });
                parent
                    .spawn((
                        node(20.0, 100.0, 100.0, 50.0),
                        Owner(window),
                        Name::new(format!("{prefix}Drag")),
                        BackgroundColor(Color::srgb(0.75, 0.35, 0.15)),
                    ))
                    .observe(on_drag_start)
                    .observe(on_drag)
                    .observe(on_drag_end)
                    .observe(on_cancel)
                    .with_children(|drag| {
                        drag.spawn(label("Drag"));
                    });
                parent
                    .spawn((
                        node(280.0, 100.0, 160.0, 80.0),
                        Owner(window),
                        BackgroundColor(Color::srgba(0.2, 0.5, 0.2, 0.65)),
                    ))
                    .observe(on_drop)
                    .with_children(|drop| {
                        drop.spawn(label("Drop target"));
                    });
                parent
                    .spawn((
                        Node {
                            overflow: Overflow::scroll_y(),
                            ..node(20.0, 220.0, 420.0, 80.0)
                        },
                        ScrollPosition::default(),
                        ScrollView,
                        Owner(window),
                        BackgroundColor(Color::srgb(0.2, 0.25, 0.35)),
                    ))
                    .observe(on_scroll)
                    .with_children(|scroll| {
                        scroll
                            .spawn((
                                Node {
                                    min_height: px(240.0),
                                    width: percent(100.0),
                                    flex_direction: FlexDirection::Column,
                                    ..default()
                                },
                                Pickable::IGNORE,
                            ))
                            .with_children(|content| {
                                for row in 1..=6 {
                                    content.spawn((
                                        Node {
                                            min_height: px(40.0),
                                            ..default()
                                        },
                                        label(&format!("Scroll row {row}")),
                                    ));
                                }
                            });
                    });
                parent.spawn((node(20.0, 320.0, 440.0, 35.0), label(prefix)));
            });
    }
}

fn on_activate(event: On<Activate>, owners: Query<&Owner>, mut evidence: Query<&mut Evidence>) {
    if let Ok(owner) = owners.get(event.entity)
        && let Ok(mut state) = evidence.get_mut(owner.0)
    {
        state.activations += 1;
    }
}

fn on_click(event: On<Pointer<Click>>, owners: Query<&Owner>, mut evidence: Query<&mut Evidence>) {
    if matches!(event.pointer_id, PointerId::Custom(_))
        && let Ok(owner) = owners.get(event.entity)
        && let Ok(mut state) = evidence.get_mut(owner.0)
    {
        state.click_counts.push(event.count);
    }
}

fn on_cancel(
    event: On<Pointer<Cancel>>,
    owners: Query<&Owner>,
    mut evidence: Query<&mut Evidence>,
) {
    if matches!(event.pointer_id, PointerId::Custom(_))
        && let Ok(owner) = owners.get(event.entity)
        && let Ok(mut state) = evidence.get_mut(owner.0)
    {
        state.cancels += 1;
    }
}

fn on_out(event: On<Pointer<Out>>, owners: Query<&Owner>, mut evidence: Query<&mut Evidence>) {
    if matches!(event.pointer_id, PointerId::Custom(_))
        && let Ok(owner) = owners.get(event.entity)
        && let Ok(mut state) = evidence.get_mut(owner.0)
    {
        state.outs += 1;
    }
}

fn on_drag_start(
    event: On<Pointer<DragStart>>,
    owners: Query<&Owner>,
    mut evidence: Query<&mut Evidence>,
) {
    if matches!(event.pointer_id, PointerId::Custom(_))
        && let Ok(owner) = owners.get(event.entity)
        && let Ok(mut state) = evidence.get_mut(owner.0)
    {
        state.drag_starts += 1;
    }
}

fn on_drag(
    event: On<Pointer<Drag>>,
    mut nodes: Query<(&Owner, &mut Node)>,
    mut evidence: Query<&mut Evidence>,
) {
    if matches!(event.pointer_id, PointerId::Custom(_))
        && let Ok((owner, mut node)) = nodes.get_mut(event.entity)
        && let Ok(mut state) = evidence.get_mut(owner.0)
    {
        state.drag_x = 20.0 + event.distance.x;
        node.left = px(state.drag_x);
    }
}

fn on_drag_end(
    event: On<Pointer<DragEnd>>,
    owners: Query<&Owner>,
    mut evidence: Query<&mut Evidence>,
) {
    if matches!(event.pointer_id, PointerId::Custom(_))
        && let Ok(owner) = owners.get(event.entity)
        && let Ok(mut state) = evidence.get_mut(owner.0)
    {
        state.drag_ends += 1;
    }
}

fn on_drop(
    event: On<Pointer<DragDrop>>,
    owners: Query<&Owner>,
    mut evidence: Query<&mut Evidence>,
) {
    if matches!(event.pointer_id, PointerId::Custom(_))
        && let Ok(owner) = owners.get(event.entity)
        && let Ok(mut state) = evidence.get_mut(owner.0)
    {
        state.drops += 1;
    }
}

fn on_scroll(
    event: On<Pointer<Scroll>>,
    mut views: Query<(&Owner, &mut ScrollPosition)>,
    mut evidence: Query<&mut Evidence>,
) {
    if matches!(event.pointer_id, PointerId::Custom(_))
        && let Ok((owner, mut position)) = views.get_mut(event.entity)
        && let Ok(mut state) = evidence.get_mut(owner.0)
    {
        let (unit, scale) = match event.unit {
            MouseScrollUnit::Line => ("Line", 20.0),
            MouseScrollUnit::Pixel => ("Pixel", 1.0),
        };
        position.0.y = (position.0.y - event.y * scale).clamp(0.0, 160.0);
        state.scroll_y = position.0.y;
        state.scroll_units.push(unit.to_owned());
    }
}

fn record_raw(
    mut motion: MessageReader<MouseMotion>,
    mut buttons: MessageReader<MouseButtonInput>,
    mut wheels: MessageReader<MouseWheel>,
    mut cursor: MessageReader<CursorMoved>,
    mut raw: ResMut<Raw>,
    mut updates: ResMut<Updates>,
    mut windows: MessageReader<bevy::window::WindowEvent>,
) {
    raw.motion += motion.read().count();
    raw.buttons += buttons.read().count();
    raw.wheels += wheels.read().count();
    raw.cursor += cursor.read().count();
    raw.window_events += windows
        .read()
        .filter(|event| {
            matches!(
                event,
                bevy::window::WindowEvent::MouseButtonInput(_)
                    | bevy::window::WindowEvent::MouseMotion(_)
                    | bevy::window::WindowEvent::MouseWheel(_)
                    | bevy::window::WindowEvent::CursorMoved(_)
            )
        })
        .count();
    updates.0 += 1;
}

fn state(_: In<Option<Value>>, world: &mut World) -> BrpResult {
    let mut snapshot = serde_json::Map::new();
    let mut windows = world.query::<(Entity, &Name, &Window, &Evidence)>();
    let mut buttons = world.query_filtered::<(&Owner, Has<Pressed>), With<ButtonView>>();
    for (entity, name, window, evidence) in windows.iter(world) {
        let mut evidence = serde_json::to_value(evidence).map_err(BrpError::internal)?;
        let pressed = buttons
            .iter(world)
            .any(|(owner, pressed)| owner.0 == entity && pressed);
        evidence["button_pressed"] = json!(pressed);
        evidence["focused"] = json!(window.focused);
        evidence["native_cursor"] = json!(window.cursor_position());
        let key = if name.as_str() == "PrimaryWindow" {
            "primary"
        } else {
            "secondary"
        };
        snapshot.insert(key.to_owned(), evidence);
    }
    snapshot.insert(
        "raw".to_owned(),
        serde_json::to_value(world.resource::<Raw>()).map_err(BrpError::internal)?,
    );
    snapshot.insert("updates".to_owned(), json!(world.resource::<Updates>().0));
    snapshot.insert(
        "active_operations".to_owned(),
        json!(world.resource::<BrpExtrasActivity>().state().active_count()),
    );
    Ok(Value::Object(snapshot))
}

fn close_window(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let window = params
        .as_ref()
        .and_then(|params| params.get("window"))
        .and_then(Value::as_u64)
        .and_then(Entity::try_from_bits)
        .filter(|window| world.get::<Window>(*window).is_some())
        .ok_or_else(|| BrpError {
            code: -32602,
            message: "Expected a live window".into(),
            data: None,
        })?;
    world.write_message(WindowCloseRequested { window });
    Ok(json!({"closing":window.to_bits()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::picking::backend::{HitData, PointerHits};
    use bevy::picking::pointer::PointerLocation;
    use bevy::picking::{InteractionPlugin, PickingPlugin, PickingSystems};
    use bevy::ui_widgets::ButtonPlugin;
    use bevy_brp_extras::BrpExtrasPlugin;

    #[derive(Resource)]
    struct Target(Entity);

    #[derive(Resource, Default)]
    struct Counts {
        cancels: usize,
        activates: usize,
    }

    fn hits(
        pointers: Query<(&PointerId, &PointerLocation)>,
        target: Res<Target>,
        mut hits: MessageWriter<PointerHits>,
    ) {
        for (id, location) in &pointers {
            if location
                .location
                .as_ref()
                .is_some_and(|location| location.position.x < 100.0)
            {
                hits.write(PointerHits::new(
                    *id,
                    vec![(target.0, HitData::new(target.0, 0.0, None, None))],
                    1.0,
                ));
            }
        }
    }

    fn invoke(app: &mut App, method: &str, params: Value) -> Value {
        let RemoteMethodSystemId::Instant(system) = *app
            .world()
            .resource::<RemoteMethods>()
            .get(method)
            .expect("method exists")
        else {
            panic!("instant method required")
        };
        app.world_mut()
            .run_system_with(system, Some(params))
            .expect("method runs")
            .expect("method succeeds")
    }

    #[test]
    fn stock_button_cancel_after_leaving_clears_pressed_without_activation_or_duplicate_cancel() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::input::InputPlugin,
            WindowPlugin {
                primary_window: None,
                ..default()
            },
            PickingPlugin,
            InteractionPlugin,
            ButtonPlugin,
        ))
        .add_plugins(BrpExtrasPlugin::without_http_transport())
        .init_resource::<Counts>()
        .add_systems(PreUpdate, hits.in_set(PickingSystems::Backend));
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        let target = app.world_mut().spawn(Button).id();
        app.insert_resource(Target(target));
        app.world_mut()
            .entity_mut(target)
            .observe(|_: On<Pointer<Cancel>>, mut counts: ResMut<Counts>| {
                counts.cancels += 1;
            })
            .observe(|_: On<Activate>, mut counts: ResMut<Counts>| {
                counts.activates += 1;
            });
        app.update();
        invoke(
            &mut app,
            "brp_extras/send_mouse_button",
            json!({"button":"Left","duration_ms":60000}),
        );
        app.update();
        app.update();
        assert!(app.world().get::<Pressed>(target).is_some());
        invoke(
            &mut app,
            "brp_extras/move_mouse",
            json!({"position":[200,0]}),
        );
        app.update();
        invoke(
            &mut app,
            "brp_extras/pointer_control",
            json!({"action":"release"}),
        );
        app.update();
        app.update();
        assert!(app.world().get::<Pressed>(target).is_none());
        assert_eq!(app.world().resource::<Counts>().cancels, 1);
        assert_eq!(app.world().resource::<Counts>().activates, 0);
        invoke(&mut app, "brp_extras/move_mouse", json!({"position":[0,0]}));
        invoke(
            &mut app,
            "brp_extras/send_mouse_button",
            json!({"button":"Left","duration_ms":0}),
        );
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(app.world().resource::<Counts>().activates, 1);
        invoke(
            &mut app,
            "brp_extras/pointer_control",
            json!({"action":"release"}),
        );
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Counts>().cancels, 1);
        assert_eq!(
            app.world()
                .resource::<BrpExtrasActivity>()
                .state()
                .active_count(),
            0
        );
    }
}
