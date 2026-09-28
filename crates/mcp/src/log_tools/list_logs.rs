use bevy_brp_mcp_macros::ParamStruct;
use bevy_brp_mcp_macros::ResultStruct;
use bevy_brp_mcp_macros::ToolFn;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;

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
pub struct ListLogsParams {
    /// Optional filter to list logs for a specific app only
    #[to_metadata(skip_if_none)]
    pub app_name: Option<String>,
    /// Include full details (path, timestamps, size in bytes). Default is false for minimal output
    #[to_metadata(skip_if_none)]
    pub verbose: Option<bool>,
    /// File source: all (default), app, or watch. `app_name` cannot be combined with watch.
    pub source: Option<LogSourceFilter>,
}

/// Result from listing log files
#[derive(Debug, Clone, Serialize, Deserialize, ResultStruct)]
pub struct ListLogResult {
    /// List of log files found
    #[to_result]
    logs: Vec<LogFileInfo>,
    /// Path to the temp directory containing logs
    #[to_metadata]
    temp_directory: String,
    /// Log file count
    #[to_metadata]
    log_count: usize,
    /// Message template for formatting responses
    #[to_message(message_template = "Found {log_count} log files")]
    message_template: String,
}

#[derive(ToolFn)]
#[tool_fn(params = "ListLogsParams", output = "ListLogResult")]
pub struct ListLogs;

#[derive(Clone, Copy)]
enum LogDetail {
    Minimal,
    Verbose,
}

impl LogDetail {
    const fn is_verbose(self) -> bool {
        matches!(self, Self::Verbose)
    }
}

impl From<bool> for LogDetail {
    fn from(value: bool) -> Self {
        if value { Self::Verbose } else { Self::Minimal }
    }
}

/// Individual log file entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogFileInfo {
    /// The filename
    filename: String,
    /// The app name extracted from the filename
    app_name: String,
    source: LogSource,
    /// Full path to the file (included in verbose mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    /// Human-readable file size (included in verbose mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<String>,
    /// File size in bytes (included in verbose mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
    /// Creation time as ISO string (included in verbose mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    created: Option<String>,
    /// Modification time as ISO string (included in verbose mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    modified: Option<String>,
}

#[allow(
    clippy::unused_async,
    reason = "ToolFn trait requires async handler signature"
)]
async fn handle_impl(params: ListLogsParams) -> Result<ListLogResult> {
    let source = params.source.unwrap_or_default();
    if params.app_name.is_some() && source == LogSourceFilter::Watch {
        return Err(Error::invalid("source", "watch cannot be combined with app_name").into());
    }
    let logs = list_log_files(
        params.app_name.as_deref(),
        LogDetail::from(params.verbose.unwrap_or(false)),
        source,
    )?;
    Ok(ListLogResult::new(
        logs.clone(),
        support::get_log_directory().display().to_string(),
        logs.len(),
    ))
}

fn list_log_files(
    app_name_filter: Option<&str>,
    log_detail: LogDetail,
    source: LogSourceFilter,
) -> Result<Vec<LogFileInfo>> {
    list_log_files_in(
        &support::get_log_directory(),
        app_name_filter,
        log_detail,
        source,
    )
}

fn list_log_files_in(
    directory: &Path,
    app_name_filter: Option<&str>,
    log_detail: LogDetail,
    source: LogSourceFilter,
) -> Result<Vec<LogFileInfo>> {
    // Use the iterator to get all log files with optional filter
    let filter = |entry: &LogFileEntry| -> bool {
        // Apply app name filter if provided
        source.includes(entry.source)
            && app_name_filter.is_none_or(|app_filter| {
                entry.source == LogSource::App && entry.app_name == app_filter
            })
    };

    let mut log_entries = support::iterate_log_files_in(directory, filter)
        .map_err(|e| Error::tool_call_failed(e.to_string()))?;

    // Sort by timestamp (newest first)
    log_entries.sort_by(|a, b| {
        let timestamp_a = a.timestamp.parse::<u128>().unwrap_or(0);
        let timestamp_b = b.timestamp.parse::<u128>().unwrap_or(0);
        timestamp_b.cmp(&timestamp_a)
    });

    // Convert to LogFileInfo structs
    let log_infos: Vec<LogFileInfo> = log_entries
        .into_iter()
        .map(|entry| {
            if log_detail.is_verbose() {
                let size_bytes = entry.metadata.len();
                let modified = entry.metadata.modified().ok().map(|t| {
                    chrono::DateTime::<chrono::Local>::from(t)
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                });
                let created = entry.metadata.created().ok().map(|t| {
                    chrono::DateTime::<chrono::Local>::from(t)
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                });

                LogFileInfo {
                    filename: entry.filename,
                    app_name: entry.app_name,
                    source: entry.source,
                    path: Some(entry.path.display().to_string()),
                    size: Some(support::format_bytes(size_bytes)),
                    size_bytes: Some(size_bytes),
                    created,
                    modified,
                }
            } else {
                LogFileInfo {
                    filename: entry.filename,
                    app_name: entry.app_name,
                    source: entry.source,
                    path: None,
                    size: None,
                    size_bytes: None,
                    created: None,
                    modified: None,
                }
            }
        })
        .collect();

    Ok(log_infos)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::ListLogsParams;
    use super::LogDetail;
    use super::LogSource;
    use super::LogSourceFilter;
    use super::handle_impl;
    use super::list_log_files_in;

    #[test]
    fn lists_app_and_watch_sources_without_server_trace() {
        let dir = tempdir().expect("temp directory");
        for filename in [
            "bevy_brp_mcp_test_app_port15702_123.log",
            "bevy_brp_mcp_watch_1_get_42_123.log",
            "bevy_brp_mcp_trace.log",
        ] {
            fs::write(dir.path().join(filename), filename).expect("test log");
        }

        let all = list_log_files_in(dir.path(), None, LogDetail::Minimal, LogSourceFilter::All)
            .expect("list logs");
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|entry| entry.source == LogSource::App));
        assert!(all.iter().any(|entry| entry.source == LogSource::Watch));

        let filtered = list_log_files_in(
            dir.path(),
            Some("test_app"),
            LogDetail::Minimal,
            LogSourceFilter::All,
        )
        .expect("list app logs");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].source, LogSource::App);
        let serialized = serde_json::to_value(&all).expect("serialize list");
        assert!(
            serialized
                .as_array()
                .expect("array")
                .iter()
                .all(|entry| { matches!(entry["source"].as_str(), Some("app" | "watch")) })
        );
    }

    #[tokio::test]
    async fn rejects_watch_source_with_app_name() {
        let result = handle_impl(ListLogsParams {
            app_name: Some(String::from("test_app")),
            verbose: None,
            source: Some(LogSourceFilter::Watch),
        })
        .await;
        assert!(result.is_err());
    }
}
