use async_trait::async_trait;

use super::constants::DEFAULT_PROFILE;
use super::launch;
use super::launch::LaunchResult;
use super::launch_params::LaunchBevyBinaryParams;
use crate::error::Error;
use crate::error::Result;
use crate::tool;
use crate::tool::HandlerContext;
use crate::tool::HandlerResult;
use crate::tool::ToolFn;
use crate::tool::ToolResult;

/// Handler for launching Bevy targets (apps or examples) using unified search
pub struct LaunchBevyTarget;

#[async_trait]
impl ToolFn for LaunchBevyTarget {
    type Output = LaunchResult;
    type Params = LaunchBevyBinaryParams;

    fn call(
        &self,
        context: HandlerContext,
    ) -> HandlerResult<'_, ToolResult<Self::Output, Self::Params>> {
        tool::call_with_typed_params(context, |_, params: LaunchBevyBinaryParams| async move {
            let (request, active) = tokio::sync::oneshot::channel::<()>();
            let pending = tokio::task::spawn_blocking(move || {
                launch::launch_bevy_target(params, DEFAULT_PROFILE, &request)
            })
            .await
            .map_err(|error| Error::ProcessManagement(format!("Launch task failed: {error}")))??;
            drop(active);
            pending.verify().await
        })
    }

    async fn handle_impl(&self, _: Self::Params) -> Result<Self::Output> {
        Err(
            Error::InvalidState("LaunchBevyTarget uses its custom call implementation".to_string())
                .into(),
        )
    }
}

/// Create a `LaunchBevyTarget` handler instance
pub const fn create_launch_handler() -> LaunchBevyTarget {
    LaunchBevyTarget
}
