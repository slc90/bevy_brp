//! Bevy UI entity bounds and target resolution.

use bevy::camera::visibility::InheritedVisibility;
use bevy::math::{Affine2, DVec2, Rect};
use bevy::prelude::*;
use bevy::ui::CalculatedClip;
use bevy::ui::ComputedNode;
use bevy::ui::ComputedUiRenderTargetInfo;
use bevy::ui::ComputedUiTargetCamera;
use bevy::ui::UiGlobalTransform;
use bevy_remote::BrpError;
use bevy_remote::BrpResult;
use bevy_remote::error_codes::INVALID_PARAMS;

use super::ValidatedCameraTarget;
use super::primary_window;
use super::validated_camera_target;

pub(super) struct ResolvedUiCapture {
    pub(super) camera: ValidatedCameraTarget,
    pub(super) rect: URect,
}

enum UiFamily<'a> {
    Absent,
    Complete {
        computed_node: &'a ComputedNode,
        computed_ui_render_target_info: &'a ComputedUiRenderTargetInfo,
        computed_ui_target_camera: &'a ComputedUiTargetCamera,
        ui_global_transform: &'a UiGlobalTransform,
    },
    Partial,
}

pub(super) fn resolve(
    world: &World,
    entity: Entity,
    requested_camera: Option<Entity>,
    padding: u32,
) -> BrpResult<Option<ResolvedUiCapture>> {
    let (
        computed_node,
        computed_ui_render_target_info,
        computed_ui_target_camera,
        ui_global_transform,
    ) = match classify(world, entity) {
        UiFamily::Absent => return Ok(None),
        UiFamily::Complete {
            computed_node,
            computed_ui_render_target_info,
            computed_ui_target_camera,
            ui_global_transform,
        } => (
            computed_node,
            computed_ui_render_target_info,
            computed_ui_target_camera,
            ui_global_transform,
        ),
        UiFamily::Partial => {
            return Err(ui_error(entity, "has partially initialized UI bounds"));
        }
    };

    let inherited_visibility = world
        .get::<InheritedVisibility>(entity)
        .ok_or_else(|| ui_error(entity, "has no initialized inherited visibility"))?;
    if !inherited_visibility.get() {
        return Err(ui_error(entity, "is hidden"));
    }

    let camera_entity = computed_ui_target_camera
        .get()
        .ok_or_else(|| ui_error(entity, "has no initialized UI target camera"))?;
    if requested_camera.is_some_and(|requested| requested != camera_entity) {
        return Err(ui_error(
            entity,
            "targets a different camera than the requested camera",
        ));
    }

    let primary_window = primary_window(world);
    let camera = validated_camera_target(world, camera_entity, primary_window)
        .ok_or_else(|| invalid_ui_camera_error(entity, camera_entity))?;
    let rect = transformed_rect(
        entity,
        computed_node,
        ui_global_transform,
        computed_ui_render_target_info,
        world.get::<CalculatedClip>(entity),
        &camera,
        padding,
    )?;

    Ok(Some(ResolvedUiCapture { camera, rect }))
}

fn classify(world: &World, entity: Entity) -> UiFamily<'_> {
    match (
        world.get::<ComputedNode>(entity),
        world.get::<UiGlobalTransform>(entity),
        world.get::<ComputedUiTargetCamera>(entity),
        world.get::<ComputedUiRenderTargetInfo>(entity),
    ) {
        (None, None, None, None) => UiFamily::Absent,
        (
            Some(computed_node),
            Some(ui_global_transform),
            Some(computed_ui_target_camera),
            Some(computed_ui_render_target_info),
        ) => UiFamily::Complete {
            computed_node,
            computed_ui_render_target_info,
            computed_ui_target_camera,
            ui_global_transform,
        },
        _ => UiFamily::Partial,
    }
}

fn transformed_rect(
    entity: Entity,
    computed_node: &ComputedNode,
    ui_global_transform: &UiGlobalTransform,
    computed_ui_render_target_info: &ComputedUiRenderTargetInfo,
    calculated_clip: Option<&CalculatedClip>,
    camera: &ValidatedCameraTarget,
    padding: u32,
) -> BrpResult<URect> {
    let size = computed_node.size();
    if !size.is_finite() || !size.cmpgt(Vec2::ZERO).all() {
        return Err(ui_error(entity, "has invalid or empty computed dimensions"));
    }

    let half_size = size / 2.0;
    let affine = ui_global_transform.affine();
    if !affine.matrix2.is_finite() || !affine.translation.is_finite() {
        return Err(ui_error(entity, "has a non-finite UI transform"));
    }
    let corners = [
        affine.transform_point2(Vec2::new(-half_size.x, -half_size.y)),
        affine.transform_point2(Vec2::new(half_size.x, -half_size.y)),
        affine.transform_point2(half_size),
        affine.transform_point2(Vec2::new(-half_size.x, half_size.y)),
    ];
    if corners.iter().any(|corner| !corner.is_finite()) {
        return Err(ui_error(entity, "produced non-finite transformed bounds"));
    }

    let viewport = camera
        .camera
        .physical_viewport_rect()
        .ok_or_else(|| invalid_ui_camera_error(entity, camera.entity))?;
    let viewport_offset = viewport.min.as_dvec2();
    let hard_max = computed_ui_render_target_info
        .physical_size()
        .as_dvec2()
        .min(viewport.size().as_dvec2())
        .min(camera.target_size.as_dvec2() - viewport_offset);
    if !hard_max.cmpgt(DVec2::ZERO).all() {
        return Err(ui_error(entity, "is outside its UI viewport or clip"));
    }
    let mut constraints = rectangle_planes(DVec2::ZERO, hard_max);
    if let Some(clip) = calculated_clip {
        let Some(rects) = clip.rects() else {
            return Err(ui_error(entity, "is fully clipped"));
        };
        for clip in rects {
            append_clip_planes(
                entity,
                clip.rect,
                clip.world_to_clip_local,
                &mut constraints,
            )?;
        }
    }

    // Clip the actual quad before taking bounds; rotated clip AABBs overestimate visibility.
    let visible = clip_polygon(
        entity,
        corners
            .into_iter()
            .map(|corner| corner.as_dvec2())
            .collect(),
        &constraints,
    )?;
    let (min, max) = polygon_bounds(entity, &visible)?;
    let padding = DVec2::splat(f64::from(padding));
    let padded = rectangle_polygon(
        (min - padding).max(DVec2::ZERO),
        (max + padding).min(hard_max),
    );
    let permitted = clip_polygon(entity, padded, &constraints)?;
    let (min, max) = polygon_bounds(entity, &permitted)?;
    let target = URect::from_corners(UVec2::ZERO, camera.target_size);
    let rect = containing_rect(min + viewport_offset, max + viewport_offset)?
        .intersect(viewport)
        .intersect(target);
    if rect.is_empty() {
        return Err(ui_error(entity, "produced an empty crop"));
    }

    Ok(rect)
}

struct HalfPlane {
    normal: DVec2,
    offset: f64,
}

fn rectangle_planes(min: DVec2, max: DVec2) -> Vec<HalfPlane> {
    vec![
        HalfPlane {
            normal: DVec2::X,
            offset: -min.x,
        },
        HalfPlane {
            normal: -DVec2::X,
            offset: max.x,
        },
        HalfPlane {
            normal: DVec2::Y,
            offset: -min.y,
        },
        HalfPlane {
            normal: -DVec2::Y,
            offset: max.y,
        },
    ]
}

fn rectangle_polygon(min: DVec2, max: DVec2) -> Vec<DVec2> {
    vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)]
}

fn append_clip_planes(
    entity: Entity,
    rect: Rect,
    transform: Affine2,
    constraints: &mut Vec<HalfPlane>,
) -> BrpResult<()> {
    if !transform.matrix2.is_finite()
        || !transform.translation.is_finite()
        || transform
            .matrix2
            .x_axis
            .as_dvec2()
            .perp_dot(transform.matrix2.y_axis.as_dvec2())
            == 0.0
    {
        return Err(ui_error(entity, "has an invalid clip transform"));
    }
    for axis in 0..2 {
        let min = f64::from(rect.min[axis]);
        let max = f64::from(rect.max[axis]);
        if min.is_nan()
            || max.is_nan()
            || min >= max
            || min == f64::INFINITY
            || max == f64::NEG_INFINITY
        {
            return Err(ui_error(entity, "has invalid clip coordinates"));
        }
        let normal = DVec2::new(
            f64::from(transform.matrix2.x_axis[axis]),
            f64::from(transform.matrix2.y_axis[axis]),
        );
        let translation = f64::from(transform.translation[axis]);
        // A legal infinite side contributes no constraint and never becomes a transformed corner.
        if min.is_finite() {
            constraints.push(HalfPlane {
                normal,
                offset: translation - min,
            });
        }
        if max.is_finite() {
            constraints.push(HalfPlane {
                normal: -normal,
                offset: max - translation,
            });
        }
    }
    Ok(())
}

fn clip_polygon(
    entity: Entity,
    mut polygon: Vec<DVec2>,
    constraints: &[HalfPlane],
) -> BrpResult<Vec<DVec2>> {
    for plane in constraints {
        let Some(&last) = polygon.last() else { break };
        let mut previous = last;
        let mut previous_distance = plane.normal.dot(previous) + plane.offset;
        let mut output = Vec::with_capacity(polygon.len() + 1);
        for &current in &polygon {
            let distance = plane.normal.dot(current) + plane.offset;
            if !distance.is_finite() || !previous_distance.is_finite() {
                return Err(ui_error(entity, "produced non-finite clip distances"));
            }
            if (distance >= 0.0) != (previous_distance >= 0.0) {
                let t = previous_distance / (previous_distance - distance);
                let intersection = previous + t * (current - previous);
                if !intersection.is_finite() || !t.is_finite() {
                    return Err(ui_error(entity, "produced a non-finite clip intersection"));
                }
                if output.last() != Some(&intersection) {
                    output.push(intersection);
                }
            }
            if distance >= 0.0 && output.last() != Some(&current) {
                output.push(current);
            }
            previous = current;
            previous_distance = distance;
        }
        if output.len() > 1 && output.first() == output.last() {
            output.truncate(output.len() - 1);
        }
        polygon = output;
    }
    Ok(polygon)
}

fn polygon_bounds(entity: Entity, polygon: &[DVec2]) -> BrpResult<(DVec2, DVec2)> {
    let Some(&origin) = polygon.first() else {
        return Err(ui_error(entity, "is outside its UI viewport or clip"));
    };
    let mut min = origin;
    let mut max = origin;
    let mut area = 0.0;
    let mut previous = origin;
    for &point in polygon {
        min = min.min(point);
        max = max.max(point);
        area += (previous - origin).perp_dot(point - origin);
        previous = point;
    }
    if !area.is_finite() || area == 0.0 || !min.is_finite() || !max.is_finite() {
        return Err(ui_error(entity, "has empty or invalid clipped bounds"));
    }
    Ok((min, max))
}

fn containing_rect(min: DVec2, max: DVec2) -> BrpResult<URect> {
    if !min.is_finite()
        || !max.is_finite()
        || min.cmplt(DVec2::ZERO).any()
        || max.cmpgt(DVec2::splat(f64::from(u32::MAX))).any()
    {
        return Err(BrpError {
            code: INVALID_PARAMS,
            message: "UI screenshot bounds contain non-finite coordinates".to_string(),
            data: None,
        });
    }
    Ok(URect::from_corners(
        min.floor().as_uvec2(),
        max.ceil().as_uvec2(),
    ))
}

fn ui_error(entity: Entity, detail: &str) -> BrpError {
    BrpError {
        code: INVALID_PARAMS,
        message: format!("Screenshot UI entity {} {detail}", entity.to_bits()),
        data: None,
    }
}

fn invalid_ui_camera_error(entity: Entity, camera: Entity) -> BrpError {
    ui_error(
        entity,
        &format!(
            "targets camera {}, which is missing, inactive, uninitialized, or has an unsupported target",
            camera.to_bits()
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::io;
    use std::io::Error as IoError;

    use bevy::app::HierarchyPropagatePlugin;
    use bevy::app::PostUpdate;
    use bevy::app::PropagateSet;
    use bevy::app::Update;
    use bevy::asset::RenderAssetUsages;
    use bevy::camera::ComputedCameraValues;
    use bevy::camera::RenderTarget;
    use bevy::camera::RenderTargetInfo;
    use bevy::camera::Viewport;
    use bevy::camera::primitives::Aabb;
    use bevy::camera::visibility::RenderLayers;
    use bevy::math::Affine2;
    use bevy::render::render_resource::Extent3d;
    use bevy::render::render_resource::TextureDimension;
    use bevy::render::render_resource::TextureFormat;
    use bevy::ui::Node;
    use bevy::ui::UiScale;
    use bevy::ui::UiTargetCamera;
    use bevy::ui::update::propagate_ui_target_cameras;
    use bevy::window::WindowRef;
    use bevy::window::WindowResolution;

    use super::*;
    use crate::constants::RESPONSE_BOUNDS_KIND_FIELD;
    use crate::constants::RESPONSE_NAME_FIELD;
    use crate::constants::SCREENSHOT_BOUNDS_KIND_UI;
    use crate::screenshot;
    use crate::screenshot::BoundsKind;
    use crate::screenshot::CaptureResponseMetadata;

    struct TestUi {
        app: App,
        camera: Entity,
        window: Entity,
    }

    fn single_clip(rect: Rect) -> CalculatedClip {
        let mut clip = CalculatedClip::default();
        clip.push_rect(rect, Affine2::IDENTITY);
        clip
    }

    fn ui_app() -> App {
        let mut app = App::new();
        app.init_resource::<UiScale>()
            .add_plugins(HierarchyPropagatePlugin::<ComputedUiTargetCamera>::new(
                PostUpdate,
            ))
            .configure_sets(
                PostUpdate,
                PropagateSet::<ComputedUiTargetCamera>::default(),
            )
            .add_plugins(HierarchyPropagatePlugin::<ComputedUiRenderTargetInfo>::new(
                PostUpdate,
            ))
            .configure_sets(
                PostUpdate,
                PropagateSet::<ComputedUiRenderTargetInfo>::default(),
            )
            .add_systems(Update, propagate_ui_target_cameras);
        app
    }

    fn test_ui(target_size: UVec2, viewport: Option<Viewport>) -> TestUi {
        let mut app = ui_app();
        let window = app
            .world_mut()
            .spawn(Window {
                resolution: WindowResolution::new(target_size.x, target_size.y),
                ..default()
            })
            .id();
        let camera = app
            .world_mut()
            .spawn((
                Camera {
                    computed: ComputedCameraValues {
                        target_info: Some(RenderTargetInfo {
                            physical_size: target_size,
                            scale_factor: 1.0,
                        }),
                        ..default()
                    },
                    viewport,
                    ..default()
                },
                RenderTarget::Window(WindowRef::Entity(window)),
            ))
            .id();
        TestUi {
            app,
            camera,
            window,
        }
    }

    fn image_test_ui(target_size: UVec2) -> TestUi {
        let mut app = ui_app();
        app.init_resource::<Assets<Image>>();
        let image = Image::new_fill(
            Extent3d {
                width: target_size.x,
                height: target_size.y,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        let image_handle = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        let camera = app
            .world_mut()
            .spawn((
                Camera {
                    computed: ComputedCameraValues {
                        target_info: Some(RenderTargetInfo {
                            physical_size: target_size,
                            scale_factor: 1.0,
                        }),
                        ..default()
                    },
                    ..default()
                },
                RenderTarget::Image(image_handle.into()),
            ))
            .id();
        TestUi {
            app,
            camera,
            window: Entity::PLACEHOLDER,
        }
    }

    fn spawn_node(test_ui: &mut TestUi, size: Vec2, affine: Affine2) -> Entity {
        let entity = test_ui
            .app
            .world_mut()
            .spawn((
                Node::default(),
                ComputedNode { size, ..default() },
                UiGlobalTransform::from(affine),
                UiTargetCamera(test_ui.camera),
                InheritedVisibility::VISIBLE,
            ))
            .id();
        test_ui.app.update();
        entity
    }

    fn resolved(
        test_ui: &mut TestUi,
        entity: Entity,
        requested_camera: Option<Entity>,
        padding: u32,
    ) -> Result<ResolvedUiCapture, IoError> {
        resolve(test_ui.app.world_mut(), entity, requested_camera, padding)
            .map_err(|error| io::Error::other(error.message))?
            .ok_or_else(|| io::Error::other("entity was not classified as UI"))
    }

    fn resolution_error(result: BrpResult<Option<ResolvedUiCapture>>) -> Result<BrpError, IoError> {
        match result {
            Ok(_) => Err(io::Error::other(
                "UI bounds resolution unexpectedly succeeded",
            )),
            Err(error) => Ok(error),
        }
    }

    #[test]
    fn transformed_bounds_are_physical_containing_rectangles() -> Result<(), Box<dyn Error>> {
        let mut test_ui = test_ui(UVec2::splat(100), None);
        let affine = Affine2::from_scale_angle_translation(
            Vec2::new(2.0, 0.5),
            std::f32::consts::FRAC_PI_2,
            Vec2::new(30.25, 20.75),
        );
        let entity = spawn_node(&mut test_ui, Vec2::new(10.0, 20.0), affine);

        let resolved = resolved(&mut test_ui, entity, None, 0)?;

        assert_eq!(resolved.rect, URect::new(25, 10, 36, 31));
        Ok(())
    }

    #[test]
    fn clip_and_viewport_translation_are_hard_bounds() -> Result<(), Box<dyn Error>> {
        let viewport = Viewport {
            physical_position: UVec2::new(10, 20),
            physical_size: UVec2::new(60, 40),
            ..default()
        };
        let mut test_ui = test_ui(UVec2::new(100, 80), Some(viewport));
        let entity = spawn_node(
            &mut test_ui,
            Vec2::new(20.0, 10.0),
            Affine2::from_translation(Vec2::new(30.0, 20.0)),
        );
        test_ui
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(single_clip(Rect::new(25.25, 18.25, 35.25, 24.25)));

        let resolved = resolved(&mut test_ui, entity, None, 20)?;

        assert_eq!(resolved.rect, URect::new(35, 38, 46, 45));
        Ok(())
    }

    #[test]
    fn padding_stays_within_viewport_and_live_target_edges() -> Result<(), Box<dyn Error>> {
        let viewport = Viewport {
            physical_position: UVec2::new(90, 70),
            physical_size: UVec2::new(20, 20),
            ..default()
        };
        let mut test_ui = test_ui(UVec2::new(100, 80), Some(viewport));
        let entity = spawn_node(
            &mut test_ui,
            Vec2::splat(4.0),
            Affine2::from_translation(Vec2::splat(2.0)),
        );

        let resolved = resolved(&mut test_ui, entity, None, 20)?;

        assert_eq!(resolved.rect, URect::new(90, 70, 100, 80));
        Ok(())
    }

    #[test]
    fn non_primary_image_targets_use_the_computed_ui_camera() -> Result<(), Box<dyn Error>> {
        let mut test_ui = image_test_ui(UVec2::new(80, 60));
        let entity = spawn_node(
            &mut test_ui,
            Vec2::new(20.0, 10.0),
            Affine2::from_translation(Vec2::new(40.0, 30.0)),
        );

        let resolved = resolved(&mut test_ui, entity, None, 0)?;

        assert_eq!(resolved.camera.entity, test_ui.camera);
        assert_eq!(resolved.rect, URect::new(30, 25, 50, 35));
        Ok(())
    }

    #[test]
    fn partial_ui_and_camera_mismatch_fail_before_aabb_fallback() -> Result<(), Box<dyn Error>> {
        let mut world = World::new();
        let partial = world
            .spawn((
                ComputedNode {
                    size: Vec2::splat(10.0),
                    ..default()
                },
                Aabb::from_min_max(Vec3::splat(-0.5), Vec3::splat(0.5)),
            ))
            .id();
        let error = resolution_error(resolve(&world, partial, None, 0))?;
        assert!(error.message.contains("partially initialized UI bounds"));

        let mut test_ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut test_ui,
            Vec2::splat(10.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        let other_camera = test_ui.app.world_mut().spawn_empty().id();
        let error = resolution_error(resolve(
            test_ui.app.world_mut(),
            entity,
            Some(other_camera),
            0,
        ))?;
        assert!(error.message.contains("different camera"));
        Ok(())
    }

    #[test]
    fn missing_inherited_visibility_is_an_initialization_error() -> Result<(), Box<dyn Error>> {
        let mut test_ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut test_ui,
            Vec2::splat(10.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        test_ui
            .app
            .world_mut()
            .entity_mut(entity)
            .remove::<InheritedVisibility>();

        let error = resolution_error(resolve(test_ui.app.world_mut(), entity, None, 0))?;

        assert_eq!(error.code, INVALID_PARAMS);
        assert!(
            error
                .message
                .contains("no initialized inherited visibility")
        );
        Ok(())
    }

    #[test]
    fn nan_or_wrong_direction_infinity_are_invalid_parameters_errors() -> Result<(), Box<dyn Error>>
    {
        let mut test_ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut test_ui,
            Vec2::splat(10.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        let invalid_clips = [
            Rect {
                min: Vec2::new(f32::NAN, 0.0),
                max: Vec2::splat(60.0),
            },
            Rect {
                min: Vec2::ZERO,
                max: Vec2::new(60.0, f32::NEG_INFINITY),
            },
        ];

        for clip in invalid_clips {
            test_ui
                .app
                .world_mut()
                .entity_mut(entity)
                .insert(single_clip(clip));
            let error = resolution_error(resolve(test_ui.app.world_mut(), entity, None, 0))?;

            assert_eq!(error.code, INVALID_PARAMS);
            assert!(error.message.contains("invalid clip coordinates"));
        }
        Ok(())
    }

    #[test]
    fn ui_precedes_aabb_and_ignores_render_layers() -> Result<(), Box<dyn Error>> {
        let mut test_ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut test_ui,
            Vec2::splat(10.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        test_ui.app.world_mut().entity_mut(entity).insert((
            Aabb::from_min_max(Vec3::splat(-0.5), Vec3::splat(0.5)),
            GlobalTransform::IDENTITY,
            Name::new("Before"),
            RenderLayers::layer(1),
        ));
        test_ui
            .app
            .world_mut()
            .entity_mut(test_ui.camera)
            .insert(RenderLayers::layer(2));

        let capture_input = screenshot::entity_capture_input(
            test_ui.app.world_mut(),
            entity,
            Some(test_ui.camera),
            0,
        )
        .map_err(|error| io::Error::other(error.message))?;
        test_ui
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(Name::new("After"));
        let response = screenshot::completed_response(
            std::path::Path::new("/tmp/ui-entity.png"),
            &capture_input.response_metadata,
        );

        let CaptureResponseMetadata::Entity(metadata) = capture_input.response_metadata else {
            return Err(io::Error::other("expected entity response metadata").into());
        };
        assert_eq!(metadata.bounds_kind, BoundsKind::Ui);
        assert_eq!(metadata.rect, URect::new(45, 45, 55, 55));
        assert_eq!(metadata.name.as_deref(), Some("Before"));
        assert_eq!(
            response
                .get(RESPONSE_BOUNDS_KIND_FIELD)
                .and_then(serde_json::Value::as_str),
            Some(SCREENSHOT_BOUNDS_KIND_UI)
        );
        assert_eq!(
            response
                .get(RESPONSE_NAME_FIELD)
                .and_then(serde_json::Value::as_str),
            Some("Before")
        );
        Ok(())
    }

    #[test]
    fn hidden_offscreen_and_missing_live_targets_fail() -> Result<(), Box<dyn Error>> {
        let mut test_ui = test_ui(UVec2::splat(100), None);
        let hidden = spawn_node(
            &mut test_ui,
            Vec2::splat(10.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        test_ui
            .app
            .world_mut()
            .entity_mut(hidden)
            .insert(InheritedVisibility::HIDDEN);
        assert!(resolve(test_ui.app.world_mut(), hidden, None, 0).is_err());

        let offscreen = spawn_node(
            &mut test_ui,
            Vec2::splat(10.0),
            Affine2::from_translation(Vec2::splat(150.0)),
        );
        assert!(resolve(test_ui.app.world_mut(), offscreen, None, 0).is_err());

        let live = spawn_node(
            &mut test_ui,
            Vec2::splat(10.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        test_ui.app.world_mut().despawn(test_ui.window);
        let error = resolution_error(resolve(test_ui.app.world_mut(), live, None, 0))?;
        assert!(error.message.contains("unsupported target"));
        Ok(())
    }

    #[test]
    fn empty_inherited_clips_preserve_bounds_but_fully_clipped_rejects() {
        let mut test_ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut test_ui,
            Vec2::splat(10.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        test_ui
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(CalculatedClip::default());
        assert_eq!(
            resolved(&mut test_ui, entity, None, 0).unwrap().rect,
            URect::new(45, 45, 55, 55)
        );
        test_ui
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(CalculatedClip::FullyClipped);
        assert_eq!(
            resolution_error(resolve(test_ui.app.world(), entity, None, 0))
                .unwrap()
                .code,
            INVALID_PARAMS
        );
    }

    #[test]
    fn legal_unbounded_axis_preserves_the_other_axis_and_padding() {
        let mut test_ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut test_ui,
            Vec2::splat(20.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        let mut clip = CalculatedClip::default();
        clip.push_rect(
            Rect {
                min: Vec2::new(f32::NEG_INFINITY, 45.0),
                max: Vec2::new(f32::INFINITY, 55.0),
            },
            Affine2::IDENTITY,
        );
        test_ui.app.world_mut().entity_mut(entity).insert(clip);
        assert_eq!(
            resolved(&mut test_ui, entity, None, 5).unwrap().rect,
            URect::new(35, 45, 65, 55)
        );
    }

    #[test]
    fn rotated_clip_uses_its_local_half_planes() {
        let mut test_ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut test_ui,
            Vec2::splat(40.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        let mut clip = CalculatedClip::default();
        clip.push_rect(
            Rect::new(-10.0, -10.0, 10.0, 10.0),
            Affine2::from_angle(std::f32::consts::FRAC_PI_4)
                * Affine2::from_translation(Vec2::splat(-50.0)),
        );
        test_ui.app.world_mut().entity_mut(entity).insert(clip);
        assert_eq!(
            resolved(&mut test_ui, entity, None, 0).unwrap().rect,
            URect::new(35, 35, 65, 65)
        );
    }

    #[test]
    fn all_inherited_clips_intersect_before_padding() {
        let mut test_ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut test_ui,
            Vec2::splat(40.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        let mut clip = CalculatedClip::default();
        clip.push_rect(Rect::new(30.0, 40.0, 55.0, 60.0), Affine2::IDENTITY);
        clip.push_rect(Rect::new(45.0, 30.0, 70.0, 55.0), Affine2::IDENTITY);
        test_ui.app.world_mut().entity_mut(entity).insert(clip);
        assert_eq!(
            resolved(&mut test_ui, entity, None, u32::MAX).unwrap().rect,
            URect::new(45, 40, 55, 55)
        );
    }

    fn diamond_clip(center: Vec2) -> (Rect, Affine2) {
        // x+y and y-x bound a diamond with radius 20 using exactly representable values.
        let transform = Affine2::from_mat2_translation(
            bevy::math::Mat2::from_cols(Vec2::new(1.0, -1.0), Vec2::ONE),
            Vec2::new(-center.x - center.y, center.x - center.y),
        );
        (Rect::new(-20.0, -20.0, 20.0, 20.0), transform)
    }

    #[test]
    fn multiple_rotated_clips_use_their_true_intersection() {
        let mut ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut ui,
            Vec2::splat(100.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        let mut clip = CalculatedClip::default();
        for center in [Vec2::splat(50.0), Vec2::new(60.0, 50.0)] {
            let (rect, transform) = diamond_clip(center);
            clip.push_rect(rect, transform);
        }
        ui.app.world_mut().entity_mut(entity).insert(clip);
        assert_eq!(
            resolved(&mut ui, entity, None, 0).unwrap().rect,
            URect::new(40, 35, 70, 65)
        );
        assert_eq!(
            resolved(&mut ui, entity, None, u32::MAX).unwrap().rect,
            URect::new(40, 35, 70, 65)
        );
    }

    #[test]
    fn overlapping_aabbs_do_not_make_disjoint_rotated_regions_visible() {
        let mut ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut ui,
            Vec2::splat(6.0),
            Affine2::from_translation(Vec2::splat(67.0)),
        );
        let (rect, transform) = diamond_clip(Vec2::splat(50.0));
        let mut clip = CalculatedClip::default();
        clip.push_rect(rect, transform);
        ui.app.world_mut().entity_mut(entity).insert(clip);
        assert_eq!(
            resolution_error(resolve(ui.app.world(), entity, None, u32::MAX))
                .unwrap()
                .code,
            INVALID_PARAMS
        );
    }

    #[test]
    fn crossing_strips_and_subpixel_area_are_visible_but_tangent_edges_are_not() {
        let mut ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut ui,
            Vec2::new(80.0, 4.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        for (clip, expected) in [
            (
                Rect::new(48.0, 10.0, 52.0, 90.0),
                URect::new(48, 48, 52, 52),
            ),
            (
                Rect::new(49.875, 49.875, 50.125, 50.125),
                URect::new(49, 49, 51, 51),
            ),
        ] {
            ui.app
                .world_mut()
                .entity_mut(entity)
                .insert(single_clip(clip));
            assert_eq!(resolved(&mut ui, entity, None, 0).unwrap().rect, expected);
        }
        for clip in [
            Rect::new(90.0, 40.0, 95.0, 60.0),
            Rect::new(90.0, 52.0, 95.0, 60.0),
        ] {
            ui.app
                .world_mut()
                .entity_mut(entity)
                .insert(single_clip(clip));
            assert!(resolve(ui.app.world(), entity, None, 100).is_err());
        }
    }

    #[test]
    fn mirrored_nonuniform_nodes_preserve_winding_independent_bounds() {
        let mut ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut ui,
            Vec2::new(10.0, 20.0),
            Affine2::from_scale_angle_translation(Vec2::new(-2.0, 0.5), 0.0, Vec2::splat(50.0)),
        );
        assert_eq!(
            resolved(&mut ui, entity, None, 0).unwrap().rect,
            URect::new(40, 45, 60, 55)
        );
        ui.app
            .world_mut()
            .entity_mut(entity)
            .insert(UiGlobalTransform::from(Affine2::from_scale(Vec2::new(
                0.0, 1.0,
            ))));
        assert!(resolve(ui.app.world(), entity, None, 0).is_err());
    }

    #[test]
    fn invalid_clip_transforms_and_directed_ranges_never_expand_capture() {
        let mut ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut ui,
            Vec2::splat(20.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        for transform in [
            Affine2::from_scale(Vec2::new(0.0, 1.0)),
            Affine2::from_translation(Vec2::new(f32::NAN, 0.0)),
            Affine2::from_scale(Vec2::splat(f32::INFINITY)),
        ] {
            let mut clip = CalculatedClip::default();
            clip.push_rect(Rect::new(40.0, 40.0, 60.0, 60.0), transform);
            ui.app.world_mut().entity_mut(entity).insert(clip);
            assert!(resolve(ui.app.world(), entity, None, u32::MAX).is_err());
        }
        for rect in [
            Rect {
                min: Vec2::splat(60.0),
                max: Vec2::splat(40.0),
            },
            Rect {
                min: Vec2::new(f32::NAN, 40.0),
                max: Vec2::splat(60.0),
            },
            Rect::new(50.0, 40.0, 50.0, 60.0),
        ] {
            ui.app
                .world_mut()
                .entity_mut(entity)
                .insert(single_clip(rect));
            assert!(resolve(ui.app.world(), entity, None, u32::MAX).is_err());
        }
    }

    #[test]
    fn rotated_unbounded_axis_never_transforms_infinite_corners() {
        let mut ui = test_ui(UVec2::splat(100), None);
        let entity = spawn_node(
            &mut ui,
            Vec2::splat(20.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        let (_, transform) = diamond_clip(Vec2::splat(50.0));
        let mut clip = CalculatedClip::default();
        clip.push_rect(
            Rect {
                min: Vec2::new(-10.0, f32::NEG_INFINITY),
                max: Vec2::new(10.0, f32::INFINITY),
            },
            transform,
        );
        ui.app.world_mut().entity_mut(entity).insert(clip);
        assert_eq!(
            resolved(&mut ui, entity, None, 10).unwrap().rect,
            URect::new(30, 30, 70, 70)
        );
    }

    #[test]
    fn engine_calculated_override_fixed_and_hidden_clips_drive_resolution() {
        let mut ui = test_ui(UVec2::splat(100), None);
        let parent = spawn_node(
            &mut ui,
            Vec2::splat(10.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        ui.app.world_mut().get_mut::<Node>(parent).unwrap().overflow = Overflow::clip();
        let child = spawn_node(
            &mut ui,
            Vec2::splat(20.0),
            Affine2::from_translation(Vec2::splat(50.0)),
        );
        ui.app.world_mut().entity_mut(parent).add_child(child);
        ui.app
            .add_systems(Last, bevy::ui::update::update_clipping_system);
        ui.app.update();
        assert_eq!(
            resolved(&mut ui, child, None, 0).unwrap().rect,
            URect::new(45, 45, 55, 55)
        );
        ui.app
            .world_mut()
            .entity_mut(child)
            .insert(bevy::ui::OverrideClip);
        ui.app.update();
        assert_eq!(
            resolved(&mut ui, child, None, 0).unwrap().rect,
            URect::new(40, 40, 60, 60)
        );
        ui.app
            .world_mut()
            .entity_mut(child)
            .remove::<bevy::ui::OverrideClip>()
            .insert(bevy::ui::FixedNode);
        ui.app.update();
        assert_eq!(
            resolved(&mut ui, child, None, 0).unwrap().rect,
            URect::new(40, 40, 60, 60)
        );
        ui.app.world_mut().get_mut::<Node>(child).unwrap().display = Display::None;
        ui.app.update();
        assert!(
            ui.app
                .world()
                .get::<CalculatedClip>(child)
                .unwrap()
                .is_fully_clipped()
        );
        assert!(resolve(ui.app.world(), child, None, 0).is_err());
    }
}
