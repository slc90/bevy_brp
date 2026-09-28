# MCP server diagnostics and logs

`bevy_brp_mcp` runs over stdio. Its normal build exposes application, BRP, watch, and log tools. It does not list or dispatch the server's own trace controls. A call to `brp_get_trace_log_path` or `brp_set_tracing_level` in this build returns an MCP unknown-tool error.

Build the normal server with `cargo build -p bevy_brp_mcp --locked`. For an MCP server failure, explicitly build and run `cargo build -p bevy_brp_mcp --locked --features mcp-debug`. That build also lists `brp_get_trace_log_path` and `brp_set_tracing_level`. Call `brp_set_tracing_level` with `{"level":"debug"}` (supported levels: `error`, `warn`, `info`, `debug`, `trace`), then call `brp_get_trace_log_path` to locate the server trace in the system temp directory. The trace file is created lazily when an enabled event is written. This option is available to any user who selects the diagnostic build; it is not tied to identity or repository location. Both builds retain the normal BRP tools.

`brp_list_logs`, `brp_read_log`, and `brp_delete_logs` handle logs from applications launched through MCP and from MCP watch subscriptions. They do not expose the server trace. `brp_list_logs` returns a `source` field (`app` or `watch`) for each file. `source` may be `all` (default), `app`, or `watch`; `app_name` filters only app logs and cannot be used with `source=watch`. For example:

```json
{"source":"app","app_name":"test_app","verbose":true}
```

Use a listed filename with `brp_read_log`, for example `{"filename":"bevy_brp_mcp_test_app_port15702_1787840000123.log","tail_lines":50}`. A watch tool also returns its `log_path`; its historical log remains readable after `brp_stop_watch`. Use `brp_delete_logs` with `{"source":"watch"}` to clean watch logs, or with `{"source":"app","app_name":"test_app"}` for one application's logs. Omitting filters retains the existing application and watch cleanup scope. Deletion errors are returned to the caller. The server trace must be managed through its own local file path; these public log tools never read or delete it.

Application errors remain available as MCP tool errors. The diagnostic build adds server trace controls for investigating failures within the MCP server itself; it does not change BRP method registration in the Bevy application.
