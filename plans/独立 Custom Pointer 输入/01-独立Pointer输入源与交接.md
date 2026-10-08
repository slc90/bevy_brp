# 01 · 建立独立 Pointer 输入源与交接规则

## 目标

建立只存在于 App 内的 BRP Custom Pointer，并把激活、真人交接和退场定义成完整生命周期。

## 范围

负责输入源状态、Picking 装配、generation、交接与取消通知责任。普通鼠标方法的具体映射和自动手势排程由后续方案承接。

## 预期产出

私有输入源及交接状态机、明确的取消清理责任和最小 Picking 验证基础。

## 与前后方案的关系

这是本仓库的起点；后续基础输入映射依赖这里确定的身份、状态和排程边界。

本文件是顺序执行链中的第 1 份，具体位置见 [总览](00-overview.md)。需要查看完整设计时，使用 [总方案](../master-plan.md)。

## 基线与共同约束

版本：方案草案 1，2026-10-05。

核对基线：`slc90/bevy_brp@7eb70b5d24a19b8e7480b11bc1726af4e80cce6c`（本次读取时的 `main`）；配套消费方为 `slc90/bevy_widgetry@f32f5d0452cf8d63b6d506dc1060236e70367dd5`。仓库当前 workspace 版本为 `0.2.2`，依赖 Bevy `0.19.1`。仓库内容通过 GitHub 连接器读取；Bevy 行为另外核对了上游 `v0.19.1` 源码，而不是用浮动 `latest` 猜测 API。[B1] [U1] [U2]

这是一份待实施方案。本次没有修改仓库，没有运行 Rust 编译、测试、MCP 或桌面验收。下面的 `BrpPointerState`、`pointer_control`、动作队列和生命周期状态均是本方案提出的设计，不是声称仓库已经提供这些能力。

### 共同边界

目标是让 BRP 的普通鼠标操作只驱动 App 内的一只 `PointerId::Custom`，不移动系统光标，不伪造系统鼠标按键，不把 Widgetry 的业务状态直接改成测试希望看到的结果。正常输入路径是 `BRP → PointerInput → Bevy Picking → Pointer<T> → Widget`。物理鼠标仍由 Bevy 默认输入适配器产生 `PointerId::Mouse`，不被永久禁用或替换。[U1] [U2]

用户已限定 BRP 和真人不会同时操作。本方案不扩展为多操作者并发框架，但“没有同时动鼠标”不等于“没有残留 hover”。输入交接仍须结束旧动作、停用旧虚拟位置，并处理停在窗口里的物理 Mouse 位置。输入源识别留在 BRP；Widgetry 普通控件不识别 BRP 的 UUID，也不引入 BRP 生产依赖。

**Native Window move/resize 完整保持现状。** 不修改 Widgetry 的 `crates/window/src/title_bar/drag.rs`、`crates/window/src/title_bar/resize.rs` 及其现有原生操作、鼠标释放和系统 cursor 处理；不加 Custom Pointer 拒绝分支，也不添加程序化拖窗、缩放窗口或 OS 自动化替代实现。虚拟输入不支持这些原生拖拽区域，测试必须避开。保持代码不变不代表那里会自动拒绝 Custom Pointer：当前 observer 仍可能收到虚拟 Press 并发出原生拖拽请求。这是本次明确保留的边界，不得包装成已隔离或已支持的能力。[W3] [W4]

统一的是普通鼠标类 move、button、click、double-click、drag、scroll。keyboard、IME、截图、窗口标题、原生触控板 pinch/rotation/double-tap 是各自独立的能力，不把它们硬译成 PointerAction，也不借此次改动重写它们。Table 列宽拖动、ScrollArea 滚动条拖动属于普通控件交互，**不在 Native Window resize 的例外中**。不新增可见虚拟光标、overlay 或新的公共控件。

贯穿各部分的实现契约是：同一 App 的 BRP Pointer 身份稳定；普通输入只排队为 PointerInput，在既定 Picking 阶段消费；跨帧动作绑定 generation 与活动责任；取消不冒充正常 Release，清理完成前不允许新一轮输入穿过。取消通知补全只修框架生命周期，不直接改控件业务 state。官方 Hovered/DirectlyHovered 的 Custom 适配由消费方负责，不能为了样式退回伪造 Mouse。新 `pointer_control` 经现有 brp_execute 调用，status/release 及错误快照用于交接和诊断，不把请求接受等同于 UI 成功。

整个实施顺序固定为：本仓库小方案 01 → 02 → 03 → 04 → 05 → 06，再执行配套 Widgetry 小方案 01 → 02 → 03 → 04 → 05。Widgetry 前几个方案并不技术依赖本仓库发布，这个总顺序只是为了交接清楚；Widgetry 的最终接入才真正依赖本仓库可消费的固定版本。不得用浮动 `main` 或覆盖旧 tag 完成交接。

## 方案正文

### 已核实的起点

`mouse/cursor.rs` 的 `SimulatedCursorPosition` 按窗口保存位置，却会由 `sync_cursor_position` 接收真实 `CursorMoved` 并回写；它不是独立输入源。`mouse/support.rs::send_motion_events` 同时发 `MouseMotion`、`CursorMoved`，还调用 `Window::set_cursor_position`。`mouse/drag.rs` 自己也有两处窗口光标位置更新，不能只改 support 后就宣布系统光标隔离完成。[B2] [B3] [B4]

上游 `PointerId` 的 required components 包括 `PointerLocation`、`PointerPress`、`PointerInteraction`。Custom pointer 需要实体和输入流；只发一个带 UUID 的消息并不等于建立了完整 pointer。[U1]

另有一个消费者侧兼容点：Bevy 0.19.1 的 `update_is_hovered` 与 `update_is_directly_hovered` 明确只查询 Mouse 的 HoverMap。Custom 的 Pointer 事件可以正常进入交互，但官方 `Hovered` / `DirectlyHovered` 不会因此自动反映 Custom。配套 Widgetry 方案在 core 统一适配这两个派生状态；BRP 不因此伪造 `PointerId::Mouse`，也不在通用输入库里替换宿主的 hover 状态算法。BRP 自带宿主应把 Pointer 事件与官方 Mouse-only 状态分别观测，不能用一种证据替代另一种。[U6]

### 输入源及装配

保留现有 `mouse` 模块的职责，不为这次改动新建跨仓库公共 crate。新增私有 Pointer 状态与排队模块，可以在 `mouse` 下采用 `pointer.rs`、`queue.rs` 等名字；这些是建议文件名。`MousePlugin` 负责装配它们，现有 BRP handler 不直接持有操作系统窗口句柄。

每个 App 只维护一只 BRP Custom pointer，UUID 在该 App 内创建一次，跨普通输入动作保持稳定；指针实体不能每次 click 都重新创建，否则多击和生命周期会被割裂。状态至少表达：指针身份和实体、当前目标窗口、各窗口最后的 BRP 逻辑坐标、当前按键、输入来源交接状态、操作 generation、动作队列与在途操作。保存的按键状态用于调度与冲突验证；Bevy 的 `PointerPress` 是消费后的状态，不能把二者混为一个随时同步的值。

建议将生命周期表达为 `Inactive / Active / Draining`。首个合法普通鼠标请求可以自动激活，不新增使用者必须记住的 begin 调用；非法请求不激活、不移动，也不拿走任何现有交互。release、目标窗口销毁和取消使当前 generation 失效，旧 timer 或旧队列项不得在新一轮激活后再释放或移动新指针。空闲但正在 hover 的 Active pointer 不等于存在持续动画任务。

生产依赖显式启用所需的 `bevy_picking` 能力，并为 UUID 使用直接、最小的依赖配置，不借测试宿主的 feature unification 掩盖缺失。保持 `extras` 原有 `--no-default-features`、`diagnostics`、`ui` 组合可检查；Pointer 输入不是只能开 `ui` 才存在的能力，3D picking 宿主也可使用。[B1] [B11]

宿主继续负责安装自己的 Picking 核心、交互插件和适用 backend。Extras 不无条件再装一套 `DefaultPickingPlugins`，不强开 UI backend，不替换宿主的真实鼠标适配器。装配顺序不能导致重复插件。无 Picking 的最小 App 仍应能安装 Extras 并使用无关方法；调用鼠标方法时若核心资源缺失，返回明确能力错误而不是 panic。资源检查能确认核心条件，不能证明所有自定义 backend 都实际产生 hit；后者由宿主及集成测试负责。

### 串行使用仍需要交接

采用“一个有效操作者”的交接，而不是 Mouse 永久优先，也不是两只指针一直 hover 后由 Widgetry 猜是谁。

BRP 首次接管时，只有物理 pointer 没有未结束按键/拖动才允许激活。保存恢复所需的物理指针位置上下文，暂时使物理 Mouse 的 Picking 位置不参与普通控件命中；不要移动 OS cursor，不写 `Window::set_cursor_position`，不修改全局 `ButtonInput<MouseButton>`，也不永久关闭 `PointerInputSettings`。默认物理输入适配器继续读取真实事件，因此其窗口定位和内部鼠标位置缓存不会停在旧值。[U2]

在 `First` 的默认输入产生之后检查新输入。若 Active 状态下出现新的 `PointerId::Mouse` Move、Press 或 Scroll，视为真人交接回来：失效 BRP generation，取消其剩余动作，停用 Custom pointer；物理事件仍由原来的管线消费。不要因系统级 `MouseMotion` 或用户在其他应用里移动鼠标就触发交接。这里识别 Mouse 的代码属于输入源适配，不是 Widgetry 的控件行为。

显式释放时，物理 Picking 位置从当前有效的原生窗口 cursor 信息或最后可信物理输入恢复；只读这些位置，不把 BRP 坐标写回原生窗口。无有效物理位置时保持 inactive，等待下一次真实输入，不恢复过期窗口实体。第一下真实 click/scroll 即使没有新的 Move，也必须使用默认输入流携带的物理位置，不能误用 BRP 最后的位置。实现不得通过清空全局 `Messages<PointerInput>` 或过滤别人的输入流来达成交接。

正常使用前提仍是不并发。真人输入撞上同帧 BRP 请求属于前提被打破的恢复场景，采用真人接回、旧 BRP 动作取消，不承诺两组动作同时成功。验收必须检查第一次真人操作没有被错误重放、变成双击或送到虚拟指针的旧位置。

### 退场不是简单 despawn

Bevy 0.19.1 的 `PointerInput::receive` 对 Press、Release、Move 更新组件，但对 Cancel 不重置 `PointerPress`、不清空 `PointerLocation`。还要把 hover 和事件两处实现合起来看：`generate_hovermap` 会过滤本帧 Cancel pointer 的全部 hit，使当前 HoverMap 为空；`pointer_events` 的 Cancel 分支却只遍历当前 HoverMap，再清内部 PointerState。因此，正常全管线中的 Cancel 不能被视为保证通知原 press/drag 目标的手段。[U1] [U4] [U7]

取消帧保留原 pointer 实体及最后一个有效 Location，让引擎根据 PreviousHoverMap 产生必要的 Out/Leave。交互派发和必要通知完成后，才在清理阶段把 BRP `PointerPress` 恢复默认、`PointerLocation` 置空；下一有效 hover 周期确认派生状态收敛。先置空位置或先 despawn 会让引擎无法取得位置并跳过 Out。原窗口/实体已经销毁时，以取消与有效性检查收尾，不承诺对不存在的目标派发事件。[U3] [U7]

源侧增加一个范围受限的取消通知补全，不复制 Click/Drag 算法。它只跟踪本 BRP Pointer 已由 Picking 实际产生的 Press、DragStart 及对应终止消息，保存原始目标、命中信息、位置和 generation；正常 Release/DragEnd 后移除对应责任。取消时在引擎正常派发后，向仍存活、仍有本轮按下或拖动责任、且尚未收到 Cancel 的原目标补发一次框架级 `Pointer<Cancel>`。通知同时遵循引擎的 observer/message 两个表面，按 pointer、generation、原目标去重，完成 deferred commands 后才确认清理。只记录实际命中的对象，不凭 BRP 坐标猜目标。

这是一处适应固定 Bevy 版本取消行为的生命周期桥接，不是正常输入旁路。日常移动、点击、滚轮与拖动仍全部从 `PointerInput` 进入真实 Picking；不得直接触发 Click、Activate、ValueChange，或修改 Table/Button 私有组件来让测试通过。原始目标已销毁或层级改变时，不制造已失效 Entity 的事件，也不承诺所有第三方 Widget 的私有会话都能由源侧代管；配套 Widgetry 仍对通用 Pointer 失效提供自己的幂等收尾。

退场同时完成动作队列取消、timer 失效、通知责任释放、PointerPress 重置和位置失效。取消不是补一个正常 Release，不能制造 Click、DragDrop 或成功完成通知。同一次取消由高层通知和 Widgetry 兜底同时看见时，只能结束原会话一次；旧 generation 的通知也不能终止新一轮手势。

### 调度与验证边界

Bevy 0.19.1 的 `PickingSystems::Input / PostInput` 在 `First`，`ProcessInput / Backend / Hover / PostHover / Last` 在 `PreUpdate`。BRP handler 只做验证、确定窗口并排队；统一生产者在 `First` 的输入阶段输出 Custom `PointerInput`，交接整理在 `PostInput` 完成，所有实体与位置准备须在下一段 `ProcessInput` 前可见。清理的完成确认置于交互处理之后，不能把 `Update` 里随手写消息等同于已经被 Picking 处理。[U2] [U5]

这一部分的产出是一套可由最小 Picking fixture 验证的输入源、队列和交接状态机。验收包含反复激活/释放、停在控件上的物理鼠标、第一次真人 click/scroll、定时按住期间移动、销毁目标窗口、没有 Picking 的宿主，以及 Cancel 后组件与派生 hover 的收敛。精确退场帧数以真实调度测试固定；不得只靠猜测 runtime 有“两帧余量”。所有临时保存的物理位置都必须同步 CursorLeft/窗口销毁等失效信息，不能只因 Entity 还存在就恢复已经离开窗口的旧 hover。

## 来源

以下链接对应本次实际核对的源码或项目约束。仓库现状、上游行为和本方案提出的设计在正文中分别标明。GitHub code search 对该 fork 返回过 incomplete results，因此没有把“搜索零结果”当作全仓不存在某种输入路径的证据；实施时仍须对本地源码做完整符号扫描。已核实的修改锚点不能替代这一回归扫描。

[B1]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/Cargo.toml
[B2]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/crates/extras/src/mouse/cursor.rs
[B3]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/crates/extras/src/mouse/support.rs
[B4]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/crates/extras/src/mouse/drag.rs
[B5]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/crates/extras/src/mouse/scroll.rs
[B6]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/crates/mcp/src/brp_tools/mouse.rs
[B7]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/crates/extras/src/mouse/click.rs
[B8]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/crates/extras/src/mouse/button.rs
[B9]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/crates/extras/src/activity.rs
[B10]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/crates/runtime/src/progress.rs
[B11]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/docs/architecture.md
[B12]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/rules/brp-mcp.md
[B13]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/crates/mcp/src/brp_tools/tools.rs
[B14]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/docs/testing.md
[B15]: https://github.com/slc90/bevy_brp/blob/7eb70b5d24a19b8e7480b11bc1726af4e80cce6c/AGENTS.md
[W1]: https://github.com/slc90/bevy_widgetry/blob/f32f5d0452cf8d63b6d506dc1060236e70367dd5/Cargo.toml
[W3]: https://github.com/slc90/bevy_widgetry/blob/f32f5d0452cf8d63b6d506dc1060236e70367dd5/crates/window/src/title_bar/drag.rs
[W4]: https://github.com/slc90/bevy_widgetry/blob/f32f5d0452cf8d63b6d506dc1060236e70367dd5/crates/window/src/title_bar/resize.rs
[W5]: https://github.com/slc90/bevy_widgetry/blob/f32f5d0452cf8d63b6d506dc1060236e70367dd5/rules/gui-debugging.md
[U1]: https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_picking/src/pointer.rs
[U2]: https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_picking/src/input.rs
[U3]: https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_picking/src/events.rs#L650-L1050
[U4]: https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_picking/src/events.rs#L1200-L1215
[U5]: https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_picking/src/lib.rs#L256-L450
[U6]: https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_picking/src/hover.rs#L320-L455
[U7]: https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_picking/src/hover.rs#L102-L214
