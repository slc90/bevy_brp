# 16 · MCP 效率优化复测结果

记录时间：2026-09-28（Asia/Shanghai）。初始专项以 [15 的当前状态样本](15-efficiency-findings.md) 为 before，实施其中 A（精简重复 `outputSchema` 注解）和 B（已知多 package 搜索范围时的定向发现指引）；对应复测见下文。用户随后确认只用 Codex，并要求基于真实 Gallery 调用优化返回内容，补充实施了 C（移除重复 JSON 文本和成功响应的参数回显）；其 before/after 单独记录。tool 名、input schema、BRP method 和 App transport 均未改变。原始方案 01 到方案 15 的差异仍单独记录在方案 15。

## 修改与兼容边界

- `ToolDef::generate_output_schema()` 保留 schemars 生成的 `$schema`、`type`、`properties`、`required`、`enum` 和 `anyOf`，递归去掉每个 tool 的 output schema 中重复的 `title` 与 `description`。47 个普通工具的 output schema 均由 1,035 变为 540 bytes；工具自身的 `description`、`title` 和 `annotations` 仍保留。公共 schema 元数据少了这些注解，依赖它们显示说明的客户端需要适配；结果字段和验证约束不变。
- `brp_list_bevy` 的 help text 和 `docs/mcp.md` 明确：目标 package 目录已知且搜索范围包含多个 package 时，可以使用既有 `path` 缩小返回集；目录未知时保持宽范围发现。`relative_path` 随搜索根改变；当搜索根恰为 package 目录时返回目录名而非 `.`，不能拼到传入的 `path` 后。需要稳定绝对目录时应取 `manifest_path` 的父目录。Gallery 所在 workspace 根目录只发现一个 target，实测更窄路径**不节省字节**，因此没有把定向发现写成无条件推荐。

## 固定测试宿主复测

使用原方案 15 的协议重放脚本，默认 feature、debug profile、协议 `2025-11-25`、同一 `bevy_brp_test_apps` 任务序列、端口 15713。方案 15 的完整 before 在 [assets/15-efficiency/](assets/15-efficiency/)；本轮完整 after 在 [assets/16-efficiency/](assets/16-efficiency/)。原始状态→当前状态与本表的当前状态→方案 16 是不同对照。

| 指标 | 方案 15 before | 方案 16 after | 判断 |
| --- | ---: | ---: | --- |
| 普通构建工具数 | 47 | 47 | 工具集合与排序相同。 |
| `tools/list` 响应 wire bytes | 109,281 | 86,650 | 净少 22,631 bytes，约 20.7%。schema 注解自身的静态减少量为 23,265 bytes；新增发现说明增加约 634 bytes。 |
| 每项 output schema 紧凑 JSON bytes | 1,035 | 540 | 47 项的非注解结构逐项相同；input schema 逐项相同。 |
| MCP 工具调用 | 25 | 25 | 初始化和目录请求另计；没有靠跳过任务动作减少调用。 |
| 完整任务 server→client wire bytes | 172,374 | 153,027 | 本次样本少 19,347 bytes；PID、日志、时间戳和路径内容会变，不能将该值视为稳定或 token 收益。 |
| 预期主窗口截图错误 | 1 | 1 | `test_app` 最小化窗口的既有错误仍可辨认；离屏 fixture 截图成功。 |

两轮离屏 PNG 均为 64×48、285 bytes，SHA-256 为 `173338a581192cfc7fc4bd19d56d78844a1dc28bac953d71b1649d1980916903`。watch 被显式停止，两个宿主均通过 `clean_shutdown` 退出。按同一 tool 顺序逐项比较，tool 名与 input schema 完全相同，after 的 output schema 恰为 before 递归删去 `title`/`description` 的结果。`structuredContent`、text content 和 `isError` 的实际路径仍由同一重放覆盖。

另一次新 MCP 进程的完整重放以与方案 15 相同的 `Measure-Command` 方式测得 12,050 ms；方案 15 的单次记录为 12,010 ms。两者相差 40 ms，样本量不足以判断耗时变化，且都包含固定等待与应用启动。该次重放返回两个 `clean_shutdown`，退出后再等待约 2 秒确认宿主进程及 15713/15703 listener 消失。

对于已知 package 目录，仍只改变 `brp_list_bevy.path` 为 `tests/test-app`：该次响应在 before 和 after 都是 7,080 bytes，单次调用本身没有被改写。after 的完整定向重放为 25 次工具调用、server→client 146,221 bytes，仍得到正确目标、截图和 clean shutdown；对应精简记录见 [scoped-discovery.json](assets/16-efficiency/scoped-discovery.json)。该场景中根目录发现返回 13 个 target、约 15.2 KB，定向发现返回 6 个 target、约 7.1 KB；同一 target 的 `relative_path` 分别为 `tests\\test-app` 和 `test-app`，后者不能拼到 `tests/test-app` 后面。B 的收益只来自调用方已知目录并实际采用窄搜索，不能由 help text 更新自动保证。

## 真实 Gallery exe 复测

用户指定的 `Documents/gallery` 目录不存在；经用户确认，使用同级 `bevy_widgetry/target/debug/widget_gallery.exe` 的预构建 exe。外部仓库及 exe 均只读，工作目录为 `bevy_widgetry` 根目录。该程序使用自己的 `BrpRuntimePlugin`，监听 Main 端口 15702、Render 端口 15703。测试从当前仓库启动 MCP stdio server，通过以下同一调用链观察 before/after：

1. `tools/list`。
2. `brp_status({"app_name":"widget_gallery","port":15702})`。
3. `rpc_discover({"port":15702})`。
4. `world_query({"data":{},"filter":{"with":["bevy_window::window::Window"]},"port":15702})`。
5. `brp_list_bevy({"path":"../bevy_widgetry"})` 与已知目录 `brp_list_bevy({"path":"../bevy_widgetry/gallery"})`，实际执行时使用相应绝对目录。

| Gallery 观测 | 修改前 | 修改后 |
| --- | ---: | ---: |
| 普通工具目录响应 wire bytes | 109,282 | 86,651 |
| 目录工具数 | 47 | 47 |
| `brp_status` 结果 bytes / 状态 | 683 / success | 683 / success |
| `rpc_discover` 结果 bytes / method 数 | 4,601 / 39 | 4,601 / 39 |
| Window 查询结果 bytes / entity 数 | 835 / 1 | 835 / 1 |
| 根目录发现结果 bytes / target 数 | 1,607 / 1 | 1,607 / 1 |
| `gallery` 子目录发现结果 bytes / target 数 | 1,627 / 1 | 1,627 / 1 |

随后在新启动的同一 exe 上查询该 Window entity 的 `bevy_window::window::Window` Component，结果中确有 `Widget Gallery` 标题；再次取得 39 个 BRP method，并用 `brp_shutdown({"app_name":"widget_gallery","port":15702})` 得到 `clean_shutdown`。退出后 Gallery 进程与 15702/15703 listener 均消失。这里 `gallery` 子目录比根目录多 20 bytes；B 在此单 target 环境没有收益。Gallery exe 是外部预构建消费者，此测试证明当前 MCP 可与它交互，不证明它使用本 checkout 的 App-side 源码。

第一次同时运行 Gallery 与 `test_app` 时，`test_app` 在绑定固定 Render 端口 15703 时遇到 `os error 10048` 并退出，完整重放因此失败。先让 Gallery clean shutdown，再重放同一测试宿主链成功。该失败属于两个真实 App 争用固定 Render 端口的测试环境冲突；没有修改 runtime 或把它算作优化回归。

## 自动化验证与未测项

- 新的 output schema 契约测试先在旧实现上失败，实施后通过；默认与 `mcp-debug` 构建分别验证。诊断构建列出 49 项，目录响应 89,198 bytes，所有 output schema 都没有 `title`/`description` 注解，两项 trace 工具仍在。
- `cargo fmt --all -- --check`、`cargo check --workspace --locked`、`cargo clippy --workspace --all-targets --locked` 及普通/诊断构建通过。
- `cargo test -p bevy_brp_mcp --locked`：105 passed、2 failed。仅有的两个失败是方案 01 已记录的 Windows dep-info 路径测试 `returns_fresh_when_binary_is_newer_than_inputs` 和 `returns_stale_when_dependency_is_newer_than_binary`；本次没有改动其实现或断言。方案 16 新增测试通过。
- 未取得真实模型客户端的输入、缓存输入、输出 token、模型轮次或费用；所有收益均按 MCP wire 原始字节或调用数陈述。完整 MCP→BRP 链的逐 HTTP 请求数未单独采集，不能用 MCP 调用数替代。严格外部 JSON Schema 校验器也未运行；本仓库测试逐项核对了 schema 非注解结构。初始 A/B 专项没有运行 Gallery 截图和输入；后续 C 的专项执行了这两类调用。

## 后续运行时调用候选（提出时未实施）

本项实施范围仍是已确认的 A/B；用户随后询问运行时 tool call 是否还能优化，故额外做了只读归因。以下证据不自动扩大方案 16 的施工范围。

- `brp_execute` 的当前成功路径先调用 `rpc_discover::discover_method_names(port)`，再用 `BrpClient::execute_raw_once()` 调用目标 method。因此一次 MCP `brp_execute` 对已注册 instant method 会发出两次 BRP/HTTP 请求。真实 Gallery 中执行 `world.query` 返回 1 个 Window entity；该 MCP 调用的单次观测为 20.15 ms。独立的 `world_query` 调用也返回 1 个 entity，单次观测 7.79 ms；两者的 MCP 结果 envelope 不同、样本量只有一次，不能把耗时差直接认定为优化收益。候选实现是对 `brp_execute` 先执行一次，只有确实返回“method 不存在”时再调用 `rpc.discover` 形成现有 `stage="discovery"` 与 `available_methods` 错误。必须同时保持普通 BRP 错误的 code/data、transport failure、watch 的 `unsupported_call_mode`，且不得重试写操作。未知 method 的现行 Gallery 响应含 39 个 `available_methods`；这条诊断路径不能因省请求而消失。可验证的目标是有效 method 从两次降为一次 BRP 请求，MCP tool call 次数仍为一次。
- `brp_list_logs` 的 `verbose` 默认是 false。仅需列出文件名后调用 `brp_read_log` 时，传 `verbose:true` 会附加路径、大小和时间。在当前临时日志集合上，用相同的 `app_name="test_app"` 查询到相同 9 个 filename；verbose 响应为 6,529 bytes，非 verbose 为 2,679 bytes，少 3,850 bytes。这是现有调用参数的选择，不需要更改服务端；若任务需要路径或时间，仍使用 verbose。
- `brp_extras_screenshot` 已支持用唯一精确 `name` 在一次 MCP 调用中完成 Name 查询和截图。若用户目标只是按已知名称截图，事先单独调用 `world_find_entities_by_name` 会多一次 MCP 调用和一次 Name 查询；当前固定重放保留该调用，因为它还承担显式发现/验证目标，不把少做验证算作同一任务的收益。

## 真实 Gallery 的单次调用输出归因（优化前）

随后按用户要求再次启动其确认的 `widget_gallery.exe`，通过本仓库默认构建的 MCP stdio server 与 Gallery 的 Main BRP 端口 15702 执行 10 次 tool call：`brp_status`、两种 `world_query`、`world_get_components` 成功与未知 Component 错误、两次 `brp_extras_move_mouse`、`brp_extras_click_mouse`、`brp_extras_screenshot`、`brp_shutdown`。Window 查询返回一个 entity；绝对移动到 `[10,10]` 后返回 `new_position=[10,10]`、`delta=[10,10]`，再相对移动 `[5,0]` 返回 `new_position=[15,10]`、`delta=[5,0]`。点击返回 `button=Left`；没有据此推断 UI 效果。截图工具返回成功，并生成 116,181 bytes 的 PNG；只核对文件生成和大小，没有目视核对画面。截图临时文件测试后删除。未知 Component 的错误仍包含 `stage=execution`、`method=world.get_components`、`port=15702`、错误 code 和说明。`brp_shutdown` 成功，Gallery 正常退出，15702/15703 不再监听。

| Gallery 调用 | 当前响应 wire bytes | 假设只去掉重复 JSON 文本 | 再假设成功响应不回显 `parameters` |
| --- | ---: | ---: | ---: |
| `world_query`（只取 Window entity ID） | 834 | 423 | 299 |
| `world_get_components`（完整 Window） | 3,690 | 1,791 | 1,698 |
| `brp_extras_move_mouse`（绝对位置） | 644 | 337 | 286 |
| `brp_extras_move_mouse`（相对移动） | 630 | 330 | 284 |
| `brp_extras_click_mouse` | 592 | 310 | 266 |
| `brp_extras_screenshot` | 1,501 | 732 | 547 |
| 10 次调用合计 | 11,345 | 5,683 | 4,875 |

这 10 项的 `content[0].text` 都与 `structuredContent` 的 JSON 数据逐项相同。表中后两列仅由同一真实响应重算 wire bytes；这些数字是优化前的预测，不是复测结果。单去掉文本副本理论上少 5,662 bytes，约 49.9%；再去掉成功响应的参数回显可多省 808 bytes。实际模型 token 是否双重计入取决于宿主如何呈现 MCP 结果，当前尚无该数据。

判断：重复文本是最高优先级的输出候选。[OpenAI Docs 的 MCP tool result 文档](https://developers.openai.com/plugins/reference#tool-results)确认 `structuredContent` 可呈现给模型，且示例允许只返回它。`parameters` 回显在成功路径的价值较低，尤其截图路径同时出现在 message、parameters 和 result 中。鼠标移动的 `new_position` 与实际 `delta` 是有用的业务结果，不宜删除。完整 Window Component 是所请求的数据本身；只需定位 Window 时，现有 `world_query` 的空 `data` 已能返回 entity ID，不需要读取整个 Component。错误响应的 `stage`、`method`、`port`、code 与说明应保留。

## Codex 结构化结果精简后的复测

`ToolCallJsonResponse` 转为 MCP `CallToolResult` 时现在返回空 `content` 和完整的 `structuredContent`；成功响应不再回显 `parameters`，错误响应仍可保留参数和既有错误字段。`outputSchema` 中的 `parameters` 原本可选，仍用于错误响应，不需删除 schema 字段。两项契约测试先在旧实现上失败，修改后通过。未更改 App 侧 BRP result，也未删掉鼠标位置、实际 delta、完整 Window Component 或截图结果。

用同一个 Gallery exe、新 MCP 进程、相同 10 项调用及参数复测，成功响应和预期错误都含 `structuredContent`，`content=[]`，成功响应均无 `parameters`。Gallery 按 `clean_shutdown` 退出，进程和 15702/15703 listener 消失。截图 PNG 仍为 116,181 bytes；此次只核对文件生成与大小，没有目视判断画面。下表是原始 MCP response wire bytes；两次 Gallery 运行的动态 PID 不同，不能把每项差额作为永久不变的常数。

| Gallery 调用 | 精简前 | 精简后 |
| --- | ---: | ---: |
| `world_query`（只取 Window entity ID） | 834 | 299 |
| `world_get_components`（完整 Window） | 3,690 | 1,698 |
| `brp_extras_move_mouse`（绝对位置） | 644 | 286 |
| `brp_extras_move_mouse`（相对移动） | 630 | 284 |
| `brp_extras_click_mouse` | 592 | 266 |
| `brp_extras_screenshot` | 1,501 | 547 |
| 同一 10 次调用合计 | 11,345 | 4,875 |

原方案 15 的 25 次调用序列也用独立测试宿主重放成功：47 个 tool、25 次 tool call、相同调用顺序和预期的最小化主窗口截图错误；离屏截图仍为 285 bytes，两个宿主均 `clean_shutdown`。这轮 tool 响应合计 31,579 bytes；优化前记录为 66,236 bytes，约少 34,657 bytes。两轮进程、路径和时间戳等动态字段不同，因此这是任务样本总量，不是精确归因到 C 的固定收益。完整 server→client 总量由原记录 153,027 bytes 变为 118,561 bytes，包含 `tools/list`；当前目录响应 86,841 bytes，比本文件早先 A/B 样本的 86,650 bytes 多 191 bytes，原因是随后修正了 `brp_list_bevy` 的 `relative_path` help text，并非 C 改动目录。`tests/public-mcp-regression.ps1` 通过，覆盖动态 method、错误与 watch 拒绝。直接 Codex 模型会话和 token 账单仍未测量。

截至 C 的结论：A 在工具目录上减少重复 schema 注解；B 只在已知目标目录且宽范围确实返回额外 package 的场景有效；C 在已测 Gallery 与固定任务样本中显著减少调用响应字节，并保留已验证的业务结果与错误上下文。`brp_execute` 的直接执行加缺失回退在下一节 D 实施。

## D · `brp_execute` 请求次数优化

用户已确认实施前述剩余候选。普通 instant method 现在直接执行一次；只有 BRP 返回 JSON-RPC method-not-found（`-32601`）才追加 `rpc.discover`，并且不重试原调用。若追加发现时 method 已注册，保留原 BRP `code`/`data` 并返回 `stage=execution`；若仍未注册，返回 `stage=discovery` 与 `available_methods`，同时保留原 BRP 的 `code`、`data` 和 BRP client 的 `method_error_message`（缺失 extras method 时可能附加插件提示）；若发现本身失败，保留原 BRP 错误并在 `discovery_error` 中附上发现错误。带 `+watch` 的名称继续先发现，以保留已注册 watch 的 `unsupported_call_mode` 和未注册 method 的发现错误。transport 失败、其他 BRP 错误均不触发发现。没有引入 capability 缓存或修改 App transport。

新的定向测试先观察到旧实现对成功和普通错误都先发 `rpc.discover`，实施后验证了成功一次请求、未知 method 的“原 method → 发现”两次请求、普通 BRP 错误一次请求，以及注册状态变化、发现回退自身失败、已注册/未知 watch、transport 失败的结构化错误与请求顺序。`cargo fmt --all -- --check`、`cargo check --workspace --locked`、`cargo clippy --workspace --all-targets --locked` 和 MCP 构建通过；`tests/public-mcp-regression.ps1` 通过。`cargo test -p bevy_brp_mcp --locked` 最终复测为 115 passed、2 failed，失败的仍是前述两个 Windows dep-info 路径测试。

真实 Gallery exe 测试通过临时本地 HTTP 计数代理将 MCP 的 15704 端口请求转发至 Gallery 的 15702 端口。固定单次 tool call 的实测 BRP method 顺序如下：

| `brp_execute` 场景 | 实测 BRP method 顺序 | MCP 结果 |
| --- | --- | --- |
| `world.query` 成功 | `world.query` | 成功，返回 1 个 Window entity。 |
| `brp_extras/move_mouse` 写操作 | `brp_extras/move_mouse` | 成功，返回 `[10,10]` 的新位置；未重复执行。 |
| `world.query` 参数错误 | `world.query` | `stage=execution`，保留 `-32602`。 |
| 未注册 `does.not.exist` | `does.not.exist` → `rpc.discover` | `stage=discovery`，含 `available_methods` 与原 BRP `code=-32601`。 |
| 已注册 `world.list_components+watch` | `rpc.discover` | `stage=unsupported_call_mode`，未打开 watch stream。 |
| 未注册 `missing+watch` | `rpc.discover` | `stage=discovery`。 |

六次 MCP tool call 合计经过代理的 BRP 请求为 7 次；Gallery 经 `brp_shutdown` 返回 `clean_shutdown` 并以退出码 0 结束，15702/15703/15704 listener 均清理。代理第一次试运行不支持 Gallery HTTP 响应的 chunked framing，导致测试代理自身超时和一次 MCP transport 错误；补齐 chunked 读取后同一场景全部通过。这不是产品实现失败。当前工具目录为 47 项、87,376 bytes，较 C 时 86,841 bytes 增加 535 bytes，来自更新 `brp_execute` 与 `brp_list_logs` 的 help text。`brp_list_logs.verbose=false`、按唯一 `name` 截图、world 查询按需选择数据已经由现有工具参数支持；本轮同步了日志的默认精简调用指引，没有添加同义接口。没有取得 Codex 模型会话的实际 token 用量。
