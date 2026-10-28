//! Single-request screenshot lifecycle and terminal PNG publication.

use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Instant;

use bevy::prelude::*;
use bevy::render::view::screenshot::Screenshot;
use bevy::render::view::screenshot::ScreenshotCaptured;
use bevy_remote::BrpError;
use bevy_remote::BrpResult;
use bevy_remote::error_codes::INTERNAL_ERROR;
use serde_json::Value;

use super::CaptureInput;
use super::screenshot_job;
use super::screenshot_job::CaptureCompletionChannel;
use super::screenshot_job::ImageConverter;
use super::screenshot_job::OwnedTempCapture;
use super::screenshot_job::ScreenshotJob;
use super::screenshot_job::WorkerCompletion;
use crate::activity;
use crate::activity::BrpExtrasActivityCancellation;
use crate::activity::BrpExtrasActivityGuard;
use crate::constants::SCREENSHOT_CAPTURE_DEADLINE;
use crate::constants::SCREENSHOT_ENTITY_NAME;
use crate::screenshot;
use crate::screenshot::request::ScreenshotRequest;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct FrameStamp(u64);

impl FrameStamp {
    const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

enum CaptureStatus {
    Capturing(ScreenshotJob),
    Encoding,
    Completed(Value),
    Failed(BrpError),
}

impl CaptureStatus {
    const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed(_) | Self::Failed(_))
    }
}

struct ActiveCapture {
    activity: Option<BrpExtrasActivityGuard>,
    cancellation: BrpExtrasActivityCancellation,
    deadline: Instant,
    delivered_frame: Option<FrameStamp>,
    request: ScreenshotRequest,
    screenshot_entity: Entity,
    seen_frame: FrameStamp,
    status: CaptureStatus,
}

impl Drop for ActiveCapture {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

impl ActiveCapture {
    fn read(&mut self, current_frame: FrameStamp) -> BrpResult<Option<Value>> {
        self.seen_frame = current_frame;
        if self.delivered_frame.is_some() {
            return Ok(None);
        }

        match &self.status {
            CaptureStatus::Completed(response) => {
                self.delivered_frame = Some(current_frame);
                Ok(Some(response.clone()))
            }
            CaptureStatus::Failed(error) => {
                self.delivered_frame = Some(current_frame);
                Err(error.clone())
            }
            CaptureStatus::Capturing(_) | CaptureStatus::Encoding => Ok(None),
        }
    }
}

#[derive(Resource, Default)]
pub(in crate::screenshot) struct PendingScreenshotCapture {
    active: Option<ActiveCapture>,
    completion_channel: Option<CaptureCompletionChannel>,
    current_frame: FrameStamp,
}

impl PendingScreenshotCapture {
    pub(super) fn read(&mut self, request: &ScreenshotRequest) -> Option<BrpResult<Option<Value>>> {
        let active = self.active.as_mut()?;
        if &active.request != request {
            return Some(Err(capture_in_progress_error()));
        }
        Some(active.read(self.current_frame))
    }

    fn start(
        &mut self,
        request: ScreenshotRequest,
        capture_input: CaptureInput,
        screenshot_entity: Entity,
        now: Instant,
        activity: BrpExtrasActivityGuard,
    ) -> BrpResult<()> {
        if self.active.is_some() {
            return Err(capture_in_progress_error());
        }

        let screenshot_job = ScreenshotJob {
            crop: capture_input.crop,
            path: request.path().to_path_buf(),
            response_metadata: capture_input.response_metadata,
        };
        let cancellation = activity.cancellation();
        self.active = Some(ActiveCapture {
            activity: Some(activity),
            cancellation,
            deadline: now + SCREENSHOT_CAPTURE_DEADLINE,
            delivered_frame: None,
            request,
            screenshot_entity,
            seen_frame: self.current_frame,
            status: CaptureStatus::Capturing(screenshot_job),
        });
        Ok(())
    }

    fn begin_frame(&mut self) -> BrpResult<Option<WorkerCompletion>> {
        self.current_frame = self.current_frame.next();
        let Some(channel) = self.completion_channel.as_ref() else {
            return Ok(None);
        };
        let receiver = channel
            .receiver
            .lock()
            .map_err(|_| capture_error("Screenshot completion channel mutex is poisoned"))?;
        Ok(receiver.try_recv().ok())
    }

    fn begin_encoding(
        &mut self,
        screenshot_entity: Entity,
    ) -> Option<(
        ScreenshotJob,
        Sender<WorkerCompletion>,
        ImageConverter,
        BrpExtrasActivityGuard,
    )> {
        let active = self.active.as_mut()?;
        if active.screenshot_entity != screenshot_entity {
            return None;
        }
        let CaptureStatus::Capturing(_) = active.status else {
            return None;
        };
        let screenshot_job = match std::mem::replace(&mut active.status, CaptureStatus::Encoding) {
            CaptureStatus::Capturing(screenshot_job) => screenshot_job,
            status => {
                active.status = status;
                return None;
            }
        };
        let activity = active.activity.take()?;
        let channel = self.completion_channel.get_or_insert_default();
        Some((
            screenshot_job,
            channel.sender.clone(),
            channel.converter,
            activity,
        ))
    }

    fn complete(&mut self, completion: WorkerCompletion, now: Instant) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        if !matches!(active.status, CaptureStatus::Encoding) {
            return;
        }
        if now >= active.deadline {
            drop(completion);
            active.status = CaptureStatus::Failed(timeout_error());
            return;
        }

        active.status = match completion.result {
            Ok(capture) => publish_capture(active.request.path(), capture),
            Err(error) => CaptureStatus::Failed(error),
        };
    }

    fn fail_completion_channel(&mut self, error: BrpError) {
        if let Some(active) = self.active.as_mut()
            && matches!(active.status, CaptureStatus::Encoding)
        {
            active.status = CaptureStatus::Failed(error);
        }
    }

    fn advance(&mut self, now: Instant) -> Option<Entity> {
        let active = self.active.as_mut()?;
        if active.seen_frame != self.current_frame {
            let screenshot_entity = active.screenshot_entity;
            self.active = None;
            self.completion_channel = None;
            return Some(screenshot_entity);
        }
        if !active.status.is_terminal() && now >= active.deadline {
            active.status = CaptureStatus::Failed(timeout_error());
            return Some(active.screenshot_entity);
        }
        None
    }

    const fn is_active(&self) -> bool {
        self.active.is_some()
    }
}

pub(super) fn start(
    world: &mut World,
    request: ScreenshotRequest,
    capture_input: CaptureInput,
) -> BrpResult<()> {
    super::readbacks::install(world);
    let render_target = capture_input.render_target.clone();
    let screenshot_entity = world
        .spawn((Screenshot(render_target), Name::new(SCREENSHOT_ENTITY_NAME)))
        .observe(on_screenshot_captured)
        .id();
    let activity = activity::begin(world);
    let result = world.resource_mut::<PendingScreenshotCapture>().start(
        request,
        capture_input,
        screenshot_entity,
        Instant::now(),
        activity,
    );
    if result.is_err() {
        world.entity_mut(screenshot_entity).despawn();
    }
    result
}

fn on_screenshot_captured(
    screenshot_captured: On<ScreenshotCaptured>,
    mut pending: ResMut<PendingScreenshotCapture>,
) {
    let Some((screenshot_job, sender, converter, activity)) =
        pending.begin_encoding(screenshot_captured.event().entity)
    else {
        return;
    };
    screenshot_job::start_capture_worker(
        screenshot_captured.event().image.clone(),
        screenshot_job,
        sender,
        converter,
        activity,
    );
}

pub(super) fn ingest_capture_completion(mut pending: ResMut<PendingScreenshotCapture>) {
    match pending.begin_frame() {
        Ok(Some(completion)) => pending.complete(completion, Instant::now()),
        Ok(None) => {}
        Err(error) => pending.fail_completion_channel(error),
    }
}

pub(super) fn screenshot_capture_active(pending: Res<PendingScreenshotCapture>) -> bool {
    pending.is_active()
}

pub(super) fn advance_capture_lifecycle(
    mut commands: Commands,
    mut pending: ResMut<PendingScreenshotCapture>,
) {
    if let Some(screenshot_entity) = pending.advance(Instant::now()) {
        commands.entity(screenshot_entity).try_despawn();
    }
}

fn publish_capture(path: &Path, capture: OwnedTempCapture) -> CaptureStatus {
    if !capture.metadata.dimensions.cmpgt(UVec2::ZERO).all() {
        return CaptureStatus::Failed(capture_error("Screenshot worker produced an empty image"));
    }
    let response_metadata = capture.metadata.response_metadata;
    match capture.temp_path.persist(path) {
        Ok(()) => {
            CaptureStatus::Completed(screenshot::completed_response(path, &response_metadata))
        }
        Err(error) => {
            let message = format!(
                "Failed to publish screenshot to {}: {}",
                path.display(),
                error.error
            );
            drop(error.path);
            CaptureStatus::Failed(capture_error(message))
        }
    }
}

fn capture_error(message: impl Into<String>) -> BrpError {
    BrpError {
        code: INTERNAL_ERROR,
        message: message.into(),
        data: None,
    }
}

fn capture_in_progress_error() -> BrpError {
    capture_error("A screenshot capture is already in progress")
}

fn timeout_error() -> BrpError {
    capture_error(format!(
        "Screenshot capture exceeded the {}-second server deadline",
        SCREENSHOT_CAPTURE_DEADLINE.as_secs()
    ))
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;
    use std::io;
    use std::sync::{Arc, Mutex, mpsc};

    use bevy::MinimalPlugins;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::render::view::screenshot::{CapturedScreenshots, Capturing, trigger_screenshots};
    use bevy_remote::RemotePlugin;
    use screenshot::CaptureResponseMetadata;
    use screenshot_job::CaptureMetadata;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::screenshot::ScreenshotPlugin;

    const ABSENT_DESTINATION_NAME: &str = "new.png";
    const EXISTING_DESTINATION_NAME: &str = "replace.png";
    const IDLE_UPDATE_COUNT: usize = 3;
    const INITIAL_DESTINATION_CONTENT: &[u8] = b"sentinel";
    const SCREENSHOT_CONTENT: &[u8] = b"complete png";

    fn begin_test_capture(world: &mut World, path: &Path) -> (Entity, ScreenshotRequest) {
        world.init_resource::<PendingScreenshotCapture>();
        let request = ScreenshotRequest::from_params(Some(json!({ "path": path }))).unwrap();
        start(
            world,
            request.clone(),
            CaptureInput {
                crop: None,
                render_target: default(),
                response_metadata: CaptureResponseMetadata::Full,
            },
        )
        .unwrap();
        let entity = world
            .resource::<PendingScreenshotCapture>()
            .active
            .as_ref()
            .unwrap()
            .screenshot_entity;
        (entity, request)
    }

    fn abandon_capture(world: &mut World) {
        world
            .resource_mut::<PendingScreenshotCapture>()
            .begin_frame()
            .unwrap();
        world.run_system_once(advance_capture_lifecycle).unwrap();
    }

    fn deliver_readback(world: &mut World, entity: Entity) {
        let (sender, receiver) = mpsc::channel();
        world.insert_resource(CapturedScreenshots(Arc::new(Mutex::new(receiver))));
        sender.send((entity, Image::default())).unwrap();
        super::super::readbacks::install(world);
        world
            .run_system_once::<_, (), _>(super::super::readbacks::forward_readbacks)
            .unwrap();
        world.run_system_once(trigger_screenshots).unwrap();
    }

    fn activity_count(world: &World) -> usize {
        world
            .resource::<activity::BrpExtrasActivity>()
            .state()
            .active_count()
    }

    fn completed_capture(destination: &Path) -> Result<OwnedTempCapture, io::Error> {
        let temp_path = screenshot_job::create_temporary_file(destination, SCREENSHOT_CONTENT)
            .map_err(|error| io::Error::other(error.message))?;
        Ok(OwnedTempCapture {
            metadata: CaptureMetadata {
                dimensions: UVec2::ONE,
                response_metadata: CaptureResponseMetadata::Full,
            },
            temp_path,
        })
    }

    #[test]
    fn idle_capture_plugin_keeps_lifecycle_dormant() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, RemotePlugin::default(), ScreenshotPlugin));
        let (_sender, receiver) = mpsc::channel();
        let source = Arc::new(Mutex::new(receiver));
        app.insert_resource(CapturedScreenshots(source.clone()));

        for _ in 0..IDLE_UPDATE_COUNT {
            app.update();
        }

        let pending = app.world().resource::<PendingScreenshotCapture>();
        assert_eq!(pending.current_frame, FrameStamp::default());
        assert!(!pending.is_active());
        assert!(pending.completion_channel.is_none());
        assert!(Arc::ptr_eq(
            &app.world().resource::<CapturedScreenshots>().0,
            &source
        ));
    }

    #[test]
    fn abandoned_readback_reclaims_entity_and_discards_late_capture() -> Result<(), Box<dyn Error>>
    {
        let temp_dir = TempDir::new()?;
        let path = temp_dir.path().join("abandoned.png");
        let mut world = World::new();
        let (entity, _) = begin_test_capture(&mut world, &path);
        world.entity_mut(entity).insert(Capturing);

        abandon_capture(&mut world);
        assert!(world.get_entity(entity).is_err());
        assert!(!world.resource::<PendingScreenshotCapture>().is_active());
        assert_eq!(
            activity_count(&world),
            0,
            "abandoned requests must release their activity lease"
        );

        deliver_readback(&mut world, entity);
        assert!(world.get_entity(entity).is_err());
        assert_eq!(activity_count(&world), 0);
        assert!(!path.exists(), "abandoned capture must not publish a PNG");
        Ok(())
    }

    #[test]
    fn timed_out_readback_preserves_destination_until_late_capture_is_discarded()
    -> Result<(), Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        let path = temp_dir.path().join("timeout.png");
        fs::write(&path, INITIAL_DESTINATION_CONTENT)?;
        let mut world = World::new();
        let (entity, request) = begin_test_capture(&mut world, &path);
        world.entity_mut(entity).insert(Capturing);
        world
            .resource_mut::<PendingScreenshotCapture>()
            .active
            .as_mut()
            .unwrap()
            .deadline = Instant::now();

        world.run_system_once(advance_capture_lifecycle).unwrap();
        let error = world
            .resource_mut::<PendingScreenshotCapture>()
            .read(&request)
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, INTERNAL_ERROR);
        assert!(world.get_entity(entity).is_err());
        assert_eq!(activity_count(&world), 1);
        abandon_capture(&mut world);
        assert_eq!(activity_count(&world), 0);

        deliver_readback(&mut world, entity);
        assert_eq!(activity_count(&world), 0);
        assert_eq!(fs::read(path)?, INITIAL_DESTINATION_CONTENT);
        Ok(())
    }

    #[test]
    fn late_readback_cannot_complete_a_new_capture() -> Result<(), Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        let old_path = temp_dir.path().join("old.png");
        let new_path = temp_dir.path().join("new.png");
        let mut world = World::new();
        let (old_entity, _) = begin_test_capture(&mut world, &old_path);
        world.entity_mut(old_entity).insert(Capturing);
        abandon_capture(&mut world);
        let (new_entity, new_request) = begin_test_capture(&mut world, &new_path);
        assert_ne!(new_entity, old_entity);
        assert_eq!(activity_count(&world), 1);

        deliver_readback(&mut world, old_entity);
        assert_eq!(activity_count(&world), 1);
        assert!(
            world
                .resource_mut::<PendingScreenshotCapture>()
                .read(&new_request)
                .unwrap()
                .unwrap()
                .is_none()
        );
        assert!(!old_path.exists());
        assert!(!new_path.exists());

        abandon_capture(&mut world);
        assert!(world.get_entity(new_entity).is_err());
        assert_eq!(activity_count(&world), 0);
        Ok(())
    }

    #[test]
    fn unsubmitted_capture_releases_entity_and_activity_on_abandonment()
    -> Result<(), Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        let mut world = World::new();
        let (entity, _) = begin_test_capture(&mut world, &temp_dir.path().join("unsubmitted.png"));
        abandon_capture(&mut world);
        assert!(world.get_entity(entity).is_err());
        assert_eq!(activity_count(&world), 0);
        Ok(())
    }

    #[test]
    fn canceled_capture_without_readback_does_not_leave_an_entity() -> Result<(), Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        let mut world = World::new();
        let (entity, _) =
            begin_test_capture(&mut world, &temp_dir.path().join("never-submitted.png"));
        world.entity_mut(entity).insert(Capturing);
        abandon_capture(&mut world);
        assert!(world.get_entity(entity).is_err());
        assert_eq!(activity_count(&world), 0);
        Ok(())
    }

    #[test]
    fn atomic_publication_replaces_existing_and_creates_absent_destinations()
    -> Result<(), Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        let existing = temp_dir.path().join(EXISTING_DESTINATION_NAME);
        fs::write(&existing, INITIAL_DESTINATION_CONTENT)?;

        let replacement = completed_capture(&existing)?;
        assert!(matches!(
            publish_capture(&existing, replacement),
            CaptureStatus::Completed(_)
        ));
        assert_eq!(fs::read(&existing)?, SCREENSHOT_CONTENT);

        let absent = temp_dir.path().join(ABSENT_DESTINATION_NAME);
        let created = completed_capture(&absent)?;
        assert!(matches!(
            publish_capture(&absent, created),
            CaptureStatus::Completed(_)
        ));
        assert_eq!(fs::read(&absent)?, SCREENSHOT_CONTENT);
        Ok(())
    }
}
