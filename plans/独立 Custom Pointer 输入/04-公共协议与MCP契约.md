# 04 · 对齐 BRP 公共方法、MCP 说明和兼容性

## 目标

让 App 侧 BRP 方法、MCP schema 和说明准确表达新的输入行为与生命周期控制。

## 范围

负责保留普通工具名、收窄不支持按键、注册最小 pointer_control 方法及错误语义；不扩建静态 MCP tool 或动态宿主 catalog。

## 预期产出

可发现、可调用的控制方法，准确的参数和结果说明，以及协议一致性测试。

## 与前后方案的关系

承接已定义的输入和手势语义；下一方案通过真实宿主检验协议到 UI 的完整结果。

本文件是顺序执行链中的第 4 份，具体位置见 [总览](00-overview.md)。需要查看完整设计时，使用 [总方案](../master-plan.md)。

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

### 保留已有普通工具名

保留 `brp_extras/move_mouse`、`send_mouse_button`、`click_mouse`、`double_click_mouse`、`drag_mouse`、`scroll_mouse` 及对应静态 MCP 工具名称。`mouse` 保留为使用习惯，不代表仍向 OS/WindowEvent 注入。大多数既有有效参数和结果字段不变；收窄的按键支持、独立位置、队列完成语义必须明确说明。

MCP 是 Agent 侧协议客户端，输入实体和调度都在 App 内 Extras，不能把虚拟光标位置搬进 MCP server。`runtime → extras`、`mcp → mcp_macros` 的现有依赖方向保持不变。[B11] [B12]

### 新增最小生命周期控制方法

新增 App 侧 instant method `brp_extras/pointer_control`，用于正常结束和观测本 App 的 BRP pointer。请求只接受以下两种动作：

```json
{"action":"status"}
```

```json
{"action":"release"}
```

返回统一的状态快照：`phase` 为 inactive/active/draining，`busy` 表示是否还有排队、timer 或清理工作，`pointer_id` 为已创建 UUID 的字符串或 null，`window` 为当前目标 Entity bits 或 null，`generation` 表示当前操作代数，`queued_actions` 为尚未输出的动作数，`pressed_buttons` 使用 Left/Right/Middle 名称。可空的 `last_error` 记录最近一次排队后失败的 generation、method、window、错误 code 与 message；首次状态为 null，新一轮成功激活时清除旧记录，查询和幂等 release 不抹掉失败证据。以上都是新方法自己的字段，不强加到全部旧方法结果里。

`release` 在 inactive 时幂等；否则启动取消并返回快照，返回 draining 不得解释成已完成。调用方继续用 status 等待 inactive，才开始明确的真人交接或关闭验证。draining 时新普通鼠标命令返回忙/状态冲突，不穿过清理屏障开启新 generation。未知 action 或多余的不合法参数返回规范 BRP 参数错误，不改变状态。

只在 `RemoteMethods` 注册这个控制方法，通过现有 `brp_execute` 调用，并由 `rpc.discover` 发现。**不新增静态 MCP tool，也不把它默认注册成宿主自己的动态 agent tool。** 这三个注册面独立，不能只添加一段 catalog 文本就假定 method 存在。公共方法注册测试要覆盖实际 handler。[B12] [B13]

### 具体同步位置

App 侧涉及 `crates/extras/src/constants.rs`、`plugin.rs`、`mouse.rs` 与新增私有控制模块。MCP 侧核对 `crates/mcp/src/brp_tools/mouse.rs`、`brp_tools/tools/brp_extras_*_mouse.rs`、`tool/name.rs` 以及对应 `help_text/brp_extras_*_mouse.txt`；控制方法经 `brp_execute` 使用，须在有关输入说明和公共 rustdoc 中给出释放与查询示例。不要机械修改通用 `tool/parameters.rs` 或 macro crate，只有 schema 生成确实不支持本次字段时才扩大范围。[B6] [B13]

帮助文本明确：坐标是窗口逻辑坐标，位置不跟随真人同步，不要求 OS 前台焦点，受宿主 Picking 和应用逻辑限制；输入返回接受不代表界面已经完成更新。后台能力不能靠把 `Window.focused` 伪造成 true 实现。

### 不作兼容伪装

原来读取 raw MouseMotion、MouseButtonInput、MouseWheel 或 `Window::cursor_position()` 的 App 逻辑，不会自动消费 Custom Pointer。将其视为这次输入行为迁移的兼容边界，不另建 hidden raw/native fallback，不提供含糊的“virtual/native 自动切换”。背靠 raw 输入的相机示例要么迁移到 Pointer，要么明确不属于新普通鼠标工具的受支持路径。

pinch/rotation/double-tap 是独立 gesture API。原有行为保留并与普通 Pointer 鼠标区分，不能把 double_tap 假装成 double_click，也不能为了全仓搜索没有 Mouse 字样而删除这些能力。Bevy 的 keyboard、MouseScrollUnit 枚举、原生输入适配器以及测试用 Mouse 身份仍合理存在。

### 此部分的验收

产出是可发现、可调用、说明与实现一致的公共协议。实际列出 MCP tools、读取 schema，并通过真实 `brp_execute` 调用新 control method；检查它没有被错当成新增静态工具。测试不支持按键、无效窗口、忙、未知动作及能力缺失错误，保留 BRP 的 code/data/method 信息，不把失败装成成功。

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
