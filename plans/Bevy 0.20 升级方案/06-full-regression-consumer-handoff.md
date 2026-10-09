# 06. 完成全链回归、独立消费与版本交接

> 迁移方案：`slc90/bevy_brp`，源码基线 `ae9fdaec25cb1f02b501f6e07199a27df144501e`；目标 Bevy `v0.20.0`。状态：**尚未实施、尚未验证**。本文是执行方案，不代表仓库已经达到文中的门槛。

## 目标

将前五阶段的编译和局部行为证据收束到同一个不变的 Bevy 0.20 revision，形成可审计、可独立消费的升级交付，而不是只留下工作区里的编译结果。

## 范围

完整 workspace 与独立 feature、实际 Windows GPU/桌面协议回归、Pointer/keyboard/screenshot/runtime/MCP、测试记录、独立 Git/path 消费者和临时 MCP 安装，以及 README、rustdoc、UPSTREAM 和实际变更涉及的文档。

## 预期产出

最终 revision/lockfile 与 binary 标识、完整编译/测试/真实图像和端口清理证据、独立审查结论、未测及已知限制、旧版回退路径和明确的 0.20 使用说明；不创建 tag、不修改 bevy_widgetry，也不覆盖用户安装。

## 与前后方案的关系

依赖方案 01—05 的全部准出状态，这是唯一执行链终点。后续 Git tag 发布或其他仓库升级需要新的独立授权，本次仅交接经过验证的不可变 commit/revision。

## 迁移边界与共同约束

**目标是让现有能力在 Bevy 0.20.0 上保持可用，不借升级重做架构。** 保留四个生产 crate 的角色、package/target 名称和两条内部依赖：`mcp → mcp_macros`、`runtime → extras`；MCP 与 App 仍通过 BRP 通信。保留 `BrpExtrasPlugin`、`BrpRuntimePlugin`、`BrpRuntimePlugin::with_port`、activity 和 agent-tool 的公共使用方式。类型签名中的 Bevy 类型自然升级为 0.20 类型，不承诺它们与 0.19 类型二进制或 Rust 类型兼容。[架构][R-ARCH]、[边界规则][R-SCOPE]。

不新增“双版本兼容层”、新 crate、替代输入框架或新的 MCP 工具。保持项目自有 BRP/MCP 的方法名、字段、默认值、错误结构和注册边界；上游 `registry.schema` 中的类型集合、类型路径及上游新增方法可以随引擎变化，但不能被误当成项目自有协议变化。所有需要改变既有外部语义的情况，都必须先记录具体不兼容及原因，不得静默修改。

Windows 实施和图形验收使用 PowerShell 7，不使用 Windows PowerShell 5。保留现有 lint、最小 feature、错误传播和资源 ownership 约束；不用扩大 `allow`、删测试、清除历史样本或启用全量 Bevy feature 来掩盖错误。生产代码不得为实现裁剪而引入 `unsafe`，也不为小段几何运算增加依赖。[代码规则][R-CODE]、[依赖规则][R-DEPS]。

每个实际产生代码或工程行为变更的阶段，都遵守仓库要求的独立审查：完成必要验证后，由全新 reviewer 调用 `code-review`，对完整 working tree 做静态审查；有 findings 就修正并由另一位全新 reviewer 重审。缺少 reviewer/subagent 能力时明确记为阻塞，不用施工者自查替代。本文的文档覆盖检查不等于代码审查。[Agent 入口][R-AGENTS]。

各阶段使用同一迁移分支、同一 lockfile 和有记录的提交/工作区状态。保护已有用户修改；不要 reset、覆盖或夹带。测试结果区分“新回归”“已存在的失败”“环境缺失而未运行”，不能把 0.19 的旧成功记录写成 0.20 的新结果。阶段 01 须先确认旧 Bevy 0.19.1 基线可编译；阶段 02 切换至 0.20.0 后必须消除整个 workspace 的所有编译错误，阶段 03—06 每阶段结束亦须重新满足同一编译门槛，不得把本阶段的编译失败留待下一份方案。只有最终验收允许宣告整个升级可用。[测试规则][R-TEST-RULES]。

外部消费者，包括 `bevy_widgetry`，不在本次修改范围。此方案不授权创建或移动 Git tag、发布 package、覆盖本机已安装 MCP 或修改外部仓库。现有 `v0.3.1` 保持不动；最终先以经过验证的完整 revision 交接。后续发布时，runtime、Extras 和 MCP 必须来自同一 revision/tag，不混用。[Git 消费规则][R-DEPS]。

## 在最终 revision 上验证编译、feature 和构建

### 构建与 feature 验收

在最终代码状态记录以下检查的结果，不能沿用方案 01 或旧 0.19 的成功日志：

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked --no-fail-fast
cargo build --workspace --locked
cargo build --workspace --examples --locked
```

独立 feature 与 MCP 两种模式分别验证，避免 test-app 的 feature 合并掩盖生产 crate 缺失依赖：

```powershell
cargo check -p bevy_brp_extras --locked --no-default-features
cargo check -p bevy_brp_extras --locked --no-default-features --features diagnostics
cargo check -p bevy_brp_extras --locked --no-default-features --features ui
cargo check -p bevy_brp_mcp --locked --no-default-features
cargo check -p bevy_brp_mcp --locked --features mcp-debug
cargo check -p bevy_brp_runtime --locked
```

保留仓库现有 lint，不添加无依据的全局 `-D warnings` 后再为旧 warning 大幅改代码，也不削弱 lint 来通过。既有同名 fixture warning 和 ignored doctest 单独列出；它们不自动成为新增失败，也不能用于掩盖新增忽略项。[现有验证约定][R-AGENTS]、[测试事实][R-TESTING]。

## Windows 真桌面、图形与协议链路

### 真实 Windows 验证入口

运行条件是可见桌面、可用 GPU、PowerShell 7、Python 3、空闲端口以及本轮构建的宿主/MCP。每条图形/协议链都固定本轮 PID、端口和输出目录；成功后 clean shutdown 并确认进程、端口、watch、日志/trace 恢复，失败保留足够证据并仅清理本轮已核实的进程。没有这些环境就明确未测，不能用 headless 测试替代桌面隔离结论。[脚本和环境要求][R-TESTING]。

**Pointer 链**使用现有目标与脚本：

```powershell
cargo build --workspace --example pointer_test --locked
cargo test --workspace --example pointer_test --locked
python tests/pointer-regression.py --port 15828 --output target/bevy020-pointer-evidence
```

输出目录必须尚不存在。系统鼠标保持静止；不把测试窗口置前台，不注入 OS 输入。同时验证实际 UI 按钮、双击、拖放、Line/Pixel 滚动、取消、UUID、窗口销毁和 idle activity，以及 OS cursor 前后相同、foreground 不变、两个宿主窗口未获焦点、mouse/cursor 各 raw 通道零增长。keyboard 和 gesture 不混进鼠标零注入断言。两窗口截图须实际查看；native 窗口标题栏移动/缩放不在此能力范围。

**Keyboard 链**保留 secondary window 路由、默认 PrimaryWindow、无效/已销毁 window、长 typing/hold 期间关闭窗口和 Ctrl/Shift 清理：

```powershell
cargo build --workspace --example keyboard_windows --locked
python tests/keyboard-window-regression.py --port 15816 --output target/bevy020-keyboard-evidence
```

该结果证明输入事件路由，不自动证明 Widgetry 具体控件的 focus 或编辑逻辑。

**普通 MCP 与诊断目录**按现有脚本入口，普通构建和 `mcp-debug` 构建切换时必须重新构建并记录实际 binary 标识：

```powershell
cargo build -p bevy_brp_mcp -p bevy_brp_test_apps --locked
& .\tests\public-mcp-regression.ps1
& .\tests\diagnostics-regression.ps1
```

诊断构建对应入口：

```powershell
cargo build -p bevy_brp_mcp -p bevy_brp_test_apps --locked --features bevy_brp_mcp/mcp-debug
& .\tests\diagnostics-regression.ps1 -DebugBuild
& .\tests\regression.ps1 -Port 15712 -KeepArtifacts
```

保留原回放中的握手、工具目录、Sprite Transform 修改、watch、输入接受、日志 marker、两次 shutdown 和 `NatesList` 截图/像素检查。历史回放只检查部分输入排队，不能替代 Pointer/keyboard 专门链；也不能替代方案 05 新补的真实反射 CRUD/type-guide 用例。脚本引用的历史 before 样本不被本轮覆盖。[回放覆盖边界][R-TESTING]。

**Runtime 端口**使用：

```powershell
cargo build -p bevy_brp_runtime --example runtime_custom_port --locked
& .\tests\runtime-port-regression.ps1
```

现有脚本要求 15752、15702、15703 空闲；Render 默认端口也要检查。该脚本不能替代方案 05 的全部休眠、背压、双端点失败与取消用例，缺少的组合在现有测试体系中补充，而不是在总结中声称一个端口脚本已经覆盖所有 runtime 行为。

脚本的 binary 查找和参数使用其实际支持的形式。不要假设它们有未实现的 `--mcp-binary` 等参数；安装版验证需要改用现有可配置入口或受控的临时启动配置，并记录 executable 的准确路径和 SHA256。

## 隔离消费与版本锁定

### 独立消费验证

建立不属于本 workspace 的临时消费工程，使用本轮待交接的同一个完整 revision（或交接前同一源码快照的 path）引用 runtime 和直接使用的 Extras，同时使用 Bevy 0.20.0。验证公共 Plugin/activity/agent-tool 导入、`DefaultPlugins` 后安装 runtime、自定义端口和实际 BRP 调用。另验证 Extras-only 的既有消费方式及其最小 feature；不靠本仓库测试宿主补齐 feature。

MCP 安装到临时 root，不覆盖用户正在使用的 executable。验证安装出的 binary 能启动、列工具并连接同 revision 的 App；不能只运行工作区里另一个旧 binary。使用 path、debug、offline、远程 Git 或 release 安装中的哪一种，就准确记录哪一种；没测 release/远程 Git 下载时不得写成已经验收。

Rust `bevy` 0.19 与 0.20 的 Plugin/ECS 类型不能混用。本轮不承诺 0.20 runtime 可直接塞进尚未升级的 0.19 Widgetry；交接文档只指出外部消费者随后需要统一引擎版本和 BRP revision，不自动修改它。

### 事实文档与版本交接

更新真正承载本次事实的现有文档：README 的兼容引擎说明和消费提示、Extras/runtime rustdoc、`UPSTREAM.md`、MCP help text 中失效的类型路径/示例、`docs/testing.md` 的新验收结果。架构边界未变时，不机械重写 `docs/architecture.md`；只有实际事实变化才同步。历史计划和旧证据不改写为“当前已经通过”。

README 要明确区分旧发布与新候选：`v0.3.1` 仍对应 Bevy 0.19.1，本轮已验证的完整 revision 才对应 0.20.0。0.20 消费示例使用该真实 revision，不能保留指向旧 tag 的依赖片段却把它标成支持 0.20；也不能编造一个尚未创建的 tag。

本轮不创建新 tag，也不把原 `v0.3.1` 改指升级结果。直接交接一个验证过的完整 commit 及 Bevy 0.20 支持范围；若提交发生在采证之后，明确证据覆盖的是哪份代码 diff，不把采证时 HEAD 冒充最终提交。工程代码或 lockfile 再改变时，必须按影响重跑验证。

后续经授权发布时，考虑到 Bevy 依赖造成 Rust API 类型不兼容，可采用新的 minor 版本而不是声称旧版可无感替换；具体版本号不是本文已执行决定。若发布阶段修改 package/internal dependency/lockfile，重新验证一致性。runtime、Extras 和 MCP 使用同一不可变 tag/revision，不能只更新 App 侧库而继续默认使用旧 MCP。

### 验收记录与回退

记录格式至少包含：源码 revision/工作区状态、lockfile 标识、工具链和平台、构建 feature、MCP/App binary 标识、命令及退出码、输出目录、实际图像查看结论、端口/进程清理结果，以及新失败/已知失败/未测条件。证据存放在本地忽略目录；只把有意义的结论写进现有事实文档，不向仓库塞入大批临时截图和日志。

最终验收必须同时满足：没有新增无法解释的编译/测试失败；独立 feature 和独立消费可用；自有 wire contract 未意外改变；Pointer/截图/runtime/type guide 的关键行为有对应证据；真实桌面和图像项不是用模拟结果冒名替代；必要的独立 Code Review 已完成。若仍缺硬件多 DPI、其他平台或 Widgetry 具体控件验证，应明确列为未测，不外推支持结论。

回退以完整旧 revision 与其 Cargo.lock 为单位。保留旧的 `v0.3.1` 和原安装可执行文件；新结果在独立分支、临时安装目录或新的不可变 revision 上验证，不破坏旧可用组合。不要只把 manifest 改回 0.19 却留下 0.20 源码/lockfile。发现已上线组合异常时，App 库和 MCP 一起恢复到匹配的旧组合，而不是混搭版本。

## 最终提交后的编译与验收门槛

前五阶段的完成不自动意味着当前最终 revision 已经通过全部验证；每次影响源码、Cargo.lock、公共 feature 或必要测试的最后更改后，均应在最终状态重跑上面的编译、构建、测试、独立消费和适用的桌面检查。无法在目标环境执行时只报告阻塞、未测与交接风险，不能把“不报错”当成“已验证”。

## 本阶段保持全仓可编译的门槛

前置的 Bevy 0.20 workspace 编译状态必须保持。阶段结束时至少重新执行 `cargo check --workspace --all-targets --locked` 与 `cargo test --workspace --locked --no-run`，并根据改动范围复核生产 crate 独立 feature、examples 和 `cargo build --workspace --examples --locked`。新增或由本阶段暴露的编译错误必须**在本阶段修正**，不能标记为“留给下一方案”后宣布完成。

如果构建成功但行为回归失败，编译门槛虽通过，本阶段的**行为验收仍不通过**，仍应修复相应 contract 或明确记录环境限制。特别是缺少真实 Windows 桌面/GPU 时，绝不能把未运行的桌面测试算成通过；最终完整证据集中在方案 06。

## 资料链接

[R-AGENTS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/AGENTS.md
[R-ARCH]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/docs/architecture.md
[R-CODE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/code.md
[R-DEPS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/dependencies.md
[R-SCOPE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/task-scope.md
[R-TEST-RULES]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/testing.md
[R-TESTING]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/docs/testing.md
