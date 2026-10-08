//! Public method registration and strict wire contract, without host Picking plugins.

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_remote::{BrpResult, RemoteMethodSystemId, RemoteMethods};
use serde_json::{Value, json};

fn invoke(app: &mut App, params: Option<Value>) -> BrpResult {
    let method = *app
        .world()
        .resource::<RemoteMethods>()
        .get("brp_extras/pointer_control")
        .expect("control method is registered");
    let RemoteMethodSystemId::Instant(system) = method else {
        panic!("control must be an instant method");
    };
    app.world_mut()
        .run_system_with(system, params)
        .expect("registered control system runs")
}

#[test]
fn control_is_callable_without_picking_and_is_absent_from_agent_catalog() {
    let mut app = App::new();
    app.add_plugins(BrpExtrasPlugin::without_http_transport());
    let initial = json!({
        "phase":"inactive", "busy":false, "pointer_id":null, "window":null,
        "generation":0, "queued_actions":0, "pressed_buttons":[], "last_error":null
    });
    assert_eq!(
        invoke(&mut app, Some(json!({"action":"status"}))).unwrap(),
        initial
    );
    for _ in 0..2 {
        assert_eq!(
            invoke(&mut app, Some(json!({"action":"release"}))).unwrap(),
            initial
        );
    }
    for params in [
        None,
        Some(Value::Null),
        Some(json!({})),
        Some(json!({"action":"unknown"})),
        Some(json!({"action":"status", "window":42})),
        Some(json!({"action":true})),
        Some(json!(["release"])),
        Some(json!({"action":{"release":null}})),
    ] {
        let error = invoke(&mut app, params).unwrap_err();
        assert_eq!(error.code, -32602);
        assert_eq!(error.data.unwrap()["method"], "brp_extras/pointer_control");
        assert_eq!(
            invoke(&mut app, Some(json!({"action":"status"}))).unwrap(),
            initial
        );
    }
    let RemoteMethodSystemId::Instant(catalog) = *app
        .world()
        .resource::<RemoteMethods>()
        .get("brp_extras/agent_tools")
        .unwrap()
    else {
        panic!("agent catalog must be instant");
    };
    let catalog = app
        .world_mut()
        .run_system_with(catalog, None)
        .unwrap()
        .unwrap();
    assert_eq!(catalog["tools"], json!([]));
}
