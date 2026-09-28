//! `brp_execute` calls a one-shot BRP method with raw JSON parameters.
use async_trait::async_trait;
use bevy_brp_mcp_macros::ParamStruct;
use bevy_brp_mcp_macros::ResultStruct;
use error_stack::Report;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use super::rpc_discover;
use crate::brp_tools;
use crate::brp_tools::BrpClient;
use crate::brp_tools::Port;
use crate::brp_tools::ResponseStatus;
use crate::error::Error;
use crate::error::Result;
use crate::tool::ToolFn;

#[derive(Clone, Deserialize, Serialize, JsonSchema, ParamStruct)]
pub struct ExecuteParams {
    /// The BRP method to execute (e.g., `rpc.discover`, `world.get_components`, `world.query`)
    pub method: String,
    /// Optional parameters for the method
    #[to_metadata(skip_if_none)]
    pub params: Option<Value>,
    /// The BRP port (default: 15702)
    #[serde(default)]
    pub port: Port,
}

/// Result type for the dynamic BRP execute tool
#[derive(Serialize, ResultStruct)]
#[brp_result]
pub struct ExecuteResult {
    /// The raw BRP response data
    #[serde(skip_serializing_if = "Option::is_none")]
    #[to_result(skip_if_none)]
    pub result: Option<Value>,

    /// Message template for formatting responses
    #[to_message(message_template = "Executed method {method}")]
    message_template: String,
}

pub struct BrpExecute;

#[async_trait]
impl ToolFn for BrpExecute {
    type Output = ExecuteResult;
    type Params = ExecuteParams;

    async fn handle_impl(&self, params: ExecuteParams) -> Result<ExecuteResult> {
        if params.method.contains("+watch") {
            let method_names = rpc_discover::discover_method_names(params.port).await?;
            if !method_is_registered(&method_names, &params.method) {
                return Err(unregistered_method_error(
                    &params.method,
                    params.port,
                    method_names,
                ));
            }
        }

        let brp_client =
            BrpClient::for_application(params.method.clone(), params.port, params.params.clone());

        let brp_result = brp_client.execute_raw_once().await?;

        match brp_result {
            ResponseStatus::Success(data) => Ok(ExecuteResult::new(data)),
            ResponseStatus::Error(error) => {
                let discovery_error = if error.code == brp_tools::JSON_RPC_ERROR_METHOD_NOT_FOUND {
                    match rpc_discover::discover_method_names(params.port).await {
                        Ok(method_names) => {
                            if !method_is_registered(&method_names, &params.method) {
                                let mut available_methods = method_names;
                                available_methods.sort_unstable();
                                let message = format!(
                                    "BRP method `{}` is not registered on port {}",
                                    params.method, params.port
                                );
                                let method_error_message = error.get_message().to_string();
                                return Err(Error::tool_call_failed_with_details(
                                    brp_tools::method_not_found_message(&params.method, &message),
                                    serde_json::json!({
                                        "stage": "discovery",
                                        "method": params.method,
                                        "port": params.port,
                                        "available_methods": available_methods,
                                        "code": error.code,
                                        "data": error.data,
                                        "method_error_message": method_error_message,
                                    }),
                                )
                                .into());
                            }
                            None
                        }
                        Err(failure) => Some(match failure.current_context() {
                            Error::ToolCall { message, details } => {
                                serde_json::json!({"message": message, "details": details})
                            }
                            context => serde_json::json!({"message": context.to_string()}),
                        }),
                    }
                } else {
                    None
                };
                let mut details = serde_json::json!({
                    "stage": "execution",
                    "method": params.method,
                    "port": params.port,
                    "code": error.code,
                    "data": error.data,
                });
                if let Some(discovery_error) = discovery_error {
                    details["discovery_error"] = discovery_error;
                }
                Err(Error::tool_call_failed_with_details(error.get_message(), details).into())
            }
        }
    }
}

fn unregistered_method_error(
    method: &str,
    port: Port,
    mut available_methods: Vec<String>,
) -> Report<Error> {
    available_methods.sort_unstable();
    let message = format!("BRP method `{method}` is not registered on port {port}");
    Error::tool_call_failed_with_details(
        brp_tools::method_not_found_message(method, &message),
        serde_json::json!({
            "stage": "discovery",
            "method": method,
            "port": port,
            "available_methods": available_methods,
        }),
    )
    .into()
}

fn method_is_registered(method_names: &[String], requested_method: &str) -> bool {
    method_names.iter().any(|method| method == requested_method)
}

#[cfg(test)]
mod tests {
    use std::io;

    use serde_json::Value;
    use serde_json::json;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::net::TcpStream;

    use super::BrpExecute;
    use super::ExecuteParams;
    use super::method_is_registered;
    use crate::brp_tools::Port;
    use crate::error::Error;
    use crate::tool::ToolFn;

    #[test]
    fn execute_params_accept_application_method_names() -> serde_json::Result<()> {
        let params = serde_json::from_value::<ExecuteParams>(json!({
            "port": 15_702,
            "method": "test/multiply",
            "params": {"value": 6, "factor": 7}
        }))?;

        assert_eq!(params.method, "test/multiply");
        assert_eq!(params.params, Some(json!({"value": 6, "factor": 7})));
        Ok(())
    }

    #[test]
    fn registration_requires_an_exact_method_name() {
        let methods = vec![String::from("rpc.discover"), String::from("test/multiply")];

        assert!(method_is_registered(&methods, "test/multiply"));
        assert!(!method_is_registered(&methods, "test/multiply_more"));
    }

    async fn read_method(stream: &mut TcpStream) -> io::Result<String> {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let count = stream.read(&mut chunk).await?;
            if count == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "incomplete HTTP request",
                ));
            }
            bytes.extend_from_slice(&chunk[..count]);
            let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let body_start = header_end + 4;
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "missing content length")
                })?;
            if bytes.len() < body_start + content_length {
                continue;
            }
            let request: Value =
                serde_json::from_slice(&bytes[body_start..body_start + content_length])?;
            return request["method"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing method"));
        }
    }

    async fn write_response(stream: &mut TcpStream, response: Value) -> io::Result<()> {
        let body = response.to_string();
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await?;
        stream.flush().await
    }

    async fn serve_sequence(
        listener: TcpListener,
        expected: Vec<(&'static str, Value)>,
    ) -> io::Result<()> {
        for (method, response) in expected {
            let (mut stream, _) = listener.accept().await?;
            assert_eq!(read_method(&mut stream).await?, method);
            write_response(&mut stream, response).await?;
        }
        Ok(())
    }

    fn discovery_response(methods: &[&str]) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "openrpc": "1.3.2",
                "info": {"title": "Bevy Remote Protocol", "version": "0.19.1"},
                "methods": methods.iter().map(|name| json!({"name": name, "params": []})).collect::<Vec<_>>(),
                "servers": null
            }
        })
    }

    #[tokio::test]
    async fn registered_instant_method_needs_only_its_own_request()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = Port::try_from(listener.local_addr()?.port())?;
        let server = tokio::spawn(serve_sequence(
            listener,
            vec![(
                "test/multiply",
                json!({"jsonrpc":"2.0","id":1,"result":{"product":42}}),
            )],
        ));
        let result = BrpExecute
            .handle_impl(ExecuteParams {
                method: "test/multiply".to_string(),
                params: Some(json!({"value": 6, "factor": 7})),
                port,
            })
            .await?;
        server.await??;
        assert_eq!(result.result, Some(json!({"product": 42})));
        Ok(())
    }

    #[tokio::test]
    async fn missing_method_discovers_available_names_after_one_failed_call()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = Port::try_from(listener.local_addr()?.port())?;
        let server = tokio::spawn(serve_sequence(
            listener,
            vec![
                (
                    "test/missing",
                    json!({"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found","data":{"request_id":7}}}),
                ),
                (
                    "rpc.discover",
                    discovery_response(&["rpc.discover", "test/multiply"]),
                ),
            ],
        ));
        let response = BrpExecute
            .handle_impl(ExecuteParams {
                method: "test/missing".to_string(),
                params: None,
                port,
            })
            .await;
        let error = match response {
            Ok(_) => return Err("missing method must return discovery guidance".into()),
            Err(error) => error,
        };
        server.await??;
        let Error::ToolCall {
            details: Some(details),
            ..
        } = error.current_context()
        else {
            return Err("expected structured tool error".into());
        };
        assert_eq!(details["stage"], "discovery");
        assert_eq!(details["method"], "test/missing");
        assert_eq!(details["port"], port.0);
        assert_eq!(
            details["available_methods"],
            json!(["rpc.discover", "test/multiply"])
        );
        assert_eq!(details["code"], -32601);
        assert_eq!(details["data"]["request_id"], 7);
        assert_eq!(details["method_error_message"], "Method not found");
        Ok(())
    }

    #[tokio::test]
    async fn ordinary_brp_error_keeps_code_and_data_without_discovery()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = Port::try_from(listener.local_addr()?.port())?;
        let server = tokio::spawn(serve_sequence(
            listener,
            vec![(
                "test/multiply",
                json!({"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"Invalid params","data":{"field":"value"}}}),
            )],
        ));
        let response = BrpExecute
            .handle_impl(ExecuteParams {
                method: "test/multiply".to_string(),
                params: Some(json!({"value":"bad"})),
                port,
            })
            .await;
        let error = match response {
            Ok(_) => return Err("invalid params must remain an execution error".into()),
            Err(error) => error,
        };
        server.await??;
        let Error::ToolCall {
            details: Some(details),
            ..
        } = error.current_context()
        else {
            return Err("expected structured tool error".into());
        };
        assert_eq!(details["stage"], "execution");
        assert_eq!(details["code"], -32602);
        assert_eq!(details["data"]["field"], "value");
        Ok(())
    }

    #[tokio::test]
    async fn method_registered_after_not_found_remains_an_execution_error()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = Port::try_from(listener.local_addr()?.port())?;
        let server = tokio::spawn(serve_sequence(
            listener,
            vec![
                (
                    "test/race",
                    json!({"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found","data":{"generation":1}}}),
                ),
                (
                    "rpc.discover",
                    discovery_response(&["rpc.discover", "test/race"]),
                ),
            ],
        ));
        let response = BrpExecute
            .handle_impl(ExecuteParams {
                method: "test/race".to_string(),
                params: None,
                port,
            })
            .await;
        let error = match response {
            Ok(_) => return Err("the original call failed and must not be retried".into()),
            Err(error) => error,
        };
        server.await??;
        let Error::ToolCall {
            details: Some(details),
            ..
        } = error.current_context()
        else {
            return Err("expected structured tool error".into());
        };
        assert_eq!(details["stage"], "execution");
        assert_eq!(details["code"], -32601);
        assert_eq!(details["data"]["generation"], 1);
        Ok(())
    }

    #[tokio::test]
    async fn failed_fallback_preserves_the_original_brp_error()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = Port::try_from(listener.local_addr()?.port())?;
        let server = tokio::spawn(serve_sequence(
            listener,
            vec![
                (
                    "test/missing",
                    json!({"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found","data":{"request_id":7}}}),
                ),
                (
                    "rpc.discover",
                    json!({"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"Discovery failed","data":{"reason":"busy"}}}),
                ),
            ],
        ));
        let response = BrpExecute
            .handle_impl(ExecuteParams {
                method: "test/missing".to_string(),
                params: None,
                port,
            })
            .await;
        let error = match response {
            Ok(_) => return Err("both requests failed".into()),
            Err(error) => error,
        };
        server.await??;
        let Error::ToolCall {
            details: Some(details),
            ..
        } = error.current_context()
        else {
            return Err("expected structured tool error".into());
        };
        assert_eq!(details["stage"], "execution");
        assert_eq!(details["method"], "test/missing");
        assert_eq!(details["code"], -32601);
        assert_eq!(details["data"]["request_id"], 7);
        assert_eq!(details["discovery_error"]["details"]["code"], -32603);
        assert_eq!(
            details["discovery_error"]["details"]["data"]["reason"],
            "busy"
        );
        Ok(())
    }

    #[tokio::test]
    async fn registered_watch_is_rejected_after_discovery_without_opening_a_stream()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = Port::try_from(listener.local_addr()?.port())?;
        let server = tokio::spawn(serve_sequence(
            listener,
            vec![(
                "rpc.discover",
                discovery_response(&["rpc.discover", "world.list_components+watch"]),
            )],
        ));
        let response = BrpExecute
            .handle_impl(ExecuteParams {
                method: "world.list_components+watch".to_string(),
                params: None,
                port,
            })
            .await;
        let error = match response {
            Ok(_) => return Err("watch must be rejected".into()),
            Err(error) => error,
        };
        server.await??;
        let Error::ToolCall {
            details: Some(details),
            ..
        } = error.current_context()
        else {
            return Err("expected structured tool error".into());
        };
        assert_eq!(details["stage"], "unsupported_call_mode");
        assert_eq!(details["method"], "world.list_components+watch");
        Ok(())
    }

    #[tokio::test]
    async fn unknown_watch_keeps_discovery_error() -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = Port::try_from(listener.local_addr()?.port())?;
        let server = tokio::spawn(serve_sequence(
            listener,
            vec![("rpc.discover", discovery_response(&["rpc.discover"]))],
        ));
        let response = BrpExecute
            .handle_impl(ExecuteParams {
                method: "test/missing+watch".to_string(),
                params: None,
                port,
            })
            .await;
        let error = match response {
            Ok(_) => return Err("unknown watch must report discovery".into()),
            Err(error) => error,
        };
        server.await??;
        let Error::ToolCall {
            details: Some(details),
            ..
        } = error.current_context()
        else {
            return Err("expected structured tool error".into());
        };
        assert_eq!(details["stage"], "discovery");
        assert_eq!(details["method"], "test/missing+watch");
        Ok(())
    }

    #[tokio::test]
    async fn transport_failure_does_not_attempt_discovery() -> Result<(), Box<dyn std::error::Error>>
    {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = Port::try_from(listener.local_addr()?.port())?;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            assert_eq!(read_method(&mut stream).await?, "test/multiply");
            drop(stream);
            Ok::<_, io::Error>(())
        });
        let response = BrpExecute
            .handle_impl(ExecuteParams {
                method: "test/multiply".to_string(),
                params: None,
                port,
            })
            .await;
        let error = match response {
            Ok(_) => return Err("closed connection must fail".into()),
            Err(error) => error,
        };
        server.await??;
        let Error::ToolCall {
            details: Some(details),
            ..
        } = error.current_context()
        else {
            return Err("expected structured tool error".into());
        };
        assert_eq!(details["stage"], "transport");
        assert_eq!(details["method"], "test/multiply");
        Ok(())
    }
}
