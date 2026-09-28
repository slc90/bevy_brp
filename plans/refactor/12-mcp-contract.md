# 12 · 面向使用者的 MCP 契约决定

状态：待用户确认的设计结论；尚未修改 tool registry、schema、help text 或运行行为。方案 13 先落实自诊断分离，方案 14 再落实公共契约。依据是当前源码、`docs/architecture.md`、[01 基线](01-baseline.md)及其 `assets/01-baseline/` 对照样本；基线的 49 项来自当时默认构建，当前源码仍以 `ToolName` 枚举注册静态 MCP tools。不能把本文件当成已实施的公共使用文档。

## 决定与取舍

普通构建保留 47 个原生工具：49 项现有工具中，仅 `brp_get_trace_log_path` 和 `brp_set_tracing_level` 转入显式诊断构建。**不合并其余工具，也不删除能力。** 这个数量由下表逐项决定，不是数量目标。已知直接操作有稳定的类型化参数、可发现的名称和独立完成语义；把它们改成 `brp_execute` 会要求先发现方法、手写原始 JSON，并失去现有参数约束或错误帮助。将多种 world 写入、鼠标动作、watch 或截图合并为一个带 `operation` 的工具，则需要互斥字段及大量条件 schema，选择错误更难定位。新增的应用方法不扩充静态目录，使用应用目录和动态执行。

表中 `保留` 表示名称、现有参数 schema、默认值、成功结果和 annotation 均保持；方案 14 只做下文明确列出的日志边界与错误保真改动。`BRP 值` 指当前 `structuredContent.result` 承载的方法结果；空结果保持可识别的成功，不伪造业务数据。每行的失败均遵守“失败契约”节；`M` 为 method/transport 错误，`L` 为本地工具错误，`W` 为 watch 生命周期错误。除列出的必需字段外，`port` 在所有 BRP 工具中可省略，默认 15702。实体 ID 为 canonical u64 数值；反射 type 名、BRP method 名大小写及拼写精确匹配。

### 应用、发现和动态调用

| 原工具 → 目标入口 | 去向与用途 | 关键参数 → 业务结果 | 失败与迁移 |
|---|---|---|---|
| `brp_list_bevy` → 同名 | 保留；找 Cargo target | `path?` → targets、count | L；不迁移 |
| `brp_launch` → 同名 | 保留；构建并启动 target | `target` 必需；`path?`, `package?`, `profile?`, `port?`, `instance_count?`, `search_order?`, `env?`, `args?` → PID/端口/启动信息 | L；保留 target 消歧和实例语义 |
| `brp_status` → 同名 | 保留；检查应用进程及 BRP | `app_name` 必需、`port?` → 运行状态、PID/端口或失败状态 | L；不迁移 |
| `brp_shutdown` → 同名 | 保留；清理应用进程 | `app_name` 必需、`port?` → PID、`method`、warning 等 | L；保留 clean shutdown 与回退结果，不伪装普通 BRP 方法 |
| `rpc_discover` → 同名 | 保留；完整 BRP/OpenRPC 方法发现 | `port?` → 原始 discovery 文档，method_count | M；不迁移 |
| `brp_list_agent_tools` → 同名 | 保留；发现开发者发布的精选应用能力 | `port?` → `{usage,tools:[{name,method,description,params_schema?,result_schema?}]}`，tool_count | M；不迁移 |
| `brp_execute` → 同名 | 保留；对 live BRP method 作原始一次性调用 | `method` 必需、`params?` 原始 JSON、`port?` → 原始 BRP result，可为 null/缺省 | M；不迁移 |

### World、资源、类型

这些工具继续直接选择对应 BRP method。`result` 保留原始 BRP 值；`world_find_entities_by_name` 返回匹配的 canonical ID 列表和数量。类型工具的格式化 guide 仍是面向调用者的辅助结果，不当作 BRP 的类型定义替代品。

| 原工具 → 目标入口 | 用途及关键参数 → 业务结果 | 失败与迁移 |
|---|---|---|
| `world_find_entities_by_name` → 同名 | `name`, `match_mode?` → `[{entity,name}]`、entity_count | M；不迁移 |
| `world_query` → 同名 | `data` 必需，`filter?`, `strict?` → entity/component 查询值及数量 | M；不迁移 |
| `world_list_components` → 同名 | `entity?` → 指定实体或注册组件列表 | M；不迁移 |
| `world_get_components` → 同名 | `entity`, `components`, `strict?` → 组件值 | M；不迁移 |
| `world_insert_components` → 同名 | `entity`, `components` type→JSON map → BRP 写入结果 | M；不迁移 |
| `world_mutate_components` → 同名 | `entity`, `component`, `path`, `value` → BRP 字段修改结果 | M；不迁移 |
| `world_remove_components` → 同名 | `entity`, `components` → BRP 删除结果 | M；不迁移 |
| `world_spawn_entity` → 同名 | `components` type→JSON map → 新 entity ID | M；不迁移 |
| `world_despawn_entity` → 同名 | `entity` → BRP 删除结果 | M；不迁移 |
| `world_reparent_entities` → 同名 | `entities`, `parent?` → BRP 层级修改结果；省略 parent 表示解除父子关系 | M；不迁移 |
| `world_trigger_event` → 同名 | `event`, `value?` → BRP 触发结果 | M；不迁移 |
| `world_list_resources` → 同名 | 无额外字段 → 资源列表、resource_count | M；不迁移 |
| `world_get_resources` → 同名 | `resource` → 资源值 | M；不迁移 |
| `world_insert_resources` → 同名 | `resource`, `value` → BRP 写入结果 | M；不迁移 |
| `world_mutate_resources` → 同名 | `resource`, `path`, `value` → BRP 字段修改结果 | M；不迁移 |
| `world_remove_resources` → 同名 | `resource` → BRP 删除结果 | M；不迁移 |
| `registry_schema` → 同名 | `with_crates?`, `with_types?`, `without_crates?`, `without_types?` → schema、type_count | M；不迁移 |
| `brp_type_guide` → 同名 | `types` → 指定 type 的 guide | M；不迁移 |
| `brp_all_type_guides` → 同名 | 无额外字段 → 全部可生成的 guide；可能很大，优先按需使用 `brp_type_guide` | M；不迁移 |

### App 扩展、输入和截图

App 内方法只有安装相应 extras 能力时可用。输入结果中的“queued/sent”只说明被 App 接受或安排，不能声称最终 UI 已响应；持续动作需保留各自终态及时间字段。截图结果在 PNG 完全发布后才成功。

| 原工具 → 目标入口 | 用途及关键参数 → 业务结果 | 失败与迁移 |
|---|---|---|
| `brp_extras_screenshot` → 同名 | `path` 必需；`entity?` 或 `name?` 二选一，`camera?`, `padding?` → 保存结果、路径/目标信息 | M；不迁移，保持终态、唯一名称和裁剪错误 |
| `brp_extras_send_keys` → 同名 | `keys`, `duration_ms?` → keys_sent、duration_ms、key_count | M；不迁移，保持按键释放语义 |
| `brp_extras_type_text` → 同名 | `text` → chars_queued、skipped | M；不迁移 |
| `brp_extras_set_window_title` → 同名 | `title` → old_title、new_title、status | M；不迁移 |
| `brp_extras_move_mouse` → 同名 | `delta?` 或 `position?`, `window?` → App 返回值 | M；不迁移，保持位置与位移约束 |
| `brp_extras_send_mouse_button` → 同名 | `button`, `duration_ms?`, `window?` → App 返回值 | M；不迁移，保持持续按压完成语义 |
| `brp_extras_click_mouse` → 同名 | `button`, `window?` → App 返回值 | M；不迁移 |
| `brp_extras_double_click_mouse` → 同名 | `button`, `delay_ms?`, `window?` → App 返回值 | M；不迁移 |
| `brp_extras_drag_mouse` → 同名 | `button`, `start`, `end`, `frames`, `window?` → App 返回值 | M；不迁移，保留帧数和 release 语义 |
| `brp_extras_scroll_mouse` → 同名 | `x`, `y`, `unit`, `window?` → App 返回值 | M；不迁移 |
| `brp_extras_pinch_gesture` → 同名 | `delta` → App 返回值 | M；不迁移 |
| `brp_extras_rotation_gesture` → 同名 | `delta` → App 返回值 | M；不迁移 |
| `brp_extras_double_tap_gesture` → 同名 | 无额外字段 → App 返回值 | M；不迁移 |
| `brp_extras_get_diagnostics` → 同名 | 无额外字段 → App 的 FPS/诊断值 | M；不迁移；这是**应用**诊断，不是 MCP Server 自诊断 |

### Watch 与日志

| 原工具 → 目标入口 | 去向、参数 → 业务结果 | 失败与迁移 |
|---|---|---|
| `world_get_components_watch` → 同名 | 保留；`entity`, 非空 `types`, `port?` → watch_id、log_path | W；不迁移 |
| `world_list_components_watch` → 同名 | 保留；`entity`, `port?` → watch_id、log_path | W；不迁移 |
| `brp_list_active_watches` → 同名 | 保留；无参数 → watches、watch_count | W；不迁移 |
| `brp_stop_watch` → 同名 | 保留；`watch_id` → 已停止的 watch_id | W；不迁移 |
| `brp_list_logs` → 同名 | 保留应用及 watch 日志目录；`app_name?`, `verbose?=false`, `source?=all` → logs、log_count、temp_directory；每条记录增 `source` | L；默认仍含应用及 watch 日志，Server trace 不进入目录 |
| `brp_read_log` → 同名 | 保留应用及 watch 日志读取；`filename`, `keyword?`, `tail_lines?` → content、filename、file_path、size_bytes、size_human、lines_read、过滤状态 | L；Server trace filename 拒绝；应用/watch 原有调用保持 |
| `brp_delete_logs` → 同名 | 保留应用及 watch 日志清理；`app_name?`, `older_than_seconds?`, `source?=all` → deleted_files、deleted_count、filter 元数据 | L；默认仍处理应用及 watch 日志，Server trace 不被删除 |
| `brp_get_trace_log_path` → 同名，诊断构建 | Server trace 文件路径、存在性、大小 | L；普通构建 `tools/list` 不列出，调用返回 MCP unknown tool |
| `brp_set_tracing_level` → 同名，诊断构建 | `level` → 生效 level 与 trace 文件 | L；普通构建 `tools/list` 不列出，调用返回 MCP unknown tool |

## 动态目录和执行边界

`tools/list` 始终只列 Server 静态 registry。`brp_list_agent_tools` 是请求运行中 App 的 `brp_extras/agent_tools` BRP method，返回应用发布的 `name`、**准确的 backing `method`**、说明及可选原始 JSON Schema。它不会创建原生 MCP tool；空目录也是成功。App 未安装 extras 时返回有 method、port、stage 的失败；目录中的任一 backing method 缺失或属于 watching method 时，整个请求失败，不返回部分目录。

调用精选方法的标准步骤：按 port 调 `brp_list_agent_tools`，按 `name` 和 description 选条目，照 `params_schema` 构造**原始 BRP params**，将该条目的 `method` 原样传给 `brp_execute`，按 `result_schema` 解读 `result`。`params_schema`/`result_schema` 缺省表示 App 没有提供 schema，不表示空 object；Server 不推断、不改写 schema，也不自动校验应用参数。`brp_execute` 适用于 `rpc.discover` 列出的**一次性** BRP method，包括未列入精选目录的 stock/应用方法；它先发现方法并要求 exact match，再转发原始 JSON。未注册方法在 `stage=discovery` 失败并给 `available_methods`；已注册但要求 watch/SSE 的方法不应被伪装成一次性成功，应返回可识别的 `unsupported_call_mode` 错误，提示使用专用 watch 工具（仅当前两种 watch）或明确说明无 MCP watch 入口。方案 14 应为此增加验证和结构化错误；不能用一次性 HTTP 超时掩盖它。动态调用不替代 Server 拥有的应用发现、启动、状态、关闭、日志或 watch 管理。

## 结果、错误与字段职责

继续使用 MCP `structuredContent` 和 `isError`。成功：`status="success"`，`result` 放业务值（可为 JSON object/array/scalar/null；无 BRP result 时允许缺省），`metadata` 只放 count、watch_id/log_path、路径、时长、过滤条件等支持后续动作的附加信息；`message` 用于人读，`call_info` 表示来源。过大的 `result` 沿用当前文件回退机制：`result` 变为含 `saved_to_file`、`filepath`、`instructions`、`original_size_tokens` 的文件引用，调用者按路径读取完整 JSON；这个回退不能被解释成业务结果本身。失败：`isError=true`、`status="error"`、人读 `message`；`error_info` 保留可机器判断的 `stage`、`method`、`port`、BRP `code`、原始 `data`、输入 `field` 或本地原因（按适用性），不把 BRP 失败包装成 success。迁移期保留现有 `parameters` 回显与可用的 `brp_extras_debug_info`，方案 14 不删除它们；它们是诊断性附加字段，消费者不应依靠其决定业务成功。`result` 不额外套统一 envelope，也不把原始 BRP error 放进 success result。

当前已实现的 `brp_execute` 能保留 BRP code/data，catalog 有明确的 catalog_fetch/decode/version/request 错误；部分类型化 BRP 路径目前只把 code 写入 message。方案 14 需要以现有 `error_info` 增补原始 code/data 与 method/port，保持现有 type_guide 辅助信息及 `isError`，不得把此目标写成现状。参数 schema 保持已有 required/optional、数字与 JSON 类型；`port` 仍接受当前有效范围（1024–65534），省略时 15702。JSON 字符串归一化只用于 schema 声明允许 object/array 的字段，不把数字字符串默默当数字。所有具体参数细节以实施时生成的 `tools/list.inputSchema` 与对应 help text 相互校验。

`M`：BRP 返回错误时保留 method、port、code、data；连接失败、畸形响应、超时按 transport/stage 标识。`L`：本地发现、build、进程和文件错误保留 target/path/filename/stage，不能伪造 BRP code。`W`：start 成功只表示订阅已建立，返回可停止的 watch_id 和日志路径；SSE 后续异常由 watch 生命周期/日志体现，stop 后从 active list 消失；启动失败不能留下 active watch。截图只在 PNG 原子发布后成功，原有文件不应被失败调用误认为新结果。输入工具保留 accepted/queued 与最终完成的区别。

## 日志和显式自诊断

当前 `brp_list_logs` 的 generic filename 扫描会包含应用与 watch 日志，`brp_read_log` 接受所有 `bevy_brp_mcp_*.log`，`brp_delete_logs` 无 app_name 时也会扫所有这类文件。方案 13 要依据**文件来源**分离：MCP 启动的 App 日志采用 `..._port<port>_<timestamp>.log`，watch 日志采用 watch manager 返回的路径及受限文件名，Server trace 由 Server tracing 模块持有。公共三工具继续服务 App 和 watch 日志，默认 `source=all` 指这两种来源；`source=app|watch` 用于明确过滤，`app_name` 只对 App 日志有效，和 `source=watch` 同时给出时返回参数错误。`brp_list_logs` 每条记录增 `source: "app"|"watch"`；`brp_read_log` 按经验证的文件名归属访问指定应用/watch 文件，不接受任意相同前缀文件。`brp_delete_logs` 保留原有无过滤批量清理应用/watch 的能力，但每个实际删除失败须可识别，不能默默少报。Server trace 仅由诊断构建的 trace 工具定位；公共 list/read/delete 在两种构建中都不能枚举、读取或删除 trace。watch 的 `log_path` 继续由 watch 工具返回，stop 后历史日志仍可读/清理。

选择复用现有 `mcp-debug` feature，并在方案 13 把默认 features 设为空。普通构建：`cargo build -p bevy_brp_mcp --locked`，静态枚举 47 项；两项 trace 工具未知且不可调用，但应用/watch 日志三工具正常。排障构建：`cargo build -p bevy_brp_mcp --locked --features mcp-debug`，枚举公共 47 项加两项现有 trace 工具；自诊断由操作者**显式选用该 binary**，不是基于用户身份的权限层。诊断构建内公共日志工具仍限 App/watch 来源；Server trace 按 `brp_get_trace_log_path` 返回的本地路径排查，不新增 MCP trace 读取/删除工具。保留 trace 原有文件与 level 机制，不为方案 13 设计另一个 transport。

## 代表性调用

以下为目标参数形状示意，实际实体 ID、端口和路径由当前会话取得，结果只展示稳定字段：

```json
{"name":"brp_launch","arguments":{"target":"test_app","package":"bevy_brp_test_apps","path":"tests/test-app","port":15712}}
{"name":"world_find_entities_by_name","arguments":{"name":"NatesList","match_mode":"exact","port":15712}}
{"name":"world_get_components","arguments":{"entity":4294967298,"components":["bevy_transform::components::transform::Transform"],"port":15712}}
{"name":"world_mutate_components","arguments":{"entity":4294967298,"component":"bevy_transform::components::transform::Transform","path":".translation.x","value":42.0,"port":15712}}
{"name":"brp_extras_screenshot","arguments":{"name":"NatesList","path":"C:/temp/nates-list.png","port":15712}}
{"name":"brp_shutdown","arguments":{"app_name":"test_app","port":15712}}
```

```json
{"name":"brp_list_agent_tools","arguments":{"port":15712}}
{"name":"brp_execute","arguments":{"method":"test/multiply","params":{"value":6,"factor":7},"port":15712}}
{"name":"world_get_components_watch","arguments":{"entity":4294967298,"types":["bevy_transform::components::transform::Transform"],"port":15712}}
{"name":"brp_stop_watch","arguments":{"watch_id":1}}
{"name":"brp_list_logs","arguments":{"app_name":"test_app"}}
{"name":"brp_read_log","arguments":{"filename":"bevy_brp_mcp_test_app_port15712_<timestamp>.log","tail_lines":50}}
```

目录条目示例为 `{"name":"multiply","method":"test/multiply","description":"...","params_schema":{"type":"object"},"result_schema":{"type":"number"}}`；它仅存在于 `brp_list_agent_tools` 的 `result.tools`，不会出现在 `tools/list`。

相应 `tools/call` 的 `structuredContent` 示例（省略与该调用无关的可选字段）：

```json
{"status":"success","message":"Executed method test/multiply","call_info":{"mcp_tool":"brp_execute"},"result":42}
{"status":"error","message":"BRP method `test/missing` is not registered on port 15712","call_info":{"mcp_tool":"brp_execute"},"metadata":{"stage":"discovery","method":"test/missing","port":15712,"available_methods":["rpc.discover","test/multiply"]}}
```

第二个示例的 MCP `isError` 为 `true`。当前框架把部分错误细节放在 `metadata`；方案 14 要把调用者需判断的稳定错误字段明确放到 `error_info`，并在迁移期保留原有 `metadata` 字段，避免静默破坏依赖现状的调用者。示例的可用方法列表仅用于说明形状，不代表真实 App 只有两个方法。

## 兼容及实施验收

不改 47 个公共 tool 名称、BRP method 名、既有参数默认值或成功字段；现有原生调用方无需迁移。变化是普通构建不再列/调用两项 Server trace 工具；需要它们的操作者显式安装 `mcp-debug` 构建。旧调用者若通过公共日志工具处理 Server trace 文件，应改用诊断构建及 `brp_get_trace_log_path` 返回的本地路径；应用/watch 日志原有调用保持，新增可选 `source` 及记录中的来源字段。Server trace 的公共日志访问收紧及 trace 工具默认隐藏是**破坏性变化**，本文件供用户确认，方案 13 不应在确认前实施。错误中新增结构化字段是兼容性扩展，不删除现有字段。

方案 13 验收：分别构建普通与 `mcp-debug` binary，经真实 stdio `tools/list` 和 `tools/call` 核对枚举与 unknown-tool 行为；启动测试 App 并建立 watch，验证三个公共日志工具对应用/watch 的默认及 `source` 过滤、trace 拒绝、删除失败报告，清理进程、watch、端口。方案 14 验收：schema/help text/annotation 与 47 项矩阵一致；真实 MCP→BRP 链依次验证应用生命周期、world read/write、精选目录与动态执行、缺失/ watching method、watch stop、截图终态及输入返回；单测覆盖参数边界、BRP code/data、transport failure 和 malformed response。用 01 的固定场景比较行为与结构，记录真实调用次数和 wire bytes；无 Codex token 数据时不宣称 token 收益。已知基线失败与新失败分开报告，未执行的 GUI/设备场景单列。用户确认本结论后再进入方案 13，随后方案 14。
