# MCP tools, diagnostics, and logs

The normal `tools/list` registry has 47 native tools. The catalog of application methods is a
separate BRP response: call `brp_list_agent_tools` with `{"port":15712}` on a running app. Select
an entry by `name` and description, then pass its exact `method` and raw JSON parameters to
`brp_execute`, for example `{"method":"test/multiply","params":{"value":6,"factor":7},"port":15712}`.
The catalog may be empty; omitted `params_schema` or `result_schema` means the app supplied no
schema. Catalog entries are not added to `tools/list`. For other one-shot methods,
`rpc_discover` lists the live BRP methods that `brp_execute` can call. The default port is 15702.
For a one-shot method, `brp_execute` calls the method directly. A method-not-found BRP response
triggers `rpc.discover` for `available_methods`; it never retries the original call. Watching
methods retain their discovery check and return `unsupported_call_mode` when registered. If the
fallback discovery fails, `error_info` retains the original method's `code` and `data` and includes
the nested `discovery_error`.
When fallback discovery confirms the method is absent, `error_info` includes `available_methods`
alongside the original BRP `code` and `data`. `method_error_message` is the BRP client's message and
may include plugin guidance for a missing extras method.
To start this repository's fixture through MCP, call `brp_launch` with
`{"target_name":"extras_plugin","package_name":"bevy_brp_test_apps","search_order":"example","path":"tests/test-app","port":15712}`.
The wire names are `target_name` and `package_name`; those fields have not been renamed.
If a search root contains multiple packages and the target's package directory is already known,
scope `brp_list_bevy` to it, for example `{"path":"tests/test-app"}` from this repository root.
A single-package root may already return one target, with no byte saving from a narrower path.
When the location is unknown, search the workspace root so other packages remain visible.
`relative_path` changes with the search root: if the root is the package directory itself, the
field is its directory name (such as `test-app`), not `.`. Do not join it to the supplied `path`;
the parent of `manifest_path` is the absolute package directory for a later `brp_launch` call.

Every listed tool retains an `outputSchema` with the same validation constraints for the
structured result. Repeated schema `title` and `description` annotations are omitted from that
schema; tool descriptions and the result fields remain available.

Tool calls return MCP `structuredContent` with `status`, `message`, `call_info`, and an optional
raw business `result`. A large result may instead be a file reference with `saved_to_file`,
`filepath`, `instructions`, and `original_size_tokens`; read the named file for the full value.
The MCP `content` array is empty because the same JSON is available in `structuredContent`.
Successful calls omit the request `parameters` echo; errors may retain it for diagnosis.
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
{"source":"app","app_name":"test_app"}
```

Use a listed filename with `brp_read_log`, for example `{"filename":"bevy_brp_mcp_test_app_port15702_1787840000123.log","tail_lines":50}`. The default non-verbose listing is enough for this step; set `verbose=true` when paths, sizes, or timestamps are needed. A watch tool also returns its `log_path`; its historical log remains readable after `brp_stop_watch`. Use `brp_delete_logs` with `{"source":"watch"}` to clean watch logs, or with `{"source":"app","app_name":"test_app"}` for one application's logs. Omitting filters retains the existing application and watch cleanup scope. Deletion errors are returned to the caller. The server trace must be managed through its own local file path; these public log tools never read or delete it.

Application errors remain available as MCP tool errors. The diagnostic build adds server trace controls for investigating failures within the MCP server itself; it does not change BRP method registration in the Bevy application.

## Ordinary mouse input and control

Version 0.3.1 requires matching App libraries and MCP from the same `v0.3.1` Git tag or full commit; see the [fixed consumption instructions](../README.md#connect-an-mcp-client). Ordinary mouse methods produce Custom Pointer input only. Their successful responses confirm acceptance; wait for input work and verify the App's actual UI separately. Hosts supply enabled Picking/interaction plugins and a backend. Raw/native mouse consumers and official Mouse-only hover consumers need migration.

| Tool suffix (`brp_extras_…`) | Parameter boundary |
| --- | --- |
| `move_mouse` | Exactly one of `position` or `delta`, each two finite f32 logical pixels; per-window BRP history starts at the origin. |
| `send_mouse_button` | `button`: Left/Right/Middle; `duration_ms`: default 100, maximum 60000, zero allowed. Same-window move/scroll can continue during the hold. |
| `click_mouse` | `button`: Left/Right/Middle; one 100 ms click after establishing a Picking hit. |
| `double_click_mouse` | `button`: Left/Right/Middle; two 100 ms clicks, `delay_ms` default 250 after the first release. Bevy's multi-click interval determines the event count. |
| `drag_mouse` | `button`: Left/Right/Middle; finite `start`/`end` pairs; `frames` at least 1 counts interpolation steps, with additional setup/release cycles. |
| `scroll_mouse` | Finite `x`/`y`, `unit`: Line/Pixel, preserved in Pointer scroll. |

Every tool accepts optional `window` entity bits. Omission resolves the last BRP target, then PrimaryWindow. Explicit invalid/destroyed windows fail, and switching windows or repeating the same button while it is held fails. Automatic gestures wait for existing timed holds and keep later input queued. New physical Mouse move/press/scroll cancels the Custom generation. Cancellation emits Cancel, which does not imply a successful Release, Click or Drop.

`brp_extras/pointer_control` is an App instant method discovered by `rpc_discover`, with no extra static MCP tool or default agent catalog entry. Invoke it through `brp_execute`:

```json
{"method":"brp_extras/pointer_control","params":{"action":"status"},"port":15712}
```

The raw value under MCP `structuredContent.result` initially has this shape:

```json
{"phase":"inactive","busy":false,"pointer_id":null,"window":null,"generation":0,"queued_actions":0,"pressed_buttons":[],"last_error":null}
```

After activation, `pointer_id` is the stable UUID and `window` is the current window's entity bits. `phase` is inactive, active or draining; an idle active Pointer can hover with busy=false. `pressed_buttons` uses Left/Right/Middle. A queued failure leaves `last_error` with `generation`, `method`, `window`, `code` and `message`; status and release preserve it until a successful new activation. Requests must be objects containing only the string `action` status or release; other shapes/fields return INVALID_PARAMS without changing state.

In a cleanup/finally block, call:

```json
{"method":"brp_extras/pointer_control","params":{"action":"release"},"port":15712}
```

Release bypasses the FIFO and invalidates pending work. It is idempotent during draining and when inactive. Poll status until phase=inactive before handing input back or starting another automation generation; ordinary input is rejected while draining. Busy=false alone does not prove that a widget action succeeded. Native Window move/resize is unsupported: avoid those regions, whose existing native observers may still react to Custom Press. The [desktop and automatic validation record](testing.md) separates tested ordinary UI behavior from Widgetry and native-window boundaries.
