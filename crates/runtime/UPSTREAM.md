# bevy_brp_runtime BRP HTTP transport 来源说明

`http.rs` 以 Bevy `0.19.1` 的 `crates/bevy_remote/src/http.rs` 为协议与实现基线：

- repository：<https://github.com/bevyengine/bevy>
- tag：`v0.19.1`
- source：<https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_remote/src/http.rs>
- license：MIT OR Apache-2.0

本目录保留对应的 `LICENSE-MIT` 与 `LICENSE-APACHE`。本地版本继续使用上游
`BrpRequest`、`BrpBatch`、`BrpResponse`、HTTP JSON 与 SSE 约定，仅在 transport ownership
和调度接缝加入以下语义：

1. BRP message 成功进入 Main 或 Render mailbox 后，通过 Winit event-loop proxy 主动 wake App。
2. mailbox 满时先 wake 已排队工作，等待当前 message 入队后再次 wake。
3. 普通请求具有 30 秒默认 deadline；result receiver 在完成、取消或 timeout 后都会请求一次
   cleanup wake，因为 HTTP method name 无法表达 ECS registration 的 Instant / Watching 类型。
4. Main / Render listener 共享失败与 shutdown lifecycle，任一侧 fatal 失败都会关闭另一侧并结束 App。
5. 在 `RemoteSystems::Cleanup` 后检查残留 mailbox，覆盖固定版本遇到未知 method 提前停止 drain 的边界。
6. Main listener 可由 `BrpRuntimePlugin::with_port` 设置代码级 fallback 端口；有效的 `BRP_EXTRAS_PORT` 仍优先，Render listener 保持上游默认端口。

## Bevy 0.20.0 兼容核对

2026-10-09 对照固定 `0.19.1` 与 `0.20.0` 发布源码；本地 transport 未重基，原始来源
和 license 不变。上游 `bevy_remote/src/http.rs` 无差异，`lib.rs` 的变化仅为 rustdoc 中
Tonemapping/DebandDither 路径迁到 `bevy_render::view`。BrpSender/Receiver/Message、
request/result 编码、Watching cleanup 和 RemoteSystems 装配保持。

Main mailbox 仍在 PreStartup 初始化，Render mailbox 仍在 RenderStartup 初始化；本地
Main Startup listener、Render 首次 Render listener 和 RemoteLast 的位置无需适配。
本地 progress 仍在 RemoteSystems::Cleanup 后核验，保留跨 world wake 和满队列保护。

Winit 保留 EventLoopProxyWrapper 与 WakeUp；0.20 增加 Ctrl+C wake、窗口退出的
OnAppExitSystems 顺序，以及 Add<Window> observer 的类型形式，本地未依赖旧 observer。
Render 的 extraction 迁入 bevy_extract，RenderApp、RenderStartup、Render 和调度顺序入口
仍可使用；内部 sets 使用 weak ordering 不改变本地 listener 所用 schedule。
不能由这些静态核对直接推断双端点、deadline、SSE、取消或休眠行为已通过；本轮真实
协议与清理结果记录在 [docs/testing.md](../../docs/testing.md)。
