use std::fs;
use std::path::Path;
use std::time::Duration;
use std::time::SystemTime;

use bevy_brp_mcp_macros::ParamStruct;
use bevy_brp_mcp_macros::ResultStruct;
use bevy_brp_mcp_macros::ToolFn;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use super::support;
use super::support::LogFileEntry;
use super::support::LogSource;
use super::support::LogSourceFilter;
use crate::error::Error;
use crate::error::Result;
use crate::tool::HandlerContext;
use crate::tool::HandlerResult;
use crate::tool::ToolFn;
use crate::tool::ToolResult;

#[derive(Clone, Deserialize, Serialize, JsonSchema, ParamStruct)]
pub struct DeleteLogsParams {
    /// Optional filter to delete logs for a specific app only
    #[to_metadata(skip_if_none)]
    pub app_name: Option<String>,
    /// Optional filter to delete logs older than N seconds
    #[to_metadata(skip_if_none)]
    pub older_than_seconds: Option<u32>,
    /// File source: all (default), app, or watch. `app_name` cannot be combined with watch.
    pub source: Option<LogSourceFilter>,
}

/// Result from cleaning up log files
#[derive(Debug, Clone, Serialize, Deserialize, ResultStruct)]
pub struct DeleteLogsResult {
    /// List of deleted filenames
    #[serde(rename = "deleted_files")]
    #[to_metadata]
    files: Vec<String>,
    /// Number of files deleted
    #[serde(rename = "deleted_count")]
    #[to_metadata]
    count: usize,
    /// App name filter that was applied (if any)
    #[to_metadata(skip_if_none)]
    app_name_filter: Option<String>,
    /// Age filter in seconds that was applied (if any)
    #[to_metadata(skip_if_none)]
    older_than_seconds: Option<u32>,
    /// Message template for formatting responses
    #[to_message(message_template = "Deleted {deleted_count} log files")]
    message_template: String,
}

#[derive(ToolFn)]
#[tool_fn(params = "DeleteLogsParams", output = "DeleteLogsResult")]
pub struct DeleteLogs;

#[allow(
    clippy::unused_async,
    reason = "ToolFn trait requires async handler signature"
)]
async fn handle_impl(params: DeleteLogsParams) -> Result<DeleteLogsResult> {
    let source = params.source.unwrap_or_default();
    if params.app_name.is_some() && source == LogSourceFilter::Watch {
        return Err(Error::invalid("source", "watch cannot be combined with app_name").into());
    }
    let files = delete_log_files(
        params.app_name.as_deref(),
        params.older_than_seconds,
        source,
    )?;

    Ok(DeleteLogsResult::new(
        files.clone(),
        files.len(),
        params.app_name.clone(),
        params.older_than_seconds,
    ))
}

fn delete_log_files(
    app_name_filter: Option<&str>,
    older_than_seconds: Option<u32>,
    source: LogSourceFilter,
) -> Result<Vec<String>> {
    delete_log_files_in(
        &support::get_log_directory(),
        app_name_filter,
        older_than_seconds,
        source,
    )
}

fn delete_log_files_in(
    directory: &Path,
    app_name_filter: Option<&str>,
    older_than_seconds: Option<u32>,
    source: LogSourceFilter,
) -> Result<Vec<String>> {
    let mut files = Vec::new();

    // Calculate cutoff time if age filter is specified
    let cutoff_time = older_than_seconds
        .map(|seconds| SystemTime::now() - Duration::from_secs(u64::from(seconds)));

    // Use the iterator to get all log files with filters
    let filter = |entry: &LogFileEntry| -> bool {
        // Apply app name filter
        if !source.includes(entry.source) {
            return false;
        }
        if let Some(app_filter) = app_name_filter
            && (entry.source != LogSource::App || entry.app_name != app_filter)
        {
            return false;
        }

        // Apply age filter if provided
        if let Some(cutoff) = cutoff_time
            && let Ok(modified) = entry.metadata.modified()
        {
            // Skip if file is newer than cutoff
            if modified > cutoff {
                return false;
            }
        }

        true
    };

    let log_entries = support::iterate_log_files_in(directory, filter)
        .map_err(|e| Error::tool_call_failed(e.to_string()))?;

    // Delete the files
    for entry in log_entries {
        fs::remove_file(&entry.path).map_err(|error| {
            Error::tool_call_failed_with_details(
                format!(
                    "Failed to delete log file {}: {error}",
                    entry.path.display()
                ),
                serde_json::json!({
                    "filename": entry.filename,
                    "deleted_files": files,
                }),
            )
        })?;
        files.push(entry.filename);
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::DeleteLogsParams;
    use super::LogSourceFilter;
    use super::delete_log_files_in;
    use super::handle_impl;

    #[test]
    fn deletes_only_selected_app_and_watch_logs_and_never_trace() {
        let dir = tempdir().expect("temp directory");
        let app = "bevy_brp_mcp_test_app_port15702_123.log";
        let watch = "bevy_brp_mcp_watch_1_get_42_123.log";
        let trace = "bevy_brp_mcp_trace.log";
        for filename in [app, watch, trace] {
            fs::write(dir.path().join(filename), filename).expect("test log");
        }

        let deleted = delete_log_files_in(dir.path(), None, None, LogSourceFilter::Watch)
            .expect("delete watch log");
        assert_eq!(deleted, [watch]);
        assert!(dir.path().join(app).exists());
        assert!(dir.path().join(trace).exists());

        let deleted = delete_log_files_in(dir.path(), None, None, LogSourceFilter::All)
            .expect("delete remaining app log");
        assert_eq!(deleted, [app]);
        assert!(dir.path().join(trace).exists());
    }

    #[test]
    fn deletion_failure_names_the_file() {
        let dir = tempdir().expect("temp directory");
        let filename = "bevy_brp_mcp_test_app_port15702_123.log";
        fs::create_dir(dir.path().join(filename)).expect("directory fixture");

        let error = delete_log_files_in(dir.path(), None, None, LogSourceFilter::App)
            .expect_err("directory cannot be removed as a file");
        let crate::error::Error::ToolCall { details, .. } = error.current_context() else {
            panic!("expected structured tool error");
        };
        assert_eq!(
            details
                .as_ref()
                .and_then(|value| value["filename"].as_str()),
            Some(filename)
        );
    }

    #[tokio::test]
    async fn rejects_watch_source_with_app_name() {
        let result = handle_impl(DeleteLogsParams {
            app_name: Some(String::from("test_app")),
            older_than_seconds: None,
            source: Some(LogSourceFilter::Watch),
        })
        .await;
        assert!(result.is_err());
    }
}
