//! BRP (Bevy Remote Protocol) client execution paths.
//!
//! `execute` converts a response to a typed result and can attach a type guide to
//! format errors. `execute_raw` returns the BRP response status, and
//! `execute_streaming` returns a streaming HTTP response for watch operations.

use reqwest::Response;
use reqwest::header::CONTENT_TYPE;
use serde_json::Value;
use std::error::Error as StdError;
use tracing::warn;

use super::constants::BRP_EXTRAS_PREFIX;
use super::constants::ERROR_PATTERNS;
use super::constants::FORMAT_ERROR_HELP_FIELD;
use super::constants::FORMAT_ERROR_HELP_MESSAGE;
use super::constants::FORMAT_ERROR_ORIGINAL_ERROR_FIELD;
use super::constants::FORMAT_ERROR_SUGGESTED_ACTION;
use super::constants::FORMAT_ERROR_SUGGESTED_ACTION_FIELD;
use super::constants::FORMAT_ERROR_TYPE_GUIDE_FIELD;
use super::constants::JSON_RPC_ERROR_METHOD_NOT_FOUND;
use super::http_client::BrpHttpClient;
use super::operation::Operation;
use super::response_handling::BrpClientCallJsonResponse;
use super::response_handling::BrpClientError;
use super::response_handling::BrpToolConfig;
use super::response_handling::FormatCorrectionStatus;
use super::response_handling::ResponseStatus;
use super::response_handling::ResultStructBrpExt;
use crate::brp_tools::Port;
use crate::brp_tools::brp_type_guide;
use crate::error::Error;
use crate::error::Result;
use crate::tool::BrpMethod;
use crate::tool::ParameterName;

/// Client for executing a BRP operation
pub struct BrpClient {
    brp_method: BrpMethodName,
    port: Port,
    params: Option<Value>,
}

enum BrpMethodName {
    Known(BrpMethod),
    Application(String),
}

impl BrpMethodName {
    const fn as_str(&self) -> &str {
        match self {
            Self::Known(method) => method.as_str(),
            Self::Application(method) => method.as_str(),
        }
    }

    const fn known(&self) -> Option<BrpMethod> {
        match self {
            Self::Known(method) => Some(*method),
            Self::Application(_) => None,
        }
    }
}

impl BrpClient {
    /// Create a new BRP client for the given method, port, and parameters
    pub const fn new(brp_method: BrpMethod, port: Port, params: Option<Value>) -> Self {
        Self {
            brp_method: BrpMethodName::Known(brp_method),
            port,
            params,
        }
    }

    /// Create a BRP client for an application-defined method.
    pub const fn for_application(brp_method: String, port: Port, params: Option<Value>) -> Self {
        Self {
            brp_method: BrpMethodName::Application(brp_method),
            port,
            params,
        }
    }

    /// Execute a BRP request and convert its response to a typed result.
    ///
    /// For supported result types, format errors include a type guide when one can
    /// be fetched. The original request is never retried.
    pub async fn execute<R>(&self) -> Result<R>
    where
        R: ResultStructBrpExt<
                Args = (
                    Option<Value>,
                    Option<Vec<Value>>,
                    Option<FormatCorrectionStatus>,
                ),
            > + BrpToolConfig
            + Send
            + 'static,
    {
        // ALWAYS execute direct first
        let direct_result = self.execute_direct_internal(false).await?;

        match direct_result {
            ResponseStatus::Success(data) => {
                // Success - no format discovery needed
                R::from_brp_client_response((
                    data,
                    None,
                    Some(FormatCorrectionStatus::NotAttempted),
                ))
            }
            ResponseStatus::Error(err) => {
                // Check if this result type supports adding the `TypeGuide`
                if R::ADD_TYPE_GUIDE_TO_ERROR && err.has_format_error_code() {
                    // embed type_guide information
                    Err(self.format_type_error(&err).await)
                } else {
                    // Regular error - enhance with context if possible
                    let enhanced_message =
                        self.enhance_error_message(err.get_message(), err.get_code());
                    Err(Error::tool_call_failed_with_details(
                        enhanced_message,
                        self.brp_error_details(&err),
                    )
                    .into())
                }
            }
        }
    }

    /// Low-level BRP execution without format discovery or result transformation
    ///
    /// This method provides direct access to BRP communication without any automatic
    /// format discovery or result type conversion. It returns raw `ResponseStatus`
    /// which can be either `Success(Option<Value>)` or `Error(BrpClientError)`.
    ///
    /// Primary use cases:
    /// - Debugging tools that need raw BRP responses (`brp_execute`)
    /// - Format discovery engine internal operations
    /// - Testing and diagnostic scenarios
    pub async fn execute_raw(&self) -> Result<ResponseStatus> {
        self.execute_direct_internal(false).await
    }

    /// Execute a one-shot method, rejecting an SSE response before reading its body.
    pub async fn execute_raw_once(&self) -> Result<ResponseStatus> {
        // Both the upstream and wake-aware HTTP transports select SSE from this marker.
        if self.brp_method.as_str().contains("+watch") {
            return Err(self.unsupported_watch_call());
        }
        self.execute_direct_internal(true).await
    }

    /// Execute the BRP request and return a streaming response
    ///
    /// This method is designed for watch operations that need to handle
    /// Server-Sent Events (SSE) streams. Unlike `execute()`, it:
    /// - Uses no timeout (streaming connections stay open)
    /// - Returns the raw response for the caller to process
    /// - Provides the same rich error context as other `BrpClient` methods
    pub async fn execute_streaming(&self) -> Result<Response> {
        // Pass `brp_method`, `port`, and cloned `params` to `BrpHttpClient::new`.
        let brp_http_client =
            BrpHttpClient::new(self.brp_method.as_str(), self.port, self.params.clone());

        // Send HTTP request using streaming version (no timeout, includes status check)
        let response = brp_http_client.send_streaming_request().await?;

        Ok(response)
    }

    /// Internal direct execution - does the actual http call - we wanted the internal version so we
    /// can distinguish a canned call generated for a `ToolFn` by our macro, and the `execute_raw()`
    /// version we still allow to be called by bespoke tools like `brp_shutdown` and `brp_status`
    /// and the like.
    async fn execute_direct_internal(&self, require_instant: bool) -> Result<ResponseStatus> {
        // Pass `brp_method`, `port`, and cloned `params` to `BrpHttpClient::new`.
        let brp_http_client =
            BrpHttpClient::new(self.brp_method.as_str(), self.port, self.params.clone());

        // Send HTTP request (includes status check)
        let response = brp_http_client.send_request().await.map_err(|error| {
            Error::tool_call_failed_with_details(
                format!(
                    "BRP transport failed for {} on port {}",
                    self.brp_method.as_str(),
                    self.port
                ),
                serde_json::json!({
                    "stage": "transport",
                    "method": self.brp_method.as_str(),
                    "port": self.port,
                    "reason": error.current_context().to_string(),
                }),
            )
        })?;

        if require_instant
            && is_event_stream(
                response
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok()),
            )
        {
            return Err(self.unsupported_watch_call());
        }

        // Parse JSON-RPC response
        let brp_response = self.parse_json_response(response).await.map_err(|error| {
            Error::tool_call_failed_with_details(
                format!(
                    "Invalid BRP response for {} on port {}",
                    self.brp_method.as_str(),
                    self.port
                ),
                serde_json::json!({
                    "stage": "response_decode",
                    "method": self.brp_method.as_str(),
                    "port": self.port,
                    "reason": error.current_context().to_string(),
                }),
            )
        })?;

        // `to_response_status` returns `ResponseStatus`, adding plugin guidance for
        // missing `bevy_brp_extras` methods.
        Ok(self.to_response_status(brp_response))
    }

    /// Parse the JSON response from the BRP call to a running bevy app
    async fn parse_json_response(&self, response: Response) -> Result<BrpClientCallJsonResponse> {
        match response.json().await {
            Ok(json_response) => Ok(json_response),
            Err(e) => {
                warn!("BRP execute_brp_method: JSON parsing failed - error={e}");
                let mut cause = e.source();
                let mut causes = Vec::new();
                while let Some(source) = cause {
                    causes.push(source.to_string());
                    cause = source.source();
                }
                Err(error_stack::Report::new(Error::JsonRpc(format!(
                    "JSON parsing failed: {e}; causes: {}",
                    causes.join(": ")
                )))
                .attach("Failed to parse BRP response JSON")
                .attach(format!(
                    "Method: {}, Port: {}",
                    self.brp_method.as_str(),
                    self.port
                ))
                .attach(format!("Error: {e}")))
            }
        }
    }

    /// Extract type names from BRP error messages using regex patterns
    fn extract_types_from_error_message(error_message: &str) -> Vec<String> {
        ERROR_PATTERNS
            .iter()
            .filter_map(|pattern| {
                regex::Regex::new(pattern)
                    .ok()
                    .and_then(|regex| regex.captures(error_message))
                    .and_then(|caps| caps.get(1))
                    .map(|m| (*m.as_str()).to_string())
            })
            .collect()
    }

    /// Enhance error messages with additional context when available
    ///
    /// Currently enhances:
    /// - `Entity` deserialization errors: Adds `Entity` IDs from parameters
    fn enhance_error_message(&self, original_message: &str, error_code: i32) -> String {
        // Check for entity deserialization errors
        if original_message.contains("Attempting to deserialize an invalid entity") {
            // Try to extract entity ID from parameters
            if let Some(params) = &self.params
                && let Some(entity_id) = params.get(ParameterName::Entity.as_ref())
            {
                return format!(
                    "Entity {entity_id} is not valid: {original_message} (error {error_code})"
                );
            }
        }

        // Default: return original message with error code
        format!("{original_message} (error {error_code})")
    }

    /// Enhanced format error creation with type guide embedding
    async fn format_type_error(&self, error: &BrpClientError) -> error_stack::Report<Error> {
        // Step 1: Try parameter-based extraction using Operation enum
        let mut extracted_types = self
            .brp_method
            .known()
            .and_then(|method| Operation::try_from(method).ok())
            .map_or_else(Vec::new, |operation| {
                let params = self.params.as_ref().unwrap_or(&Value::Null);
                operation.extract_type_names(params)
            });

        // Step 2: Fallback to error message parsing if parameter extraction failed
        if extracted_types.is_empty() {
            extracted_types = Self::extract_types_from_error_message(error.get_message());
        }

        // Step 3: Handle results based on whether types were extracted
        if extracted_types.is_empty() {
            self.create_minimal_type_error(error)
        } else {
            self.add_type_guide_to_error(error, extracted_types).await
        }
    }

    /// Create minimal error when no types can be extracted
    fn create_minimal_type_error(&self, error: &BrpClientError) -> error_stack::Report<Error> {
        Error::tool_call_failed_with_details(
            "Format error occurred but could not extract type information",
            serde_json::json!({
                FORMAT_ERROR_ORIGINAL_ERROR_FIELD: error.get_message(),
                "stage": "execution",
                "method": self.brp_method.as_str(),
                "port": self.port,
                "code": error.code,
                "data": error.data,
                FORMAT_ERROR_TYPE_GUIDE_FIELD: {
                    FORMAT_ERROR_HELP_FIELD: FORMAT_ERROR_HELP_MESSAGE,
                    FORMAT_ERROR_SUGGESTED_ACTION_FIELD: FORMAT_ERROR_SUGGESTED_ACTION
                }
            }),
        )
        .into()
    }

    /// Create full error with type guide embedded for extracted types
    async fn add_type_guide_to_error(
        &self,
        error: &BrpClientError,
        extracted_types: Vec<String>,
    ) -> error_stack::Report<Error> {
        match brp_type_guide::generate_type_guide_response(self.port, &extracted_types).await {
            Ok(type_guide_response) => Error::tool_call_failed_with_details(
                "Format error - see 'type_guide' field for correct format",
                serde_json::json!({
                    FORMAT_ERROR_ORIGINAL_ERROR_FIELD: error.get_message(),
                    "stage": "execution",
                    "method": self.brp_method.as_str(),
                    "port": self.port,
                    "code": error.code,
                    "data": error.data,
                    FORMAT_ERROR_TYPE_GUIDE_FIELD: type_guide_response
                }),
            )
            .into(),
            Err(guide_error) => Error::tool_call_failed_with_details(
                format!(
                    "BRP method {} failed: {}",
                    self.brp_method.as_str(),
                    error.message
                ),
                serde_json::json!({
                    "stage": "execution",
                    "method": self.brp_method.as_str(),
                    "port": self.port,
                    "code": error.code,
                    "data": error.data,
                    "type_guide_error": guide_error.current_context().to_string(),
                }),
            )
            .into(),
        }
    }

    fn brp_error_details(&self, error: &BrpClientError) -> Value {
        serde_json::json!({
            "stage": "execution",
            "method": self.brp_method.as_str(),
            "port": self.port,
            "code": error.code,
            "data": error.data,
        })
    }

    fn unsupported_watch_call(&self) -> error_stack::Report<Error> {
        Error::tool_call_failed_with_details(
            format!(
                "BRP method `{}` uses a watch stream and cannot be called with brp_execute",
                self.brp_method.as_str()
            ),
            serde_json::json!({
                "stage": "unsupported_call_mode",
                "method": self.brp_method.as_str(),
                "port": self.port,
                "reason": "watching_method",
                "suggestion": "Use world_get_components_watch or world_list_components_watch when applicable; other watching methods have no MCP watch entry",
            }),
        )
        .into()
    }

    /// Convert the response JSON to a `ResponseStatus`
    fn to_response_status(&self, brp_response_json: BrpClientCallJsonResponse) -> ResponseStatus {
        if let Some(error) = brp_response_json.error {
            warn!(
                "BRP execute_brp_method: BRP returned error - code={}, message={}",
                error.code, error.message
            );

            // Check if this is a bevy_brp_extras method that's not found
            let enhanced_message = if error.code == JSON_RPC_ERROR_METHOD_NOT_FOUND {
                method_not_found_message(self.brp_method.as_str(), &error.message)
            } else {
                error.message
            };

            ResponseStatus::Error(BrpClientError {
                code: error.code,
                message: enhanced_message,
                data: error.data,
            })
        } else {
            ResponseStatus::Success(brp_response_json.result)
        }
    }
}

fn is_event_stream(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/event-stream"))
    })
}

pub(crate) fn method_not_found_message(method: &str, message: &str) -> String {
    if method.starts_with(BRP_EXTRAS_PREFIX) {
        format!(
            "{message}. This method requires the bevy_brp_extras crate to be added to your Bevy app with the BrpExtrasPlugin"
        )
    } else {
        message.to_string()
    }
}

#[cfg(test)]
mod public_contract_tests {
    use serde_json::json;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    use super::BrpClient;
    use super::BrpClientError;
    use super::is_event_stream;
    use crate::brp_tools::Port;

    #[test]
    fn event_stream_is_not_an_instant_result() {
        assert!(is_event_stream(Some("text/event-stream; charset=utf-8")));
        assert!(!is_event_stream(Some("application/json")));
        assert!(!is_event_stream(None));
    }

    #[test]
    fn typed_brp_error_keeps_method_port_code_and_data() {
        let client = BrpClient::for_application("test/multiply".to_string(), Port(15_712), None);
        let error = BrpClientError {
            code: -32602,
            message: "invalid value".to_string(),
            data: Some(json!({"field":"value"})),
        };
        assert_eq!(
            client.brp_error_details(&error),
            json!({
                "stage":"execution", "method":"test/multiply", "port":15_712,
                "code":-32602, "data":{"field":"value"}
            })
        );
    }

    #[tokio::test]
    async fn named_watch_is_rejected_before_transport() {
        let client = BrpClient::for_application(
            "world.list_components+watch".to_string(),
            Port(15_712),
            None,
        );
        let result = client.execute_raw_once().await;
        assert!(matches!(
            result.as_ref().err().map(|report| report.current_context()),
            Some(crate::error::Error::ToolCall { details: Some(details), .. })
                if details["stage"] == "unsupported_call_mode"
        ));
    }

    #[tokio::test]
    async fn disconnected_transport_keeps_method_port_and_stage()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = Port::try_from(listener.local_addr()?.port())?;
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await?;
            drop(stream);
            Ok::<_, std::io::Error>(())
        });

        let client = BrpClient::for_application("test/multiply".to_string(), port, None);
        let error = client
            .execute_raw_once()
            .await
            .expect_err("connection closed without response");
        server.await??;
        let crate::error::Error::ToolCall {
            details: Some(details),
            ..
        } = error.current_context()
        else {
            return Err("expected a structured transport error".into());
        };
        assert_eq!(details["stage"], "transport");
        assert_eq!(details["method"], "test/multiply");
        assert_eq!(details["port"], port.0);
        Ok(())
    }

    #[tokio::test]
    async fn malformed_json_response_keeps_method_port_and_stage()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = Port::try_from(listener.local_addr()?.port())?;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let mut request = [0_u8; 4096];
            let _read = stream.read(&mut request).await?;
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 8\r\n\r\nnot-json").await?;
            stream.flush().await?;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            Ok::<_, std::io::Error>(())
        });

        let client = BrpClient::for_application("test/multiply".to_string(), port, None);
        let error = client
            .execute_raw_once()
            .await
            .expect_err("malformed JSON-RPC response");
        server.await??;
        let crate::error::Error::ToolCall {
            details: Some(details),
            ..
        } = error.current_context()
        else {
            return Err("expected a structured decode error".into());
        };
        assert_eq!(details["stage"], "response_decode");
        assert!(
            details["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("expected")),
            "{}",
            details["reason"]
        );
        assert_eq!(details["method"], "test/multiply");
        assert_eq!(details["port"], port.0);
        Ok(())
    }

    #[tokio::test]
    async fn http_status_error_names_the_status_code() -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = Port::try_from(listener.local_addr()?.port())?;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let mut request = [0_u8; 4096];
            let _read = stream.read(&mut request).await?;
            stream
                .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n")
                .await?;
            stream.flush().await?;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            Ok::<_, std::io::Error>(())
        });

        let client = BrpClient::for_application("test/multiply".to_string(), port, None);
        let error = client
            .execute_raw_once()
            .await
            .expect_err("HTTP 503 should fail");
        server.await??;
        let crate::error::Error::ToolCall {
            details: Some(details),
            ..
        } = error.current_context()
        else {
            return Err("expected a structured transport error".into());
        };
        assert_eq!(details["stage"], "transport");
        assert!(
            details["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("503"))
        );
        Ok(())
    }
}
