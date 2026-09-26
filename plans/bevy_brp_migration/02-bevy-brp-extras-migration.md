# bevy_brp Extras 已验证改动正式迁入

**Repository:** `slc90/bevy_brp`

## 目标

把 Widgetry vendored `bevy_brp_extras` 中已经运行验证的 methods-only transport 和跨帧 activity lifecycle 行为正式纳入 fork 的 `extras/`。

## 范围

仅限 `slc90/bevy_brp/extras` 及为其编译所需的 workspace 依赖关系。不重新设计 Extras API，不直接用 vendor 目录整体覆盖 fork crate。

## 预期产出

fork 内 `bevy_brp_extras` 自身具有 `without_http_transport()` 以及 `BrpExtrasActivity` / `BrpExtrasActivityState` 的现有语义，Widgetry 的 `PATCHES.md` 不再是这些行为的维护边界。

## 与前后方案的关系

依赖方案 01 的仓库基线。它为方案 03 的 `bevy_brp_runtime` 提供 methods-only Extras 和 activity state 接缝。

## 方案内容

## 5. Extras 迁移

以 fork 中 `extras/` 的 0.22.7 源码为基础，把 Widgetry `vendor/bevy_brp_extras` 已验证的语义改动正式合入 `extras/`，不要直接用 vendor 目录整体覆盖仓库 crate。

迁入内容包括：

```text
新增：
extras/src/activity.rs

修改：
extras/src/lib.rs
extras/src/plugin.rs
extras/src/keyboard/keys.rs
extras/src/keyboard/mod.rs
extras/src/keyboard/typing.rs
extras/src/mouse/button.rs
extras/src/mouse/click.rs
extras/src/mouse/drag.rs
extras/src/mouse/support.rs
extras/src/screenshot/capture/pending_screenshot_capture.rs
extras/src/screenshot/capture/screenshot_job.rs
extras/src/shutdown.rs
```

必须保留两类现有行为：

- `BrpExtrasPlugin::without_http_transport()`：只注册 Extras methods / systems，并安装或复用 `RemotePlugin`，不安装 HTTP transport。
- `BrpExtrasActivity` / `BrpExtrasActivityState`：跟踪 keyboard、mouse、typing、drag、double click、screenshot、deferred shutdown 等真实跨帧 activity。

原 Widgetry vendor 中的 `PATCHES.md` 不作为新仓库的 patch 机制继续维护；这些行为迁入后就是 `bevy_brp_extras` 本体。
