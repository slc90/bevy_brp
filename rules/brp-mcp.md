# BRP 与 MCP 边界规则

## 运行位置与注册面

MCP server 负责 stdio 协议、静态 tool registry、BRP client、App 发现与进程操作；Bevy App 内的 method、ECS system、输入、截图和 transport 推进由 App 侧插件承担。MCP 通过 BRP 调用 App，不把 App 内部执行搬到 MCP server，也不让 App library 依赖 MCP 的工具框架。

BRP method 注册在 App 的 `RemoteMethods`。`AppAgentToolExt::register_agent_tool` 发布指定 method 的元数据，供 `brp_extras/agent_tools` 查询；catalog 请求时验证 backing method 已注册且为 instant method。元数据注册本身不注册 handler，也不创建静态 MCP tool。`brp_list_agent_tools`/`brp_execute` 等现有 MCP 工具和 App 的动态 method catalog 是不同的注册面。修改其中一面时，逐项说明另一面是否受影响，不因名称或 schema 相似就自动同步生成工具。当前公共工具集合以运行中的 registry、源码和 help text 为准，不在规则中预定最终工具清单。

## BRP client、HTTP 与插件 ownership

MCP 侧 BRP client 只处理目标 port 上的协议请求、响应与传输错误；App 侧 HTTP transport 负责 listener、mailbox、wake、deadline 和结束路径。改变这些职责时，应先明确请求及结果的 owner、timeout、cancellation、watch 终止和 cleanup，再同步受影响的两端 contract。

App 安装扩展方法与 HTTP transport 时必须有明确的 transport owner。单独使用 `BrpExtrasPlugin` 按其公开配置安装或复用 HTTP 插件；由 `BrpRuntimePlugin` 组合时，extras 只注册方法和所需 system，runtime 安装自己的 wake-aware transport。不得意外安装两个相互竞争的 HTTP transport，或使端口配置在已有 transport 下被默默解释成另一套监听行为。派生 transport 的来源与本地差异以 [`UPSTREAM.md`](../crates/runtime/UPSTREAM.md) 为准。

## 结果、错误与诊断

MCP 的结构化结果、BRP JSON-RPC result/error 与工具展示文本各有用途。跨层映射应保留调用方需要的 method、port、stage、field、错误 code/data 及已有成功/失败语义；不得仅为统一 envelope 抹去原始 BRP 错误或把失败伪装成成功。新增或改变 wire contract 时，按 [`documentation.md`](documentation.md) 同步 schema、help text 和 rustdoc，并按 [`testing.md`](testing.md) 验证结构与实际调用。

普通使用者需要从应用日志和结构化错误定位 App 问题；MCP server 自身 trace 用于服务端诊断。两者保持独立的 ownership 和读取边界，具体日志级别、输出位置与敏感信息规则见 [`logging.md`](logging.md)。这里不预设未来工具集合或日志启用机制。
