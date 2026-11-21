use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

use error_stack::Report;
use futures::future::join_all;
use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};
use serde_json::json;
use tracing::warn;

use super::config::LaunchedInstance;
use super::ports::PortPair;
use crate::app_tools::process;
use crate::brp_tools::{BrpClient, Port, ResponseStatus};
use crate::error::{Error, Result};
use crate::tool::BrpMethod;

const LISTENER_STARTUP_TIMEOUT: Duration = Duration::from_secs(8);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const DISCOVER_TIMEOUT: Duration = Duration::from_millis(400);
const LOG_TAIL_BYTES: u64 = 16_384;

#[derive(Default)]
struct ListenerOwners {
    ipv4: Vec<u32>,
    ipv6_wildcard: Vec<u32>,
}

impl ListenerOwners {
    fn record(&mut self, address: IpAddr, pids: Vec<u32>) {
        match address {
            IpAddr::V4(address) if is_brp_ipv4_address(address) => self.ipv4.extend(pids),
            IpAddr::V6(address) if address.is_unspecified() => self.ipv6_wildcard.extend(pids),
            IpAddr::V6(address) if address.to_ipv4_mapped().is_some_and(is_brp_ipv4_address) => {
                self.ipv4.extend(pids);
            }
            _ => {}
        }
    }

    fn can_reach(&self, pid: u32) -> bool {
        self.ipv4.contains(&pid) || (self.ipv4.is_empty() && self.ipv6_wildcard.contains(&pid))
    }
}

fn is_brp_ipv4_address(address: Ipv4Addr) -> bool {
    address == Ipv4Addr::LOCALHOST || address.is_unspecified()
}

pub(super) struct StartingInstance {
    child: Option<Child>,
    log_file: PathBuf,
    ports: PortPair,
    listeners: [bool; 2],
}

impl StartingInstance {
    pub(super) const fn new(child: Child, log_file: PathBuf, ports: PortPair) -> Self {
        Self {
            child: Some(child),
            log_file,
            ports,
            listeners: [false; 2],
        }
    }

    pub(super) fn finish(mut self) -> Result<LaunchedInstance> {
        let child = self
            .child
            .take()
            .ok_or_else(|| Error::InvalidState("No process to finish launching".to_owned()))?;
        let pid = child.id();
        process::reap_app_process(child);
        Ok(LaunchedInstance {
            pid,
            log_file: self.log_file.display().to_string(),
            port: self.ports.main,
            render_port: self.ports.render,
            main_listening: self.listeners[0],
            render_listening: self.listeners[1],
        })
    }

    fn early_exit(&self, instance: usize, pid: u32, status: ExitStatus) -> Report<Error> {
        match runtime_bind_failure(&self.log_file) {
            Ok(Some((endpoint, port))) => {
                let planned_port = if endpoint == "Main" {
                    self.ports.main
                } else {
                    self.ports.render
                };
                let message = match port {
                    Some(port) => {
                        format!("Instance {instance} failed to bind its {endpoint} port {port}")
                    }
                    None => format!(
                        "Instance {instance} reported a {endpoint} bind failure. Its requested connection port was {planned_port}."
                    ),
                };
                return Error::tool_call_failed_with_details(message, json!({
                    "stage":"listener_startup", "instance":instance, "endpoint":endpoint,
                    "port":port, "planned_port":planned_port, "log_file":self.log_file.display().to_string()
                })).into();
            }
            Ok(None) => {}
            Err(error) => warn!(pid, %error, "Could not read app startup log"),
        }
        Error::tool_call_failed_with_details(
            format!("Instance {instance} exited during startup with {status}"),
            json!({"stage":"listener_startup", "instance":instance, "port":self.ports.main,
                "render_port":self.ports.render, "log_file":self.log_file.display().to_string()}),
        )
        .into()
    }
}

impl Drop for StartingInstance {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let pid = child.id();
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        match child.kill() {
            Ok(()) => {
                if let Err(error) = child.wait() {
                    warn!(pid, %error, "Failed to wait for app after startup ended");
                }
            }
            Err(error) => {
                warn!(pid, %error, "Failed to stop app after startup ended");
                process::reap_app_process(child);
            }
        }
    }
}

fn endpoint_error(
    stage: &str,
    instance: usize,
    endpoint: &str,
    port: Port,
    reason: impl std::fmt::Display,
) -> Report<Error> {
    Error::tool_call_failed_with_details(
        format!("Instance {instance} {endpoint} port {port}: {reason}"),
        json!({"stage":stage, "instance":instance, "endpoint":endpoint, "port":port}),
    )
    .into()
}

pub(super) fn check_ports_available(pairs: &[PortPair]) -> Result<()> {
    for (index, pair) in pairs.iter().enumerate() {
        for (endpoint, port) in pair.endpoints() {
            TcpListener::bind((Ipv4Addr::LOCALHOST, *port)).map_err(|error| {
                endpoint_error("port_preflight", index + 1, endpoint, port, error)
            })?;
        }
    }
    Ok(())
}

pub(super) async fn wait_for_listeners(
    instances: &mut [StartingInstance],
    require_both: bool,
) -> Result<()> {
    let deadline = Instant::now() + LISTENER_STARTUP_TIMEOUT;
    loop {
        let listeners = tokio::task::spawn_blocking(local_listeners)
            .await
            .map_err(|error| {
                Error::ProcessManagement(format!("Could not collect local listeners: {error}"))
            })??;
        let mut discovery_checks = Vec::new();
        for (index, instance) in instances.iter_mut().enumerate() {
            let child = instance.child.as_mut().ok_or_else(|| {
                Error::InvalidState("No process to verify during launch".to_owned())
            })?;
            let pid = child.id();
            if let Some(status) = child.try_wait().map_err(|error| {
                Error::ProcessManagement(format!(
                    "Could not check startup of instance {}: {error}",
                    index + 1
                ))
            })? {
                return Err(instance.early_exit(index + 1, pid, status));
            }

            for ((endpoint, port), ready) in instance
                .ports
                .endpoints()
                .into_iter()
                .zip(&mut instance.listeners)
            {
                let Some(owners) = listeners.get(&port) else {
                    *ready = false;
                    continue;
                };
                if owners.can_reach(pid) {
                    if !*ready {
                        discovery_checks.push((port, ready));
                    }
                } else {
                    *ready = false;
                    if let Some(other) = owners.ipv4.first() {
                        return Err(endpoint_error(
                            "listener_startup",
                            index + 1,
                            endpoint,
                            port,
                            format_args!("a different process (PID {other}) acquired the port"),
                        ));
                    }
                }
            }
        }
        join_all(discovery_checks.into_iter().map(
            |(port, ready)| async move {
                let client = BrpClient::new(BrpMethod::RpcDiscover, port, None);
                *ready = matches!(
                    tokio::time::timeout(DISCOVER_TIMEOUT, client.execute_raw_once()).await,
                    Ok(Ok(ResponseStatus::Success(Some(result)))) if result["methods"].is_array()
                );
            },
        )).await;
        if instances
            .iter()
            .all(|instance| instance.listeners == [true; 2])
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            if require_both {
                for (index, instance) in instances.iter().enumerate() {
                    for ((endpoint, port), ready) in instance
                        .ports
                        .endpoints()
                        .into_iter()
                        .zip(instance.listeners)
                    {
                        if !ready {
                            return Err(endpoint_error(
                                "listener_startup",
                                index + 1,
                                endpoint,
                                port,
                                format_args!(
                                    "BRP discovery did not respond within {} seconds",
                                    LISTENER_STARTUP_TIMEOUT.as_secs()
                                ),
                            ));
                        }
                    }
                }
            }
            return Ok(());
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn local_listeners() -> Result<HashMap<Port, ListenerOwners>> {
    let families = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let sockets = netstat2::get_sockets_info(families, ProtocolFlags::TCP).map_err(|error| {
        Error::ProcessManagement(format!("Could not inspect local BRP listeners: {error}"))
    })?;
    let mut listeners = HashMap::<Port, ListenerOwners>::new();
    for socket in sockets {
        if let ProtocolSocketInfo::Tcp(tcp) = socket.protocol_socket_info
            && tcp.state == TcpState::Listen
        {
            listeners
                .entry(Port(tcp.local_port))
                .or_default()
                .record(tcp.local_addr, socket.associated_pids);
        }
    }
    Ok(listeners)
}

fn runtime_bind_failure(path: &Path) -> io::Result<Option<(&'static str, Option<u16>)>> {
    let mut file = File::open(path)?;
    let offset = file.metadata()?.len().saturating_sub(LOG_TAIL_BYTES);
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.take(LOG_TAIL_BYTES).read_to_end(&mut bytes)?;
    let ansi_style = regex::Regex::new(r"\x1b\[[0-9;]*m").map_err(io::Error::other)?;
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .rev()
        .find_map(|line| {
            if !line.contains("bevy_brp_runtime::http")
                || !line.contains("failed to bind listener:")
            {
                return None;
            }
            let line = ansi_style.replace_all(line, "");
            let endpoint = if line.contains("endpoint=\"Main\"") {
                "Main"
            } else if line.contains("endpoint=\"Render\"") {
                "Render"
            } else {
                return None;
            };
            let port = line
                .split_ascii_whitespace()
                .find_map(|field| field.strip_prefix("port=")?.parse().ok());
            Some((endpoint, port))
        }))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn launch_port_probe_prefers_ipv4_and_recognizes_dual_stack_candidates() {
        let mut owners = ListenerOwners::default();
        owners.record("127.0.0.2".parse().unwrap(), vec![11]);
        owners.record("::1".parse().unwrap(), vec![12]);
        owners.record("::".parse().unwrap(), vec![13]);
        assert!(!owners.can_reach(11));
        assert!(!owners.can_reach(12));
        assert!(owners.can_reach(13));
        assert!(owners.ipv4.is_empty());

        owners.record("::ffff:127.0.0.1".parse().unwrap(), vec![14]);
        assert!(!owners.can_reach(13));
        assert!(owners.can_reach(14));
    }

    #[test]
    fn launch_port_preflight_identifies_the_instance_and_occupied_endpoint() {
        let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let render = Port::try_from(occupied.local_addr().unwrap().port()).unwrap();
        let main = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let main_port = Port::try_from(main.local_addr().unwrap().port()).unwrap();
        drop(main);
        let error = check_ports_available(&[PortPair {
            main: main_port,
            render,
        }])
        .unwrap_err();
        match error.current_context() {
            Error::ToolCall {
                details: Some(details),
                ..
            } => assert_eq!(
                details,
                &json!({"stage":"port_preflight", "instance":1, "endpoint":"Render", "port":render}),
            ),
            other => panic!("expected structured endpoint details, got {other}"),
        }
    }

    #[test]
    fn launch_port_bind_error_recognizes_colored_runtime_log_fields() {
        let mut log = tempfile::NamedTempFile::new().unwrap();
        writeln!(log, "\x1b[31mERROR\x1b[0m bevy_brp_runtime::http: BRP transport failed \
            \x1b[3mendpoint\x1b[0m\x1b[2m=\x1b[0m\"Render\" port=15753 error=failed to bind listener: address in use")
            .unwrap();
        assert_eq!(
            runtime_bind_failure(log.path()).unwrap(),
            Some(("Render", Some(15753)))
        );
    }
}
