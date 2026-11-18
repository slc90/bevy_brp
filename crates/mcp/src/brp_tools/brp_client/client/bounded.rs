use std::time::Duration;

use serde_json::Value;

use super::BrpClient;
use super::BrpClientCallJsonResponse;
use super::BrpClientError;
use super::BrpHttpClient;
use crate::brp_tools::brp_client::constants::JSONRPC_DEFAULT_ID;
use crate::brp_tools::brp_client::constants::JSONRPC_VERSION;

pub(crate) enum BoundedBrpError {
    Transport(reqwest::Error),
    HttpStatus(u16),
    ResponseTooLarge,
    InvalidResponse(&'static str),
    Remote(BrpClientError),
}

impl BrpClient {
    pub(crate) async fn execute_local_bounded(
        &self,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<Value, BoundedBrpError> {
        let client = BrpHttpClient::new(self.brp_method.as_str(), self.port, self.params.clone());
        let mut response = client
            .send_local_request(timeout)
            .await
            .map_err(BoundedBrpError::Transport)?;
        if !response.status().is_success() {
            return Err(BoundedBrpError::HttpStatus(response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > max_bytes as u64)
        {
            return Err(BoundedBrpError::ResponseTooLarge);
        }

        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(BoundedBrpError::Transport)? {
            if chunk.len() > max_bytes - body.len() {
                return Err(BoundedBrpError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }

        let envelope = serde_json::from_slice::<Value>(&body)
            .map_err(|_| BoundedBrpError::InvalidResponse("Invalid JSON-RPC response body"))?;
        if envelope.get("result").is_some() == envelope.get("error").is_some() {
            return Err(BoundedBrpError::InvalidResponse(
                "Expected exactly one JSON-RPC result or error",
            ));
        }
        let reply = serde_json::from_value::<BrpClientCallJsonResponse>(envelope)
            .map_err(|_| BoundedBrpError::InvalidResponse("Invalid JSON-RPC response body"))?;
        if reply.jsonrpc != JSONRPC_VERSION || reply.id != JSONRPC_DEFAULT_ID {
            return Err(BoundedBrpError::InvalidResponse(
                "JSON-RPC version or request ID does not match",
            ));
        }
        match (reply.result, reply.error) {
            (Some(result), None) => Ok(result),
            (None, Some(error)) => Err(BoundedBrpError::Remote(BrpClientError {
                code: error.code,
                message: error.message,
                data: error.data,
            })),
            _ => Err(BoundedBrpError::InvalidResponse(
                "JSON-RPC result or error was null",
            )),
        }
    }
}
