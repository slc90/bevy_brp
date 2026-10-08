# 06 · 形成可供 Widgetry 消费的固定版本

## 目标

形成事实说明一致、可被 Widgetry 固定消费的新 BRP 版本。

## 范围

负责实际文档、兼容性声明、版本和 lockfile 一致性、工程验证及跨仓库交接；不修改 Widgetry 业务实现。

## 预期产出

固定 release/tag 或完整 commit、配套 MCP 与消费说明、清楚的验收记录。

## 与前后方案的关系

这是 BRP 执行链的末尾；完成后按总顺序进入 Widgetry 的五份方案。Widgetry 最终接入依赖这里的固定产物。

本文件是顺序执行链中的第 6 份，具体位置见 [总览](00-overview.md)。需要查看完整设计时，使用 [总方案](../master-plan.md)。

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

### 文档和版本

同步实际承载输入行为的 Extras rustdoc、普通鼠标 MCP help、`docs/testing.md`。`docs/architecture.md` 只在职责或装配事实确实改变时更新，不能把尚未实施的方案先写成已实现事实。`crates/runtime/UPSTREAM.md` 只有 runtime 派生语义实际改变时才更新；仅消费新 Extras 不机械重写上游说明。[B11] [B15]

原始输入消费者不再被普通鼠标方法驱动，官方 Mouse-only Hovered 消费者仍需单独适配，且 Back/Forward 支持被收窄，这是可观察的兼容性变化，发布说明必须明确，不能写成“完全无行为变化的修复”。具体新 tag 在实施通过后确定，本方案用 `<POINTER_RELEASE>` 表示，绝不伪造一个已存在版本。workspace package 版本、内部 version/path 声明、lockfile 和配套 MCP 安装说明一起一致更新；不覆盖 `v0.2.2`。

### 交接内容

向 Widgetry 提供同一个固定 release/tag 或完整 commit 对应的 runtime、Extras 和 MCP，附上普通鼠标工具的参数边界、`pointer_control` 的实际请求/响应、新旧输入语义区别和已完成/未完成验收。交接还要说明普通方法返回接受、status.busy 归零、UI 状态完成三者不是同一个断言。

Widgetry 当前在根 `Cargo.toml` 同时固定 `bevy_brp_runtime` 的 Git tag、MCP 安装命令及 `[workspace.metadata.tools]` 版本。消费方必须同时更新这些位置与 Cargo.lock，不能只换运行库而继续用旧 MCP。[W1]

BRP 不需要知道 Tooltip、Table 或 FileDialog 的私有类型。配套 Widgetry 会在 core 为官方 Hovered/DirectlyHovered 建立唯一的通用 Pointer 写入者、改造 Tooltip 的 Mouse-only 筛选，并补足普通控件的输入级取消/失效清理；不把这些差异反向做进通用自动化库。原生拖窗和原生窗口 resize 保持不支持且不改动。

### 实施约束与最终验收

实施前按本仓库 `AGENTS.md` 读取命中的 rules，包括 scope、development、code、architecture、dependencies、brp-mcp、testing、documentation；不要将历史 plans 当现状。代码和工程行为修改完成后执行仓库要求的独立 Code Review；缺少新 reviewer/subagent 能力时明确记录未完成，不能由施工者自审冒充通过。[B15]

本部分的产出是可消费的版本和事实一致的交接材料。发布门槛为：普通鼠标路径不存在 raw/native fallback；已测的三按钮、窗口、时序、取消、交接和 idle 恢复满足契约；Native Window 例外有明确记录；没有被省略的桌面验收声明。此后按既定总顺序进入 Widgetry 的五份小方案。

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
