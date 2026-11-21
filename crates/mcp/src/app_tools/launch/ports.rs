use std::collections::HashSet;

use error_stack::Report;
use serde_json::json;

use super::super::instance_count::InstanceCount;
use crate::brp_tools::{MAX_VALID_PORT, Port};
use crate::error::{Error, Result};

const DEFAULT_RENDER_PORT: Port = Port(15703);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PortPair {
    pub(super) main: Port,
    pub(super) render: Port,
}

impl PortPair {
    pub(super) const fn endpoints(self) -> [(&'static str, Port); 2] {
        [("Main", self.main), ("Render", self.render)]
    }
}

pub(super) struct LaunchPorts {
    pub(super) pairs: Vec<PortPair>,
    pub(super) override_env: bool,
    pub(super) require_both: bool,
}

pub(super) fn plan_ports(
    main: Option<Port>,
    render: Option<Port>,
    from_config: bool,
    count: InstanceCount,
) -> Result<LaunchPorts> {
    if from_config && main.is_none() {
        return Err(port_parameter_error(
            "port",
            "ports_from_config=true requires port",
        ));
    }
    if from_config && render.is_none() {
        return Err(port_parameter_error(
            "render_port",
            "ports_from_config=true requires render_port",
        ));
    }
    let base = match (main, render) {
        (None, None) => PortPair {
            main: Port::default(),
            render: DEFAULT_RENDER_PORT,
        },
        (Some(main), Some(render)) => PortPair { main, render },
        (Some(main), None) => PortPair {
            main,
            render: offset_port("Render", main, 1, 1)?,
        },
        (None, Some(_)) => {
            return Err(port_parameter_error("port", "render_port requires port"));
        }
    };
    if base.main == base.render {
        return Err(port_parameter_error(
            "render_port",
            "Main and Render ports must differ",
        ));
    }

    Ok(LaunchPorts {
        pairs: allocate_pairs(base, count)?,
        override_env: *count > 1 || (main.is_some() && !from_config),
        require_both: *count > 1 || render.is_some(),
    })
}

fn port_parameter_error(field: &str, message: &str) -> Report<Error> {
    Error::tool_call_failed_with_details(
        message,
        json!({"stage":"parameter_validation", "field":field}),
    )
    .into()
}

fn allocate_pairs(base: PortPair, count: InstanceCount) -> Result<Vec<PortPair>> {
    let mut used = HashSet::with_capacity(usize::from(*count) * 2);
    let mut pairs = Vec::with_capacity(usize::from(*count));
    let mut offset = 0;

    for instance in 1..=*count {
        loop {
            let main = offset_port("Main", base.main, offset, instance)?;
            let render = offset_port("Render", base.render, offset, instance)?;
            offset += 1;
            if used.contains(&main) || used.contains(&render) {
                continue;
            }
            used.extend([main, render]);
            pairs.push(PortPair { main, render });
            break;
        }
    }

    Ok(pairs)
}

fn offset_port(endpoint: &str, base: Port, offset: u32, instance: u16) -> Result<Port> {
    let port = u32::from(*base) + offset;
    u16::try_from(port)
        .ok()
        .and_then(|port| Port::try_from(port).ok())
        .ok_or_else(|| Error::tool_call_failed_with_details(
            format!("Instance {instance} {endpoint} port {port} is outside the allowed range (maximum {MAX_VALID_PORT})"),
            json!({"stage":"port_allocation", "instance":instance, "endpoint":endpoint, "port":port}),
        ).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_port_plan_distinguishes_default_app_config_and_overrides() {
        let default = plan_ports(None, None, false, InstanceCount(1)).unwrap();
        assert_eq!(
            default.pairs,
            [PortPair {
                main: Port(15702),
                render: Port(15703)
            }]
        );
        assert!(!default.override_env);

        let app = plan_ports(Some(Port(9000)), Some(Port(9010)), true, InstanceCount(1)).unwrap();
        assert_eq!(
            app.pairs,
            [PortPair {
                main: Port(9000),
                render: Port(9010)
            }]
        );
        assert!(!app.override_env);

        let explicit = plan_ports(Some(Port(9000)), None, false, InstanceCount(1)).unwrap();
        assert_eq!(
            explicit.pairs,
            [PortPair {
                main: Port(9000),
                render: Port(9001)
            }]
        );
        assert!(explicit.override_env);
    }

    #[test]
    fn launch_port_plan_allocates_unique_adjacent_and_nonadjacent_pairs() {
        let default = plan_ports(None, None, false, InstanceCount(3)).unwrap();
        assert_eq!(
            default
                .pairs
                .iter()
                .map(|pair| (pair.main.0, pair.render.0))
                .collect::<Vec<_>>(),
            [(15702, 15703), (15704, 15705), (15706, 15707)]
        );
        assert!(default.override_env);

        let app = plan_ports(Some(Port(9000)), Some(Port(9002)), true, InstanceCount(5)).unwrap();
        assert_eq!(
            app.pairs
                .iter()
                .map(|pair| (pair.main.0, pair.render.0))
                .collect::<Vec<_>>(),
            [
                (9000, 9002),
                (9001, 9003),
                (9004, 9006),
                (9005, 9007),
                (9008, 9010)
            ]
        );
        assert!(app.override_env);
    }

    #[test]
    fn launch_port_plan_rejects_incomplete_duplicate_and_out_of_range_pairs() {
        let a = Some(Port(9000));
        let high = Some(Port(MAX_VALID_PORT));
        for (main, render, from_config, count) in [
            (None, None, true, 1),
            (a, None, true, 1),
            (None, a, false, 1),
            (a, a, false, 1),
            (high, None, false, 1),
            (Some(Port(MAX_VALID_PORT - 1)), high, false, 2),
        ] {
            assert!(plan_ports(main, render, from_config, InstanceCount(count)).is_err());
        }
    }
}
