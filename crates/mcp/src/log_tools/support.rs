use std::ffi::OsStr;
use std::fs;
use std::fs::Metadata;
use std::path::Path;
use std::path::PathBuf;
use std::sync::LazyLock;

use error_stack::ResultExt;
use regex::Regex;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use super::constants::BYTES_PER_UNIT;
use super::constants::UNITS;
use crate::error::Error;
use crate::error::Result;

// Static regex for parsing app log filenames
static APP_LOG_REGEX: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^bevy_brp_mcp_([^/\\]+?)_port\d+_(\d+)\.log$").ok());
static WATCH_LOG_REGEX: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^bevy_brp_mcp_watch_\d+_(get|list)_\d+_(\d+)\.log$").ok());

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogSourceFilter {
    #[default]
    All,
    App,
    Watch,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum LogSource {
    App,
    Watch,
}

impl LogSourceFilter {
    pub(super) const fn includes(self, source: LogSource) -> bool {
        matches!(self, Self::All)
            || matches!(
                (self, source),
                (Self::App, LogSource::App) | (Self::Watch, LogSource::Watch)
            )
    }
}

/// Represents a log file entry with metadata
#[derive(Debug, Clone)]
pub(super) struct LogFileEntry {
    pub(super) filename: String,
    pub(super) app_name: String,
    pub(super) timestamp: String,
    pub(super) source: LogSource,
    pub(super) path: PathBuf,
    pub(super) metadata: Metadata,
}

/// Validates if a filename follows the `bevy_brp_mcp` log naming convention
pub(super) fn is_valid_log_filename(filename: &str) -> bool {
    parse_log_filename(filename).is_some()
}

/// Parses app log filename with port pattern into app name and timestamp
/// Returns `Some((app_name, timestamp_str))` if matches app log pattern, `None` otherwise
///
/// Format: `bevy_brp_mcp`_{`app_name`}_port{number}_{timestamp}.log
/// Extracts `app_name` as the part between "`bevy_brp_mcp`_" and "_port{number}"
pub(super) fn parse_app_log_filename(filename: &str) -> Option<(String, String)> {
    // Use the static regex, returning None if regex compilation failed
    let regex = APP_LOG_REGEX.as_ref()?;

    if let Some(captures) = regex.captures(filename) {
        let app_name = captures.get(1)?.as_str().to_string();
        let timestamp = captures.get(2)?.as_str().to_string();
        return Some((app_name, timestamp));
    }

    None
}

/// Parses any log filename into app name and timestamp components
/// Returns `Some((app_name, timestamp_str))` if valid, `None` otherwise
///
/// Tries app log pattern first, falls back to generic pattern for other log types
pub(super) fn parse_log_filename(filename: &str) -> Option<(LogSource, String, String)> {
    // Try app log pattern first
    if let Some(result) = parse_app_log_filename(filename) {
        return Some((LogSource::App, result.0, result.1));
    }

    let captures = WATCH_LOG_REGEX.as_ref()?.captures(filename)?;
    let timestamp = captures.get(2)?.as_str().to_string();
    Some((LogSource::Watch, String::from("watch"), timestamp))
}

/// Formats bytes into human-readable string with appropriate unit
pub(super) fn format_bytes(bytes: u64) -> String {
    #[allow(
        clippy::cast_precision_loss,
        reason = "log file sizes never approach 2^53, where f64 loses integer precision"
    )]
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= BYTES_PER_UNIT && unit_index < UNITS.len() - 1 {
        size /= BYTES_PER_UNIT;
        unit_index += 1;
    }

    let unit = UNITS[unit_index];
    if unit_index == 0 {
        format!("{bytes} {unit}")
    } else {
        format!("{size:.2} {unit}")
    }
}

/// Gets the log directory (system temp directory)
pub(super) fn get_log_directory() -> PathBuf {
    std::env::temp_dir()
}

/// Gets the full path for a log file given its filename
pub(super) fn get_log_file_path(filename: &str) -> PathBuf {
    get_log_directory().join(filename)
}

pub(super) fn iterate_log_files_in<F>(temp_dir: &Path, filter: F) -> Result<Vec<LogFileEntry>>
where
    F: Fn(&LogFileEntry) -> bool,
{
    let mut log_entries = Vec::new();

    // Read the temp directory
    let entries = fs::read_dir(temp_dir)
        .change_context(Error::FileOperation(
            "Failed to read temp directory".to_string(),
        ))
        .attach(format!("Path: {}", temp_dir.display()))?;

    // Process each entry
    for entry in entries {
        let entry = entry
            .change_context(Error::FileOperation(
                "Failed to read directory entry".to_string(),
            ))
            .attach(format!("Directory: {}", temp_dir.display()))?;

        let path = entry.path();
        let filename = path.file_name().and_then(OsStr::to_str).unwrap_or("");

        // Parse the filename
        if let Some((source, app_name, timestamp)) = parse_log_filename(filename) {
            // Get file metadata
            let metadata = entry
                .metadata()
                .change_context(Error::FileOperation(
                    "Failed to get file metadata".to_string(),
                ))
                .attach(format!("Path: {}", path.display()))?;

            let log_entry = LogFileEntry {
                filename: filename.to_string(),
                app_name,
                timestamp,
                source,
                path,
                metadata,
            };

            // Apply filter
            if filter(&log_entry) {
                log_entries.push(log_entry);
            }
        }
    }

    Ok(log_entries)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use super::LogSource;
    use super::parse_app_log_filename;
    use super::parse_log_filename;

    /// Mirrors the filename built in `app_tools::launch::logging`. Both sides must agree,
    /// or the `app_name` filters in `list_logs` and `delete_logs` silently match nothing.
    fn app_log_filename(name: &str, port: u16, timestamp: u128) -> String {
        format!("bevy_brp_mcp_{name}_port{port}_{timestamp}.log")
    }

    #[test]
    fn parses_app_name_written_by_launch() {
        let filename = app_log_filename("test_app", 20202, 1_787_840_000_123);

        let (app_name, timestamp) =
            parse_app_log_filename(&filename).expect("launch log filename must parse");

        assert_eq!(app_name, "test_app");
        assert_eq!(timestamp, "1787840000123");
    }

    #[test]
    fn keeps_port_out_of_app_name_for_underscored_names() {
        let filename = app_log_filename("extras_plugin", 20100, 1_787_840_000_123);

        let (app_name, _) =
            parse_app_log_filename(&filename).expect("launch log filename must parse");

        assert_eq!(app_name, "extras_plugin");
    }

    #[test]
    fn rejects_watch_logs_from_app_log_pattern() {
        // Watch logs carry no `_port{number}` segment and must not be treated as app logs.
        let filename = "bevy_brp_mcp_watch_2_list_4294966727_1787840693.log";

        assert!(parse_app_log_filename(filename).is_none());
        assert_eq!(
            parse_log_filename(filename).map(|value| value.0),
            Some(LogSource::Watch)
        );
    }

    #[test]
    fn generic_parser_prefers_app_pattern() {
        let filename = app_log_filename("test_app", 20202, 1_787_840_000_123);

        let (_, app_name, _) = parse_log_filename(&filename).expect("filename must parse");

        assert_eq!(app_name, "test_app");
    }

    #[test]
    fn excludes_server_trace_and_lookalike_files() {
        for filename in [
            "bevy_brp_mcp_trace.log",
            "bevy_brp_mcp_trace_123.log",
            "bevy_brp_mcp_watch_1_other_42_123.log",
            "bevy_brp_mcp_watch_1_get_42_123.log/../bevy_brp_mcp_trace.log",
            "bevy_brp_mcp_app_port15702_123.log\\..\\bevy_brp_mcp_trace.log",
        ] {
            assert!(parse_log_filename(filename).is_none(), "{filename}");
        }
    }
}
