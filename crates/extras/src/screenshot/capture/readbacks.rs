//! Filter reclaimed request entities before Bevy consumes screenshot readbacks.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::sync::mpsc::Sender;

use bevy::prelude::*;
use bevy::render::view::screenshot::CapturedScreenshots;

#[derive(Resource)]
pub(super) struct ReadbackRelay {
    source: CapturedScreenshots,
    destination: Sender<(Entity, Image)>,
}

pub(super) fn install(world: &mut World) {
    if !world.contains_resource::<ReadbackRelay>() {
        let Some(source) = world.remove_resource::<CapturedScreenshots>() else {
            return;
        };
        let (destination, receiver) = mpsc::channel();
        world.insert_resource(CapturedScreenshots(Arc::new(Mutex::new(receiver))));
        world.insert_resource(ReadbackRelay {
            source,
            destination,
        });
    }
}

pub(super) fn forward_readbacks(relay: Res<ReadbackRelay>, entities: Query<()>) -> Result {
    // The render world retains the original sender. Results arriving after this system
    // stay in the source channel until the next frame and cannot bypass the check.
    let source = relay.source.0.lock().map_err(|error| {
        BevyError::error(format!(
            "Screenshot readback channel mutex is poisoned: {error}"
        ))
    })?;
    while let Ok((entity, image)) = source.try_recv() {
        if entities.get(entity).is_ok() {
            relay.destination.send((entity, image)).map_err(|error| {
                BevyError::error(format!("Screenshot delivery channel is closed: {error}"))
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::render::view::screenshot::{Captured, trigger_screenshots};

    use super::*;

    #[test]
    fn readbacks_preserve_live_results_and_discard_reclaimed_generations() {
        let mut world = World::new();
        let (sender, receiver) = mpsc::channel();
        world.insert_resource(CapturedScreenshots(Arc::new(Mutex::new(receiver))));
        let live = world.spawn_empty().id();
        let reclaimed = world.spawn_empty().id();
        let replacement = world.despawn_no_free(reclaimed).unwrap();
        world.spawn_empty_at(replacement).unwrap();
        assert_eq!(reclaimed.index(), replacement.index());
        assert_ne!(reclaimed, replacement);
        sender.send((live, Image::default())).unwrap();
        sender.send((reclaimed, Image::default())).unwrap();

        install(&mut world);
        world
            .run_system_once::<_, (), _>(forward_readbacks)
            .unwrap();
        sender.send((reclaimed, Image::default())).unwrap();
        world.run_system_once(trigger_screenshots).unwrap();
        assert!(world.get::<Captured>(live).is_some());
        assert!(world.get::<Captured>(replacement).is_none());

        world
            .run_system_once::<_, (), _>(forward_readbacks)
            .unwrap();
        world.run_system_once(trigger_screenshots).unwrap();
        assert!(world.get::<Captured>(replacement).is_none());
        sender.send((replacement, Image::default())).unwrap();
        world
            .run_system_once::<_, (), _>(forward_readbacks)
            .unwrap();
        world.run_system_once(trigger_screenshots).unwrap();
        assert!(world.get::<Captured>(replacement).is_some());
    }
}
