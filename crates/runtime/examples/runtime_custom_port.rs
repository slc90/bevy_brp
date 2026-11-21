use bevy::{app::AppExit, prelude::*};
use bevy_brp_runtime::BrpRuntimePlugin;

fn main() -> AppExit {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(BrpRuntimePlugin::with_ports(15752, 15753))
        .run()
}
