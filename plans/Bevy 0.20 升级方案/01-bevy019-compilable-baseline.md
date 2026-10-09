# 01. 建立 Bevy 0.19.1 可编译基线

> 迁移方案：`slc90/bevy_brp`，源码基线 `ae9fdaec25cb1f02b501f6e07199a27df144501e`；目标 Bevy `v0.20.0`。状态：**尚未实施、尚未验证**。本文是执行方案，不代表仓库已经达到文中的门槛。

## 目标

在依赖尚未升级时，固定可重复的旧版本状态，证明当前源码与既有工具链、lockfile 的关系，并为后续判断回归提供对照。

## 范围

当前 `main` 基线、旧 Cargo.lock、Rust/Cargo/PowerShell/Python/目标平台、旧版仓库构建结果与代表性 BRP/MCP、输入、截图、runtime 证据。**不修改** Bevy 依赖、不开始 Pointer、截图或其他 0.20 API 迁移。

## 预期产出

包含准确 revision、已有 working tree 变更、旧依赖树/工具目录/运行记录、旧版编译和可运行测试结果、已存在失败/缺少环境与必要的回退参照。没有必要增加生产代码 diff。

## 与前后方案的关系

这是唯一执行链的起点。只有此阶段的旧版基线记录可用，方案 02 才将全 workspace 同时切换为 0.20；旧版已经存在的失败必须被标注，不自动归咎于 0.20。之后不再重新生成彼此冲突的依赖基线。

## 迁移边界与共同约束

**目标是让现有能力在 Bevy 0.20.0 上保持可用，不借升级重做架构。** 保留四个生产 crate 的角色、package/target 名称和两条内部依赖：`mcp → mcp_macros`、`runtime → extras`；MCP 与 App 仍通过 BRP 通信。保留 `BrpExtrasPlugin`、`BrpRuntimePlugin`、`BrpRuntimePlugin::with_port`、activity 和 agent-tool 的公共使用方式。类型签名中的 Bevy 类型自然升级为 0.20 类型，不承诺它们与 0.19 类型二进制或 Rust 类型兼容。[架构][R-ARCH]、[边界规则][R-SCOPE]。

不新增“双版本兼容层”、新 crate、替代输入框架或新的 MCP 工具。保持项目自有 BRP/MCP 的方法名、字段、默认值、错误结构和注册边界；上游 `registry.schema` 中的类型集合、类型路径及上游新增方法可以随引擎变化，但不能被误当成项目自有协议变化。所有需要改变既有外部语义的情况，都必须先记录具体不兼容及原因，不得静默修改。

Windows 实施和图形验收使用 PowerShell 7，不使用 Windows PowerShell 5。保留现有 lint、最小 feature、错误传播和资源 ownership 约束；不用扩大 `allow`、删测试、清除历史样本或启用全量 Bevy feature 来掩盖错误。生产代码不得为实现裁剪而引入 `unsafe`，也不为小段几何运算增加依赖。[代码规则][R-CODE]、[依赖规则][R-DEPS]。

每个实际产生代码或工程行为变更的阶段，都遵守仓库要求的独立审查：完成必要验证后，由全新 reviewer 调用 `code-review`，对完整 working tree 做静态审查；有 findings 就修正并由另一位全新 reviewer 重审。缺少 reviewer/subagent 能力时明确记为阻塞，不用施工者自查替代。本文的文档覆盖检查不等于代码审查。[Agent 入口][R-AGENTS]。

各阶段使用同一迁移分支、同一 lockfile 和有记录的提交/工作区状态。保护已有用户修改；不要 reset、覆盖或夹带。测试结果区分“新回归”“已存在的失败”“环境缺失而未运行”，不能把 0.19 的旧成功记录写成 0.20 的新结果。阶段 01 须先确认旧 Bevy 0.19.1 基线可编译；阶段 02 切换至 0.20.0 后必须消除整个 workspace 的所有编译错误，阶段 03—06 每阶段结束亦须重新满足同一编译门槛，不得把本阶段的编译失败留待下一份方案。只有最终验收允许宣告整个升级可用。[测试规则][R-TEST-RULES]。

外部消费者，包括 `bevy_widgetry`，不在本次修改范围。此方案不授权创建或移动 Git tag、发布 package、覆盖本机已安装 MCP 或修改外部仓库。现有 `v0.3.1` 保持不动；最终先以经过验证的完整 revision 交接。后续发布时，runtime、Extras 和 MCP 必须来自同一 revision/tag，不混用。[Git 消费规则][R-DEPS]。

## 迁移事实与基线约束

目标引擎固定为正式 `v0.20.0`，不跟随 Bevy `main`；当前生产 package 为 `0.3.1`，Bevy 依赖为 `0.19.1`。Pointer 扁平事件与 UI 裁剪形状的不兼容已由原方案的官方源码检查确认，其余必须等待实际编译和宿主运行来判定。[仓库基线][R-BASE]、[依赖声明][R-CARGO]。

旧代码不需要为了建立基线先改 API；不改四个生产 crate 的角色，不改任何 MCP/BRP contract，不编辑其他仓库和 `v0.3.1` tag。记录与旧版之间真正存在的差异，避免把历史旧结果当作这次迁移已经获得的新结果。

### 基线记录应包含什么

记录 `git rev-parse HEAD`、工作区已有修改、`rustc -Vv`、`cargo -V`、目标 triple、PowerShell/Python 版本、当前 lockfile 标识以及命令退出码。原分支已经与本文基线不同时，先确认差异是否改变本文命中的调用点；不要按旧行号盲改。

在旧依赖状态记录普通构建和诊断构建的 MCP tool schema、`rpc.discover`、代表性的 `registry.schema`，以及可运行的输入、截图、runtime 回归结果。图形环境不具备时直接列出未测条件。已实施测试文档中的 47/49 工具、旧截图或旧 idle update 次数只作为核对线索，不作为本轮测量结果；不要改写历史 before 数据。[现有验证入口和历史边界][R-TESTING]。

## 本阶段编译准出门槛（Bevy 0.19.1）

在**不修改现有依赖版本**的状态下确认可编译基线，不把源代码上任何历史失败伪造成新的 0.20 问题。旧基线应尽量执行：

```powershell
cargo check --workspace --all-targets --locked
cargo test --workspace --locked --no-run
cargo build --workspace --examples --locked
```

独立 feature 的既有验证同样记录。凡是未能运行、因系统依赖/平台缺失中止或已在旧版失败的项，都记录命令、错误、平台及阻塞原因；在没有可信旧版编译结果前不能宣称“旧基线编译已通过”，也不应把下一阶段的差异归因建立在猜测上。

**这里不会修改 `Cargo.toml` 或 `Cargo.lock` 里的 Bevy 版本。** 只有方案 02 才首次把 workspace 切到 Bevy 0.20.0。没有可用的旧版对照时，应将基线缺口显式带入方案 02，不声称这是一条已验证的零回归迁移。

## 资料链接

[R-AGENTS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/AGENTS.md
[R-ARCH]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/docs/architecture.md
[R-BASE]: https://github.com/slc90/bevy_brp/commit/ae9fdaec25cb1f02b501f6e07199a27df144501e
[R-CARGO]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/Cargo.toml
[R-CODE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/code.md
[R-DEPS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/dependencies.md
[R-SCOPE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/task-scope.md
[R-TEST-RULES]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/testing.md
[R-TESTING]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/docs/testing.md
