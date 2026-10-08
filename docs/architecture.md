# 当前 workspace 架构

根 `Cargo.toml` 是 virtual workspace，统一管理依赖版本与 lint；根 `Cargo.lock` 由 workspace 共用。生产 package 位于 `crates/`，测试宿主 package 位于 `tests/`。目录名不改变 Cargo package 名或 target 名。

| 路径 | Package 与入口 | 职责 |
| --- | --- | --- |
| `crates/mcp/` | `bevy_brp_mcp`，binary `bevy_brp_mcp` | Agent 侧 stdio MCP server；内部模块完成工具注册、BRP HTTP 调用、App 发现和进程管理。`help_text/` 承载工具说明。 |
| `crates/mcp_macros/` | `bevy_brp_mcp_macros`，proc-macro library | MCP server 使用的工具描述、注册、参数和结果 derive。 |
| `crates/extras/` | `bevy_brp_extras`，library | Bevy App 的 BRP 扩展方法、输入、截图与插件配置。可单独安装 `BrpExtrasPlugin`。 |
| `crates/runtime/` | `bevy_brp_runtime`，library | 将 extras 方法与 wake-aware HTTP transport 组合，提供 `BrpRuntimePlugin`。上游派生说明见 [`UPSTREAM.md`](../crates/runtime/UPSTREAM.md)。 |

生产 crate 的内部依赖只有 `bevy_brp_mcp → bevy_brp_mcp_macros` 与 `bevy_brp_runtime → bevy_brp_extras`。MCP server 与 Bevy App 之间通过 BRP 协议通信，没有互相引用的 Cargo dependency。

## 模块与公共入口

`crates/mcp/src/main.rs` 装配 stdio server。`mcp_service.rs` 处理 MCP tool listing 与调用；`tool/` 保存静态工具定义、参数、结果及响应装配；`app_tools/` 负责查找、构建、启动和管理 App；`brp_tools/` 包含 BRP tool 与 HTTP client；`log_tools/` 管理 Server trace 及经 MCP 启动的 App 日志读取。`crates/mcp_macros/` 在编译期生成部分工具定义、参数和结果代码。MCP 是 binary package；其模块 re-export 服务本 binary，不构成可依赖的 Rust library API。

`crates/extras/src/plugin.rs` 装配 App 内扩展方法与可配置的 HTTP 插件；`agent_tools/` 发布指定 BRP method 的元数据，并在 catalog 请求时核对 live method；`keyboard/`、`mouse/`、`screenshot/` 等模块实现对应 App 内能力。`crates/runtime/src/lib.rs` 的 `BrpRuntimePlugin` 装入不自带 HTTP 的 `BrpExtrasPlugin` 与 runtime 自己的 wake-aware HTTP 插件；`http.rs` 管理传输和 listener lifecycle，`progress.rs` 管理请求到达后的推进状态。runtime 的上游来源与本地语义差异见 [`UPSTREAM.md`](../crates/runtime/UPSTREAM.md)。

对外 Rust 入口由 `crates/extras/src/lib.rs` 的 re-export 和 `crates/runtime/src/lib.rs` 的 `BrpRuntimePlugin` 提供。`BrpRuntimePlugin::with_port` 可在代码中设置 Main BRP 端口；有效的 `BRP_EXTRAS_PORT` 环境变量优先，Render 端口保持 Bevy 默认值。extras 的 re-export 包括 `BrpExtrasPlugin`、`AgentTool`、`AppAgentToolExt`、`BrpExtrasActivity`、`BrpExtrasActivityState`、`DEFAULT_REMOTE_PORT`、`ExternalTransport`、`HasEffectivePort`、`HttpPluginConfigured`、`PortConfigured`、`PortDisplay`、`Unconfigured`。macro crate 暴露 `ToolDescription`、`BrpTools`、`ParamStruct`、`ResultStruct`、`ToolFn` 五个 derive，当前由 MCP binary 使用。`extras` 可独立安装扩展方法及其 HTTP transport；使用 `runtime` 时由 runtime 组合扩展方法与自己的 transport。App 通过 `RemoteMethods` 注册 BRP method；`AppAgentToolExt::register_agent_tool` 只发布 method 的 agent 元数据，catalog 请求时会验证对应 method 是已注册的 instant method。该调用不注册 BRP handler，也不创建 MCP tool。MCP 对外入口是 binary、当前 tool registry/schema/help text 与 BRP 通信。各 crate 的 feature 和具体依赖以其 `Cargo.toml` 为准。

经 MCP 启动的应用日志与 MCP server 自身的 trace 分属不同用途和读取路径；App library 本身不安装全局日志 subscriber。当前日志 ownership 与输出约束见 [`rules/logging.md`](../rules/logging.md)。

Extras 的 `mouse` 模块以 App 内稳定的 Custom Pointer 和统一 `First` 输入生产者承接 move、button、click、double-click、drag、scroll；在 Picking backend 前暂时停用物理 Mouse 的命中位置，新物理输入使 Custom generation 失效。取消在交互派发后补齐原 press/drag 目标的框架 Cancel，并等待下一 Picking 周期确认退场。Extras 显式依赖 `bevy_picking` 与 UUID，但不安装 Picking 核心、交互插件或 backend，宿主承担装配责任；无 Picking 的宿主仍能安装 Extras 和使用其他能力。自动手势共用 FIFO，以真实经过时间和 Picking 周期分别保证等待与命中时序，结束消费后释放 activity；timed hold 期间允许同窗口移动和滚轮，自动手势等待已有 hold 结束并阻挡后续输入。窗口或 Pointer 实体失效同样取消旧 generation。

`brp_extras/pointer_control` 只注册为 App instant method，经既有 `brp_execute` 调用并由 `rpc.discover` 发现，不增加静态 MCP tool 或默认动态 agent tool。status 返回生命周期、工作状态、稳定 UUID、目标窗口、generation、待输出动作、按键及异步错误快照；release 绕过 FIFO 启动取消，调用方查询至 inactive 后交接。状态查询不增加 activity，空闲 active Pointer 允许继续 hover 而不维持忙状态。

## 测试宿主

| 路径 | Package 与 Cargo target | 验证用途 |
| --- | --- | --- |
| `tests/test-app/` | `bevy_brp_test_apps`；bin `test_app`，examples `extras_plugin`、`no_extras_plugin`、`event_test`、`mouse_test`、`keyboard_windows`、`pointer_test`、`test_app` | 真实 Bevy App、runtime、输入、事件、截图和 stock BRP 宿主。`keyboard_windows`、`pointer_test` 显示两个窗口各自收到的输入，并提供正常窗口关闭的测试方法。 |
| `tests/test-duplicate-a/` | `test-app-a`；examples `extras_plugin_duplicate`、`test_app` | 与另一个 package 的同名 example、与 `test-app` 的同名 target，用于发现、路径和搜索顺序验证。 |
| `tests/test-duplicate-b/` | `test-app-b`；example `extras_plugin_duplicate` | 同名 example 的第二个宿主。 |
| `crates/extras/tests/` | `bevy_brp_extras` 的 integration test target | 从 crate 外部验证 agent tool 与 pointer_control 注册/协议。 |

`tests/` 容纳多个 workspace member 及 Windows 协议回归脚本 `regression.ps1`，不是根 package 的 Cargo test harness。各宿主的 bin/example 需要通过对应 package 的 `cargo run -p …` 或 MCP 启动；`cargo test --workspace` 本身不会运行完整的 MCP→BRP 交互。普通与图形回归入口见[测试说明](testing.md)。

`bevy_brp_test_apps` 直接依赖 `bevy_brp_runtime` 和 `bevy_brp_extras`；`test-app-a`、`test-app-b` 直接依赖 `bevy_brp_extras`。三个宿主的 Bevy feature 用于真实场景 fixture，未传播给生产 crate。`crates/extras/tests/` 则由 `bevy_brp_extras` package 的 Cargo test harness 运行。

从仓库根目录使用 MCP 的 `path` 参数时，目录应使用当前实际路径，例如 `tests/test-app` 或 `crates/extras`。`brp_list_bevy` 返回的 `relative_path` 也反映这些物理目录；跨 package 的同名 target 仍可用 `package_name` 消歧。直接使用本仓库旧 manifest 路径的本地 `path` dependency 需要改为对应的 `crates/<name>` 路径；按 package 名消费的 Git dependency 和 `bevy_brp_mcp` binary 名称不变。
