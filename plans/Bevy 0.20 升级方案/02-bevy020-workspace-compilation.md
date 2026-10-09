# 02. 一次性完成 Bevy 0.20 全 workspace 编译兼容

> 迁移方案：`slc90/bevy_brp`，源码基线 `ae9fdaec25cb1f02b501f6e07199a27df144501e`；目标 Bevy `v0.20.0`。状态：**尚未实施、尚未验证**。本文是执行方案，不代表仓库已经达到文中的门槛。

## 目标

统一升级 Bevy 依赖并修复跨所有 crate、测试和 example 的已知与实际发现的编译断点，使整套工程在 0.20.0 下重新成为**可编译、可构建测试产物**的单一代码状态。

## 范围

根 `Cargo.toml`/`Cargo.lock`、生产 crate 的真实 API 接缝、Pointer 高层事件与组件、UI 裁剪 API、runtime 与 Winit/Remote 的直接调用、MCP/proc-macro 的编译路径、所有测试宿主与 examples。只有因编译必要才调整实现；不新增 dual-version shim，不改公共协议和 crate 架构。

## 预期产出

同一 Bevy 0.20.0 依赖图及 lockfile，全部目标/测试可编译，生产 crate 独立 feature 可编译，包含实际编译错误及其解决记录、必要行为适配及仍需深入运行验证的项目。**不允许留存“等 Pointer/截图阶段再修”的编译错误。**

## 与前后方案的关系

承接方案 01 旧版基线。这是从 0.19 跳到 0.20 的唯一破坏性编译切换点；方案 03 和 04 负责输入与截图行为回归，方案 05 验证 Runtime/MCP 协议，方案 06 汇总真实桌面、独立消费和发布交接。后续方案均以本阶段可编译的 0.20 workspace 为起点。

## 迁移边界与共同约束

**目标是让现有能力在 Bevy 0.20.0 上保持可用，不借升级重做架构。** 保留四个生产 crate 的角色、package/target 名称和两条内部依赖：`mcp → mcp_macros`、`runtime → extras`；MCP 与 App 仍通过 BRP 通信。保留 `BrpExtrasPlugin`、`BrpRuntimePlugin`、`BrpRuntimePlugin::with_port`、activity 和 agent-tool 的公共使用方式。类型签名中的 Bevy 类型自然升级为 0.20 类型，不承诺它们与 0.19 类型二进制或 Rust 类型兼容。[架构][R-ARCH]、[边界规则][R-SCOPE]。

不新增“双版本兼容层”、新 crate、替代输入框架或新的 MCP 工具。保持项目自有 BRP/MCP 的方法名、字段、默认值、错误结构和注册边界；上游 `registry.schema` 中的类型集合、类型路径及上游新增方法可以随引擎变化，但不能被误当成项目自有协议变化。所有需要改变既有外部语义的情况，都必须先记录具体不兼容及原因，不得静默修改。

Windows 实施和图形验收使用 PowerShell 7，不使用 Windows PowerShell 5。保留现有 lint、最小 feature、错误传播和资源 ownership 约束；不用扩大 `allow`、删测试、清除历史样本或启用全量 Bevy feature 来掩盖错误。生产代码不得为实现裁剪而引入 `unsafe`，也不为小段几何运算增加依赖。[代码规则][R-CODE]、[依赖规则][R-DEPS]。

每个实际产生代码或工程行为变更的阶段，都遵守仓库要求的独立审查：完成必要验证后，由全新 reviewer 调用 `code-review`，对完整 working tree 做静态审查；有 findings 就修正并由另一位全新 reviewer 重审。缺少 reviewer/subagent 能力时明确记为阻塞，不用施工者自查替代。本文的文档覆盖检查不等于代码审查。[Agent 入口][R-AGENTS]。

各阶段使用同一迁移分支、同一 lockfile 和有记录的提交/工作区状态。保护已有用户修改；不要 reset、覆盖或夹带。测试结果区分“新回归”“已存在的失败”“环境缺失而未运行”，不能把 0.19 的旧成功记录写成 0.20 的新结果。阶段 01 须先确认旧 Bevy 0.19.1 基线可编译；阶段 02 切换至 0.20.0 后必须消除整个 workspace 的所有编译错误，阶段 03—06 每阶段结束亦须重新满足同一编译门槛，不得把本阶段的编译失败留待下一份方案。只有最终验收允许宣告整个升级可用。[测试规则][R-TEST-RULES]。

外部消费者，包括 `bevy_widgetry`，不在本次修改范围。此方案不授权创建或移动 Git tag、发布 package、覆盖本机已安装 MCP 或修改外部仓库。现有 `v0.3.1` 保持不动；最终先以经过验证的完整 revision 交接。后续发布时，runtime、Extras 和 MCP 必须来自同一 revision/tag，不混用。[Git 消费规则][R-DEPS]。

## 统一依赖和编译图

### 依赖修改

根 `[workspace.dependencies]` 中四项 Bevy 依赖统一更新：

```toml
bevy        = { version = "0.20.0", default-features = false }
bevy_mesh   = { version = "0.20.0" }
bevy_remote = { version = "0.20.0", default-features = false }
bevy_winit  = { version = "0.20.0" }
```

以上保留原有 default-features 设置，只更换版本。实际成员继续 `workspace = true`，内部 version/path 关系不改成 Git。`version = "0.20.0"` 是 Cargo 的兼容版本范围，不是精确版本钉死；本轮由 Cargo.lock 记录实际解析的 0.20.0，并在验收中核对。以后若要支持更新的 0.20 补丁版本，另外运行对应验证，不能把本轮结论自动外推。[当前依赖][R-CARGO]。

首次解析允许 Cargo 更新 lockfile；之后所有检查恢复 `--locked`。不要先删除 Cargo.lock 或执行无约束的全依赖升级。保留 rmcp 3.5.1 等与引擎迁移无关的直接依赖版本；确实被新 Bevy 约束迫使改变的传递依赖，记录原因和范围。

`image = "=0.25.9"` 不应为了“跟着升级”而随意放开。0.20 的 `bevy_image` 声明仍允许 `image` 0.25.2 兼容系列，静态看并没有必须解除 0.25.9 精确约束的证据；实际仍要检查解算和 `Image`/`DynamicImage` 转换的类型一致性。UUID 同理：Custom Pointer 接收的是 Bevy 所用 UUID 类型，需核对版本来源一致，不新增另一套 UUID 包装。[Bevy 图像依赖][B-IMAGE]、[Pointer 类型][B-POINTER]。

仓库要求 Rust 1.98.1，0.20.0 的 manifest 要求 1.97.1；因此本轮保持已有工具链下限，不把更新 Rust 或 edition 混入迁移。正式实施仍以实际工具链和传递依赖解算为准。[仓库 manifest][R-CARGO]、[引擎 manifest][B-CARGO]。

### Feature 与重复依赖检查

`extras` 继续使用定向的 `bevy_log`、`bevy_picking`、`bevy_render`、`png`、`serialize`，默认 `diagnostics`/`ui` 不随意改变；`runtime` 保持窗口/渲染和 wake transport 所需 feature；MCP 保持无渲染的独立服务端定位。测试宿主的全默认 feature 不得传播成生产 crate 的新默认值。[各 crate 声明][R-EXTRAS-CARGO]、[runtime][R-RUNTIME-CARGO]、[MCP][R-MCP-CARGO]。

用 `cargo tree -d` 和必要的反向依赖查询核实有无残留 0.19 的 Bevy 子 crate，以及直接依赖和传递依赖是否来自不同版本或不同 source。不是要求依赖图中任何重复包都消失，而是排除会跨公共 API 边界传递的两套 Bevy/图像/UUID 类型。不要因本轮迁移顺手删掉现有未使用 workspace 声明；无关清理另开任务。

### 依赖解析的完成条件与阻塞

manifest 与 lockfile 一致解析到预期引擎版本；版本变动都有依赖原因；没有未解释的双 Bevy 类型图；原有 feature 边界仍明确；编译错误已经归属。无法从注册源取得正式版本、出现不可解依赖或工具链不满足约束时，停止在依赖阶段并写明阻塞，不通过降级到其他版本假装完成 0.20 迁移。

## 跨模块 0.20 API 编译适配

### 不以“临时兼容”破坏行为

这一阶段的主目标是**完整编译边界**，不是只编辑四行版本号。Bevy workspace 的源码、测试和 examples 互相依赖，不能让每个 API 模块先在不可编译的 workspace 里孤立完成。相较于旧拆分，本阶段特意较大，属于必要的跨模块编译切换点。

Pointer 不允许用删除事件消费面、取消派发、按键状态查询或禁用 Picking 测试来换取编译成功。事件层 `PointerPress` 与组件 `PointerPressState` 必须正确区分，底层 `PointerInput.pointer_id` 不能跟着机械改名。`finish_cycle` 等手工 Cancel 构造不能用错误默认值凑类型通过。

UI 截图不能用 `CalculatedClip::Rects(_) => 不裁剪`、旋转 AABB 交集或所有 infinity 都当无效等快捷实现跨过编译：该接口变化本身涉及几何语义，初始适配应遵循下述小型凸多边形与半平面相交设计；方案 04 再用更完整的测试和图像证据校准与修正。若某个 API 修正必须修改 runtime/test fixture 的调用者，本阶段一起消除其编译错误。

MCP 中硬编码反射路径大多是字符串，可能**编译通过仍是错误语义**；这类不阻断 Rust 构建的差异不必在本阶段凭猜测提前改写，留给方案 05 的 live registry 验证。runtime 的接口存在性也不自动证明 wake、SSE 和收尾正确。编译验收是必要条件，后续行为验收是独立条件。

## Pointer 的已知编译断点

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

## UI 裁剪的已知编译断点及必要几何语义

### 已确认的类型变化

原来的 `CalculatedClip { clip: Rect }` 变为：

```rust
pub enum CalculatedClip {
    Rects(SmallVec<[CalculatedClipRect; 2]>),
    FullyClipped,
}

pub struct CalculatedClipRect {
    pub rect: Rect,
    pub world_to_clip_local: Affine2,
}
```

每一项的矩形在产生裁剪的节点局部坐标中，变换方向是 UI world → clip local，不是反过来。`Rects` 的各项共同约束可见区域，不能取第一项、最后一项或把它们求并集。缺少组件或 `Rects([])` 表示没有继承裁剪，不代表完全不可见；`FullyClipped` 必须拒绝 entity 截图。[结构与方法][B-UI-NODE]、[上游裁剪传播][B-UI-CLIPPING]。

上游在 overflow 的一个方向为 Visible 时，会在该轴使用 `-∞/+∞`。这是合法的无界裁剪，不得继续用 `clip.min/max.is_finite()` 一刀切拒绝。NaN、非法的有向边界、非有限变换和不可判定的几何仍应失败。测试中原来把 infinity 当坏输入的情况要区分语义，不能通过删除校验让 NaN 也被接受。[无界轴的来源][B-UI-NODE]。

### 本次采用的几何设计

采用“小型凸多边形与半平面相交”，而不是把每个旋转裁剪矩形先变成轴对齐包围盒。后者会把实际不可见的区域判成可见，也不能判断两个倾斜裁剪区是否真正相交。这个算法是本方案为截图场景选择的本地实现，不是声称 Bevy 自带了截图多边形裁剪 API。

继续以目标相机 viewport 内的物理像素坐标作为 UI 几何空间。`ComputedNode::size()`、`UiGlobalTransform` 和 `ComputedUiRenderTargetInfo` 已提供计算后的信息，不再次乘 `UiScale` 或 window scale factor。Pointer 使用的窗口逻辑坐标与这里的物理截图坐标不能混用。[UI target 传播][B-UI-UPDATE]、[当前截图实现][R-UI-SHOT]。

记节点实际四角构成的凸四边形为 Q，相机 viewport 起点为 v。Q 的顶点必须沿矩形边界排列，不能沿用一个非环绕顺序的四角数组直接做多边形裁剪。旋转、镜像和非均匀缩放都通过现有仿射变换作用于四角；镜像不应因为绕序反转被误判为空。

记硬边界 H 为下列区域的交集：UI target 的本地物理范围、选定 camera 的 viewport、本次仍存活的 render target 范围，以及所有继承 clip。viewport 和实际 target 先统一转换到同一 UI 坐标空间，不能在中途重复加 v。

每个 clip 项给出 `u(p) = world_to_clip_local.transform_point2(p)`。有限边界分别产生四个线性约束：

```text
u(p).x >= rect.min.x    u(p).x <= rect.max.x
u(p).y >= rect.min.y    u(p).y <= rect.max.y
```

合法的 `min = -∞` 或 `max = +∞` 只是不产生该侧约束。直接在这个方向评估约束，不把无穷远矩形四角拿去做矩阵乘法，也不依赖先对变换求逆来制造一个无限大的四边形。零宽/零高、反向边界或只剩线段/点的相交结果不构成可截图面积。

用半平面裁剪求 `V = Q ∩ H`。逐边处理入/出界顶点，跨边界时由有符号距离计算线段交点，例如 `t = d_start / (d_start - d_end)`。可用 f64 中间计算减少边界误差，保留最终与 Bevy 像素坐标的一致性；不能以一个整像素的容差扩大区域。相邻重复点、共线边、平行线和零面积结果须有明确处理。非法/非有限交点不能被静默略过，更不能退回“直接截整块节点”。

`CalculatedClip::contains_point()` 适合做辅助性质测试，不适合代替相交算法。只检查节点四角或中心会漏掉“两块区域交叉但彼此角点都在外面”的可见情况；按像素扫描整张图又会让开销依赖截图分辨率，不采用这两种方案。

### Padding、取整和矩形输出

V 为空时返回既有 UI bounds 类的参数错误。V 非空时先取其轴对齐包围盒 R，再将 R 按请求 padding 扩大，然后再次与 H 求交，得到允许截取的有界区域 D：

```text
V = Q ∩ H
R = bounds(V)
D = expand(R, padding) ∩ H
crop = pixel_cover(bounds(D) + v) ∩ viewport_pixels ∩ live_target_pixels
```

最终 `pixel_cover` 对 min 向下取整、max 向上取整，保持已有“覆盖边界像素”的规则。padding 仍是物理像素，只增加节点周围可截图范围，不能越过相机/target 硬边界；整数扩展继续使用防溢出方式。坐标必须在验证、有限范围裁定和最终 target 约束之后再转为无符号像素整数，避免负值、巨大浮点数或 NaN 被转换过程掩盖。[当前取整与 padding][R-UI-SHOT]。

**返回的仍是一张矩形截图，不是节点独占像素或带透明遮罩的导出。** 对旋转裁剪而言，矩形包围盒的角落可能不在 D 中；那里可能显示背景或其他实体，这是矩形 crop 的正常性质。实际截图来自引擎已经合成并裁剪的 render target。本次不增加 alpha mask、不保证只有目标实体像素，也不添加新的 bounds_kind。精确几何用来确定可见性和最小覆盖范围，不能在文档中误写成“返回矩形每一点都处于旋转裁剪内”。

对原有轴对齐场景，取整、padding 和 viewport 偏移必须保持原测试结果。例如现有 clip/viewport 测试中的 `URect::new(35, 38, 46, 45)` 不应无理由改变。若结果变化，先查变换方向、取整时机和坐标空间，而不是直接更新 expected。

## Runtime、MCP 与反射/插件装配的编译接缝

### 先判断是否真的需要修改实现

0.20 正式源码仍提供 `BrpSender`、`BrpReceiver`、`BrpMessage`、`BrpRequest`、`BrpResponse`、`RemoteLast`、`RemoteSystems`、`RenderApp` 和现有 Winit proxy 入口。因此没有依据预先决定重写整个 `http.rs`。但“同名类型仍存在”不等于组合语义已经通过验证。[0.20 Remote 装配与消息][B-REMOTE]、[0.20 HTTP][B-HTTP]、[Winit][B-WINIT]。

对比固定的上游 `v0.19.1` 和 `v0.20.0` Remote HTTP/lib、Winit 及 Render 相关调度，明确每项是未变、编译适配，还是行为改变。不要把本地 transport 整文件覆盖成新的上游实现；那会丢失这个 crate 的 wake、deadline、双端点 lifecycle 和 progress 语义。[本地来源与六项差异][R-UPSTREAM]。

如果比对和回归证明现有实现已经兼容，可以不改核心代码，只记录验证；不要为显得“完成了迁移”制造无关 diff。

### 装配与调度边界

`BrpRuntimePlugin` 继续在窗口插件之后安装，依赖 Winit event-loop proxy。runtime 组合不自带 HTTP 的 Extras，并且自己拥有唯一 transport；与 stock `RemoteHttpPlugin` 的冲突在插件前后两种安装顺序下都必须明确拒绝。单独使用 Extras 和 stock HTTP 的消费方式继续有效。[公共 runtime 入口][R-RUNTIME-LIB]、[BRP ownership 规则][R-BRP-RULES]。

Main mailbox 的初始化必须早于 Main listener 接收需要处理的请求；当前上游在 `PreStartup` 建 mailbox，本地在 `Startup` 启动 Main listener。Render mailbox 的初始化与本地 Render listener 首次启动也要保持明确先后；0.20 上游在 `RenderStartup` 建 mailbox，并在 Render 后调度 `RemoteLast`。不要把这两种世界当成共享一个 mailbox，也不要把首次资源尚未存在解释成永久不支持 Render。[上游装配][B-REMOTE]、[本地 HTTP][R-RUNTIME-HTTP]。

本地进度核验仍位于 `RemoteSystems::Cleanup` 之后。Main 观察整体是否需要下一帧，Render 报告自己的残留工作；Render 在 Main 判空后才发现工作时，必须补发 wake，不能丢失跨世界边缘事件。

### MCP 不是完全与 Bevy 类型无关

MCP 的 transport 大部分通过 JSON-RPC 工作，但 `type_knowledge.rs` 保存了静态格式知识，`constants.rs` 包含类型路径、字段和示例值；这些内容可能在编译成功后仍然过时。`registry_schema.rs` 的结果保存为 JSON Value，并不自动证明所有新类型已被 type guide 正确理解。[静态类型知识][R-TYPE-KNOWLEDGE]、[常量][R-TYPE-CONSTANTS]、[schema 工具][R-REGISTRY-TOOL]。

核查范围包括 `guide.rs`、`type_kind.rs`、`type_knowledge.rs`、`constants.rs`、`mutation_path_builder/`、`tool_type_guide.rs` 和 `tool_all_types.rs`。只修改实际失效的知识或解析，不借这次升级重新设计递归框架、缓存、tool registry 或宏系统。

## 恢复全量测试宿主的编译覆盖

本阶段覆盖 `tests/test-app` 全部 examples/bin、`tests/test-duplicate-a`、`tests/test-duplicate-b`，以及 Extras/runtime 的直接消费 examples；保留测试宿主的对象、target、fixture 和回归用途。原方案对宿主“能启动并保持行为”的要求依然有效，但**本阶段硬门槛为所有目标可编译**；实际启动、交互与图像在方案 03—06 中按职责验证。

### 广覆盖宿主不能当成无用样例删掉

`tests/test-app/examples/extras_plugin.rs` 故意构造大量 Bevy 组件，为类型指导、mutation 和截图提供目标，因此其全默认 Bevy feature 有实际用途。保留这些 fixture 的对象名称和发现入口，除非上游明确删除了对应类型并有同等用途替代。两个 duplicate package 和同名 bin/example 保护 target 消歧，不能为消除 warning 删除或重命名。[宿主声明][R-TEST-CARGO]、[测试边界][R-TEST-RULES]。

构建范围必须包括全部相关 examples；仅 `cargo check --workspace` 或默认 `cargo build` 不能替代显式的 all-targets/example 检查。能够编译也不等于能启动：注册、渲染资源、组件 requirement 或 shader 加载的问题需要运行才能发现。

### 具体排查规则

以下是与现有宿主用途相关的排查范围，不表示每条都已证明命中当前文件。只对搜索到的真实调用点和实际失败修改，避免把引擎的整份迁移指南变成仓库必改清单。

| 范围 | 修改规则与保持条件 |
| --- | --- |
| Pointer 观察者 | 核对本方案及方案 03 是否覆盖 `pointer_test`、`mouse_test` 及其他实际消费者，不保留泛型旧事件。 |
| CalculatedClip fixture | 使用新枚举和变换；测试真实含义，不把默认 Rects 空列表当成一个零尺寸 clip。 |
| 字体来源和 TextReader | 命中旧泛型字体枚举时改用新构造方式；命中 TextReader 时区分文本和 inline box，不能把非文本项当字符串。 |
| EditableText/TextInput | 只有宿主实际构造可编辑控件时才适配分离后的组件；键盘事件展示不是完整 Widget 编辑行为验收。 |
| Sprite 渲染迁移 | 保持现有测试 entity 的名字、Transform、命中/截图目标和可见结果；不能因为内部渲染实现变化就重写 MCP 工具。 |
| 几何和曲线 crate 拆分 | prelude 仍可用的地方不机械改 import；真实显式路径报错再处理，不预先增加不必要依赖。 |
| MeshAabb 方法、材质/图像/渲染类型 | 只处理实际调用点。AABB 截图读取已有 Aabb，不因此必然需要调用新的 mesh 方法。 |
| 生命周期 observer 和系统 API | 真实命中旧 Bundle 泛型或直接 System 实现才改；普通 `fn(&mut World)` 不预先重构。 |
| WESL/渲染资源 | 有本地 shader 或实际编译失败才做对应迁移；不要凭上游更换 shader 语言就新增 shader 文件。 |

上面来自正式迁移说明的条目只是核查入口，实施替换时以目标 tag 源码和实际类型为准。[迁移说明][B-GUIDE]。

旧 `bevy::ui::widget::Button`、`Interaction` 等若在 0.20 仍可用而只是 deprecated，不为“消灭所有警告”就把通用类型 fixture 改造成另一套控件。先区分兼容性修复与可延后的清理；若现有 lint 确实要求处理，限制到受影响 fixture，说明保留或替代的理由。Pointer 的 stock Button 取消测试仍必须验证真正的 Button 行为，而非一个自制计数器。

### 需要保持的行为和失败边界

`pointer_test` 保留默认 UI Picking backend、stock Button、普通拖动目标、scroll overflow、多窗口和休眠设置；`mouse_test` 的 raw/native 计数仍仅用于物理输入展示，不能被拿来判定新的普通 BRP 鼠标成功。`keyboard_windows` 保留窗口选择、修饰键清理和正常关闭窗口的路径。`event_test` 继续验证真实 observer 行为，而不只接受请求。[现有宿主职责][R-TESTING]。

截图 fixture 中的相机、viewport、非主窗口、UI/AABB 边界和确定性颜色继续存在。若新渲染路径导致像素变化，确认是引擎预期差异还是 crop/坐标错误后，再调整最小必要 expected；记录调整理由，不能整批重录后当成通过。

### 宿主的构建条件与待运行边界

本阶段要求目标宿主和 examples 全部能按原用途构建；按原用途启动并提供真实场景的要求仍然保留，由后续行为阶段检验，不能仅凭编译通过宣告启动和行为已验收；没有因为迁移删除回归能力或扩大生产 feature；剩余平台/图形限制有独立记录。后续方案可以通过固定 target、package_name、窗口和独立端口取得可重复的真实类型与行为结果。

## 编译诊断归属

### 首轮诊断怎样归属

| 编译与行为范围 | 本次六阶段中的处理位置 |
| --- | --- |
| `Pointer<T>`、高层事件字段、按键状态组件 | **02** 完成所有源代码、测试目标和 example 的编译适配；**03** 深入验证状态机、物理接管、取消和桌面隔离。 |
| `CalculatedClip.clip`、旧构造方式和 UI 几何代码 | **02** 依照既定的精确裁剪设计形成可编译实现；**04** 验证旋转、无界轴、真实图像和错误边界。 |
| BRP mailbox、Render schedule、Winit proxy | **02** 处理真实编译不兼容；**05** 验证 Main/Render transport、wake、deadline、watch、shutdown 语义。 |
| 广覆盖宿主类型/字体/渲染变化 | **02** 使所有 bin、examples、tests 可编译，保持 fixture；**03/04/05/06** 按所属行为实际运行。 |
| MCP 静态类型知识、schema 和反射路径 | **02** 保证 MCP 和 proc-macro 的构建与测试产物可编译；**05** 校准 live registry、生成示例和协议真实调用。 |
| 图形桌面、公共 MCP、独立消费、版本交接 | 需要的编译接缝先在 **02** 消除；由 **06** 汇总最终真实验证证据。 |

分类是**问题的责任分配，不是允许编译错误延后的豁免**。任何实际挡住本阶段 `cargo check --workspace --all-targets --locked` 的问题，无论属于哪一个模块，都必须在方案 02 修复。每个源代码适配需能解释属于 0.20 的哪个真实 API 变化，不能凭整份迁移指南提前改不相关的代码。

## 本阶段全仓编译准出门槛（Bevy 0.20.0）

本阶段只有在**完整 workspace** 恢复可编译后才算完成。只检查 `extras`、只检查默认 bin、或者只得到 `cargo check -p <package>` 成功都不算达标。至少执行并记录：

```powershell
cargo check --workspace --all-targets --locked
cargo test --workspace --locked --no-run
cargo build --workspace --examples --locked
```

第一条覆盖各 package、bin、examples、tests 等目标的类型检查；第二条对实际测试产物进行构建/链接；第三条进一步确认 examples 的构建路径。它们是不同的检查面，不能互相替代。对于公共 feature 和生产 crate，还必须避免被 test-app 的 feature unification 掩盖问题：

```powershell
cargo check -p bevy_brp_extras --locked --no-default-features
cargo check -p bevy_brp_extras --locked --no-default-features --features diagnostics
cargo check -p bevy_brp_extras --locked --no-default-features --features ui
cargo check -p bevy_brp_runtime --locked
cargo check -p bevy_brp_mcp --locked --no-default-features
cargo check -p bevy_brp_mcp --locked --features mcp-debug
```

这些是**计划中的验收命令，并非已经运行通过的事实**。如果因真实依赖、平台或工具链问题无法完成，应记录精确阻塞并把本阶段标成未完成；如果因源码 API 不兼容失败，就继续在本阶段修复。不得为了先过门槛而删除宿主/测试、屏蔽功能、增加假的成功实现、用 `cfg` 绕开编译或扩大生产 Bevy features。运行时和真实图形行为另有后续方案逐项验证，编译通过不等于升级完成。

## 资料链接

[B-CARGO]: https://github.com/bevyengine/bevy/blob/v0.20.0/Cargo.toml
[B-EVENTS]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_picking/src/events.rs
[B-GUIDE]: https://bevy.org/learn/migration-guides/0-19-to-0-20/
[B-HTTP]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_remote/src/http.rs
[B-IMAGE]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_image/Cargo.toml
[B-PICKING-LIB]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_picking/src/lib.rs
[B-POINTER]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_picking/src/pointer.rs
[B-REMOTE]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_remote/src/lib.rs
[B-UI-CLIPPING]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_ui/src/layout/clipping.rs
[B-UI-NODE]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_ui/src/ui_node.rs
[B-UI-UPDATE]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_ui/src/update.rs
[B-WINIT]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_winit/src/lib.rs
[R-AGENTS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/AGENTS.md
[R-ARCH]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/docs/architecture.md
[R-BRP-RULES]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/brp-mcp.md
[R-CARGO]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/Cargo.toml
[R-CODE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/code.md
[R-DEPS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/dependencies.md
[R-EXTRAS-CARGO]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/extras/Cargo.toml
[R-MCP-CARGO]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/mcp/Cargo.toml
[R-POINTER]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/extras/src/mouse/pointer.rs
[R-REGISTRY-TOOL]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/mcp/src/brp_tools/tools/registry_schema.rs
[R-RUNTIME-CARGO]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/runtime/Cargo.toml
[R-RUNTIME-HTTP]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/runtime/src/http.rs
[R-RUNTIME-LIB]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/runtime/src/lib.rs
[R-SCOPE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/task-scope.md
[R-SCROLL]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/extras/src/mouse/scroll.rs
[R-TEST-CARGO]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/tests/test-app/Cargo.toml
[R-TEST-RULES]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/testing.md
[R-TESTING]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/docs/testing.md
[R-TYPE-CONSTANTS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/mcp/src/brp_tools/brp_type_guide/constants.rs
[R-TYPE-KNOWLEDGE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/mcp/src/brp_tools/brp_type_guide/type_knowledge.rs
[R-UI-SHOT]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/extras/src/screenshot/ui.rs
[R-UPSTREAM]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/runtime/UPSTREAM.md
