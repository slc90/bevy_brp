//! Wake-aware BRP runtime transport for Bevy applications.
//!
//! Add [`BrpRuntimePlugin`] after Bevy's windowing plugins to install BRP Extras methods together
//! with an HTTP transport that wakes the main event loop whenever remote work arrives.

mod http;
mod progress;

use std::time::Duration;

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;

use self::http::BrpRemoteHttpPlugin;

/// Installs BRP Extras methods and the wake-aware HTTP transport.
pub struct BrpRuntimePlugin {
    /// Maximum time to wait for the final result of a regular HTTP request.
    request_deadline: Duration,
}

impl Default for BrpRuntimePlugin {
    fn default() -> Self {
        Self {
            request_deadline: Duration::from_secs(30),
        }
    }
}

impl Plugin for BrpRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            BrpExtrasPlugin::without_http_transport(),
            BrpRemoteHttpPlugin::new(self.request_deadline),
        ));
    }
}
