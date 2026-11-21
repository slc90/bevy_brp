use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use error_stack::Report;
use tokio::sync::oneshot::Sender;
use tokio::sync::{Mutex, MutexGuard};
use tracing::debug;
use tracing::warn;

use super::build;
use super::build::BuildScope;
use super::build::BuildState;
use super::config;
use super::config::LaunchParams;
use super::config::LaunchResult;
use super::constants::ERROR_CHAIN_FIELD;
use super::constants::ERROR_FIELD;
use super::ports::{LaunchPorts, PortPair, plan_ports};
use super::startup::{StartingInstance, check_ports_available, wait_for_listeners};
use crate::app_tools::launch_params::LaunchBevyBinaryParams;
use crate::app_tools::launch_params::SearchOrder;
use crate::app_tools::process;
use crate::app_tools::targets;
use crate::app_tools::targets::AvailableTarget;
use crate::app_tools::targets::BevyTarget;
use crate::app_tools::targets::TargetType;
use crate::app_tools::targets::UnifiedTargetNotFoundError;
use crate::error::Error;
use crate::error::Result;

// Keep concurrent launch calls from checking the same ports before either child binds.
static LAUNCH_PORT_CHECK: Mutex<()> = Mutex::const_new(());

pub(in crate::app_tools) struct PendingLaunch {
    instances: Vec<StartingInstance>,
    result: LaunchResult,
    started_at: Instant,
    require_both: bool,
    _port_check: MutexGuard<'static, ()>,
}

impl PendingLaunch {
    pub(in crate::app_tools) async fn verify(mut self) -> Result<LaunchResult> {
        wait_for_listeners(&mut self.instances, self.require_both).await?;
        let instances = self
            .instances
            .into_iter()
            .map(StartingInstance::finish)
            .collect::<Result<_>>()?;
        Ok(self
            .result
            .with_started_instances(instances, self.started_at))
    }
}

fn prepare_launch_environment<T: config::LaunchConfigTrait>(
    config: &T,
    target: &BevyTarget,
    ports: PortPair,
    override_env: bool,
) -> Result<(Command, PathBuf, PathBuf, File)> {
    let manifest_dir = build::validate_manifest_directory(&target.manifest)?;
    let command = config.build_command(target, override_env.then_some(ports));
    let (log_file_path, log_file_for_redirect) = build::setup_launch_logging(
        config.target(),
        T::TARGET_TYPE,
        config.profile(),
        Path::new(command.get_program()),
        manifest_dir,
        ports.main,
        config.extra_log_info(target).as_deref(),
    )?;

    Ok((
        command,
        manifest_dir.to_path_buf(),
        log_file_path,
        log_file_for_redirect,
    ))
}

fn launch_instances<T: config::LaunchConfigTrait>(
    config: &T,
    target: &BevyTarget,
    port_plan: &LaunchPorts,
    request: &Sender<()>,
) -> Result<Vec<StartingInstance>> {
    check_ports_available(&port_plan.pairs)?;
    let mut instances = Vec::with_capacity(port_plan.pairs.len());
    for &ports in &port_plan.pairs {
        if request.is_closed() {
            return Err(Error::tool_call_failed("Launch cancelled").into());
        }
        let (command, manifest_dir, log_file, log_file_for_redirect) =
            prepare_launch_environment(config, target, ports, port_plan.override_env)?;
        let child = process::spawn_app_process(
            &command,
            &manifest_dir,
            log_file_for_redirect,
            config.target(),
        )?;
        instances.push(StartingInstance::new(child, log_file, ports));
    }

    Ok(instances)
}

fn handle_target_discovery_error(error: Report<Error>) -> Report<Error> {
    if let Error::Structured { .. } = error.current_context() {
        return error;
    }

    let error_message = format!("{}", error.current_context());
    let details = serde_json::json!({
        ERROR_FIELD: error_message,
        ERROR_CHAIN_FIELD: format!("{:?}", error)
    });
    Error::tool_call_failed_with_details(error_message, details).into()
}

pub(in crate::app_tools) fn launch_bevy_target(
    typed_params: LaunchBevyBinaryParams,
    default_profile: &'static str,
    request: &Sender<()>,
) -> Result<PendingLaunch> {
    let params = typed_params.to_launch_params(default_profile);
    let port_plan = plan_ports(
        params.port,
        params.render_port,
        params.ports_from_config,
        params.instance_count,
    )?;

    let search_roots = targets::resolve_search_paths(params.path.as_deref())?;

    let (first, second) = match params.search_order {
        SearchOrder::App => (TargetType::App, TargetType::Example),
        SearchOrder::Example => (TargetType::Example, TargetType::App),
    };

    let scope_path: Option<PathBuf> = params.path.as_ref().map(PathBuf::from);

    let mut first_targets =
        targets::find_all_targets_by_name(&params.target, Some(first), &search_roots);
    if let Some(ref scope) = scope_path {
        first_targets = targets::filter_targets_by_path_scope(first_targets, scope);
    }
    if !first_targets.is_empty() {
        return launch_found_target(
            first,
            first_targets,
            &params,
            &search_roots,
            &port_plan,
            request,
        );
    }

    let mut second_targets =
        targets::find_all_targets_by_name(&params.target, Some(second), &search_roots);
    if let Some(ref scope) = scope_path {
        second_targets = targets::filter_targets_by_path_scope(second_targets, scope);
    }
    if !second_targets.is_empty() {
        return launch_found_target(
            second,
            second_targets,
            &params,
            &search_roots,
            &port_plan,
            request,
        );
    }

    let mut all_targets = targets::scan_bevy_targets(&search_roots);
    if let Some(ref scope) = scope_path {
        all_targets = targets::filter_targets_by_path_scope(all_targets, scope);
    }
    let available: Vec<AvailableTarget> = all_targets
        .into_iter()
        .map(|target| AvailableTarget {
            name: target.name,
            kind: target.target_type.to_string(),
            path: target.relative.to_string_lossy().to_string(),
        })
        .collect();

    let unified_target_not_found_error = UnifiedTargetNotFoundError::new(params.target, available);
    Err(Error::Structured {
        result: Box::new(unified_target_not_found_error),
    }
    .into())
}

fn launch_found_target(
    target_type: TargetType,
    cached_targets: Vec<BevyTarget>,
    params: &LaunchParams,
    roots: &[PathBuf],
    port_plan: &LaunchPorts,
    request: &Sender<()>,
) -> Result<PendingLaunch> {
    match target_type {
        TargetType::App => {
            let config = config::LaunchConfig::<config::App>::from(params);
            launch_target_with_cached(&config, roots, cached_targets, port_plan, request)
        }
        TargetType::Example => {
            let config = config::LaunchConfig::<config::Example>::from(params);
            launch_target_with_cached(&config, roots, cached_targets, port_plan, request)
        }
    }
}

fn launch_target_with_cached<T: config::LaunchConfigTrait>(
    config: &T,
    search_paths: &[PathBuf],
    cached_targets: Vec<BevyTarget>,
    port_plan: &LaunchPorts,
    request: &Sender<()>,
) -> Result<PendingLaunch> {
    let launch_start = Instant::now();

    // cargo applies --bin and --example to every package it selects, so a name that
    // two members of one workspace both define would build both and write them to a
    // single output path. Record each candidate's workspace before the lookup below
    // consumes the list, then keep the build package-scoped when the name repeats.
    let candidate_workspaces: Vec<PathBuf> = cached_targets
        .iter()
        .map(|candidate| candidate.workspace_root.clone())
        .collect();

    let target = find_and_validate_target_with_cache(config, search_paths, cached_targets)
        .map_err(handle_target_discovery_error)?;

    let shares_name_in_workspace = candidate_workspaces
        .iter()
        .filter(|workspace| **workspace == target.workspace_root)
        .count()
        > 1;
    let build_scope = if shares_name_in_workspace {
        BuildScope::Package
    } else {
        BuildScope::Workspace
    };

    let build_state = config.ensure_built(&target, build_scope)?;
    match build_state {
        BuildState::Fresh => debug!("Target was already up to date, launching immediately"),
        BuildState::Rebuilt => debug!("Target was rebuilt before launch"),
        BuildState::NotFound => {
            warn!("Target not found in build output but build succeeded");
        }
    }

    let port_check = LAUNCH_PORT_CHECK.blocking_lock();
    let instances = launch_instances(config, &target, port_plan, request)?;
    let result = config::build_launch_result(instances.len(), config, &target);
    Ok(PendingLaunch {
        instances,
        result,
        started_at: launch_start,
        require_both: port_plan.require_both,
        _port_check: port_check,
    })
}

fn find_and_validate_target_with_cache<T: config::LaunchConfigTrait>(
    config: &T,
    search_paths: &[PathBuf],
    cached_targets: Vec<BevyTarget>,
) -> Result<BevyTarget> {
    targets::find_required_target_with_package_name(
        config.target(),
        T::TARGET_TYPE,
        config.package(),
        search_paths,
        Some(cached_targets),
    )
    .map_err(Report::new)
}
