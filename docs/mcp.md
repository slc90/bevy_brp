# MCP tools, diagnostics, and logs

The normal `tools/list` registry has 47 native tools. The catalog of application methods is a
separate BRP response: call `brp_list_agent_tools` with `{"port":15712}` on a running app. Select
an entry by `name` and description, then pass its exact `method` and raw JSON parameters to
`brp_execute`, for example `{"method":"test/multiply","params":{"value":6,"factor":7},"port":15712}`.
The catalog may be empty; omitted `params_schema` or `result_schema` means the app supplied no
schema. Catalog entries are not added to `tools/list`. For other one-shot methods,
`rpc_discover` lists the live BRP methods that `brp_execute` can call. The default port is 15702.
To start this repository's fixture through MCP, call `brp_launch` with
`{"target_name":"extras_plugin","package_name":"bevy_brp_test_apps","search_order":"example","path":"tests/test-app","port":15712}`.
The wire names are `target_name` and `package_name`; those fields have not been renamed.

Tool calls return MCP `structuredContent` with `status`, `message`, `call_info`, and an optional
raw business `result`. A large result may instead be a file reference with `saved_to_file`,
`filepath`, `instructions`, and `original_size_tokens`; read the named file for the full value.
Failures set MCP `isError` and `status="error"`. `error_info` carries stable context such as
`stage`, `method`, `port`, BRP `code` and `data`, while `metadata` retains the earlier detail fields
during migration. An unregistered method has `stage="discovery"` and `available_methods`.
Invalid tool parameter types return `stage="parameter_validation"` with a reason.
A watching method has `stage="unsupported_call_mode"`; use `world_get_components_watch` or
`world_list_components_watch` for the supported watch subscriptions, then `brp_stop_watch`.
Other watching methods have no MCP watch entry. An accepted input or queued text result does not
prove that the UI responded; check application state separately. Screenshot success means the PNG
has been published at the requested path.

`bevy_brp_mcp` runs over stdio. Its normal build exposes application, BRP, watch, and log tools. It does not list or dispatch the server's own trace controls. A call to `brp_get_trace_log_path` or `brp_set_tracing_level` in this build returns an MCP unknown-tool error.

Build the normal server with `cargo build -p bevy_brp_mcp --locked`. For an MCP server failure, explicitly build and run `cargo build -p bevy_brp_mcp --locked --features mcp-debug`. That build also lists `brp_get_trace_log_path` and `brp_set_tracing_level`. Call `brp_set_tracing_level` with `{"level":"debug"}` (supported levels: `error`, `warn`, `info`, `debug`, `trace`), then call `brp_get_trace_log_path` to locate the server trace in the system temp directory. The trace file is created lazily when an enabled event is written. This option is available to any user who selects the diagnostic build; it is not tied to identity or repository location. Both builds retain the normal BRP tools.

`brp_list_logs`, `brp_read_log`, and `brp_delete_logs` handle logs from applications launched through MCP and from MCP watch subscriptions. They do not expose the server trace. `brp_list_logs` returns a `source` field (`app` or `watch`) for each file. `source` may be `all` (default), `app`, or `watch`; `app_name` filters only app logs and cannot be used with `source=watch`. For example:

```json
{"source":"app","app_name":"test_app","verbose":true}
```

Use a listed filename with `brp_read_log`, for example `{"filename":"bevy_brp_mcp_test_app_port15702_1787840000123.log","tail_lines":50}`. A watch tool also returns its `log_path`; its historical log remains readable after `brp_stop_watch`. Use `brp_delete_logs` with `{"source":"watch"}` to clean watch logs, or with `{"source":"app","app_name":"test_app"}` for one application's logs. Omitting filters retains the existing application and watch cleanup scope. Deletion errors are returned to the caller. The server trace must be managed through its own local file path; these public log tools never read or delete it.

Application errors remain available as MCP tool errors. The diagnostic build adds server trace controls for investigating failures within the MCP server itself; it does not change BRP method registration in the Bevy application.
