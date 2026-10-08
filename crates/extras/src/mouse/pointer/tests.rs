use super::*;
use bevy::picking::backend::{HitData, PointerHits};
use bevy::picking::{InteractionPlugin, PickingPlugin};
use bevy::window::PrimaryWindow;

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
