use async_channel::{Receiver, Sender, TrySendError};
use async_io::Async;
use bevy::app::{AppExit, Last, Startup};
use bevy::ecs::schedule::{IntoScheduleConfigs, common_conditions::run_once};
use bevy::prelude::*;
use bevy::render::{Render, RenderApp};
use bevy::tasks::futures_lite::{StreamExt as LiteStreamExt, future as lite_future};
use bevy::tasks::{IoTaskPool, Task};
use bevy::window::RequestRedraw;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};
use bevy_brp_extras::BrpExtrasActivity;
use bevy_remote::http::{DEFAULT_ADDR, DEFAULT_RENDER_PORT, HostAddress, HostPort};
use bevy_remote::{
    BrpBatch, BrpError, BrpMessage, BrpReceiver, BrpRequest, BrpResponse, BrpResult, BrpSender,
    RemoteLast, RemoteSystems, error_codes,
};
use futures_util::future::{self, FutureExt};
use futures_util::stream::{FuturesUnordered, StreamExt as FuturesStreamExt};
use futures_util::{Future, pin_mut, select_biased};
use http_body_util::{BodyExt, Full};
use hyper::body::{Body, Bytes, Frame, Incoming};
use hyper::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use hyper::server::conn::http1;
use hyper::{Request, Response, service};
use serde_json::Value;
use smol_hyper::rt::{FuturesIo, SmolTimer};
use std::error::Error;
use std::fmt;
use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use super::progress::BrpProgress;

const MAIN_ENDPOINT: &str = "Main";
const RENDER_ENDPOINT: &str = "Render";
const RUNNING: u8 = 0;
const APP_SHUTDOWN: u8 = 1;
const TRANSPORT_FAILED: u8 = 2;
const FRAME_FALLBACK_DELAY: Duration = Duration::from_nanos(16_666_667);

type TransportResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
type ConnectionFuture = Pin<Box<dyn Future<Output = ConnectionOutcome> + Send>>;

/// runtime crate 内部的 wake-aware BRP HTTP plugin。
pub(super) struct BrpRemoteHttpPlugin {
    request_deadline: Duration,
}

/// 单个 World 的 HTTP endpoint 配置；Main 与 Render 共享 lifecycle 和主 event loop proxy。
#[derive(Resource, Clone)]
struct HttpEndpointConfig {
    /// 用于日志区分 Main 与 Render World。
    endpoint: &'static str,
    /// listener 绑定地址。
    address: IpAddr,
    /// 当前 World 独立使用的 listener 端口。
    port: u16,
    /// 追加到所有 HTTP response 的 header。
    headers: HeaderMap,
    /// 非 SSE 请求等待最终结果的最长时间。
    request_deadline: Duration,
    /// 指向 主 event loop 的 proxy。
    event_loop_proxy: EventLoopProxy<WinitUserEvent>,
    /// 两个 endpoint 共用的关闭和失败 state。
    lifecycle: TransportLifecycle,
    /// 两个 World 与 I/O task 共用的按需续帧状态。
    progress: BrpProgress,
}

/// 保存 server task，使 App World 的 ownership 控制 listener 与所有 connection future。
#[derive(Resource)]
struct HttpServerTask {
    /// resource 被移除或 World drop 时取消完整 server future。
    _task: Task<()>,
}

/// Main 与 Render transport 共享的失败和关闭 state。
#[derive(Clone)]
struct TransportLifecycle {
    /// 允许 Main、Render 与 I/O task 共享同一份 lifecycle state。
    inner: Arc<TransportLifecycleInner>,
}

/// 保存跨线程 lifecycle state 与首个 fatal failure。
struct TransportLifecycleInner {
    /// 区分运行中、正常 App shutdown 和 transport failure。
    state: AtomicU8,
    /// 通过 close 通知全部 listener 停止接受 connection。
    shutdown_sender: Sender<()>,
    /// 每个 server future clone 后等待同一次关闭通知。
    shutdown_receiver: Receiver<()>,
    /// 只向 Main World 交付首个 fatal failure。
    failure: Mutex<Option<TransportFailure>>,
}

/// 传递给 Main World 的首个致命 transport 失败。
#[derive(Clone)]
struct TransportFailure {
    /// 发生失败的 World endpoint。
    endpoint: &'static str,
    /// 保留底层 bind 或 accept error context。
    message: String,
}

/// mailbox 提交阶段能够区分的关闭原因。
#[derive(Debug, Eq, PartialEq)]
enum SubmitError {
    /// App 已释放对应 World 的 BRP mailbox。
    MailboxClosed,
    /// Winit event loop 已结束，不能再请求 update。
    EventLoopClosed,
}

/// 等待普通请求结果时的 transport 失败。
enum ResponseWaitError {
    /// 请求超过配置的 transport deadline。
    Deadline,
    /// App 在发送结果前关闭了 result channel。
    ResultChannelClosed,
}

/// HTTP server loop 在 accept、connection 完成和关闭之间等待的事件。
enum ServerEvent {
    /// listener 完成一次 accept。
    Accepted(io::Result<(Async<TcpStream>, SocketAddr)>),
    /// 一个受 owner 管理的 connection future 已结束。
    ConnectionFinished(Option<ConnectionOutcome>),
    /// App 退出或任一 endpoint 失败后停止 server。
    Shutdown,
}

/// 单个 connection 的完成信息，用于区分 peer 与错误来源。
struct ConnectionOutcome {
    /// 当前 connection 的远端地址。
    peer: SocketAddr,
    /// 正常关闭或 HTTP connection error。
    result: TransportResult<()>,
}

/// 在 result receiver 结束或被取消时触发一次 wake，使 watcher cleanup 能继续运行。
struct CleanupWake {
    /// 提供 event-loop proxy 与 shutdown state。
    config: HttpEndpointConfig,
}

/// 普通 HTTP 请求从成功提交到首个 result 的独立工作责任。
struct PendingResultWait {
    /// 释放时更新 controller 并 wake event loop 交付收尾帧。
    config: HttpEndpointConfig,
}

/// 把 Watching method 的多次结果编码为上游约定的 SSE body。
struct BrpStream {
    /// 原样附加到每个 BRP response 的 request id。
    id: Option<Value>,
    /// 接收 Watching method 的后续结果。
    receiver: Pin<Box<Receiver<BrpResult>>>,
    /// stream 取消或结束时请求 watcher cleanup。
    _cleanup: CleanupWake,
}

/// 区分普通 JSON response 与 SSE stream。
enum BrpHttpResponse<C, S> {
    /// 已经取得最终 BRP response。
    Complete(C),
    /// 持续交付 Watching method 结果。
    Stream(S),
}

/// Hyper connection 使用的统一 response body。
enum BrpHttpBody {
    /// 单次 JSON response body。
    Complete(Full<Bytes>),
    /// boxed SSE body，避免放大普通 response variant。
    Stream(Box<BrpStream>),
}

impl BrpRemoteHttpPlugin {
    /// 使用给定普通请求 deadline 创建 runtime transport。
    pub(super) const fn new(request_deadline: Duration) -> Self {
        Self { request_deadline }
    }
}

impl Plugin for BrpRemoteHttpPlugin {
    #[allow(
        clippy::panic,
        reason = "the runtime cannot provide wake-aware transport without WinitPlugin"
    )]
    fn build(&self, app: &mut App) {
        let Some(event_loop_proxy) = app.world().get_resource::<EventLoopProxyWrapper>() else {
            error!("BRP runtime HTTP transport 缺少 Winit event loop proxy");
            panic!("BrpRemoteHttpPlugin requires WinitPlugin");
        };
        let event_loop_proxy = EventLoopProxy::clone(event_loop_proxy);
        let lifecycle = TransportLifecycle::new();
        let progress = BrpProgress::default();
        let callback_progress = progress.clone();
        let callback_proxy = event_loop_proxy.clone();
        let callback_lifecycle = lifecycle.clone();
        app.world()
            .resource::<BrpExtrasActivity>()
            .set_change_callback(move |activity| {
                if callback_progress.update_extras(activity)
                    && wake_event_loop(&callback_proxy).is_err()
                    && !callback_lifecycle.is_app_shutdown()
                {
                    warn!("Extras activity 变化后无法 wake event loop");
                }
            });
        let main_port = resolve_main_port();
        let main_config = HttpEndpointConfig {
            endpoint: MAIN_ENDPOINT,
            address: DEFAULT_ADDR,
            port: main_port,
            headers: HeaderMap::new(),
            request_deadline: self.request_deadline,
            event_loop_proxy: event_loop_proxy.clone(),
            lifecycle: lifecycle.clone(),
            progress: progress.clone(),
        };

        app.insert_resource(HostAddress(DEFAULT_ADDR))
            .insert_resource(HostPort(main_port))
            .insert_resource(main_config)
            .add_systems(Startup, start_http_server)
            .add_systems(Last, shutdown_transport_on_app_exit)
            .add_systems(
                RemoteLast,
                (report_transport_failure, drive_main_progress)
                    .chain()
                    .after(RemoteSystems::Cleanup),
            );

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        let render_config = HttpEndpointConfig {
            endpoint: RENDER_ENDPOINT,
            address: DEFAULT_ADDR,
            port: DEFAULT_RENDER_PORT,
            headers: HeaderMap::new(),
            request_deadline: self.request_deadline,
            event_loop_proxy,
            lifecycle,
            progress,
        };
        render_app
            .insert_resource(HostAddress(DEFAULT_ADDR))
            .insert_resource(HostPort(DEFAULT_RENDER_PORT))
            .insert_resource(render_config)
            .add_systems(Render, start_http_server.run_if(run_once))
            .add_systems(
                RemoteLast,
                report_render_progress.after(RemoteSystems::Cleanup),
            );
    }
}

impl TransportLifecycle {
    /// 建立由 close 传播到全部 listener 的共享 lifecycle。
    fn new() -> Self {
        let (shutdown_sender, shutdown_receiver) = async_channel::bounded(1);
        Self {
            inner: Arc::new(TransportLifecycleInner {
                state: AtomicU8::new(RUNNING),
                shutdown_sender,
                shutdown_receiver,
                failure: Mutex::new(None),
            }),
        }
    }

    /// 等待 App 退出或任一 endpoint 失败。
    async fn wait_for_shutdown(&self) {
        match self.inner.shutdown_receiver.recv().await {
            Ok(()) | Err(_) => {},
        }
    }

    /// App 正常退出时关闭两个 endpoint。
    fn shutdown_for_app_exit(&self) {
        if self
            .inner
            .state
            .compare_exchange(RUNNING, APP_SHUTDOWN, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.inner.shutdown_sender.close();
        }
    }

    /// 保存首个致命失败并关闭两个 endpoint，避免只留下单侧 listener。
    fn fail(&self, failure: TransportFailure) {
        if self
            .inner
            .state
            .compare_exchange(
                RUNNING,
                TRANSPORT_FAILED,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_err()
        {
            return;
        }

        match self.inner.failure.lock() {
            Ok(mut slot) => *slot = Some(failure),
            Err(error) => {
                error!(error = %error, "BRP runtime transport failure state 已损坏");
            },
        }
        self.inner.shutdown_sender.close();
    }

    /// 由 Main World 取出尚未报告的致命失败。
    fn take_failure(&self) -> Option<TransportFailure> {
        match self.inner.failure.lock() {
            Ok(mut slot) => slot.take(),
            Err(error) => {
                error!(error = %error, "BRP runtime transport failure state 无法读取");
                None
            },
        }
    }

    /// 区分正常 App shutdown 与运行期 event loop 异常关闭。
    fn is_app_shutdown(&self) -> bool {
        self.inner.state.load(Ordering::SeqCst) == APP_SHUTDOWN
    }

    /// endpoint 启动前检查另一端是否已经触发整体关闭。
    fn is_running(&self) -> bool {
        self.inner.state.load(Ordering::SeqCst) == RUNNING
    }
}

impl fmt::Display for SubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MailboxClosed => formatter.write_str("BRP mailbox is closed"),
            Self::EventLoopClosed => formatter.write_str("Winit event loop is closed"),
        }
    }
}

impl Error for SubmitError {}

impl CleanupWake {
    /// 为已经入队并持有 result receiver 的请求建立取消清理 guard。
    const fn new(config: HttpEndpointConfig) -> Self {
        Self { config }
    }
}

impl PendingResultWait {
    /// 登记一个已成功提交的普通请求。
    fn new(config: HttpEndpointConfig) -> Self {
        config.progress.begin_result_wait();
        Self { config }
    }
}

impl Drop for CleanupWake {
    fn drop(&mut self) {
        if wake_event_loop(&self.config.event_loop_proxy).is_err()
            && !self.config.lifecycle.is_app_shutdown()
        {
            warn!(
                endpoint = self.config.endpoint,
                "BRP result receiver 结束后无法 wake event loop"
            );
        }
    }
}

impl Drop for PendingResultWait {
    fn drop(&mut self) {
        self.config.progress.finish_result_wait();
        if wake_event_loop(&self.config.event_loop_proxy).is_err()
            && !self.config.lifecycle.is_app_shutdown()
        {
            warn!(
                endpoint = self.config.endpoint,
                "BRP result 等待结束后无法 wake event loop"
            );
        }
    }
}

impl Body for BrpStream {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.as_mut().receiver.poll_next(context) {
            Poll::Ready(Some(result)) => {
                let response = BrpResponse::new(self.id.clone(), result);
                match serde_json::to_string(&response) {
                    Ok(serialized) => Poll::Ready(Some(Ok(Frame::data(Bytes::from(format!(
                        "data: {serialized}\n\n"
                    )))))),
                    Err(error) => Poll::Ready(Some(Err(io::Error::other(error)))),
                }
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.receiver.is_closed()
    }
}

impl Body for BrpHttpBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.get_mut() {
            Self::Complete(body) => match Body::poll_frame(Pin::new(body), context) {
                Poll::Ready(Some(Ok(frame))) => Poll::Ready(Some(Ok(frame))),
                Poll::Ready(Some(Err(error))) => match error {},
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            },
            Self::Stream(body) => Body::poll_frame(Pin::new(body.as_mut()), context),
        }
    }
}

/// 解析 Main port，保持合法环境变量优先、非法值回退默认端口的原有语义。
fn resolve_main_port() -> u16 {
    let Some(value) = std::env::var_os("BRP_EXTRAS_PORT") else {
        return bevy_brp_extras::DEFAULT_REMOTE_PORT;
    };
    match value.to_string_lossy().parse::<u16>() {
        Ok(port) => port,
        Err(error) => {
            warn!(
                value = %value.to_string_lossy(),
                %error,
                fallback_port = bevy_brp_extras::DEFAULT_REMOTE_PORT,
                "BRP_EXTRAS_PORT 无效，使用默认端口"
            );
            bevy_brp_extras::DEFAULT_REMOTE_PORT
        },
    }
}

/// 在对应 World mailbox 已建立后绑定 listener 并把 server task 交给 World 持有。
fn start_http_server(
    mut commands: Commands,
    request_sender: Res<BrpSender>,
    config: Res<HttpEndpointConfig>,
) {
    if !config.lifecycle.is_running() {
        return;
    }

    let listener = match Async::<TcpListener>::bind((config.address, config.port)) {
        Ok(listener) => listener,
        Err(error) => {
            fail_transport(&config, format!("failed to bind listener: {error}"));
            return;
        },
    };
    let task_config = config.clone();
    let task = IoTaskPool::get().spawn(server_main(listener, request_sender.clone(), task_config));
    commands.insert_resource(HttpServerTask { _task: task });
}

/// 接受 connection 并在同一 owned server future 中并发推进，shutdown 时统一取消。
async fn server_main(
    listener: Async<TcpListener>,
    request_sender: Sender<BrpMessage>,
    config: HttpEndpointConfig,
) {
    let mut connections = FuturesUnordered::<ConnectionFuture>::new();

    loop {
        let event = {
            let accept = listener.accept().fuse();
            let shutdown = config.lifecycle.wait_for_shutdown().fuse();
            let connection_finished: Pin<
                Box<dyn Future<Output = Option<ConnectionOutcome>> + Send + '_>,
            > = if connections.is_empty() {
                Box::pin(future::pending())
            } else {
                Box::pin(FuturesStreamExt::next(&mut connections))
            };
            let connection_finished = connection_finished.fuse();
            pin_mut!(accept, shutdown, connection_finished);

            select_biased! {
                () = shutdown => ServerEvent::Shutdown,
                outcome = connection_finished => ServerEvent::ConnectionFinished(outcome),
                accepted = accept => ServerEvent::Accepted(accepted),
            }
        };

        match event {
            ServerEvent::Accepted(Ok((client, peer))) => {
                let connection_sender = request_sender.clone();
                let connection_config = config.clone();
                connections.push(Box::pin(async move {
                    ConnectionOutcome {
                        peer,
                        result: handle_client(client, connection_sender, connection_config).await,
                    }
                }));
            },
            ServerEvent::Accepted(Err(error)) => {
                fail_transport(&config, format!("listener accept failed: {error}"));
                break;
            },
            ServerEvent::ConnectionFinished(Some(outcome)) => {
                if let Err(error) = outcome.result {
                    warn!(
                        endpoint = config.endpoint,
                        peer = %outcome.peer,
                        %error,
                        "BRP 客户端连接异常结束"
                    );
                }
            },
            ServerEvent::ConnectionFinished(None) => {},
            ServerEvent::Shutdown => break,
        }
    }
}

/// 运行单个 HTTP/1 connection；错误返回 owner 统一诊断，不影响其他 connection。
async fn handle_client(
    client: Async<TcpStream>,
    request_sender: Sender<BrpMessage>,
    config: HttpEndpointConfig,
) -> TransportResult<()> {
    http1::Builder::new()
        .timer(SmolTimer::new())
        .serve_connection(
            FuturesIo::new(client),
            service::service_fn(|request| process_request_batch(request, &request_sender, &config)),
        )
        .await?;
    Ok(())
}

/// 保持上游 BRP 的 single、batch、SSE 与解析错误响应格式。
async fn process_request_batch(
    request: Request<Incoming>,
    request_sender: &Sender<BrpMessage>,
    config: &HttpEndpointConfig,
) -> TransportResult<Response<BrpHttpBody>> {
    let batch_bytes = request.into_body().collect().await?.to_bytes();
    let batch = serde_json::from_slice::<BrpBatch>(&batch_bytes);

    let result = match batch {
        Ok(BrpBatch::Single(request)) => {
            match process_single_request(request, request_sender, config).await {
                BrpHttpResponse::Complete(response) => {
                    BrpHttpResponse::Complete(serde_json::to_string(&response)?)
                },
                BrpHttpResponse::Stream(stream) => BrpHttpResponse::Stream(stream),
            }
        },
        Ok(BrpBatch::Batch(requests)) => {
            let mut responses = Vec::new();
            for request in requests {
                match process_single_request(request, request_sender, config).await {
                    BrpHttpResponse::Complete(response) => responses.push(response),
                    BrpHttpResponse::Stream(BrpStream { id, .. }) => {
                        responses.push(BrpResponse::new(
                            id,
                            Err(BrpError {
                                code: error_codes::INVALID_REQUEST,
                                message: "Streaming can not be used in batch requests".to_string(),
                                data: None,
                            }),
                        ));
                    },
                }
            }
            BrpHttpResponse::Complete(serde_json::to_string(&responses)?)
        },
        Err(error) => BrpHttpResponse::Complete(serde_json::to_string(&BrpResponse::new(
            None,
            Err(BrpError {
                code: error_codes::INVALID_REQUEST,
                message: error.to_string(),
                data: None,
            }),
        ))?),
    };

    let mut response = match result {
        BrpHttpResponse::Complete(serialized) => {
            let mut response =
                Response::new(BrpHttpBody::Complete(Full::new(Bytes::from(serialized))));
            response
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            response
        },
        BrpHttpResponse::Stream(stream) => {
            let mut response = Response::new(BrpHttpBody::Stream(Box::new(stream)));
            response
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
            response
        },
    };
    for (name, value) in &config.headers {
        response.headers_mut().insert(name, value.clone());
    }
    Ok(response)
}

/// 解析并提交单条 BRP 请求；普通请求受 deadline 约束，SSE 仅随 connection 生命周期结束。
async fn process_single_request(
    request: Value,
    request_sender: &Sender<BrpMessage>,
    config: &HttpEndpointConfig,
) -> BrpHttpResponse<BrpResponse, BrpStream> {
    let id = request
        .as_object()
        .and_then(|object| object.get("id"))
        .cloned();
    let request = match serde_json::from_value::<BrpRequest>(request) {
        Ok(request) => request,
        Err(error) => {
            return BrpHttpResponse::Complete(BrpResponse::new(
                id,
                Err(BrpError {
                    code: error_codes::INVALID_REQUEST,
                    message: error.to_string(),
                    data: None,
                }),
            ));
        },
    };

    let method = request.method.clone();
    let watch = method.contains("+watch");
    let result_capacity = if watch { 8 } else { 1 };
    let (result_sender, result_receiver) = async_channel::bounded(result_capacity);
    let message = BrpMessage {
        method: request.method,
        params: request.params,
        sender: result_sender,
    };
    let wake = || wake_event_loop(&config.event_loop_proxy);
    if let Err(error) = submit_message(request_sender, message, wake).await {
        warn!(
            endpoint = config.endpoint,
            method,
            error = %error,
            "BRP runtime 请求提交失败"
        );
        return BrpHttpResponse::Complete(transport_error_response(request.id, error));
    }

    let cleanup = CleanupWake::new(config.clone());
    if watch {
        return BrpHttpResponse::Stream(BrpStream {
            id: request.id,
            receiver: Box::pin(result_receiver),
            _cleanup: cleanup,
        });
    }

    let _pending_result = PendingResultWait::new(config.clone());
    // HTTP method name 无法表达 ECS registration 是 Instant 还是 Watching。普通 response 完成后
    // 仍保留 guard，使 screenshot 等隐式 Watching method 能在下一次 update 观察已关闭的
    // result receiver。
    let _cleanup = cleanup;
    match receive_result(result_receiver, config.request_deadline).await {
        Ok(result) => BrpHttpResponse::Complete(BrpResponse::new(request.id, result)),
        Err(ResponseWaitError::Deadline) => {
            warn!(
                endpoint = config.endpoint,
                method,
                timeout_seconds = config.request_deadline.as_secs_f64(),
                "BRP runtime 请求等待超时"
            );
            BrpHttpResponse::Complete(BrpResponse::new(
                request.id,
                Err(BrpError {
                    code: error_codes::INTERNAL_ERROR,
                    message: format!(
                        "BRP request timed out after {} seconds",
                        config.request_deadline.as_secs_f64()
                    ),
                    data: None,
                }),
            ))
        },
        Err(ResponseWaitError::ResultChannelClosed) => {
            warn!(
                endpoint = config.endpoint,
                method, "BRP runtime result channel 已关闭"
            );
            BrpHttpResponse::Complete(transport_error_response(
                request.id,
                SubmitError::MailboxClosed,
            ))
        },
    }
}

/// 先提交 message，再 wake；满 mailbox 先唤醒旧工作，等待成功入队后再次 wake。
async fn submit_message(
    request_sender: &Sender<BrpMessage>,
    message: BrpMessage,
    wake: impl Fn() -> Result<(), SubmitError>,
) -> Result<(), SubmitError> {
    match request_sender.try_send(message) {
        Ok(()) => wake(),
        Err(TrySendError::Full(message)) => {
            wake()?;
            request_sender
                .send(message)
                .await
                .map_err(|_| SubmitError::MailboxClosed)?;
            wake()
        },
        Err(TrySendError::Closed(_)) => Err(SubmitError::MailboxClosed),
    }
}

/// 等待普通请求结果或 deadline；future 被取消时 receiver 随调用栈释放。
async fn receive_result(
    result_receiver: Receiver<BrpResult>,
    deadline: Duration,
) -> Result<BrpResult, ResponseWaitError> {
    enum WaitOutcome {
        Result(Result<BrpResult, async_channel::RecvError>),
        Deadline,
    }

    match lite_future::race(
        async { WaitOutcome::Result(result_receiver.recv().await) },
        async {
            async_io::Timer::after(deadline).await;
            WaitOutcome::Deadline
        },
    )
    .await
    {
        WaitOutcome::Result(Ok(result)) => Ok(result),
        WaitOutcome::Result(Err(_)) => Err(ResponseWaitError::ResultChannelClosed),
        WaitOutcome::Deadline => Err(ResponseWaitError::Deadline),
    }
}

/// 把 transport 失败转换为保留原 request id 的 BRP error response。
fn transport_error_response(id: Option<Value>, error: SubmitError) -> BrpResponse {
    BrpResponse::new(
        id,
        Err(BrpError {
            code: error_codes::INTERNAL_ERROR,
            message: error.to_string(),
            data: None,
        }),
    )
}

/// 使用 Winit 的 public proxy 请求下一次 App update。
fn wake_event_loop(event_loop_proxy: &EventLoopProxy<WinitUserEvent>) -> Result<(), SubmitError> {
    event_loop_proxy
        .send_event(WinitUserEvent::WakeUp)
        .map_err(|_| SubmitError::EventLoopClosed)
}

/// fatal server 失败会关闭双 endpoint，并 wake Main World 走 `AppExit` 路径。
fn fail_transport(config: &HttpEndpointConfig, message: String) {
    config.lifecycle.fail(TransportFailure {
        endpoint: config.endpoint,
        message,
    });
    if wake_event_loop(&config.event_loop_proxy).is_err() && !config.lifecycle.is_app_shutdown() {
        error!(
            endpoint = config.endpoint,
            "BRP transport 失败后无法 wake event loop"
        );
    }
}

/// 在 Main World 报告 fatal transport 失败并结束 App。
fn report_transport_failure(config: Res<HttpEndpointConfig>, mut app_exit: MessageWriter<AppExit>) {
    let Some(failure) = config.lifecycle.take_failure() else {
        return;
    };
    error!(
        endpoint = failure.endpoint,
        error = %failure.message,
        "BRP runtime HTTP transport 无法继续运行"
    );
    app_exit.write(AppExit::from_code(1));
}

/// `AppExit` 已产生时关闭 listener 与所有 connection future，不在 World 线程同步 join。
fn shutdown_transport_on_app_exit(
    mut app_exit: MessageReader<AppExit>,
    config: Res<HttpEndpointConfig>,
) {
    if app_exit.read().next().is_some() {
        config.progress.shutdown();
        config.lifecycle.shutdown_for_app_exit();
    }
}

/// Main World cleanup 后汇总全部工作来源，并按需请求 redraw 与单次 fallback。
fn drive_main_progress(
    receiver: Res<BrpReceiver>,
    config: Res<HttpEndpointConfig>,
    mut redraw: MessageWriter<RequestRedraw>,
) {
    let action = config.progress.finish_main_update(!receiver.is_empty());
    if action.request_redraw {
        redraw.write(RequestRedraw);
    }
    if let Some(generation) = action.fallback_generation {
        schedule_fallback(config.clone(), generation);
    }
}

/// Render World cleanup 后只报告残余 mailbox state，不跨线程读取 World。
fn report_render_progress(receiver: Res<BrpReceiver>, config: Res<HttpEndpointConfig>) {
    if config.progress.report_render_mailbox(!receiver.is_empty())
        && wake_event_loop(&config.event_loop_proxy).is_err()
        && !config.lifecycle.is_app_shutdown()
    {
        fail_transport(
            &config,
            "event loop closed while reporting Render BRP progress".to_string(),
        );
    }
}

/// 复用 I/O executor 预约一个 generation 绑定的单次 `WakeUp` fallback。
fn schedule_fallback(config: HttpEndpointConfig, generation: u64) {
    IoTaskPool::get()
        .spawn(async move {
            async_io::Timer::after(FRAME_FALLBACK_DELAY).await;
            if config.progress.fire_fallback(generation)
                && wake_event_loop(&config.event_loop_proxy).is_err()
                && !config.lifecycle.is_app_shutdown()
            {
                fail_transport(
                    &config,
                    "event loop closed while BRP progress remained active".to_string(),
                );
            }
        })
        .detach();
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "test setup and assertions require these operations to succeed"
)]
mod tests {
    use async_channel::{Receiver, Sender};
    use bevy::tasks::block_on;
    use bevy_remote::{BrpMessage, BrpResult};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::{
        ResponseWaitError, SubmitError, TransportFailure, TransportLifecycle, receive_result,
        submit_message,
    };

    /// 构造只用于观察 mailbox 提交顺序的 BRP message。
    fn message() -> BrpMessage {
        let (result_sender, _result_receiver) = async_channel::bounded::<BrpResult>(1);
        BrpMessage {
            method: "test/method".to_string(),
            params: None,
            sender: result_sender,
        }
    }

    /// 验证正常提交只在 message 已进入 mailbox 后触发 wake。
    #[test]
    fn successful_submission_wakes_after_enqueue() {
        let (sender, receiver) = async_channel::bounded(1);
        let wake_count = AtomicUsize::new(0);

        block_on(submit_message(&sender, message(), || {
            assert_eq!(receiver.len(), 1);
            wake_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }))
        .unwrap();

        assert_eq!(wake_count.load(Ordering::SeqCst), 1);
    }

    /// 验证满 mailbox 先唤醒已有工作，并在当前 message 成功入队后再次 wake。
    #[test]
    fn full_mailbox_wakes_before_and_after_waiting_for_capacity() {
        let (sender, receiver) = async_channel::bounded(1);
        sender.try_send(message()).unwrap();
        let wake_count = AtomicUsize::new(0);

        block_on(submit_message(&sender, message(), || {
            let count = wake_count.fetch_add(1, Ordering::SeqCst);
            if count == 0 {
                receiver.try_recv().unwrap();
            } else {
                assert_eq!(receiver.len(), 1);
            }
            Ok(())
        }))
        .unwrap();

        assert_eq!(wake_count.load(Ordering::SeqCst), 2);
    }

    /// 验证 mailbox 已关闭时立即报告 App 不可用，且不会产生虚假的 wake。
    #[test]
    fn closed_mailbox_rejects_submission_without_wake() {
        let (sender, receiver): (Sender<BrpMessage>, Receiver<BrpMessage>) =
            async_channel::bounded(1);
        receiver.close();
        let wake_count = AtomicUsize::new(0);

        let result = block_on(submit_message(&sender, message(), || {
            wake_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));

        assert_eq!(result, Err(SubmitError::MailboxClosed));
        assert_eq!(wake_count.load(Ordering::SeqCst), 0);
    }

    /// 验证 deadline 会结束等待并释放 result receiver，不把 HTTP timeout 留成 watcher 持有者。
    #[test]
    fn deadline_releases_result_receiver() {
        let (result_sender, result_receiver) = async_channel::bounded::<BrpResult>(1);

        let result = block_on(receive_result(result_receiver, Duration::ZERO));

        assert!(matches!(result, Err(ResponseWaitError::Deadline)));
        assert!(result_sender.is_closed());
    }

    /// 验证任一 endpoint 的 fatal failure 会关闭共享 lifecycle，并只交付该首个失败。
    #[test]
    fn transport_failure_closes_both_endpoint_lifecycle() {
        let lifecycle = TransportLifecycle::new();
        lifecycle.fail(TransportFailure {
            endpoint: "Main",
            message: "bind failed".to_string(),
        });
        lifecycle.fail(TransportFailure {
            endpoint: "Render",
            message: "second failure".to_string(),
        });

        block_on(lifecycle.wait_for_shutdown());
        let failure = lifecycle.take_failure().unwrap();

        assert_eq!(failure.endpoint, "Main");
        assert_eq!(failure.message, "bind failed");
        assert!(!lifecycle.is_running());
    }
}
