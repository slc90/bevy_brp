# 04. 验证 UI 截图的精确裁剪与图像语义

> 迁移方案：`slc90/bevy_brp`，源码基线 `ae9fdaec25cb1f02b501f6e07199a27df144501e`；目标 Bevy `v0.20.0`。状态：**尚未实施、尚未验证**。本文是执行方案，不代表仓库已经达到文中的门槛。

## 目标

验证 Bevy 0.20 多重带变换 `CalculatedClip` 产生的可见区域、截图包围盒和错误行为，并保持旧轴对齐截图的边界与像素语义；按测试发现修复实现。

## 范围

`crates/extras/src/screenshot/ui.rs`、其私有几何与单元测试、`tests/test-app/examples/extras_plugin/screenshot_fixtures.rs`。`capture.rs`、`pending_screenshot_capture.rs`、`screenshot_job.rs`、`target_rgb_image.rs` 只在直接 API 或回归要求时修改。不重做 HTTP、队列或 AABB 相机投影。

## 预期产出

明确的精确多边形裁剪规则、多重旋转与无穷轴的测试、padding/取整/viewport/live target 的旧新兼容证据，以及可检查的真实图像、尺寸、关键像素和错误 contract；全仓继续可编译。

## 与前后方案的关系

依赖方案 02 的编译基线与方案 03 完整输入行为阶段；截图几何本身不依赖 Pointer 算法。随后方案 05 检验与 runtime/MCP 的完成和传输交互；缺少 GPU 的真实图像证据在方案 06 明确标为未测，不伪称通过。

## 迁移边界与共同约束

**目标是让现有能力在 Bevy 0.20.0 上保持可用，不借升级重做架构。** 保留四个生产 crate 的角色、package/target 名称和两条内部依赖：`mcp → mcp_macros`、`runtime → extras`；MCP 与 App 仍通过 BRP 通信。保留 `BrpExtrasPlugin`、`BrpRuntimePlugin`、`BrpRuntimePlugin::with_port`、activity 和 agent-tool 的公共使用方式。类型签名中的 Bevy 类型自然升级为 0.20 类型，不承诺它们与 0.19 类型二进制或 Rust 类型兼容。[架构][R-ARCH]、[边界规则][R-SCOPE]。

不新增“双版本兼容层”、新 crate、替代输入框架或新的 MCP 工具。保持项目自有 BRP/MCP 的方法名、字段、默认值、错误结构和注册边界；上游 `registry.schema` 中的类型集合、类型路径及上游新增方法可以随引擎变化，但不能被误当成项目自有协议变化。所有需要改变既有外部语义的情况，都必须先记录具体不兼容及原因，不得静默修改。

Windows 实施和图形验收使用 PowerShell 7，不使用 Windows PowerShell 5。保留现有 lint、最小 feature、错误传播和资源 ownership 约束；不用扩大 `allow`、删测试、清除历史样本或启用全量 Bevy feature 来掩盖错误。生产代码不得为实现裁剪而引入 `unsafe`，也不为小段几何运算增加依赖。[代码规则][R-CODE]、[依赖规则][R-DEPS]。

每个实际产生代码或工程行为变更的阶段，都遵守仓库要求的独立审查：完成必要验证后，由全新 reviewer 调用 `code-review`，对完整 working tree 做静态审查；有 findings 就修正并由另一位全新 reviewer 重审。缺少 reviewer/subagent 能力时明确记为阻塞，不用施工者自查替代。本文的文档覆盖检查不等于代码审查。[Agent 入口][R-AGENTS]。

各阶段使用同一迁移分支、同一 lockfile 和有记录的提交/工作区状态。保护已有用户修改；不要 reset、覆盖或夹带。测试结果区分“新回归”“已存在的失败”“环境缺失而未运行”，不能把 0.19 的旧成功记录写成 0.20 的新结果。阶段 01 须先确认旧 Bevy 0.19.1 基线可编译；阶段 02 切换至 0.20.0 后必须消除整个 workspace 的所有编译错误，阶段 03—06 每阶段结束亦须重新满足同一编译门槛，不得把本阶段的编译失败留待下一份方案。只有最终验收允许宣告整个升级可用。[测试规则][R-TEST-RULES]。

外部消费者，包括 `bevy_widgetry`，不在本次修改范围。此方案不授权创建或移动 Git tag、发布 package、覆盖本机已安装 MCP 或修改外部仓库。现有 `v0.3.1` 保持不动；最终先以经过验证的完整 revision 交接。后续发布时，runtime、Extras 和 MCP 必须来自同一 revision/tag，不混用。[Git 消费规则][R-DEPS]。

## 多重变换裁剪的几何设计与完整边界

方案 02 已为源码编译落实 0.20 的 `CalculatedClip` 结构及必要的几何接缝，本阶段以**真实语义和几何测试**而非换字段名为验收核心。不能因为已经能编译，就回退到近似 AABB 的剪裁。

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

### 保持截图入口与生命周期

继续通过 `ComputedUiTargetCamera` 选择 UI 实际使用的相机；显式传入不同 camera 时拒绝。保留 UI family 的 absent/complete/partial 区分：完全不是 UI 才考虑原有 AABB 路径；部分初始化的 UI、有 UI 但 FullyClipped/hidden 的 entity 不得回退到 AABB 绕过校验。`CalculatedClip` 本来就是可缺省组件，缺少它不构成 partial UI。[UI 截图入口][R-UI-SHOT]。

保留对不可见、空尺寸、未初始化/失活 camera、失效 target、窗口/image/texture target 的现有边界。相机 viewport 与实际 target 尺寸不同、target 在等待捕获期间变化、实体被改名或销毁时，仍按当前请求快照和 pending capture contract 处理，不能把当前世界的新状态冒充原请求。`capture.rs`、`pending_screenshot_capture.rs`、`screenshot_job.rs`、`target_rgb_image.rs` 仅在正式 API 或直接回归显示有必要时改动。[截图入口与捕获模块][R-SCREENSHOT]、[捕获调度][R-CAPTURE]。

### 自动化与图像验收

| 几何/状态类别 | 覆盖要求 |
| --- | --- |
| 无 clip、空 Rects、FullyClipped | 前两者使用原节点/viewport 范围，最后一种不能成功截图。 |
| 单个平移 clip、多个轴对齐 clip | 保持旧结果，使用交集而不是并集。 |
| 旋转节点/旋转祖先、多重旋转 clip | 与手工构造的可见多边形一致；加入 AABB 相交但实际区域不相交的反例。 |
| 交叉区域、细长区域、边相切 | 不用角点测试漏掉真实可见部分；零面积接触不变成成功截图。 |
| 镜像、非均匀缩放、奇异/非有限变换 | 合法情况正确处理，非法情况明确失败，不放大为整窗截图。 |
| 单轴无界、NaN、反向范围 | 合法 infinity 被支持，非法数值仍拒绝。 |
| DPI、UiScale、非零 viewport 起点、live target 截断 | 不重复缩放/偏移；结果始终落在实际捕获图像中。 |
| padding 为零、较大值、数值边界 | 不溢出、不突破矩形 target 硬边界，旧轴对齐 expected 保持。 |
| OverrideClip/FixedNode/Display::None | 消费引擎计算后的 clip，不自行错误重建祖先裁剪。 |

在已有 screenshot fixture 中增加确定性旋转/嵌套裁剪场景，设置易分辨的前景和背景。验收同时核对返回的几何/尺寸、关键像素和实际图像，不只断言 PNG 文件存在。旧 `NatesList` 图像仍保留检查；渲染变化导致的像素差异需要解释来源，不能直接放宽所有像素断言。[现有截图验收][R-TESTING]。

### 完成条件

算法、错误路径和旧用例均有测试；新增旋转与无界轴用例有自动化覆盖；图像输出含义在现有 rustdoc 中说清楚，没有改变公共 schema。真实图形证据在方案 06 汇总；没有 GPU/桌面时只能记为局部验证完成、图像验收未测。

## 本阶段保持全仓可编译的门槛

前置的 Bevy 0.20 workspace 编译状态必须保持。阶段结束时至少重新执行 `cargo check --workspace --all-targets --locked` 与 `cargo test --workspace --locked --no-run`，并根据改动范围复核生产 crate 独立 feature、examples 和 `cargo build --workspace --examples --locked`。新增或由本阶段暴露的编译错误必须**在本阶段修正**，不能标记为“留给下一方案”后宣布完成。

如果构建成功但行为回归失败，编译门槛虽通过，本阶段的**行为验收仍不通过**，仍应修复相应 contract 或明确记录环境限制。特别是缺少真实 Windows 桌面/GPU 时，绝不能把未运行的桌面测试算成通过；最终完整证据集中在方案 06。

## 资料链接

[B-UI-CLIPPING]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_ui/src/layout/clipping.rs
[B-UI-NODE]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_ui/src/ui_node.rs
[B-UI-UPDATE]: https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_ui/src/update.rs
[R-AGENTS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/AGENTS.md
[R-ARCH]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/docs/architecture.md
[R-CAPTURE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/extras/src/screenshot/capture.rs
[R-CODE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/code.md
[R-DEPS]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/dependencies.md
[R-SCOPE]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/task-scope.md
[R-SCREENSHOT]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/extras/src/screenshot.rs
[R-TEST-RULES]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/rules/testing.md
[R-TESTING]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/docs/testing.md
[R-UI-SHOT]: https://github.com/slc90/bevy_brp/blob/ae9fdaec25cb1f02b501f6e07199a27df144501e/crates/extras/src/screenshot/ui.rs
