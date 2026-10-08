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
    drag_starts: Vec<Pointer<bevy::picking::events::DragStart>>,
    drag_ends: Vec<Pointer<bevy::picking::events::DragEnd>>,
    drag_drops: Vec<Pointer<bevy::picking::events::DragDrop>>,
    cancels: Vec<Pointer<Cancel>>,
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
    mut drag_starts: MessageReader<Pointer<bevy::picking::events::DragStart>>,
    mut drag_ends: MessageReader<Pointer<bevy::picking::events::DragEnd>>,
    mut drag_drops: MessageReader<Pointer<bevy::picking::events::DragDrop>>,
    mut cancels: MessageReader<Pointer<Cancel>>,
) {
    trace.inputs.extend(inputs.read().cloned());
    trace.clicks.extend(clicks.read().cloned());
    trace.scrolls.extend(scrolls.read().cloned());
    trace.drag_starts.extend(drag_starts.read().cloned());
    trace.drag_ends.extend(drag_ends.read().cloned());
    trace.drag_drops.extend(drag_drops.read().cloned());
    trace.cancels.extend(cancels.read().cloned());
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

#[derive(Resource)]
struct SecondTarget(Entity);

fn hits(
    pointers: Query<(&PointerId, &PointerLocation)>,
    target: Res<Target>,
    second: Option<Res<SecondTarget>>,
    mut output: MessageWriter<PointerHits>,
) {
    for (id, location) in &pointers {
        if let Some(location) = &location.location {
            let target = if location.position.x < 100.0 {
                Some(target.0)
            } else {
                second.as_ref().map(|target| target.0)
            };
            let Some(target) = target else {
                continue;
            };
            output.write(PointerHits::new(
                *id,
                vec![(target, HitData::new(target, 0.0, None, None))],
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

fn finish_work(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while app
        .world()
        .resource::<crate::BrpExtrasActivity>()
        .state()
        .is_active()
    {
        assert!(Instant::now() < deadline, "Pointer work failed to converge");
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn first_click_establishes_hit_and_releases_on_later_picking_cycles() {
    let (mut app, window, target) = fixture();
    super::super::click::click_mouse_handler(In(Some(json!({"button":"Left"}))), app.world_mut())
        .unwrap();
    finish_work(&mut app);
    let trace = app.world().resource::<Trace>();
    assert_eq!(trace.clicks.len(), 1);
    assert_eq!(trace.clicks[0].entity, target);
    assert_eq!(
        trace.clicks[0].pointer_location,
        location(window, Vec2::ZERO)
    );
    assert_eq!(trace.inputs.len(), 3);
    assert!(matches!(trace.inputs[0].action, PointerAction::Move { .. }));
    assert!(matches!(
        trace.inputs[1].action,
        PointerAction::Press(PointerButton::Primary)
    ));
    assert!(matches!(
        trace.inputs[2].action,
        PointerAction::Release(PointerButton::Primary)
    ));
    assert_eq!(trace.raw_count, 0);
}

#[test]
fn double_click_count_comes_from_the_engine_interval() {
    for (interval_ms, delay_ms, expected) in [(1000, 0, vec![1, 2]), (50, 75, vec![1, 1])] {
        let (mut app, _, target) = fixture();
        app.world_mut()
            .resource_mut::<PickingSettings>()
            .multi_click_interval = Duration::from_millis(interval_ms);
        super::super::click::double_click_mouse_handler(
            In(Some(json!({"button":"Left","delay_ms":delay_ms}))),
            app.world_mut(),
        )
        .unwrap();
        finish_work(&mut app);
        let trace = app.world().resource::<Trace>();
        assert_eq!(
            trace
                .clicks
                .iter()
                .map(|event| event.count)
                .collect::<Vec<_>>(),
            expected
        );
        assert!(trace.clicks.iter().all(|event| event.entity == target));
        assert_eq!(trace.raw_count, 0);
        assert!(
            trace
                .inputs
                .iter()
                .all(|input| input.pointer_id.is_custom())
        );
    }
}

#[test]
fn minimum_drag_visits_start_before_press_and_end_before_release() {
    let (mut app, window, target) = fixture();
    super::super::drag::drag_mouse_handler(
        In(Some(
            json!({"button":"Left","start":[5.0,6.0],"end":[70.0,80.0],"frames":1}),
        )),
        app.world_mut(),
    )
    .unwrap();
    finish_work(&mut app);
    let trace = app.world().resource::<Trace>();
    assert_eq!(trace.inputs.len(), 4);
    assert!(matches!(trace.inputs[0].action, PointerAction::Move { .. }));
    assert_eq!(
        trace.inputs[0].location,
        location(window, Vec2::new(5.0, 6.0))
    );
    assert!(matches!(trace.inputs[1].action, PointerAction::Press(_)));
    assert_eq!(
        trace.inputs[2].location,
        location(window, Vec2::new(70.0, 80.0))
    );
    assert!(matches!(trace.inputs[3].action, PointerAction::Release(_)));
    assert_eq!(trace.drag_starts.len(), 1);
    assert_eq!(trace.drag_starts[0].entity, target);
    assert_eq!(trace.drag_ends.len(), 1);
    assert_eq!(trace.raw_count, 0);
}

#[test]
fn queued_click_move_click_does_not_collapse_two_targets_into_one_cycle() {
    let (mut app, _, target) = fixture();
    super::super::click::click_mouse_handler(In(Some(json!({"button":"Left"}))), app.world_mut())
        .unwrap();
    super::super::cursor::move_mouse_handler(
        In(Some(json!({"position":[200.0,10.0]}))),
        app.world_mut(),
    )
    .unwrap();
    super::super::click::click_mouse_handler(In(Some(json!({"button":"Left"}))), app.world_mut())
        .unwrap();
    finish_work(&mut app);
    let trace = app.world().resource::<Trace>();
    assert_eq!(
        trace
            .clicks
            .iter()
            .filter(|event| event.entity == target)
            .count(),
        1
    );
    assert_eq!(trace.raw_count, 0);
}

#[test]
fn cancel_during_auto_click_discards_following_moves_and_timers_without_click() {
    let (mut app, _, _) = fixture();
    super::super::click::click_mouse_handler(In(Some(json!({"button":"Left"}))), app.world_mut())
        .unwrap();
    super::super::cursor::move_mouse_handler(
        In(Some(json!({"position":[40.0,50.0]}))),
        app.world_mut(),
    )
    .unwrap();
    app.update();
    app.update();
    release(app.world_mut());
    finish_work(&mut app);
    let trace = app.world().resource::<Trace>();
    assert_eq!(trace.clicks.len(), 0);
    assert!(
        !trace
            .inputs
            .iter()
            .any(|input| matches!(input.action, PointerAction::Release(_)))
    );
    assert!(
        !trace
            .inputs
            .iter()
            .any(|input| input.location.position == Vec2::new(40.0, 50.0))
    );
    assert_eq!(trace.raw_count, 0);
}

#[test]
fn automatic_drag_waits_for_timed_hold_and_later_move_stays_behind_it() {
    let (mut app, _, _) = fixture();
    super::super::button::send_mouse_button_handler(
        In(Some(json!({"button":"Middle","duration_ms":30}))),
        app.world_mut(),
    )
    .unwrap();
    super::super::drag::drag_mouse_handler(
        In(Some(
            json!({"button":"Left","start":[10.0,0.0],"end":[50.0,0.0],"frames":1}),
        )),
        app.world_mut(),
    )
    .unwrap();
    super::super::cursor::move_mouse_handler(
        In(Some(json!({"position":[80.0,0.0]}))),
        app.world_mut(),
    )
    .unwrap();
    app.update();
    app.update();
    app.update();
    assert_eq!(app.world().resource::<Trace>().inputs.len(), 2);
    finish_work(&mut app);
    let trace = app.world().resource::<Trace>();
    let release_middle = trace
        .inputs
        .iter()
        .position(|input| matches!(input.action, PointerAction::Release(PointerButton::Middle)))
        .unwrap();
    let press_left = trace
        .inputs
        .iter()
        .position(|input| matches!(input.action, PointerAction::Press(PointerButton::Primary)))
        .unwrap();
    assert!(release_middle < press_left);
    assert_eq!(
        trace.inputs.last().unwrap().location.position,
        Vec2::new(80.0, 0.0)
    );
    assert_eq!(trace.raw_count, 0);
}

#[test]
fn drag_to_second_target_produces_engine_drop_and_one_exact_endpoint_step() {
    let (mut app, _, original) = fixture();
    let destination = app.world_mut().spawn_empty().id();
    app.insert_resource(SecondTarget(destination));
    super::super::drag::drag_mouse_handler(
        In(Some(
            json!({"button":"Left","start":[10.0,0.0],"end":[200.0,0.0],"frames":3}),
        )),
        app.world_mut(),
    )
    .unwrap();
    finish_work(&mut app);
    let trace = app.world().resource::<Trace>();
    assert_eq!(trace.drag_starts[0].entity, original);
    assert_eq!(trace.drag_ends[0].entity, original);
    assert_eq!(trace.drag_drops.len(), 1);
    assert_eq!(trace.drag_drops[0].entity, destination);
    assert_eq!(
        trace
            .inputs
            .iter()
            .filter(|input| matches!(input.action, PointerAction::Move { .. })
                && input.location.position == Vec2::new(200.0, 0.0))
            .count(),
        1
    );
}

#[test]
fn cancelled_drag_after_leaving_hit_never_sends_success_end_or_drop() {
    let (mut app, _, original) = fixture();
    super::super::drag::drag_mouse_handler(
        In(Some(
            json!({"button":"Left","start":[0.0,0.0],"end":[400.0,0.0],"frames":3}),
        )),
        app.world_mut(),
    )
    .unwrap();
    app.update();
    app.update();
    app.update();
    release(app.world_mut());
    finish_work(&mut app);
    let trace = app.world().resource::<Trace>();
    assert_eq!(trace.drag_starts.len(), 1);
    assert_eq!(
        trace
            .cancels
            .iter()
            .filter(|event| event.entity == original)
            .count(),
        1
    );
    assert_eq!(trace.drag_ends.len(), 0);
    assert_eq!(trace.drag_drops.len(), 0);
    assert_eq!(trace.clicks.len(), 0);
}

#[test]
fn zero_distance_drag_finishes_with_no_pressed_state() {
    let (mut app, _, _) = fixture();
    super::super::drag::drag_mouse_handler(
        In(Some(
            json!({"button":"Right","start":[0.0,0.0],"end":[0.0,0.0],"frames":1}),
        )),
        app.world_mut(),
    )
    .unwrap();
    finish_work(&mut app);
    let state = app.world().resource::<BrpPointerState>();
    assert!(state.pressed.is_empty());
    assert!(
        !app.world()
            .get::<PointerPress>(state.entity.unwrap())
            .unwrap()
            .is_any_pressed()
    );
    assert_eq!(app.world().resource::<Trace>().raw_count, 0);
}

#[test]
fn destroying_custom_entity_cancels_work_instead_of_waiting_forever_for_location() {
    let (mut app, _, _) = fixture();
    super::super::click::click_mouse_handler(In(Some(json!({"button":"Left"}))), app.world_mut())
        .unwrap();
    let entity = app.world().resource::<BrpPointerState>().entity.unwrap();
    app.world_mut().despawn(entity);
    for _ in 0..3 {
        app.update();
    }
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
fn lost_pointer_component_cancels_press_and_next_generation_reuses_only_identity() {
    let (mut app, _, target) = fixture();
    super::super::click::click_mouse_handler(In(Some(json!({"button":"Left"}))), app.world_mut())
        .unwrap();
    app.update();
    app.update();
    let state = app.world().resource::<BrpPointerState>();
    let (entity, id) = (state.entity.unwrap(), state.id);
    app.world_mut()
        .entity_mut(entity)
        .remove::<PointerLocation>();
    app.update();
    app.update();
    let state = app.world().resource::<BrpPointerState>();
    assert_eq!(state.phase, Phase::Inactive);
    assert_eq!(
        state.last_error.as_ref().unwrap().method,
        "brp_extras/click_mouse"
    );
    assert_eq!(
        app.world()
            .resource::<Trace>()
            .cancels
            .iter()
            .filter(|event| event.entity == target)
            .count(),
        1
    );
    assert!(app.world().resource::<Trace>().clicks.is_empty());
    let engine = app
        .world()
        .resource::<bevy::picking::events::PointerState>();
    let button = engine.get(id.unwrap(), PointerButton::Primary).unwrap();
    assert!(button.pressing.is_empty());
    assert!(button.dragging.is_empty());
    super::super::click::click_mouse_handler(In(Some(json!({"button":"Left"}))), app.world_mut())
        .unwrap();
    finish_work(&mut app);
    let state = app.world().resource::<BrpPointerState>();
    assert_eq!(state.id, id);
    assert_ne!(state.entity, Some(entity));
    let trace = app.world().resource::<Trace>();
    assert_eq!(trace.clicks.len(), 1);
}

fn control(app: &mut App, action: &str) -> serde_json::Value {
    super::super::control::pointer_control_handler(
        In(Some(json!({"action":action}))),
        app.world_mut(),
    )
    .unwrap()
}

#[test]
fn control_snapshot_tracks_queue_press_cleanup_and_idle_without_waking() {
    let (mut app, window, _) = fixture();
    super::super::button::send_mouse_button_handler(
        In(Some(json!({"button":"Left","duration_ms":60000}))),
        app.world_mut(),
    )
    .unwrap();
    let queued = control(&mut app, "status");
    assert_eq!(queued["phase"], "active");
    assert_eq!(queued["window"], window.to_bits());
    assert_eq!(queued["queued_actions"], 1);
    let id = queued["pointer_id"].as_str().unwrap();
    uuid::Uuid::parse_str(id).unwrap();
    app.update();
    app.update();
    let pressed = control(&mut app, "status");
    assert_eq!(pressed["pressed_buttons"], json!(["Left"]));
    assert_eq!(pressed["queued_actions"], 0);
    assert_eq!(pressed["busy"], true);
    let draining = control(&mut app, "release");
    assert_eq!(draining["phase"], "draining");
    assert_eq!(
        draining["generation"],
        queued["generation"].as_u64().unwrap() + 1
    );
    assert_eq!(control(&mut app, "release"), draining);
    let rejected = super::super::cursor::move_mouse_handler(
        In(Some(json!({"position":[20.0,30.0]}))),
        app.world_mut(),
    )
    .unwrap_err();
    assert_eq!(rejected.code, INVALID_PARAMS);
    app.update();
    assert_eq!(control(&mut app, "status")["phase"], "draining");
    app.update();
    let inactive = control(&mut app, "status");
    assert_eq!(inactive["phase"], "inactive");
    assert_eq!(inactive["busy"], false);
    assert_eq!(inactive["pressed_buttons"], json!([]));
    assert_eq!(inactive["window"], serde_json::Value::Null);
    assert_eq!(inactive["pointer_id"], id);
    assert_eq!(control(&mut app, "release"), inactive);
    super::super::cursor::move_mouse_handler(
        In(Some(json!({"position":[20.0,30.0]}))),
        app.world_mut(),
    )
    .unwrap();
    app.update();
    let idle = control(&mut app, "status");
    assert_eq!(idle["phase"], "active");
    assert_eq!(idle["busy"], false);
    for _ in 0..3 {
        assert_eq!(control(&mut app, "status"), idle);
    }
    assert!(
        !app.world()
            .resource::<crate::BrpExtrasActivity>()
            .state()
            .is_active()
    );
}

#[test]
fn control_preserves_failure_until_successful_reactivation() {
    let (mut app, window, _) = fixture();
    super::super::click::click_mouse_handler(In(Some(json!({"button":"Middle"}))), app.world_mut())
        .unwrap();
    let failed_generation = control(&mut app, "status")["generation"].clone();
    app.world_mut().despawn(window);
    app.update();
    app.update();
    let failed = control(&mut app, "status");
    assert_eq!(failed["phase"], "inactive");
    assert_eq!(failed["busy"], false);
    assert_eq!(failed["last_error"]["generation"], failed_generation);
    assert_eq!(failed["last_error"]["method"], "brp_extras/click_mouse");
    assert_eq!(failed["last_error"]["window"], window.to_bits());
    assert_eq!(failed["last_error"]["code"], INVALID_PARAMS);
    assert!(failed["last_error"]["message"].is_string());
    assert_eq!(control(&mut app, "release"), failed);
    let replacement = app.world_mut().spawn(Window::default()).id();
    super::super::cursor::move_mouse_handler(
        In(Some(
            json!({"position":[0.0,0.0],"window":replacement.to_bits()}),
        )),
        app.world_mut(),
    )
    .unwrap();
    assert_eq!(
        control(&mut app, "status")["last_error"],
        serde_json::Value::Null
    );
}

#[test]
fn non_object_control_and_non_string_actions_cannot_cancel_active_input() {
    let (mut app, _, _) = fixture();
    super::super::click::click_mouse_handler(In(Some(json!({"button":"Left"}))), app.world_mut())
        .unwrap();
    let before = control(&mut app, "status");
    for params in [json!(["release"]), json!({"action":{"release":null}})] {
        let error =
            super::super::control::pointer_control_handler(In(Some(params)), app.world_mut())
                .unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);
        assert_eq!(control(&mut app, "status"), before);
    }
}
