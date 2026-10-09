# 05. 验证 BRP Runtime 与 MCP 的整条协议链

> 迁移方案：`slc90/bevy_brp`，源码基线 `ae9fdaec25cb1f02b501f6e07199a27df144501e`；目标 Bevy `v0.20.0`。状态：**尚未实施、尚未验证**。本文是执行方案，不代表仓库已经达到文中的门槛。

## 目标

在已经能构建和运行的 0.20 App 测试宿主上，证明 Main/Render BRP、wake-aware HTTP、request/watch 生命周期、MCP 工具 schema、反射类型指导和真实协议调用能够保持原有合同。

## 范围

App 端 `crates/runtime/src/{http.rs,lib.rs,progress.rs}`、`UPSTREAM.md` 和必要 Extras 插件/活动接口；Agent 端 `crates/mcp/src/brp_tools/brp_type_guide/`、工具、BRP client/watch、help text 和测试。依赖方案 02 产出的测试宿主，不预设重写 transport、MCP server 或宏 crate。

## 预期产出

上游 0.19.1→0.20.0 transport 差异核对、最小必要补丁、Main/Render/wake/timeout/cancel/SSE/unknown-method/shutdown 证据、当前 live registry 对应的类型示例和真实 CRUD/事件/watch 调用、完整工具目录及错误数据对照；workspace 仍然可编译。

## 与前后方案的关系

方案 02 消除了 Rust 编译不兼容，方案 03/04 已证明输入和截图的直接语义；本阶段把它们与 transport 和 MCP 真实调用串成同一协议验收目标。下一方案再次运行全部 Windows 链路并做独立消费者、事实文档与版本交接。

## 迁移边界与共同约束

**目标是让现有能力在 Bevy 0.20.0 上保持可用，不借升级重做架构。** 保留四个生产 crate 的角色、package/target 名称和两条内部依赖：`mcp → mcp_macros`、`runtime → extras`；MCP 与 App 仍通过 BRP 通信。保留 `BrpExtrasPlugin`、`BrpRuntimePlugin`、`BrpRuntimePlugin::with_port`、activity 和 agent-tool 的公共使用方式。类型签名中的 Bevy 类型自然升级为 0.20 类型，不承诺它们与 0.19 类型二进制或 Rust 类型兼容。[架构][R-ARCH]、[边界规则][R-SCOPE]。

不新增“双版本兼容层”、新 crate、替代输入框架或新的 MCP 工具。保持项目自有 BRP/MCP 的方法名、字段、默认值、错误结构和注册边界；上游 `registry.schema` 中的类型集合、类型路径及上游新增方法可以随引擎变化，但不能被误当成项目自有协议变化。所有需要改变既有外部语义的情况，都必须先记录具体不兼容及原因，不得静默修改。

Windows 实施和图形验收使用 PowerShell 7，不使用 Windows PowerShell 5。保留现有 lint、最小 feature、错误传播和资源 ownership 约束；不用扩大 `allow`、删测试、清除历史样本或启用全量 Bevy feature 来掩盖错误。生产代码不得为实现裁剪而引入 `unsafe`，也不为小段几何运算增加依赖。[代码规则][R-CODE]、[依赖规则][R-DEPS]。

每个实际产生代码或工程行为变更的阶段，都遵守仓库要求的独立审查：完成必要验证后，由全新 reviewer 调用 `code-review`，对完整 working tree 做静态审查；有 findings 就修正并由另一位全新 reviewer 重审。缺少 reviewer/subagent 能力时明确记为阻塞，不用施工者自查替代。本文的文档覆盖检查不等于代码审查。[Agent 入口][R-AGENTS]。

各阶段使用同一迁移分支、同一 lockfile 和有记录的提交/工作区状态。保护已有用户修改；不要 reset、覆盖或夹带。测试结果区分“新回归”“已存在的失败”“环境缺失而未运行”，不能把 0.19 的旧成功记录写成 0.20 的新结果。阶段 01 须先确认旧 Bevy 0.19.1 基线可编译；阶段 02 切换至 0.20.0 后必须消除整个 workspace 的所有编译错误，阶段 03—06 每阶段结束亦须重新满足同一编译门槛，不得把本阶段的编译失败留待下一份方案。只有最终验收允许宣告整个升级可用。[测试规则][R-TEST-RULES]。

外部消费者，包括 `bevy_widgetry`，不在本次修改范围。此方案不授权创建或移动 Git tag、发布 package、覆盖本机已安装 MCP 或修改外部仓库。现有 `v0.3.1` 保持不动；最终先以经过验证的完整 revision 交接。后续发布时，runtime、Extras 和 MCP 必须来自同一 revision/tag，不混用。[Git 消费规则][R-DEPS]。

## BRP Runtime：wake-aware transport 的兼容与生命周期

这个子域直接决定真实 BRP HTTP/Render 请求能否送达和按时结束；与 MCP 端协议合在本阶段，是为了验证同一条 **App 内 Remote method → transport → MCP 客户端** 链路。虽然包含两个模块群，主目标只有一个：让这条链路在 Bevy 0.20 上保持原有可观察协议与生命周期开销。

### 先判断是否真的需要修改实现

0.20 正式源码仍提供 `BrpSender`、`BrpReceiver`、`BrpMessage`、`BrpRequest`、`BrpResponse`、`RemoteLast`、`RemoteSystems`、`RenderApp` 和现有 Winit proxy 入口。因此没有依据预先决定重写整个 `http.rs`。但“同名类型仍存在”不等于组合语义已经通过验证。[0.20 Remote 装配与消息][B-REMOTE]、[0.20 HTTP][B-HTTP]、[Winit][B-WINIT]。

对比固定的上游 `v0.19.1` 和 `v0.20.0` Remote HTTP/lib、Winit 及 Render 相关调度，明确每项是未变、编译适配，还是行为改变。不要把本地 transport 整文件覆盖成新的上游实现；那会丢失这个 crate 的 wake、deadline、双端点 lifecycle 和 progress 语义。[本地来源与六项差异][R-UPSTREAM]。

如果比对和回归证明现有实现已经兼容，可以不改核心代码，只记录验证；不要为显得“完成了迁移”制造无关 diff。

### 装配与调度边界

`BrpRuntimePlugin` 继续在窗口插件之后安装，依赖 Winit event-loop proxy。runtime 组合不自带 HTTP 的 Extras，并且自己拥有唯一 transport；与 stock `RemoteHttpPlugin` 的冲突在插件前后两种安装顺序下都必须明确拒绝。单独使用 Extras 和 stock HTTP 的消费方式继续有效。[公共 runtime 入口][R-RUNTIME-LIB]、[BRP ownership 规则][R-BRP-RULES]。

Main mailbox 的初始化必须早于 Main listener 接收需要处理的请求；当前上游在 `PreStartup` 建 mailbox，本地在 `Startup` 启动 Main listener。Render mailbox 的初始化与本地 Render listener 首次启动也要保持明确先后；0.20 上游在 `RenderStartup` 建 mailbox，并在 Render 后调度 `RemoteLast`。不要把这两种世界当成共享一个 mailbox，也不要把首次资源尚未存在解释成永久不支持 Render。[上游装配][B-REMOTE]、[本地 HTTP][R-RUNTIME-HTTP]。

本地进度核验仍位于 `RemoteSystems::Cleanup` 之后。Main 观察整体是否需要下一帧，Render 报告自己的残留工作；Render 在 Main 判空后才发现工作时，必须补发 wake，不能丢失跨世界边缘事件。

### 不能丢失的 transport 语义

成功提交消息后唤醒对应 App 主循环。mailbox 已满时，先唤醒已有工作让消费者排空，等待本消息成功入队后再唤醒；不能只在发送完成后 wake，否则休眠 App 和满队列可能互相等待。请求不能为了让队列腾空被悄悄丢弃。[本地 transport 来源说明][R-UPSTREAM]。

普通 HTTP 请求保留默认 30 秒 deadline。一次请求的责任从成功入队持续到首个结果、取消、超时或结果通道关闭；每条结束路径都更新计数并请求必要的 cleanup wake。不能根据 HTTP method 名称推测 ECS 注册一定是 Instant，因为普通请求也可能命中 Watching handler。

SSE 保留 request id、BRP result/error 编码及 Watching 生命周期。客户端断开、stream 取消或结果 receiver drop 后，watcher 必须得到清理机会。超时、未知方法、无效参数和底层传输失败仍保持可区分的错误，不将失败改成空成功。[上游 HTTP 约定][B-HTTP]、[项目错误边界][R-BRP-RULES]。

Main/Render 共用失败和 shutdown 状态；任一 listener 出现 fatal bind/accept 失败都关闭另一侧，并按现有 contract 结束 App。listener、connection future、server Task、watch 和临时捕获任务都有 owner 和结束路径，不能新增脱离 App 生命周期的后台任务。同步锁不跨 `await`。

Main 端口继续遵循有效 `BRP_EXTRAS_PORT` 优先、代码 `with_port` 次之、默认端口兜底；无效环境值沿用现有处理。Render 仍是上游默认 15703，不因 Main 改为测试端口就跟随变更。无 RenderApp 的合法消费场景不应为了启动第二 listener 而凭空增加渲染子 App。

### Progress 与 activity

`BrpProgress` 仍区分普通请求等待、Main/Render 残留 mailbox 和 Extras 持续动作。输入 timed hold、排队手势和 pending screenshot 期间应能续帧；工作结束后的有限 tail updates 用来交付清理，而不是永久切换成持续渲染。

保留 generation 对过期 activity 通知和 fallback 的隔离。旧 generation 的计时回调不能重新激活已经关闭的工作，完成计数不能下溢，shutdown 后不能被残留回调重新唤醒为 busy。

针对“未知 method 后还有合法请求”的组合专门回归。即使上游某版本调整了未知方法的 drain 行为，本地对剩余 mailbox 的检查仍须通过测试判断是否需要；不要只看到一处上游修复就删掉所有进度保护。[当前 progress][R-PROGRESS]、[来源说明][R-UPSTREAM]。

### 验证设计与文档事实

覆盖 Main 请求、Render 请求、Main/Render 同时有工作、满 mailbox、unknown→valid、普通请求超时、客户端中断、SSE 建立/取消、listener 单边失败、AppExit 以及 App/resource drop。对每个路径检查实际结果和进程/端口/watch/activity 收敛，而不只检查 HTTP 状态码。

在 `WinitSettings::desktop_app()` 的非活动窗口上，验证休眠后远程请求能得到响应，长输入/截图能继续完成，工作结束后 update 增长回落为有界行为。不要用“永远持续渲染”或“把宿主窗口置前台”掩盖 wake 丢失，也不要求观察期间绝对零 update——查询本身和系统窗口事件也会唤醒 App。

`UPSTREAM.md` 保留真实来源：当前代码最初派生于 0.19.1，不能只因依赖更新就把“来源 tag”机械改成 0.20.0。若只是兼容适配，补充本次核对的 0.20.0 版本及本地差异；只有实际重基上游实现时才记录新的重基来源，并继续保留历史来源与 license。[来源事实][R-UPSTREAM]。

### 完成条件

有逐项上游接缝核验和必要补丁；局部 progress/mailbox 测试可通过；未发现通过改变休眠模式、端口规则或错误语义绕过问题的实现。真实双端点和桌面行为由方案 06 完整复验。若新的上游语义确实无法保持现有 contract，应写出具体冲突并停在该受影响能力，不能以结构重构之名改变外部行为。

## MCP：反射类型知识、工具目录和协议调用

### MCP 不是完全与 Bevy 类型无关

MCP 的 transport 大部分通过 JSON-RPC 工作，但 `type_knowledge.rs` 保存了静态格式知识，`constants.rs` 包含类型路径、字段和示例值；这些内容可能在编译成功后仍然过时。`registry_schema.rs` 的结果保存为 JSON Value，并不自动证明所有新类型已被 type guide 正确理解。[静态类型知识][R-TYPE-KNOWLEDGE]、[常量][R-TYPE-CONSTANTS]、[schema 工具][R-REGISTRY-TOOL]。

核查范围包括 `guide.rs`、`type_kind.rs`、`type_knowledge.rs`、`constants.rs`、`mutation_path_builder/`、`tool_type_guide.rs` 和 `tool_all_types.rs`。只修改实际失效的知识或解析，不借这次升级重新设计递归框架、缓存、tool registry 或宏系统。

### 三层兼容性分开判断

**项目工具层：** tool 名称、参数、required/optional、默认值、annotation、结构化结果、错误 data 和普通/诊断可见性应保持。比较完整 schema，不只比较工具数量。当前文档记录的普通 47、诊断 49 可作为基线线索；本轮以实际构建的目录为准。

**上游协议层：** `rpc.discover` 可以因为 Bevy 增加方法而出现新条目，但不自动给每个上游方法生成一个本仓库静态 MCP tool。`world.*` 请求和 watch 的 request/response envelope、ID、错误传播都要实际验证。App agent catalog、App backing method 和 MCP 静态工具仍是三件不同的事；`register_agent_tool` 不变成注册 handler 的替代方式。[注册边界][R-BRP-RULES]。

**反射类型层：** 新的类型路径、结构、枚举或默认值属于目标引擎事实，不承诺旧的 fully-qualified 类型名继续可用。Rust re-export 能编译，不意味着 BRP 的字符串键仍旧有效。0.20 的 `Tonemapping`、`DebandDither` 路径分别改为 `bevy_render::view::Tonemapping`、`bevy_render::view::DebandDither`；执行时必须从宿主 live registry 取证并检查项目代码、帮助示例和回归输入中是否真的保存了旧路径，不预先给它们添加兼容别名。[官方迁移说明][B-GUIDE]。

### 用 live registry 校准静态知识

从固定 revision 的 0.20 宿主取得 `registry.schema`、所需组件/资源的实际 get 结果和关联反射 trait。记录测试端口、宿主 revision、MCP binary 标识；重启或使用既有刷新方式排除本轮旧缓存干扰，不为此新增缓存框架。

核对静态常量是否仍命中真实类型、列出的字段是否存在、enum/tuple/list/map/array 的形状是否能被现有解析器理解，以及生成的 mutation path 是否真能作用于对应字段。对于 `CalculatedClip` 这样的新结构，不从 Rust 声明直接猜 BRP JSON；Bevy 反射格式与普通 serde 格式并不总相同，必须以真实 schema、get 和引擎接受的输入共同校验。[静态知识的用途][R-TYPE-KNOWLEDGE]。

保留已有支持范围，不把“引擎新增了类型”理解成“本仓库要支持所有新类型的写入”。对于不支持/只读类型，返回现有明确的不可变或不支持说明；不能生成看似成功、实际无法提交的示例，也不能因为遇到新结构就 panic 或静默省略错误。

计算生成的 UI/渲染组件不应为了验证示例就盲目写入运行中的主场景。可写格式验证使用隔离 fixture 和安全组件/资源；计算态只验证 schema、读取以及正确的不可写解释。保留现有避免危险默认值的知识，例如时间参数的非零约束；不能为匹配新 schema 把这些保护直接删除。

若发现旧版本已经存在的过时规则，先区分它是否直接阻止本次 0.20 支持：阻止本次目标的最小配套修复可以纳入并注明原有缺陷；无关的全面清理不混入。

### 真实调用的验证矩阵

| 能力 | 验证方式 |
| --- | --- |
| `rpc.discover`、`registry.schema` | Main/需要时 Render 分别调用；核对过滤条件、结构和实际类型，不把引擎类型总数固化为恒定值。 |
| query/get/list、按名称查找 | 使用本轮创建或发现的实体 ID，核对实际目标、组件和值。 |
| component/resource insert/mutate/remove | 对安全 fixture 执行，再 get/查询实际状态；只收到成功 envelope 不算完成。 |
| spawn/despawn/reparent | 验证实体存活和父子关系；不要重用旧版本快照里的实体 ID。 |
| event/消息能力 | 对已注册、明确支持的类型验证实际 observer/message 副作用；未知或未注册类型必须正确报错。 |
| type guide 与 mutation 示例 | 选择 primitive、enum、Option、嵌套 struct、序列/map、Entity/Handle 等已有支持的代表类型；生成示例后实际提交或准确说明不可写原因。 |
| watch | 建立、收到实际更新、主动停止/断开并完成远端清理；验证无残留工作。 |
| 错误路径 | unknown method/type、非法 entity、malformed 参数/响应、连接失败和 timeout 不被压成同一种成功或无上下文错误。 |
| 普通/诊断 MCP | 重建对应 binary 后比较完整工具目录，trace 工具不能意外进入普通构建。 |
| App agent catalog | 注册元数据不增加静态 MCP 工具；无 backing method 或错误 handler 类别按原 contract 失败。 |

实体 ID 保持原有 u64 精度，JSON 处理不能通过浮点转换丢失高位；缺失字段和显式 null 不被一概当成同义。错误继续携带现有 method、port、stage、field、code/data 等必要上下文。[协议测试规则][R-TEST-RULES]。

`brp_execute` 可继续调用宿主实际支持的方法，但不能因此宣称新上游事件的反射注册、可序列化条件和所有写入能力都已具备。旧 0.19 App 与新 MCP 混用不在本轮保证范围；项目自有 wire 外形未变，不等于跨引擎类型指导兼容已经证明。

### MCP 兼容的完成条件

项目自有 schema 差异为空，或每项不可避免的差异都已经明确提出而不是静默接受；反射路径和格式的实际失效点有最小修正；代表性读写、类型指导和 watch 有真实 App 证据；不支持的能力能诚实报错。MCP-only 构建仍不依赖 Extras/runtime 或宿主的渲染 feature。

## 贯穿整个协议链的宿主行为

### 需要保持的行为和失败边界

`pointer_test` 保留默认 UI Picking backend、stock Button、普通拖动目标、scroll overflow、多窗口和休眠设置；`mouse_test` 的 raw/native 计数仍仅用于物理输入展示，不能被拿来判定新的普通 BRP 鼠标成功。`keyboard_windows` 保留窗口选择、修饰键清理和正常关闭窗口的路径。`event_test` 继续验证真实 observer 行为，而不只接受请求。[现有宿主职责][R-TESTING]。

截图 fixture 中的相机、viewport、非主窗口、UI/AABB 边界和确定性颜色继续存在。若新渲染路径导致像素变化，确认是引擎预期差异还是 crop/坐标错误后，再调整最小必要 expected；记录调整理由，不能整批重录后当成通过。

## 本阶段保持全仓可编译的门槛

前置的 Bevy 0.20 workspace 编译状态必须保持。阶段结束时至少重新执行 `cargo check --workspace --all-targets --locked` 与 `cargo test --workspace --locked --no-run`，并根据改动范围复核生产 crate 独立 feature、examples 和 `cargo build --workspace --examples --locked`。新增或由本阶段暴露的编译错误必须**在本阶段修正**，不能标记为“留给下一方案”后宣布完成。

如果构建成功但行为回归失败，编译门槛虽通过，本阶段的**行为验收仍不通过**，仍应修复相应 contract 或明确记录环境限制。特别是缺少真实 Windows 桌面/GPU 时，绝不能把未运行的桌面测试算成通过；最终完整证据集中在方案 06。

## 资料链接

[B-GUIDE]: https://bevy.org/learn/migration-guides/0-19-to-0-20/
[B-HTTP]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_remote/src/http.rs
[B-REMOTE]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_remote/src/lib.rs
[B-WINIT]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_winit/src/lib.rs
[R-AGENTS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/AGENTS.md
[R-ARCH]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/docs/architecture.md
[R-BRP-RULES]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/brp-mcp.md
[R-CODE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/code.md
[R-DEPS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/dependencies.md
[R-PROGRESS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/runtime/src/progress.rs
[R-REGISTRY-TOOL]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/mcp/src/brp_tools/tools/registry_schema.rs
[R-RUNTIME-HTTP]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/runtime/src/http.rs
[R-RUNTIME-LIB]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/runtime/src/lib.rs
[R-SCOPE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/task-scope.md
[R-TEST-RULES]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/testing.md
[R-TESTING]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/docs/testing.md
[R-TYPE-CONSTANTS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/mcp/src/brp_tools/brp_type_guide/constants.rs
[R-TYPE-KNOWLEDGE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/mcp/src/brp_tools/brp_type_guide/type_knowledge.rs
[R-UPSTREAM]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/runtime/UPSTREAM.md
