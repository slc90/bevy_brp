# 01 · 当前 main 改造基线

记录时间：2026-09-27（Asia/Shanghai）。本文件只记录方案 01 的 before 状态，不包含修复、重构或 tool surface 调整。

## 起点与工作区

- 仓库：`slc90/bevy_brp`，本地分支 `main`。
- 基线提交：`ccc13ae736de3cc491b6ece8100775cfc0866aed`（`增加重构方案`，2026-09-27T12:28:48+08:00）。
- 开始执行时 `git status --porcelain=v2` 无文件项，工作区干净；不存在需要隔离的用户未提交改动。
- 本地记录的 `origin/main` 为 `60aa2d4b3f313fa25b7320561a4cec5d61fa1db1`，本地 `main` 领先 1 个提交、落后 0 个提交。领先提交只增加 `plans/bevy-brp-refactor/` 下的改造方案。本次按用户指定的当前本地 `main` 固定基线，没有 fetch、同步上游或把结果表述成远端 `origin/main` 的纯净结果。
- 仓库内没有 `AGENTS.md`，因此本项没有额外命中规则。
- 执行后仅新增本文件和 `plans/refactor/assets/01-baseline/` 中的测量资产；生产代码未变。没有提交、推送或发布。

## 环境

- OS：Windows 10 Pro 64 位，build 19045。
- Rust：`rustc 1.98.1 (48a229cea 2026-09-01)`，host `x86_64-pc-windows-msvc`，LLVM 22.1.8。
- Cargo：`cargo 1.98.1 (797e8a9bc 2026-08-05)`。
- toolchain：`stable-x86_64-pc-windows-msvc`（default）。仓库 `rustfmt.toml` 使用 nightly-only 选项，但本项没有格式化 Rust 源码。
- 实际 Bevy 启动日志：Windows kernel 19045，AMD Ryzen 9 3900X（12 cores），31.9 GiB memory；NVIDIA GeForce RTX 4070 Ti SUPER，NVIDIA 595.79，Vulkan backend。
- MCP/BRP 对照端口：15712；完成后端口连接数为 0，`test_app` / `extras_plugin` 进程数为 0。

## 当前入口、crate、feature 与消费者

| 角色 | 当前入口与依赖事实 |
|---|---|
| MCP 生产入口 | `mcp/src/main.rs`，crate `bevy_brp_mcp`，stdio MCP server；默认 feature 为 `mcp-debug`，因此当前工具目录包含 trace level/path 自诊断工具。 |
| Bevy runtime | `runtime/src/lib.rs` 暴露 `BrpRuntimePlugin`；`runtime/src/http.rs` 是 wake-aware BRP HTTP transport。runtime 直接启用 Bevy 的 `bevy_log`、`bevy_render`、`bevy_winit`。 |
| Bevy extras | `extras/src/lib.rs` 暴露 `BrpExtrasPlugin`；默认 features 为 `diagnostics`、`ui`，native target 使用 Bevy Remote HTTP transport，并启用 PNG/image/tempfile 支持。 |
| MCP 宏 | `mcp_macros/src/lib.rs`，crate `bevy_brp_mcp_macros`，为参数、结果、描述和工具注册生成代码。 |
| 主要测试宿主 | `test-app/src/bin/test_app.rs` 使用 `BrpRuntimePlugin::default()`；`test-app/examples/event_test.rs`、`mouse_test.rs` 也走 runtime。 |
| extras 专项宿主 | `test-app/examples/extras_plugin.rs` 直接使用 `BrpExtrasPlugin`，覆盖类型、输入、agent tools 与截图 fixture；`no_extras_plugin.rs` 保留原生 `RemotePlugin + RemoteHttpPlugin` 边界。 |
| Bevy feature 实际范围 | workspace 的 Bevy 默认关闭；各生产 crate 定向开启所需 feature。`bevy_brp_test_apps` 为类型/渲染 fixture 使用 Bevy 全默认 feature，不能代表生产 crate 的 feature 面。 |
| 本仓库内依赖 | `test-app` 对 `bevy_brp_runtime`、`bevy_brp_extras` 都使用 workspace/path 依赖。 |
| 已知仓库外消费者 | 只读核对 `C:\Users\<USER>\Documents\bevy_widgetry`：workspace 以 Git tag `v0.1.0` 引用 `bevy_brp_runtime`，lockfile 固定到 `60aa2d4b3f313fa25b7320561a4cec5d61fa1db1`；Gallery 继承该 workspace 依赖。该消费者未修改。 |
| MCP 分发方式 | README 指引从同一 Git tag 安装 `bevy_brp_mcp`；crate 均未发布到 crates.io。当前 `v0.1.0` 解析到提交 `60aa2d4b...`。 |

## 功能状态

“已证实”只表示本轮真实调用通过；工具目录中存在但没有调用的能力不会据此标为可用。

| 能力 | 状态 | 本轮证据 |
|---|---|---|
| MCP 启动、握手、工具枚举与调用 | 已证实 | stdio 初始化协商 `2025-11-25`；`tools/list` 返回 49 项、`private` cache scope、TTL 0；后续 27 次工具调用均得到协议响应。 |
| 应用发现、启动与状态 | 已证实 | `brp_list_bevy` 后分别以 app 和 example 启动 `test_app`、`extras_plugin`；`brp_status` 均确认指定端口有 BRP。 |
| BRP 方法发现 | 已证实 | runtime 宿主的 `rpc_discover` 返回 Bevy 0.19.1 OpenRPC 文档和 39 个方法。 |
| BRP 查找、读取与修改 | 已证实 | 用 `world_query` 找到唯一 Sprite entity；读取 Transform 的 `translation.x = 0.0`，修改为 `42.0` 后再次读取确认。 |
| Bevy runtime 请求推进 | 已证实 | `test_app` 在启动系统中主动最小化窗口，仍通过 `BrpRuntimePlugin` 完成 discovery、query、两次 read、mutation、watch、输入、日志和 clean shutdown 请求；这固定了 wake-aware runtime 的 before 行为。 |
| watch 建立与结束 | 已证实 | 对 Sprite Transform 建立 watch 1，active list 返回 1 项；修改后等待 500 ms，再显式 stop 成功。watch 日志路径由 MCP 返回并保留在系统临时目录，不纳入版本库。 |
| 应用日志 | 已证实 | 两个应用均能 list/read；runtime 日志包含 `MARKER:baseline-01`、GPU 初始化和 runtime transport 日志，fixture 日志包含实体及截图 fixture ready 信息。 |
| MCP 自诊断 | 已证实 | `brp_set_tracing_level(debug)` 成功，`brp_get_trace_log_path` 返回已存在的 trace 文件及大小。临时路径只在样本中以 `<USER_HOME>` 保存。 |
| 输入 | 部分证实 | 两个宿主的 `brp_extras_type_text("baseline")` 都接受并返回 queued 8、skipped 0；本轮没有断言最终 UI 文本内容。其他键盘、鼠标和手势工具仅在工具目录中出现，未调用。 |
| 截图 | 部分证实 | runtime `test_app` 的默认主窗口截图真实调用失败：`Screenshot capture requires a primary window`。随后在 `extras_plugin` 恢复窗口，使用已有 `NatesList` 确定性 fixture；warm-up 和保存调用均终态成功。保存图为 64×48、285 bytes，人工检查为非黑的蓝/黄 fixture 内容。 |
| 应用关闭 | 已证实 | 两个宿主都由 `brp_shutdown` 使用 `clean_shutdown`，没有退化为 process kill；随后进程和端口均清空。 |
| 资源 CRUD、entity spawn/despawn/reparent、事件、agent catalog/execute、type guides、diagnostics、其余输入工具 | 未测 | 当前工具目录存在这些工具，但本项没有真实调用，不能据此断言行为可用。 |

核心链路可运行，因此当前没有阻断后续行为等价验收的环境前提。主窗口截图失败是需要与后续结果区分的宿主/会话基线，不影响已有离屏 fixture 的截图对照。

## 固定 MCP 对照样本

样本时间为 2026-09-27 13:04–13:05（Asia/Shanghai），使用 debug profile、端口 15712。

1. 初始化 MCP，保存完整 `tools/list`。
2. 打开 MCP debug tracing、读取 trace 路径，枚举 Bevy targets。
3. 启动 `bevy_brp_test_apps/test_app --marker baseline-01`，确认状态并发现 BRP 方法。
4. 查询 Sprite entity，读取 Transform，建立 watch，把 `.translation.x` 改为 `42.0`，再次读取确认，列出并停止 watch。
5. 排队输入 `baseline`，保存主窗口截图的预期失败，读取应用日志，clean shutdown。
6. 启动 `bevy_brp_test_apps/examples/extras_plugin`，恢复 Windows 窗口并定位 `NatesList`；等待渲染后先 warm-up，再保存同一 64×48 entity crop；排队输入、读取日志并 clean shutdown。

计量口径：共 29 个有 id 的 MCP request（initialize 1、tools/list 1、tools/call 27）和 1 个 initialized notification。27 个工具调用中 26 个成功，1 个是被固定的主窗口截图错误。按实际 newline-delimited JSON UTF-8 wire（包含行结束符）统计，client→server 为 4,829 bytes，server→client 为 174,014 bytes；请求数与原始字节分开记录。

本次使用的是无模型的本地协议重放器，所以没有 Codex/模型轮次、客户端 token usage 或缓存输入数据。`null` 表示确实取不到；没有用文件大小、服务端请求数或估算值代替 token。

## 测试基线

执行：

```powershell
cargo test --workspace --no-fail-fast
```

结果：exit 1。各目标合计报告 184 passed、2 failed、9 ignored；`extras` 80 unit + 1 integration 通过，runtime 11 通过，相关 doc-tests 通过。Cargo 另报告测试 workspace 内重复 example 名的 output filename collision warning，这是专用重复目标 fixture 带来的现状。

仅有的两个失败位于 `mcp/src/app_tools/launch/build_freshness.rs`：

- `returns_fresh_when_binary_is_newer_than_inputs`
- `returns_stale_when_dependency_is_newer_than_binary`

两者都发生在 Windows dep-info 路径解析：生成的路径把临时目录/target 片段错误拼接，随后报告 `dependency listed in dep-info is missing`。本项开始时生产代码未改，这两项因此记录为 before 的既有 Windows 失败，不在方案 01 修复。

随后执行：

```powershell
cargo build -p bevy_brp_mcp -p bevy_brp_test_apps
```

结果：exit 0。最终重放脚本 exit 0，截图人工检查通过，端口及进程清理通过。

## fixture 与回归边界索引

- `runtime/src/http.rs` tests：mailbox enqueue/full/closed、deadline receiver cleanup、Main/Render fatal lifecycle；用于保护 wake 和 transport 结束语义。
- `runtime/src/progress.rs` tests：并发 activity、generation/fallback、render mailbox 与 bounded tail updates；用于保护请求推进策略。
- `mcp/src/app_tools/launch/build_freshness.rs` tests：dep-info 转义、缺失、fresh/stale 分支；当前两个 Windows 失败属于这里，后续不能在无替代覆盖时删除。
- `test-duplicate-a/`、`test-duplicate-b/`：跨 package 重名 examples；保护 `brp_launch` 的 path/package 歧义处理。
- `test-app/examples/test_app.rs` 与同名 bin：保护 `search_order=app|example` 的同名 target 选择。
- `test-app/examples/extras_plugin/screenshot_fixtures.rs`：固定 256×192 image target、camera viewport、UI/AABB bounds、隐藏/重复名/不支持类型错误；本基线采用其中 `NatesList` 的 64×48 crop。
- `extras` screenshot/keyboard/activity tests：保护 terminal publication、camera/entity 选择、裁剪、临时文件所有权、输入 release 和 activity ownership。
- `extras/tests/agent_tool_registration.rs` 与 MCP agent tool/name/screenshot tests：保护公开注册 API、catalog、exact name resolution 及结构化错误。
- MCP launch target scanning、log filename parsing、watch、type guide/mutation parser tests：分别保护发现/消歧、日志归属、watch 生命周期和 mutation/type contract。

该索引只说明当前直接相关用途，不宣称已穷举仓库所有测试。

## 样本位置与复现

- `assets/01-baseline/tools-list.json`：完整、脱敏的 tools/list request/response 与各自 wire bytes。
- `assets/01-baseline/interaction.jsonl`：按方向保存的固定交互；每行包含步骤名、原始 wire byte 数和脱敏 JSON payload。
- `assets/01-baseline/summary.json`：工具名、调用/字节计量、固定参数和终态摘要。
- `assets/01-baseline/test-app.png`：最终 `NatesList` 对照图。
- `assets/01-baseline/replay.ps1`：可重放脚本。默认写入唯一的系统临时目录，避免覆盖本 before 样本；需要指定目录时使用 `-OutputDirectory`。

从仓库根目录复现：

```powershell
cargo build -p bevy_brp_mcp -p bevy_brp_test_apps
& .\plans\refactor\assets\01-baseline\replay.ps1
```

脚本要求端口 15712 空闲，可用 `-Port` 更换。Windows 上脚本会恢复 `extras_plugin` 窗口并等待渲染；非 Windows 跳过该动作。输出会把仓库根路径替换为 `<REPO>`、用户目录替换为 `<USER_HOME>`；版本库样本不包含凭据。进程 PID、entity ID、时间戳、日志文件名和 response byte 数在重放时预期变化，因此比较行为与结构时不应把这些值当稳定 contract。

后续方案应引用而不是覆盖本目录；需要新测量时写到新目录。方案 01 到此交接，不自动进入方案 02。
