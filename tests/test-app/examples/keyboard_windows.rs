//! 真实 MCP→BRP multi-window keyboard 回归宿主。
//! 两个窗口分别显示自身接收到的文本，并记录 pointer、press/release 与全局 modifier 状态。

use bevy::camera::RenderTarget;
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowCloseRequested, WindowRef};
use bevy::winit::WinitSettings;
use bevy_brp_extras::BrpExtrasActivity;
use bevy_brp_runtime::BrpRuntimePlugin;
use bevy_remote::error_codes::INVALID_PARAMS;
use bevy_remote::{BrpError, BrpResult, RemoteMethodSystemId, RemoteMethods};
use serde_json::{Value, json};

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
struct WindowInputEvidence {
    text: String,
    presses: u32,
    releases: u32,
    pointer_clicks: u32,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
struct GlobalKeyboardEvidence {
    ctrl_pressed: bool,
    shift_pressed: bool,
    active_operations: usize,
}

#[derive(Component)]
struct InputView(Entity);

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Primary keyboard fixture".into(),
            resolution: (420, 220).into(),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(WinitSettings::desktop_app())
    .add_plugins(BrpRuntimePlugin::default())
    .register_type::<WindowInputEvidence>()
    .register_type::<GlobalKeyboardEvidence>()
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        (record_keyboard, display_input, global_state).chain(),
    );
    let close = app.world_mut().register_system(close_window);
    app.world_mut()
        .resource_mut::<RemoteMethods>()
        .insert("fixture/close_window", RemoteMethodSystemId::Instant(close));
    app.run();
}

fn setup(mut commands: Commands, primary: Single<Entity, With<PrimaryWindow>>) {
    let primary = *primary;
    let secondary = commands
        .spawn((
            Window {
                title: "Secondary keyboard fixture".into(),
                resolution: (420, 220).into(),
                ..default()
            },
            Name::new("SecondaryWindow"),
            WindowInputEvidence::default(),
        ))
        .id();
    commands.entity(primary).insert((
        Name::new("PrimaryWindow"),
        WindowInputEvidence::default(),
        GlobalKeyboardEvidence::default(),
    ));
    for (window, camera_name, label) in [
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
                Node {
                    width: percent(100),
                    height: percent(100),
                    padding: UiRect::all(px(20)),
                    ..default()
                },
                UiTargetCamera(camera),
                BackgroundColor(Color::srgb(0.1, 0.15, 0.25)),
                InputView(window),
            ))
            .observe(
                move |_event: On<PointerClick>, mut windows: Query<&mut WindowInputEvidence>| {
                    if let Ok(mut state) = windows.get_mut(window) {
                        state.pointer_clicks += 1;
                    }
                },
            )
            .with_children(|parent| {
                parent.spawn((
                    Text::new(format!("{label}: ")),
                    TextFont {
                        font_size: FontSize::Px(28.0),
                        ..default()
                    },
                    InputView(window),
                    Pickable::IGNORE,
                ));
            });
    }
}

fn record_keyboard(
    mut events: MessageReader<KeyboardInput>,
    mut windows: Query<&mut WindowInputEvidence>,
) {
    for event in events.read() {
        if let Ok(mut state) = windows.get_mut(event.window) {
            match event.state {
                ButtonState::Pressed => {
                    state.presses += 1;
                    if let Some(text) = &event.text
                        && state.text.len() < 1024
                    {
                        state.text.push_str(text);
                    }
                }
                ButtonState::Released => state.releases += 1,
            }
        }
    }
}

fn display_input(
    windows: Query<(&Name, &WindowInputEvidence)>,
    mut views: Query<(&InputView, &mut Text)>,
) {
    for (view, mut text) in &mut views {
        if let Ok((name, evidence)) = windows.get(view.0) {
            text.0 = format!("{}: {}", name.as_str(), evidence.text);
        }
    }
}

fn global_state(
    keys: Res<ButtonInput<KeyCode>>,
    activity: Res<BrpExtrasActivity>,
    mut evidence: Single<&mut GlobalKeyboardEvidence>,
) {
    evidence.ctrl_pressed = keys.pressed(KeyCode::ControlLeft);
    evidence.shift_pressed = keys.pressed(KeyCode::ShiftLeft);
    evidence.active_operations = activity.state().active_count();
}

fn close_window(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let bits = params
        .as_ref()
        .and_then(|value| value.get("window"))
        .and_then(Value::as_u64)
        .ok_or_else(|| BrpError {
            code: INVALID_PARAMS,
            message: "window is required".into(),
            data: None,
        })?;
    let window = Entity::try_from_bits(bits)
        .filter(|entity| world.get::<Window>(*entity).is_some())
        .ok_or_else(|| BrpError {
            code: INVALID_PARAMS,
            message: "window is not live".into(),
            data: None,
        })?;
    world.write_message(WindowCloseRequested { window });
    Ok(json!({"window":bits,"queued":true}))
}
