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

