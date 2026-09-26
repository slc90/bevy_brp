# bevy_brp 迁入独立 Runtime crate

**Repository:** `slc90/bevy_brp`

## 目标

将 Widgetry Gallery 中已经工作的 wake-aware BRP HTTP transport 与 progress controller 原样迁入新的 `bevy_brp_runtime` crate。

## 范围

仅限 `slc90/bevy_brp` 中新增 `runtime/` 和直接相关的 workspace 依赖、来源说明与许可证文本。不在本次迁移中重写 transport/progress 行为。

## 预期产出

workspace 中出现 `bevy_brp_runtime`，对外只需 `BrpRuntimePlugin`，内部组合 `BrpExtrasPlugin::without_http_transport()` 和 wake-aware HTTP transport，并保留现有 30 秒 deadline、Main/Render wake、activity/pending-result/tail-update/fallback 与共享 lifecycle 语义。

## 与前后方案的关系

依赖方案 02 已把 Extras 所需语义合入 fork。完成后，方案 04 才能让主测试目标真正走 `runtime -> extras -> Bevy` 链路。

## 方案内容

## 6. 新增 runtime crate

新增 workspace member：

```text
runtime/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── http.rs
    └── progress.rs
```

package：`bevy_brp_runtime`。

直接迁移 Widgetry 当前已经工作的：

```text
gallery/src/brp.rs
gallery/src/brp/http.rs
gallery/src/brp/progress.rs
```

只做独立 crate 必需的调整，不重写行为：

- `GalleryBrpPlugin` -> `BrpRuntimePlugin`
- `GalleryRemoteHttpPlugin` -> `BrpRemoteHttpPlugin`
- 日志、注释和 panic 文案去掉 Gallery 专属措辞
- `http` / `progress` 保持 crate 私有实现

对外入口收敛为：

```rust
use bevy_brp_runtime::BrpRuntimePlugin;

app.add_plugins(BrpRuntimePlugin::default());
```

`BrpRuntimePlugin` 内部组合：

```text
BrpExtrasPlugin::without_http_transport()
+
BrpRemoteHttpPlugin
```

默认普通请求 deadline 继续为 30 秒。

现有 wake / progress 语义原样迁移，包括 Main/Render mailbox wake、Extras activity、pending result、两帧 tail update、generation fallback、共享 transport lifecycle 和 fatal failure shutdown。

`runtime` 吸收当前 Gallery BRP 实现使用的 transport 依赖，包括 `async-channel`、`async-io`、`bevy_remote`、`futures-util`、`http-body-util`、`hyper`、`serde_json`、`smol-hyper` 等。

`gallery/src/brp/UPSTREAM.md` 以及该目录的 MIT / Apache license 文件随派生的 HTTP transport 代码迁到 `runtime/`，用于保留 Bevy `bevy_remote` 派生代码的来源与许可证信息。
