use bevy::prelude::*;
use bevy_brp_runtime::BrpRuntimePlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(BrpRuntimePlugin::with_port(15752))
        .run();
}
