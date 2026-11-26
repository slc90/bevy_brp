use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

use cargo_metadata::Message;
use error_stack::Report;
use tracing::debug;
use tracing::info;

use super::constants::CARGO_RELEASE_FLAG;
use super::logging;
use super::ports::PortPair;
use crate::app_tools::constants::CARGO_BUILD_SUBCOMMAND;
use crate::app_tools::constants::CARGO_COMMAND_NAME;
use crate::app_tools::constants::CARGO_MESSAGE_FORMAT_JSON_FLAG;
use crate::app_tools::constants::CARGO_PACKAGE_FLAG;
use crate::app_tools::constants::CARGO_TARGET_DIR_FLAG;
use crate::app_tools::constants::CARGO_WORKSPACE_FLAG;
use crate::app_tools::constants::PROFILE_RELEASE;
use crate::app_tools::targets::{BevyTarget, TargetType};
use crate::brp_tools::BRP_EXTRAS_PORT_ENV_VAR;
use crate::brp_tools::Port;
use crate::error::Error;
use crate::error::Result;

const WINDOW_FOCUSED_ENV_VAR: &str = "BRP_EXTRAS_WINDOW_FOCUSED";
const RENDER_PORT_ENV_VAR: &str = "BRP_EXTRAS_RENDER_PORT";

#[derive(Debug, Clone, Copy)]
pub(super) enum BuildState {
    NotFound,
    Fresh,
    Rebuilt,
}

pub(super) struct BuiltTarget {
    pub(super) executable: PathBuf,
    pub(super) state: BuildState,
}

pub(super) fn validate_manifest_directory(manifest_path: &Path) -> Result<&Path> {
    manifest_path.parent().ok_or_else(|| {
        Report::new(Error::FileOrPathNotFound(
            "Invalid manifest path".to_string(),
        ))
        .attach("No parent directory found")
        .attach(format!("Path: {}", manifest_path.display()))
    })
}

fn set_brp_env_vars(command: &mut Command, override_ports: Option<PortPair>) {
    if let Some(ports) = override_ports {
        command.env(BRP_EXTRAS_PORT_ENV_VAR, ports.main.to_string());
        command.env(RENDER_PORT_ENV_VAR, ports.render.to_string());
    } else {
        command.env_remove(BRP_EXTRAS_PORT_ENV_VAR);
        command.env_remove(RENDER_PORT_ENV_VAR);
    }
}

fn set_user_env_vars(command: &mut Command, env: Option<&HashMap<String, String>>) {
    if let Some(env_vars) = env {
        for (key, value) in env_vars {
            command.env(key, value);
        }
    }
}

pub(super) fn setup_launch_logging(
    name: &str,
    target_type: TargetType,
    profile: &str,
    binary_path: &Path,
    manifest_dir: &Path,
    port: Port,
    extra_log_info: Option<&str>,
) -> Result<(PathBuf, File)> {
    let (log_file_path, _) =
        logging::create_log_file(name, target_type, profile, binary_path, manifest_dir, port)
            .map_err(|e| Error::tool_call_failed(format!("Failed to create log file: {e}")))?;

    if let Some(extra_info) = extra_log_info {
        logging::append_to_log_file(&log_file_path, &format!("{extra_info}\n"))
            .map_err(|e| Error::tool_call_failed(format!("Failed to append to log file: {e}")))?;
    }

    let log_file_for_redirect =
        logging::open_log_file_for_redirect(&log_file_path).map_err(|e| {
            Error::tool_call_failed(format!("Failed to open log file for redirect: {e}"))
        })?;

    Ok((log_file_path, log_file_for_redirect))
}

pub(super) fn build_app_command(
    binary_path: &Path,
    override_ports: Option<PortPair>,
    env: Option<&HashMap<String, String>>,
    command_line_arguments: Option<&[String]>,
    activate_window: bool,
) -> Command {
    let mut command = Command::new(binary_path);
    if let Some(user_arguments) = command_line_arguments {
        command.args(user_arguments);
    }
    set_user_env_vars(&mut command, env);
    set_brp_env_vars(&mut command, override_ports);
    if cfg!(windows) {
        command.env(WINDOW_FOCUSED_ENV_VAR, activate_window.to_string());
    }
    command
}

fn build_cargo_command(target: &BevyTarget, profile: &str, build_dir: &Path) -> Command {
    let mut command = Command::new(CARGO_COMMAND_NAME);
    command.current_dir(build_dir);
    command.arg(CARGO_BUILD_SUBCOMMAND);
    if let Some(target_dir) = &target.isolated_target_dir {
        // Cargo can mark a package fresh even after another package overwrites its output.
        command
            .arg(CARGO_PACKAGE_FLAG)
            .arg(&target.package_name)
            .arg(CARGO_TARGET_DIR_FLAG)
            .arg(target_dir);
    } else {
        // Use Cargo's workspace feature resolution to reuse ordinary workspace builds.
        command.arg(CARGO_WORKSPACE_FLAG);
    }

    target
        .target_type
        .add_cargo_args(&mut command, &target.name);

    if profile == PROFILE_RELEASE {
        command.arg(CARGO_RELEASE_FLAG);
    }

    command.arg(CARGO_MESSAGE_FORMAT_JSON_FLAG);

    command
}

fn execute_build_command(
    command: &mut Command,
    target_name: &str,
    target_type: TargetType,
    profile: &str,
    build_dir: &Path,
) -> Result<Output> {
    debug!("Running cargo build for {target_type} '{target_name}' with command: {command:?}");

    let output = command.output().map_err(|e| {
        Error::ProcessManagement(format!(
            "Failed to run cargo build for {target_type} '{target_name}' (profile: {profile}, dir: {}): {e}",
            build_dir.display()
        ))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::ProcessManagement(format!(
            "Cargo build failed for {target_type} '{target_name}' (profile: {profile}, dir: {}): {stderr}",
            build_dir.display()
        ))
        .into());
    }

    Ok(output)
}

fn parse_build_output(stdout: &[u8], target: &BevyTarget) -> Option<BuiltTarget> {
    for message in Message::parse_stream(stdout) {
        if let Ok(Message::CompilerArtifact(artifact)) = message
            && artifact.manifest_path.as_std_path() == target.manifest
            && artifact.target.name == target.name
            && TargetType::from_cargo(&artifact.target) == Some(target.target_type)
            && let Some(executable) = artifact.executable
        {
            let state = if artifact.fresh {
                BuildState::Fresh
            } else {
                BuildState::Rebuilt
            };
            return Some(BuiltTarget {
                executable: executable.into_std_path_buf(),
                state,
            });
        }
    }
    None
}

fn log_build_result(build_state: BuildState, target_name: &str, target_type: TargetType) {
    match build_state {
        BuildState::NotFound => {
            debug!("Target '{target_name}' not found in build output, assuming it was built");
        }
        BuildState::Fresh => {
            debug!("{target_type} '{target_name}' was already up to date");
        }
        BuildState::Rebuilt => {
            info!("{target_type} '{target_name}' was built successfully");
        }
    }
}

pub(super) fn run_cargo_build(target: &BevyTarget, profile: &str) -> Result<BuiltTarget> {
    let build_dir = if target.isolated_target_dir.is_some() {
        validate_manifest_directory(&target.manifest)?
    } else {
        &target.workspace_root
    };
    let mut command = build_cargo_command(target, profile, build_dir);
    let output = execute_build_command(
        &mut command,
        &target.name,
        target.target_type,
        profile,
        build_dir,
    )?;
    let built = match parse_build_output(&output.stdout, target) {
        Some(built) => built,
        None if target.isolated_target_dir.is_some() => {
            return Err(Error::ProcessManagement(format!(
                "Cargo did not report an executable for {} '{}' in package '{}'",
                target.target_type, target.name, target.package_name
            ))
            .into());
        }
        None => BuiltTarget {
            executable: target.get_binary_path(profile),
            state: BuildState::NotFound,
        },
    };
    log_build_result(built.state, &target.name, target.target_type);
    Ok(built)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[cfg(windows)]
    #[test]
    fn window_startup_focus_takes_precedence_over_the_user_environment() {
        for (activate_window, expected, conflicting) in
            [(false, "false", "true"), (true, "true", "false")]
        {
            let env = HashMap::from([(
                "BRP_EXTRAS_WINDOW_FOCUSED".to_lowercase(),
                conflicting.to_string(),
            )]);
            let command = build_app_command(
                Path::new("example.exe"),
                None,
                Some(&env),
                None,
                activate_window,
            );
            let effective = command
                .get_envs()
                .find(|(key, _)| key.eq_ignore_ascii_case("BRP_EXTRAS_WINDOW_FOCUSED"));
            assert_eq!(
                effective.and_then(|(_, value)| value),
                Some(OsStr::new(expected))
            );
        }
    }

    #[test]
    fn launch_port_environment_uses_the_managed_pair_or_leaves_both_unset() {
        let env = HashMap::from([
            (BRP_EXTRAS_PORT_ENV_VAR.to_owned(), "9000".to_owned()),
            (RENDER_PORT_ENV_VAR.to_owned(), "9001".to_owned()),
            ("APP_SETTING".to_owned(), "value".to_owned()),
        ]);
        for (ports, main, render) in [
            (
                Some(PortPair {
                    main: Port(9010),
                    render: Port(9020),
                }),
                Some("9010"),
                Some("9020"),
            ),
            (None, None, None),
        ] {
            let command = build_app_command(Path::new("example"), ports, Some(&env), None, false);
            let environment: HashMap<_, _> = command.get_envs().collect();
            assert_eq!(
                environment[OsStr::new(BRP_EXTRAS_PORT_ENV_VAR)],
                main.map(OsStr::new)
            );
            assert_eq!(
                environment[OsStr::new(RENDER_PORT_ENV_VAR)],
                render.map(OsStr::new)
            );
            assert_eq!(
                environment[OsStr::new("APP_SETTING")],
                Some(OsStr::new("value"))
            );
        }
    }
}
