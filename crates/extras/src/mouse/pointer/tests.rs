use super::*;
use bevy::ecs::system::In;
use bevy::picking::backend::{HitData, PointerHits};
use bevy::picking::{InteractionPlugin, PickingPlugin};
use bevy::window::PrimaryWindow;
use serde_json::json;

#[derive(Resource, Default)]
struct Trace {
    inputs: Vec<PointerInput>,
    clicks: Vec<Pointer<bevy::picking::events::Click>>,
    scrolls: Vec<Pointer<bevy::picking::events::Scroll>>,
    raw_count: usize,
}

fn capture(
    mut trace: ResMut<Trace>,
    mut inputs: MessageReader<PointerInput>,
    mut clicks: MessageReader<Pointer<bevy::picking::events::Click>>,
    mut scrolls: MessageReader<Pointer<bevy::picking::events::Scroll>>,
    mut button: MessageReader<bevy::input::mouse::MouseButtonInput>,
    mut motion: MessageReader<bevy::input::mouse::MouseMotion>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut cursor: MessageReader<bevy::window::CursorMoved>,
    mut windows: MessageReader<WindowEvent>,
) {
    trace.inputs.extend(inputs.read().cloned());
    trace.clicks.extend(clicks.read().cloned());
    trace.scrolls.extend(scrolls.read().cloned());
    trace.raw_count += button.read().count()
        + motion.read().count()
        + wheel.read().count()
        + cursor.read().count();
    trace.raw_count += windows
        .read()
        .filter(|event| {
            matches!(
                event,
                WindowEvent::MouseButtonInput(_)
                    | WindowEvent::MouseMotion(_)
                    | WindowEvent::MouseWheel(_)
                    | WindowEvent::CursorMoved(_)
            )
        })
        .count();
}

fn location(window: Entity, position: Vec2) -> Location {
    super::location(window, position).unwrap()
}

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::input::InputPlugin,
        bevy::window::WindowPlugin {
            primary_window: None,
            ..default()
        },
        PickingPlugin,
        InteractionPlugin,
    ));
    app.add_plugins(super::super::MousePlugin);
    app.add_systems(PreUpdate, hits.in_set(PickingSystems::Backend));
    app.init_resource::<Trace>().add_systems(Last, capture);
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    let target = app.world_mut().spawn_empty().id();
    app.insert_resource(Target(target));
    app.update();
    (app, window, target)
}

#[derive(Resource)]
struct Target(Entity);

fn hits(
    pointers: Query<(&PointerId, &PointerLocation)>,
    target: Res<Target>,
    mut output: MessageWriter<PointerHits>,
) {
    for (id, location) in &pointers {
        if location
            .location
            .as_ref()
            .is_some_and(|location| location.position.x < 100.0)
        {
            output.write(PointerHits::new(
                *id,
                vec![(target.0, HitData::new(target.0, 0.0, None, None))],
                1.0,
            ));
        }
    }
}

#[test]
fn activation_release_preserves_identity_and_converges_after_two_cycles() {
    let (mut app, window, target) = fixture();
    enqueue(
        app.world_mut(),
        window,
        Vec2::new(10.0, 20.0),
        PointerAction::Move { delta: Vec2::ZERO },
        "test",
    )
    .unwrap();
    app.update();
    let state = app.world().resource::<BrpPointerState>();
    let id = state.id.unwrap();
    let entity = state.entity.unwrap();
    assert!(
        app.world()
            .resource::<HoverMap>()
            .get(&id)
            .unwrap()
            .contains_key(&target)
    );
    assert!(
        !app.world()
            .resource::<crate::BrpExtrasActivity>()
            .state()
            .is_active()
    );
    release(app.world_mut());
    assert_eq!(
        app.world().resource::<BrpPointerState>().phase,
        Phase::Draining
    );
    app.update();
    assert!(
        app.world()
            .get::<PointerLocation>(entity)
            .unwrap()
            .location
            .is_none()
    );
    app.update();
    assert_eq!(
        app.world().resource::<BrpPointerState>().phase,
        Phase::Inactive
    );
    assert!(
        !app.world()
            .resource::<crate::BrpExtrasActivity>()
            .state()
            .is_active()
    );
    enqueue(
        app.world_mut(),
        window,
        Vec2::ZERO,
        PointerAction::Move { delta: Vec2::ZERO },
        "test",
    )
    .unwrap();
    app.update();
    assert_eq!(app.world().resource::<BrpPointerState>().id, Some(id));
    assert_eq!(
        app.world().resource::<BrpPointerState>().entity,
        Some(entity)
    );
}

#[test]
fn missing_picking_rejects_without_activation() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::input::InputPlugin,
        bevy::window::WindowPlugin::default(),
        super::super::MousePlugin,
    ));
    let window = app.world_mut().spawn(Window::default()).id();
    assert!(
        enqueue(
            app.world_mut(),
            window,
            Vec2::ZERO,
            PointerAction::Move { delta: Vec2::ZERO },
            "test"
        )
        .is_err()
    );
    app.update();
    assert_eq!(
        app.world().resource::<BrpPointerState>().phase,
        Phase::Inactive
    );
}

#[test]
fn cancel_notifies_original_press_target_once_without_click_and_keeps_out_location() {
    let (mut app, window, target) = fixture();
    enqueue(
        app.world_mut(),
        window,
        Vec2::ZERO,
        PointerAction::Move { delta: Vec2::ZERO },
        "test",
    )
    .unwrap();
    app.update();
    enqueue(
        app.world_mut(),
        window,
        Vec2::ZERO,
        PointerAction::Press(PointerButton::Primary),
        "test",
    )
    .unwrap();
    app.update();
    release(app.world_mut());
    app.update();
    let messages = app.world().resource::<Messages<Pointer<Cancel>>>();
    let cancels: Vec<_> = messages.get_cursor().read(messages).cloned().collect();
    assert_eq!(
        cancels
            .iter()
            .filter(|event| event.entity == target)
            .count(),
        1
    );
    let messages = app
        .world()
        .resource::<Messages<Pointer<bevy::picking::events::Out>>>();
    assert!(
        messages
            .get_cursor()
            .read(messages)
            .any(|event| event.entity == target)
    );
    let messages = app
        .world()
        .resource::<Messages<Pointer<bevy::picking::events::Click>>>();
    assert_eq!(messages.get_cursor().read(messages).count(), 0);
    let entity = app.world().resource::<BrpPointerState>().entity.unwrap();
    assert!(
        !app.world()
            .get::<PointerPress>(entity)
            .unwrap()
            .is_any_pressed()
    );
    app.update();
    release(app.world_mut());
    assert_eq!(
        app.world().resource::<BrpPointerState>().phase,
        Phase::Inactive
    );
}

#[test]
fn parked_mouse_is_suppressed_and_first_physical_press_restores_its_own_position() {
    let (mut app, window, _) = fixture();
    let physical = location(window, Vec2::new(30.0, 40.0));
    let mouse = app
        .world_mut()
        .spawn((PointerId::Mouse, PointerLocation::new(physical.clone())))
        .id();
    enqueue(
        app.world_mut(),
        window,
        Vec2::new(5.0, 6.0),
        PointerAction::Move { delta: Vec2::ZERO },
        "test",
    )
    .unwrap();
    app.update();
    assert!(
        app.world()
            .get::<PointerLocation>(mouse)
            .unwrap()
            .location
            .is_none()
    );
    app.world_mut().write_message(PointerInput::new(
        PointerId::Mouse,
        physical.clone(),
        PointerAction::Press(PointerButton::Primary),
    ));
    app.update();
    assert_eq!(
        app.world().get::<PointerLocation>(mouse).unwrap().location,
        Some(physical)
    );
    assert_eq!(
        app.world().resource::<BrpPointerState>().phase,
        Phase::Draining
    );
    app.update();
    assert_eq!(
        app.world().resource::<BrpPointerState>().phase,
        Phase::Inactive
    );
}

#[test]
fn pressed_physical_mouse_prevents_takeover() {
    let (mut app, window, _) = fixture();
    app.world_mut().spawn((
        PointerId::Mouse,
        PointerLocation::new(location(window, Vec2::ZERO)),
    ));
    app.world_mut().write_message(PointerInput::new(
        PointerId::Mouse,
        location(window, Vec2::ZERO),
        PointerAction::Press(PointerButton::Primary),
    ));
    app.update();
    assert!(
        enqueue(
            app.world_mut(),
            window,
            Vec2::ZERO,
            PointerAction::Move { delta: Vec2::ZERO },
            "test"
        )
        .is_err()
    );
    assert_eq!(
        app.world().resource::<BrpPointerState>().phase,
        Phase::Inactive
    );
}

#[test]
fn destroyed_window_cancels_pending_generation_and_draining_rejects_requests() {
    let (mut app, window, _) = fixture();
    enqueue(
        app.world_mut(),
        window,
        Vec2::ZERO,
        PointerAction::Move { delta: Vec2::ZERO },
        "test",
    )
    .unwrap();
    let old_generation = app.world().resource::<BrpPointerState>().generation;
    app.world_mut().despawn(window);
    app.update();
    assert_eq!(
        app.world().resource::<BrpPointerState>().generation,
        old_generation + 1
    );
    assert_eq!(
        app.world().resource::<BrpPointerState>().phase,
        Phase::Draining
    );
    app.update();
    assert_eq!(
        app.world().resource::<BrpPointerState>().phase,
        Phase::Inactive
    );
    assert!(
        !app.world()
            .resource::<crate::BrpExtrasActivity>()
            .state()
            .is_active()
    );
}

#[test]
fn cancellation_after_leaving_hit_notifies_drag_owner_on_both_surfaces() {
    let (mut app, window, target) = fixture();
    #[derive(Resource, Default)]
    struct Observed(usize);
    app.init_resource::<Observed>();
    app.world_mut()
        .entity_mut(target)
        .observe(|_: On<Pointer<Cancel>>, mut observed: ResMut<Observed>| observed.0 += 1);
    for (position, action) in [
        (Vec2::ZERO, PointerAction::Move { delta: Vec2::ZERO }),
        (Vec2::ZERO, PointerAction::Press(PointerButton::Primary)),
        (
            Vec2::new(200.0, 0.0),
            PointerAction::Move {
                delta: Vec2::new(200.0, 0.0),
            },
        ),
    ] {
        enqueue(app.world_mut(), window, position, action, "test").unwrap();
        app.update();
    }
    release(app.world_mut());
    assert!(
        enqueue(
            app.world_mut(),
            window,
            Vec2::ZERO,
            PointerAction::Move { delta: Vec2::ZERO },
            "test"
        )
        .is_err()
    );
    app.update();
    assert_eq!(app.world().resource::<Observed>().0, 1);
    let messages = app.world().resource::<Messages<Pointer<Cancel>>>();
    assert_eq!(
        messages
            .get_cursor()
            .read(messages)
            .filter(|event| event.entity == target)
            .count(),
        1
    );
}

#[test]
fn normal_release_retires_cancel_responsibility() {
    let (mut app, window, target) = fixture();
    for action in [
        PointerAction::Move { delta: Vec2::ZERO },
        PointerAction::Press(PointerButton::Primary),
        PointerAction::Release(PointerButton::Primary),
    ] {
        enqueue(app.world_mut(), window, Vec2::ZERO, action, "test").unwrap();
        app.update();
    }
    release(app.world_mut());
    app.update();
    let messages = app.world().resource::<Messages<Pointer<Cancel>>>();
    assert!(
        !messages
            .get_cursor()
            .read(messages)
            .any(|event| event.entity == target)
    );
}

#[test]
fn takeover_sends_physical_out_and_cursor_left_prevents_stale_restore() {
    let (mut app, window, target) = fixture();
    let mouse = app
        .world_mut()
        .spawn((
            PointerId::Mouse,
            PointerLocation::new(location(window, Vec2::ZERO)),
        ))
        .id();
    app.update();
    enqueue(
        app.world_mut(),
        window,
        Vec2::ZERO,
        PointerAction::Move { delta: Vec2::ZERO },
        "test",
    )
    .unwrap();
    app.update();
    let messages = app
        .world()
        .resource::<Messages<Pointer<bevy::picking::events::Out>>>();
    assert!(
        messages
            .get_cursor()
            .read(messages)
            .any(|event| event.entity == target && event.pointer_id == PointerId::Mouse)
    );
    app.world_mut()
        .write_message(WindowEvent::CursorLeft(bevy::window::CursorLeft { window }));
    release(app.world_mut());
    app.update();
    app.update();
    assert!(
        app.world()
            .get::<PointerLocation>(mouse)
            .unwrap()
            .location
            .is_none()
    );
}

#[test]
fn cursor_left_after_move_in_same_cycle_does_not_restore_old_hover() {
    let (mut app, window, _) = fixture();
    let mouse = app.world_mut().spawn(PointerId::Mouse).id();
    enqueue(
        app.world_mut(),
        window,
        Vec2::ZERO,
        PointerAction::Move { delta: Vec2::ZERO },
        "test",
    )
    .unwrap();
    app.update();
    app.world_mut().write_message(PointerInput::new(
        PointerId::Mouse,
        location(window, Vec2::ZERO),
        PointerAction::Move { delta: Vec2::ZERO },
    ));
    app.world_mut()
        .write_message(WindowEvent::CursorLeft(bevy::window::CursorLeft { window }));
    app.update();
    app.update();
    assert!(
        app.world()
            .get::<PointerLocation>(mouse)
            .unwrap()
            .location
            .is_none()
    );
}

#[test]
fn takeover_removes_physical_ray_before_backend_in_the_first_cycle() {
    use bevy::camera::{ComputedCameraValues, RenderTargetInfo};
    use bevy::picking::backend::ray::{RayId, RayMap};
    let (mut app, window, _) = fixture();
    let camera = app
        .world_mut()
        .spawn((
            Camera {
                computed: ComputedCameraValues {
                    clip_from_view: Mat4::IDENTITY,
                    target_info: Some(RenderTargetInfo {
                        physical_size: UVec2::new(800, 600),
                        scale_factor: 1.0,
                    }),
                    ..default()
                },
                ..default()
            },
            RenderTarget::Window(WindowRef::Entity(window)),
            GlobalTransform::IDENTITY,
        ))
        .id();
    app.world_mut().spawn((
        PointerId::Mouse,
        PointerLocation::new(location(window, Vec2::new(20.0, 20.0))),
    ));
    app.update();
    assert!(
        app.world()
            .resource::<RayMap>()
            .map
            .contains_key(&RayId::new(camera, PointerId::Mouse))
    );
    enqueue(
        app.world_mut(),
        window,
        Vec2::new(40.0, 40.0),
        PointerAction::Move { delta: Vec2::ZERO },
        "test",
    )
    .unwrap();
    app.update();
    let custom = app.world().resource::<BrpPointerState>().id.unwrap();
    let rays = app.world().resource::<RayMap>();
    assert!(!rays.map.contains_key(&RayId::new(camera, PointerId::Mouse)));
    assert!(rays.map.contains_key(&RayId::new(camera, custom)));
}

#[test]
fn basic_handlers_drive_custom_picking_without_raw_or_native_cursor_writes() {
    let (mut app, window, target) = fixture();
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(Vec2::new(70.0, 80.0)));
    super::super::cursor::move_mouse_handler(
        In(Some(
            json!({"position":[10.0,20.0],"window":window.to_bits()}),
        )),
        app.world_mut(),
    )
    .unwrap();
    super::super::scroll::scroll_mouse_handler(
        In(Some(json!({"x":0.0,"y":-2.0,"unit":"Line"}))),
        app.world_mut(),
    )
    .unwrap();
    super::super::button::send_mouse_button_handler(
        In(Some(json!({"button":"Left","duration_ms":0}))),
        app.world_mut(),
    )
    .unwrap();
    for _ in 0..8 {
        app.update();
    }
    let custom = app.world().resource::<BrpPointerState>().id.unwrap();
    let trace = app.world().resource::<Trace>();
    assert_eq!(trace.clicks.len(), 1);
    assert_eq!(trace.clicks[0].entity, target);
    assert_eq!(trace.clicks[0].pointer_id, custom);
    assert_eq!(trace.scrolls.len(), 1);
    assert_eq!(trace.scrolls[0].pointer_id, custom);
    assert_eq!(
        trace.scrolls[0].unit,
        bevy::input::mouse::MouseScrollUnit::Line
    );
    assert_eq!(trace.raw_count, 0);
    assert!(
        !app.world()
            .get::<PointerPress>(app.world().resource::<BrpPointerState>().entity.unwrap())
            .unwrap()
            .is_any_pressed()
    );
    assert!(
        app.world()
            .resource::<HoverMap>()
            .get(&custom)
            .unwrap()
            .contains_key(&target)
    );
    assert_eq!(
        app.world().get::<Window>(window).unwrap().cursor_position(),
        Some(Vec2::new(70.0, 80.0))
    );
    assert_eq!(
        app.world()
            .resource::<Messages<bevy::input::mouse::MouseButtonInput>>()
            .len(),
        0
    );
    assert_eq!(
        app.world()
            .resource::<Messages<bevy::input::mouse::MouseMotion>>()
            .len(),
        0
    );
    assert_eq!(
        app.world()
            .resource::<Messages<bevy::input::mouse::MouseWheel>>()
            .len(),
        0
    );
    assert_eq!(
        app.world()
            .resource::<Messages<bevy::window::CursorMoved>>()
            .len(),
        0
    );
    assert!(
        !app.world()
            .resource::<crate::BrpExtrasActivity>()
            .state()
            .is_active()
    );
}

#[test]
fn basic_button_rejects_unsupported_buttons_and_repeated_or_cross_window_holds() {
    let (mut app, window, _) = fixture();
    for button in [json!("Back"), json!("Forward"), json!({"Other":4})] {
        assert!(
            super::super::button::send_mouse_button_handler(
                In(Some(json!({"button":button}))),
                app.world_mut()
            )
            .is_err()
        );
    }
    assert_eq!(
        app.world().resource::<BrpPointerState>().phase,
        Phase::Inactive
    );
    super::super::button::send_mouse_button_handler(
        In(Some(json!({"button":"Left","duration_ms":60000}))),
        app.world_mut(),
    )
    .unwrap();
    assert!(
        super::super::button::send_mouse_button_handler(
            In(Some(json!({"button":"Left"}))),
            app.world_mut()
        )
        .is_err()
    );
    let second = app.world_mut().spawn(Window::default()).id();
    assert!(
        super::super::cursor::move_mouse_handler(
            In(Some(
                json!({"position":[1.0,2.0],"window":second.to_bits()})
            )),
            app.world_mut()
        )
        .is_err()
    );
    assert_eq!(
        app.world()
            .resource::<super::super::cursor::SimulatedCursorPosition>()
            .last_window,
        Some(window)
    );
}

#[test]
fn timed_hold_allows_moves_and_scroll_and_cancel_invalidates_its_timer() {
    let (mut app, _, _) = fixture();
    super::super::button::send_mouse_button_handler(
        In(Some(json!({"button":"Middle","duration_ms":60000}))),
        app.world_mut(),
    )
    .unwrap();
    for _ in 0..3 {
        app.update();
    }
    super::super::cursor::move_mouse_handler(
        In(Some(json!({"position":[45.0,-10.0]}))),
        app.world_mut(),
    )
    .unwrap();
    super::super::scroll::scroll_mouse_handler(
        In(Some(json!({"x":1.0,"y":2.0,"unit":"Pixel"}))),
        app.world_mut(),
    )
    .unwrap();
    app.update();
    app.update();
    let entity = app.world().resource::<BrpPointerState>().entity.unwrap();
    assert_eq!(
        app.world()
            .get::<PointerLocation>(entity)
            .unwrap()
            .location
            .as_ref()
            .unwrap()
            .position,
        Vec2::new(45.0, -10.0)
    );
    assert!(
        app.world()
            .get::<PointerPress>(entity)
            .unwrap()
            .is_middle_pressed()
    );
    release(app.world_mut());
    app.update();
    app.update();
    assert!(
        !app.world()
            .get::<PointerPress>(entity)
            .unwrap()
            .is_any_pressed()
    );
    assert!(
        !app.world()
            .resource::<crate::BrpExtrasActivity>()
            .state()
            .is_active()
    );
}

#[test]
fn invalid_window_bits_non_window_and_coordinate_delta_overflow_are_rejected() {
    let (mut app, _, _) = fixture();
    let ordinary = app.world_mut().spawn_empty().id();
    for window in [ordinary.to_bits(), u64::MAX] {
        assert!(
            super::super::cursor::move_mouse_handler(
                In(Some(json!({"position":[0.0,0.0],"window":window}))),
                app.world_mut()
            )
            .is_err()
        );
    }
    super::super::cursor::move_mouse_handler(
        In(Some(json!({"position":[f32::MAX,0.0]}))),
        app.world_mut(),
    )
    .unwrap();
    assert!(
        super::super::cursor::move_mouse_handler(
            In(Some(json!({"position":[-f32::MAX,0.0]}))),
            app.world_mut()
        )
        .is_err()
    );
}

#[test]
fn first_scroll_and_three_buttons_establish_location_before_picking_actions() {
    for button in ["Left", "Right", "Middle"] {
        let (mut app, window, target) = fixture();
        super::super::scroll::scroll_mouse_handler(
            In(Some(json!({"x":1.0,"y":-5.0,"unit":"Pixel"}))),
            app.world_mut(),
        )
        .unwrap();
        super::super::button::send_mouse_button_handler(
            In(Some(json!({"button":button,"duration_ms":0}))),
            app.world_mut(),
        )
        .unwrap();
        for _ in 0..6 {
            app.update();
        }
        let trace = app.world().resource::<Trace>();
        assert_eq!(trace.scrolls.len(), 1);
        assert_eq!(trace.scrolls[0].entity, target);
        assert_eq!(
            trace.scrolls[0].unit,
            bevy::input::mouse::MouseScrollUnit::Pixel
        );
        assert_eq!(
            trace.scrolls[0].pointer_location,
            location(window, Vec2::ZERO)
        );
        assert_eq!(trace.clicks.len(), 1);
        assert_eq!(trace.clicks[0].entity, target);
        assert_eq!(trace.raw_count, 0);
        assert!(matches!(trace.inputs[0].action, PointerAction::Move { .. }));
    }
}

#[test]
fn window_coordinates_are_independent_logical_values_at_different_scales() {
    let (mut app, first, _) = fixture();
    let second = app
        .world_mut()
        .spawn(Window {
            resolution: bevy::window::WindowResolution::new(1000, 800)
                .with_scale_factor_override(2.0),
            ..default()
        })
        .id();
    for (window, position) in [
        (first, Vec2::new(-30.0, 900.0)),
        (second, Vec2::new(200.0, 300.0)),
    ] {
        super::super::cursor::move_mouse_handler(
            In(Some(
                json!({"position":[position.x,position.y],"window":window.to_bits()}),
            )),
            app.world_mut(),
        )
        .unwrap();
    }
    app.update();
    app.update();
    super::super::cursor::move_mouse_handler(
        In(Some(json!({"delta":[10.0,20.0],"window":first.to_bits()}))),
        app.world_mut(),
    )
    .unwrap();
    app.update();
    let trace = app.world().resource::<Trace>();
    assert_eq!(
        trace.inputs[0].location,
        location(first, Vec2::new(-30.0, 900.0))
    );
    assert_eq!(
        trace.inputs[1].location,
        location(second, Vec2::new(200.0, 300.0))
    );
    assert_eq!(
        trace.inputs[2].location,
        location(first, Vec2::new(-20.0, 920.0))
    );
    assert_eq!(trace.raw_count, 0);
}

#[test]
fn positive_timed_hold_uses_elapsed_real_time_when_virtual_time_is_paused() {
    let (mut app, _, _) = fixture();
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    super::super::button::send_mouse_button_handler(
        In(Some(json!({"button":"Right","duration_ms":10}))),
        app.world_mut(),
    )
    .unwrap();
    app.update();
    app.update();
    let entity = app.world().resource::<BrpPointerState>().entity.unwrap();
    assert!(
        app.world()
            .get::<PointerPress>(entity)
            .unwrap()
            .is_secondary_pressed()
    );
    assert!(
        app.world()
            .resource::<crate::BrpExtrasActivity>()
            .state()
            .is_active()
    );
    std::thread::sleep(Duration::from_millis(15));
    app.update();
    assert!(
        !app.world()
            .get::<PointerPress>(entity)
            .unwrap()
            .is_any_pressed()
    );
    assert!(
        !app.world()
            .resource::<crate::BrpExtrasActivity>()
            .state()
            .is_active()
    );
}

#[test]
fn unrelated_raw_motion_is_preserved_and_does_not_trigger_handover() {
    let (mut app, _, _) = fixture();
    app.world_mut()
        .write_message(bevy::input::mouse::MouseMotion {
            delta: Vec2::new(1.0, 2.0),
        });
    super::super::button::send_mouse_button_handler(
        In(Some(json!({"button":"Left","duration_ms":0}))),
        app.world_mut(),
    )
    .unwrap();
    for _ in 0..4 {
        app.update();
    }
    let trace = app.world().resource::<Trace>();
    assert_eq!(trace.raw_count, 1);
    assert_eq!(trace.clicks.len(), 1);
    assert_eq!(
        app.world().resource::<BrpPointerState>().phase,
        Phase::Active
    );
}

#[test]
fn destroyed_timed_hold_reports_the_operation_that_owned_the_button() {
    let (mut app, window, _) = fixture();
    super::super::button::send_mouse_button_handler(
        In(Some(json!({"button":"Left","duration_ms":60000}))),
        app.world_mut(),
    )
    .unwrap();
    app.update();
    app.update();
    app.world_mut().despawn(window);
    app.update();
    let error = app
        .world()
        .resource::<BrpPointerState>()
        .last_error
        .as_ref()
        .unwrap();
    assert_eq!(error.method, "brp_extras/send_mouse_button");
    assert_eq!(error.window, Some(window.to_bits()));
}
