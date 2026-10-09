# 03. 验证 Custom Pointer 输入生命周期与桌面隔离

> 迁移方案：`slc90/bevy_brp`，源码基线 `ae9fdaec25cb1f02b501f6e07199a27df144501e`；目标 Bevy `v0.20.0`。状态：**尚未实施、尚未验证**。本文是执行方案，不代表仓库已经达到文中的门槛。

## 目标

在已编译通过的 0.20 源码上，证明远程鼠标按键、点击、取消、拖动、滚动和生命周期仍遵守原有 contract；对运行中暴露的回归做必要局部修复。

## 范围

主要涉及 `crates/extras/src/mouse/pointer.rs`、`mouse/pointer/tests.rs`、`tests/test-app/examples/pointer_test.rs`、`mouse_test.rs` 和 `crates/extras/tests/pointer_control.rs`。`button.rs`、`click.rs`、`drag.rs`、`scroll.rs`、`queue.rs`、`cursor.rs`、`support.rs` 只处理直接命中的状态与调用变化，不重新组织模块。

## 预期产出

有可重复测试证据的事件生命周期、责任 Cancel、UUID/generation、按键和自动手势 FIFO、物理接管、失效恢复和 activity 收敛；未具备桌面条件时明确记录未测范围。整个 0.20 workspace 仍保持可编译。

## 与前后方案的关系

前置是方案 02 的完整编译门槛；下一方案检验 UI 截图，二者无必然算法依赖，固定串行是为了避免并发修改 Extras/test-app 及造成回归归属混淆。真实系统鼠标/前台隔离的最终证据由方案 06 复核。

## 迁移边界与共同约束

**目标是让现有能力在 Bevy 0.20.0 上保持可用，不借升级重做架构。** 保留四个生产 crate 的角色、package/target 名称和两条内部依赖：`mcp → mcp_macros`、`runtime → extras`；MCP 与 App 仍通过 BRP 通信。保留 `BrpExtrasPlugin`、`BrpRuntimePlugin`、`BrpRuntimePlugin::with_port`、activity 和 agent-tool 的公共使用方式。类型签名中的 Bevy 类型自然升级为 0.20 类型，不承诺它们与 0.19 类型二进制或 Rust 类型兼容。[架构][R-ARCH]、[边界规则][R-SCOPE]。

不新增“双版本兼容层”、新 crate、替代输入框架或新的 MCP 工具。保持项目自有 BRP/MCP 的方法名、字段、默认值、错误结构和注册边界；上游 `registry.schema` 中的类型集合、类型路径及上游新增方法可以随引擎变化，但不能被误当成项目自有协议变化。所有需要改变既有外部语义的情况，都必须先记录具体不兼容及原因，不得静默修改。

Windows 实施和图形验收使用 PowerShell 7，不使用 Windows PowerShell 5。保留现有 lint、最小 feature、错误传播和资源 ownership 约束；不用扩大 `allow`、删测试、清除历史样本或启用全量 Bevy feature 来掩盖错误。生产代码不得为实现裁剪而引入 `unsafe`，也不为小段几何运算增加依赖。[代码规则][R-CODE]、[依赖规则][R-DEPS]。

每个实际产生代码或工程行为变更的阶段，都遵守仓库要求的独立审查：完成必要验证后，由全新 reviewer 调用 `code-review`，对完整 working tree 做静态审查；有 findings 就修正并由另一位全新 reviewer 重审。缺少 reviewer/subagent 能力时明确记为阻塞，不用施工者自查替代。本文的文档覆盖检查不等于代码审查。[Agent 入口][R-AGENTS]。

各阶段使用同一迁移分支、同一 lockfile 和有记录的提交/工作区状态。保护已有用户修改；不要 reset、覆盖或夹带。测试结果区分“新回归”“已存在的失败”“环境缺失而未运行”，不能把 0.19 的旧成功记录写成 0.20 的新结果。阶段 01 须先确认旧 Bevy 0.19.1 基线可编译；阶段 02 切换至 0.20.0 后必须消除整个 workspace 的所有编译错误，阶段 03—06 每阶段结束亦须重新满足同一编译门槛，不得把本阶段的编译失败留待下一份方案。只有最终验收允许宣告整个升级可用。[测试规则][R-TEST-RULES]。

外部消费者，包括 `bevy_widgetry`，不在本次修改范围。此方案不授权创建或移动 Git tag、发布 package、覆盖本机已安装 MCP 或修改外部仓库。现有 `v0.3.1` 保持不动；最终先以经过验证的完整 revision 交接。后续发布时，runtime、Extras 和 MCP 必须来自同一 revision/tag，不混用。[Git 消费规则][R-DEPS]。

## Pointer 的 0.20 类型适配与行为合同

以下事件结构与组件区分在方案 02 已按编译要求进入代码，本阶段再次保留全部约束，以免只凭编译成功推断事件生命周期正确。

### 三种类型必须分清

| 用途 | 0.19 | 0.20 |
| --- | --- | --- |
| 按下事件 | `events::Pointer<events::Press>` | `events::PointerPress` |
| 松开/点击/取消事件 | `Pointer<Release/Click/Cancel>` | `PointerRelease/PointerClick/PointerCancel` |
| 拖动事件 | `Pointer<DragStart/Drag/DragEnd/DragDrop>` | `PointerDragStart/PointerDrag/PointerDragEnd/PointerDragDrop` |
| 滚动/移出事件 | `Pointer<Scroll/Out>` | `PointerScroll/PointerOut` |
| 其余实际命中的高层事件 | `Pointer<Over/Move/DragEnter/DragOver/DragLeave>` 等 | 对应 `PointerOver/PointerMove/PointerDragEnter/PointerDragOver/PointerDragLeave` 等独立类型 |
| 保存按键状态的组件 | `pointer::PointerPress` | `pointer::PointerPressState` |
| 底层输入消息 | `pointer::PointerInput` | 仍是 `PointerInput` |

高层事件的公共信息变成 `event.pointer.id`、`event.pointer.position`、`event.pointer.target`，需要完整 `Location` 时用 `event.pointer.location()`。事件特有的 `button`、`hit`、`distance` 等字段位于具体事件本身。底层 `PointerInput` 的 `pointer_id`、`location`、`action` 没有按同样方式改名，因此不得全局替换所有 `.pointer_id`。[0.20 高层事件][B-EVENTS]、[0.20 底层输入及按键状态][B-POINTER]。

所有接收面一起迁移：`MessageCursor`、`MessageReader`、`Messages<T>` 资源存在性检查、宿主的 `On<T>` observer、收集事件的测试容器以及手动事件构造。只改 observer 会漏掉 `capability()` 和 `finish_cycle()` 中的实际能力判定与状态收尾。

组件查询、组件完整性检查和清零操作使用 `PointerPressState`，事件游标使用 `PointerPress`。显式区分这两个导入，不让同名缩写掩盖其用途。确有泛型事件代码时使用 `PointerEvent` 表达共同能力，但不为了这次改名额外抽出泛型框架。

### 手工 Cancel 的迁移与去重

`finish_cycle()` 对曾经 press/drag、后来已不在 hover 目标中的实体补发 Cancel，这项责任不能因为上游也处理 `PointerAction::Cancel` 就直接删除。0.20 的上游取消派发仍需要和本地责任记录一起验证，不能只检查当前 hover。[当前收尾代码][R-POINTER]、[引擎事件派发][B-EVENTS]。

构造方式改为下面的形状；使用 `Pointer::new`，不要直接构造含私有传播字段的 `Pointer`：

```rust
use bevy::picking::events::{Pointer, PointerCancel};

let cancel = PointerCancel {
    entity: responsibility.target,
    pointer: Pointer::new(id, responsibility.location),
    hit: responsibility.hit,
};
world.write_message(cancel.clone());
world.trigger(cancel);
```

继续同时写 message 和触发 observer，因为它们是不同的消费面。高层事件的 message 保留原事件实体；observer 在传播中会访问不同实体，不能把传播途中的实体误当成所有责任记录的唯一来源。按旧 generation 和目标实体做补偿去重：上游已通知的目标不再补发，已销毁的实体不触发事件，旧 generation 不得清空新 generation 的责任。

保留清空按键状态、移除 Custom Pointer 命中位置、flush observer commands，以及等待下一 Picking 周期确认退场的语义。完成取消不等于立即改成 inactive；必须保证旧的 Pressed、drag 和 hover 状态有机会收敛，且取消过程中不会生成成功 Click/Activate。

### 保持状态与调度约束

稳定 UUID 在多轮远程控制间复用；`inactive → active → draining → inactive` 的公共阶段含义不变。`pointer_control.release` 仍绕过 FIFO 使当前 generation 失效，status 查询不制造 activity；active 且没有待执行工作时可以继续 hover，但不能维持无限续帧。实际工作结束后 activity guard 释放，异步失败继续保留来源 method/window。[当前输入架构][R-ARCH]。

统一生产者仍放在 `First`，保持与物理 `mouse_pick_events` 的先后关系。物理指针命中位置的暂时抑制必须发生在 `PointerInput::receive` 之后、RayMap 重建及 Picking backend 之前；为 Out 恢复旧位置的逻辑在 backend 之后、Hover 派发之前；责任收尾在 Hover 之后，并及时应用 observer 的延迟命令。实施时对照 0.20 的 `PickingSystems` 和实际 schedule graph 核实这些相对关系，而不是因系统集名字相似就认定调度无变化。[上游 Picking 装配][B-PICKING-LIB]。

物理移动、按下或滚动引发交接时，终止旧远程 generation，并正确恢复物理指针；不得清空物理 raw message 队列。窗口关闭、Custom Pointer entity 或必需组件被移除时，也要走同一责任收尾路径，不留下按下状态或悬空工作。

自动点击/双击/拖动继续共享 FIFO。`Instant`/真实经过时间用于 hold 和动作等待，不改成会被应用暂停或缩放的虚拟时间；Picking 周期用于保证首次 move、press、跨目标 drag 和 release 的可观察先后。timed hold 期间允许的同窗口 move/scroll 保持，自动手势与后续输入的阻挡规则保持。

滚轮已有 `TouchPhase::Moved`，这不是本次新增字段。保持 `Line`/`Pixel` 单位和 x/y 值原样进入 PointerAction，不给公共请求新增 phase。不要把远程鼠标重新降回 `MouseMotion`、`MouseButtonInput`、`MouseWheel`、`CursorMoved` 或 OS 输入注入。[当前 scroll 实现][R-SCROLL]。

Extras 仍不安装 Picking 核心、InteractionPlugin 或具体 backend。宿主无 Picking 时可以安装 Extras 并使用其他功能；鼠标请求按既有错误 contract 报告缺失能力。不要为方便测试把宿主责任变成生产插件的新默认行为。

### 验证设计

| 场景 | 必须观察到的结果 |
| --- | --- |
| 首次 move + click、后续不 move 的 click、三按钮、零时长按键 | 命中目标和 press/release 成对，不能因同帧输入丢失第一次点击。 |
| 引擎多击阈值内外、最小步数跨目标拖动 | click count 与实际拖放结果正确，不靠固定 sleep 猜测结果。 |
| press 后离开目标再 release/cancel | 原目标解除 Pressed；取消不成功 Activate；原责任目标收到且只收到一次补偿 Cancel。 |
| 原目标已销毁、窗口销毁、指针 entity/组件失效 | 不 panic，不误操作其他 entity；收敛为 inactive，保留应有异步错误。 |
| 物理输入接管、不同窗口与不同 DPI | 逻辑坐标和交接正确，旧 generation 不继续输出。 |
| 虚拟时间暂停/缩放、timed hold、排队自动手势 | 真实时间语义不退化，FIFO 和 activity 收尾保持。 |
| status 查询、空闲 active、多轮复用 | UUID 稳定，查询不额外增 busy，续帧有界。 |

尽量用真实 Picking 系统与测试 backend 验证，而不直接写 fixture 的业务计数来伪造行为。已有 `pointer_test` 继续使用 stock Button 和真实 observer。迁移后的测试还要覆盖输入层 `pointer_id` 未误改、事件层新字段能被正确读取。

局部测试可用 `cargo test -p bevy_brp_extras --locked --no-default-features mouse::pointer` 缩小 UI 编译影响；完整 example test 与桌面回归在依赖它们的宿主适配完成后由方案 06 重跑。若本阶段出现编译错误，必须在本阶段修复并通过全仓编译门槛；环境缺失导致无法执行的桌面测试则明确记为未测，不删测试或扩大 ignore。

### 完成条件

现有鼠标公共 schema 和错误数据没有漂移；所有实际命中的事件消费面及按键状态组件已迁移；局部可运行测试证明责任、generation、调度和 activity 不变。纯 Bevy 单元测试不构成 OS cursor/foreground 隔离证据，后者必须留到真实 Windows 桌面验收。

## 本阶段保持全仓可编译的门槛

前置的 Bevy 0.20 workspace 编译状态必须保持。阶段结束时至少重新执行 `cargo check --workspace --all-targets --locked` 与 `cargo test --workspace --locked --no-run`，并根据改动范围复核生产 crate 独立 feature、examples 和 `cargo build --workspace --examples --locked`。新增或由本阶段暴露的编译错误必须**在本阶段修正**，不能标记为“留给下一方案”后宣布完成。

如果构建成功但行为回归失败，编译门槛虽通过，本阶段的**行为验收仍不通过**，仍应修复相应 contract 或明确记录环境限制。特别是缺少真实 Windows 桌面/GPU 时，绝不能把未运行的桌面测试算成通过；最终完整证据集中在方案 06。

## 资料链接

[B-EVENTS]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_picking/src/events.rs
[B-PICKING-LIB]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_picking/src/lib.rs
[B-POINTER]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_picking/src/pointer.rs
[R-AGENTS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/AGENTS.md
[R-ARCH]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/docs/architecture.md
[R-CODE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/code.md
[R-DEPS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/dependencies.md
[R-POINTER]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/extras/src/mouse/pointer.rs
[R-SCOPE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/task-scope.md
[R-SCROLL]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/extras/src/mouse/scroll.rs
[R-TEST-RULES]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/testing.md
