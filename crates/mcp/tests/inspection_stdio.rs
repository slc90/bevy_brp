use std::io;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::Lines;
use tokio::net::TcpListener;
use tokio::process::Child;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;
use tokio::process::Command;
use tokio::time::timeout;

const GUIDE_URI: &str = "bevy-brp://guide/inspection";
const PROMPT: &str = "inspect_bevy_app";
const WAIT: Duration = Duration::from_secs(15);

struct StdioClient {
    server: Child,
    input: ChildStdin,
    output: Lines<BufReader<ChildStdout>>,
    request_id: u32,
    modern: bool,
    log_dir: TempDir,
}

impl StdioClient {
    fn start(modern: bool) -> Self {
        let log_dir = tempfile::tempdir().expect("isolated server log directory");
        let mut server = Command::new(env!("CARGO_BIN_EXE_bevy_brp_mcp"))
            .env("TMP", log_dir.path())
            .env("TEMP", log_dir.path())
            .env("TMPDIR", log_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("start MCP server");
        let input = server.stdin.take().expect("piped MCP stdin");
        let output = BufReader::new(server.stdout.take().expect("piped MCP stdout")).lines();
        Self {
            server,
            input,
            output,
            request_id: 0,
            modern,
            log_dir,
        }
    }

    async fn send(&mut self, message: Value) {
        timeout(
            WAIT,
            self.input.write_all(format!("{message}\n").as_bytes()),
        )
        .await
        .expect("MCP request write deadline")
        .expect("write MCP message");
    }

    async fn request(&mut self, method: &str, mut params: Value) -> Value {
        self.request_id += 1;
        let id = self.request_id;
        if self.modern {
            params["_meta"] = json!({
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientInfo": {"name": "inspection-tests", "version": "1"},
                "io.modelcontextprotocol/clientCapabilities": {}
            });
        }
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await;
        let line = timeout(WAIT, self.output.next_line())
            .await
            .expect("MCP response deadline")
            .expect("read MCP response")
            .expect("server closed stdout before responding");
        let response: Value = serde_json::from_str(&line).expect("JSON-RPC response");
        assert_eq!(response["id"], id, "{response}");
        assert_eq!(response["jsonrpc"], "2.0", "{response}");
        response
    }

    async fn result(&mut self, method: &str, params: Value) -> Value {
        let response = self.request(method, params).await;
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    }

    async fn connect(&mut self) -> Value {
        if self.modern {
            return self.result("server/discover", json!({})).await;
        }
        let result = self
            .result(
                "initialize",
                json!({
                    "protocolVersion": "2025-11-25",
                    "clientInfo": {"name": "inspection-tests", "version": "1"},
                    "capabilities": {}
                }),
            )
            .await;
        assert_eq!(result["protocolVersion"], "2025-11-25");
        self.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await;
        result
    }

    async fn finish(mut self) -> String {
        drop(self.input);
        assert!(
            timeout(WAIT, self.server.wait())
                .await
                .expect("MCP shutdown deadline")
                .expect("wait for MCP server")
                .success()
        );
        match std::fs::read_to_string(self.log_dir.path().join("bevy_brp_mcp_trace.log")) {
            Ok(trace) => trace,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => panic!("read isolated server trace: {error}"),
        }
    }
}

#[tokio::test]
async fn inspection_capabilities_work_on_both_protocol_versions() {
    for modern in [false, true] {
        let mut client = StdioClient::start(modern);
        let server_info = client.connect().await;
        let capabilities = &server_info["capabilities"];
        for key in ["tools", "resources", "prompts"] {
            assert!(capabilities[key].is_object(), "{server_info}");
        }
        assert_ne!(capabilities["resources"]["subscribe"], true);

        let tools = client.result("tools/list", json!({})).await;
        let tool_count = if cfg!(feature = "mcp-debug") { 49 } else { 47 };
        assert_eq!(
            tools["tools"].as_array().expect("tool list").len(),
            tool_count
        );

        let resources = client.result("resources/list", json!({})).await;
        assert_eq!(
            resources["resources"]
                .as_array()
                .expect("resource list")
                .len(),
            1
        );
        let guide = &resources["resources"][0];
        assert_eq!(guide["uri"], GUIDE_URI);
        assert_eq!(guide["mimeType"], "text/markdown");

        let templates = client.result("resources/templates/list", json!({})).await;
        assert_eq!(
            templates["resourceTemplates"]
                .as_array()
                .expect("resource templates")
                .len(),
            1
        );
        let template = &templates["resourceTemplates"][0];
        assert_eq!(template["uriTemplate"], "bevy-brp://app/{port}/methods");
        assert_eq!(template["mimeType"], "application/json");

        let read = client
            .result("resources/read", json!({"uri": GUIDE_URI}))
            .await;
        assert_eq!(
            read["contents"].as_array().expect("guide contents").len(),
            1
        );
        assert_eq!(read["contents"][0]["uri"], guide["uri"]);
        assert_eq!(read["contents"][0]["mimeType"], guide["mimeType"]);
        let markdown = read["contents"][0]["text"].as_str().expect("Markdown text");
        for expected in [
            "15702",
            "rpc_discover",
            "brp_execute",
            "world_get_components",
            "brp_extras_screenshot",
        ] {
            assert!(markdown.contains(expected));
        }

        let prompts = client.result("prompts/list", json!({})).await;
        assert_eq!(prompts["prompts"].as_array().expect("prompt list").len(), 1);
        let prompt = &prompts["prompts"][0];
        assert_eq!(prompt["name"], PROMPT);
        assert!(
            !prompt["description"]
                .as_str()
                .expect("prompt description")
                .is_empty()
        );
        assert_eq!(
            prompt["arguments"]
                .as_array()
                .expect("prompt arguments")
                .len(),
            1
        );
        assert_eq!(prompt["arguments"][0]["name"], "port");
        assert_eq!(prompt["arguments"][0]["required"], true);

        for port in ["15712", "65534"] {
            let get = client
                .result(
                    "prompts/get",
                    json!({
                        "name": PROMPT, "arguments": {"port": port}
                    }),
                )
                .await;
            assert_eq!(
                get["messages"].as_array().expect("prompt messages").len(),
                1
            );
            let message = &get["messages"][0];
            assert_eq!(message["role"], "user");
            assert_eq!(message["content"]["type"], "text");
            let preview = message["content"]["text"].as_str().expect("prompt text");
            assert!(preview.contains(&format!("bevy-brp://app/{port}/methods")));
            assert!(preview.contains(GUIDE_URI));
            assert!(preview.contains(&format!("port {port}")));
            assert!(!preview.contains("15702"));
            assert_eq!(get.get("resultType").is_some(), modern, "{get}");
        }

        for listing in [&resources, &templates, &prompts] {
            assert!(listing.get("nextCursor").is_none(), "{listing}");
        }
        for method in ["resources/list", "resources/templates/list", "prompts/list"] {
            let reply = client.request(method, json!({"cursor": "unknown"})).await;
            assert_eq!(reply["error"]["code"], -32602, "{reply}");
            assert_eq!(reply["error"]["data"]["field"], "cursor", "{reply}");
        }
        for result in [&resources, &templates, &read, &prompts] {
            if modern {
                assert_eq!(result["resultType"], "complete");
                assert!(result["ttlMs"].is_u64(), "{result}");
                assert_eq!(result["cacheScope"], "private");
            } else {
                assert!(result.get("resultType").is_none(), "{result}");
            }
        }
        client.finish().await;
    }
}

#[tokio::test]
async fn inspection_rejects_invalid_parameters() {
    let mut client = StdioClient::start(false);
    client.connect().await;
    for (params, field) in [
        (json!({"name": "other"}), "name"),
        (json!({"name": PROMPT}), "port"),
        (
            json!({"name": PROMPT, "arguments": {"port": 15712}}),
            "port",
        ),
        (
            json!({"name": PROMPT, "arguments": {"port": "15712", "extra": true}}),
            "arguments",
        ),
        (
            json!({"name": PROMPT, "arguments": {"port": "+15712"}}),
            "port",
        ),
        (
            json!({"name": PROMPT, "arguments": {"port": "65535"}}),
            "port",
        ),
    ] {
        let reply = client.request("prompts/get", params).await;
        assert_eq!(reply["error"]["code"], -32602, "{reply}");
        assert_eq!(reply["error"]["data"]["field"], field, "{reply}");
    }
    for port in ["0", "1023", "65535", "%31", "1/15712", "15712?query=1"] {
        let reply = client
            .request(
                "resources/read",
                json!({"uri": format!("bevy-brp://app/{port}/methods")}),
            )
            .await;
        assert_eq!(reply["error"]["code"], -32602, "{reply}");
        assert_eq!(reply["error"]["data"]["field"], "port", "{reply}");
    }
    let reply = client
        .request("resources/read", json!({"uri": "bevy-brp://guide/other"}))
        .await;
    assert_eq!(reply["error"]["code"], -32002, "{reply}");
    client.finish().await;
}

async fn read_methods_reply(client: &mut StdioClient, port: u16) -> Value {
    client
        .request(
            "resources/read",
            json!({"uri": format!("bevy-brp://app/{port}/methods")}),
        )
        .await
}

async fn read_discovery_request(stream: &mut tokio::net::TcpStream) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .expect("read BRP request line");
    assert_eq!(line, "POST /jsonrpc HTTP/1.1\r\n");
    let mut content_length = None;
    loop {
        line.clear();
        assert_ne!(
            reader.read_line(&mut line).await.expect("read BRP header"),
            0
        );
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = Some(value.trim().parse::<usize>().expect("valid content length"));
        }
    }
    let mut body = vec![0; content_length.expect("request with a content length")];
    reader
        .read_exact(&mut body)
        .await
        .expect("read BRP request body");
    assert_eq!(
        serde_json::from_slice::<Value>(&body).expect("valid BRP request JSON"),
        json!({"jsonrpc": "2.0", "id": 1, "method": "rpc.discover", "params": null})
    );
}

async fn with_brp_reply(client: &mut StdioClient, wire: String) -> (u16, Value) {
    let (listener, port) = bind_brp_fixture().await;
    let serve = async {
        let (mut stream, _) = listener.accept().await.expect("accept BRP connection");
        read_discovery_request(&mut stream).await;
        stream.write_all(wire.as_bytes()).await?;
        stream.shutdown().await
    };
    let (result, served) = timeout(WAIT, async {
        tokio::join!(read_methods_reply(client, port), serve)
    })
    .await
    .expect("BRP fixture round trip deadline");
    match served {
        Ok(()) => {}
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::BrokenPipe
            ) => {}
        Err(error) => panic!("failed to write BRP fixture: {error}"),
    }
    (port, result)
}

async fn bind_brp_fixture() -> (TcpListener, u16) {
    for _ in 0..8 {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind BRP fixture");
        let port = listener.local_addr().expect("bound BRP address").port();
        if (1024..=65534).contains(&port) {
            return (listener, port);
        }
    }
    panic!("could not allocate a valid BRP fixture port");
}

fn http_json(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

fn assert_discovery_error(reply: &Value, port: u16, kind: &str) {
    assert!(reply.get("result").is_none(), "{reply}");
    let error = &reply["error"];
    assert_eq!(error["code"], -32603, "{reply}");
    assert_eq!(error["data"]["method"], "rpc.discover", "{reply}");
    assert_eq!(error["data"]["port"], port, "{reply}");
    assert_eq!(error["data"]["kind"], kind, "{reply}");
}

#[tokio::test]
async fn inspection_reads_live_discovery_and_propagates_failures() {
    let mut client = StdioClient::start(true);
    client.connect().await;
    let document = json!({
        "openrpc": "1.3.2",
        "info": {"title": "Bevy Remote Protocol", "version": "0.20.0"},
        "methods": [{"name": "world.query", "params": [], "description": "Query entities"}],
        "servers": null
    });
    let (port, reply) = with_brp_reply(
        &mut client,
        http_json(&json!({"jsonrpc": "2.0", "id": 1, "result": document}).to_string()),
    )
    .await;
    let result = &reply["result"];
    assert_eq!(result["ttlMs"], 0);
    assert_eq!(result["cacheScope"], "private");
    assert_eq!(
        result["contents"]
            .as_array()
            .expect("directory contents")
            .len(),
        1
    );
    let content = &result["contents"][0];
    assert_eq!(content["uri"], format!("bevy-brp://app/{port}/methods"));
    assert_eq!(content["mimeType"], "application/json");
    assert_eq!(
        serde_json::from_str::<Value>(content["text"].as_str().expect("directory text"))
            .expect("valid OpenRPC JSON"),
        document
    );

    let (closed, port) = bind_brp_fixture().await;
    drop(closed);
    assert_discovery_error(
        &read_methods_reply(&mut client, port).await,
        port,
        "connection",
    );

    for (wire, kind) in [
        (http_json("not JSON"), "invalid_response"),
        (
            http_json(r#"{"jsonrpc":"2.0","id":1,"result":{"methods":null}}"#),
            "invalid_response",
        ),
        (
            http_json(r#"{"jsonrpc":"2.0","id":99,"result":{}}"#),
            "invalid_response",
        ),
        (
            http_json(
                r#"{"jsonrpc":"2.0","id":1,"result":null,"error":{"code":-32601,"message":"Method not found"}}"#,
            ),
            "invalid_response",
        ),
        (
            "HTTP/1.1 200 OK\r\nContent-Length: 99999999\r\n\r\n".to_owned(),
            "response_too_large",
        ),
        (
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n".to_owned(),
            "http_status",
        ),
    ] {
        let (port, reply) = with_brp_reply(&mut client, wire).await;
        assert_discovery_error(&reply, port, kind);
        if kind == "http_status" {
            assert_eq!(reply["error"]["data"]["status"], 503);
        }
    }
    let body = " ".repeat(1_048_577);
    let (port, reply) = with_brp_reply(
        &mut client,
        format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
            body.len()
        ),
    )
    .await;
    assert_discovery_error(&reply, port, "response_too_large");

    let brp_error = json!({
        "code": -32601,
        "message": "Method not found",
        "data": {"source": "test-brp-error-payload"}
    });
    let (port, reply) = with_brp_reply(
        &mut client,
        http_json(&json!({"jsonrpc": "2.0", "id": 1, "error": brp_error}).to_string()),
    )
    .await;
    assert_discovery_error(&reply, port, "brp_error");
    assert_eq!(reply["error"]["data"]["brp_error"], brp_error);

    let (listener, port) = bind_brp_fixture().await;
    let serve = async {
        let (mut stream, _) = listener.accept().await.expect("accept BRP connection");
        read_discovery_request(&mut stream).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n{")
            .await
            .expect("write a partial BRP response");
        let mut byte = [0];
        assert_eq!(
            stream
                .read(&mut byte)
                .await
                .expect("read after BRP timeout"),
            0
        );
    };
    let (reply, ()) = timeout(WAIT, async {
        tokio::join!(read_methods_reply(&mut client, port), serve)
    })
    .await
    .expect("slow BRP fixture deadline");
    assert_discovery_error(&reply, port, "timeout");
    let trace = client.finish().await;
    assert!(!trace.contains("test-brp-error-payload"), "{trace}");
}

#[cfg(feature = "mcp-debug")]
#[tokio::test]
async fn inspection_payloads_are_not_written_to_server_trace() {
    let mut client = StdioClient::start(false);
    client.connect().await;
    let setting = client
        .result(
            "tools/call",
            json!({"name": "brp_set_tracing_level", "arguments": {"level": "trace"}}),
        )
        .await;
    assert_eq!(setting["structuredContent"]["status"], "success");
    let guide = client
        .result("resources/read", json!({"uri": GUIDE_URI}))
        .await;
    let text = guide["contents"][0]["text"].as_str().expect("guide text");
    let heading = text.lines().next().expect("guide heading");
    let invalid_argument = "test-private-prompt-argument";
    let reply = client
        .request(
            "prompts/get",
            json!({"name": PROMPT, "arguments": {"port": invalid_argument}}),
        )
        .await;
    assert_eq!(reply["error"]["data"]["field"], "port");
    let trace = client.finish().await;
    assert!(trace.contains("Tracing level set to: trace"));
    for payload in [heading, invalid_argument] {
        assert!(!trace.contains(payload), "trace contains {payload}");
    }
}
