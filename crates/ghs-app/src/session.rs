//! Local Console session recording and read-only replay.
//!
//! Recording is a sender-side text log: [`SessionRecorder`] appends each new
//! Console entry to `<app config dir>/exports/session.jsonl`. It never opens a
//! transport, builds a controller command, or feeds the engine. Replay
//! ([`read_session_recording`]) reads that one bounded file back into plain
//! Console text; it cannot send, start, or change machine state.
//!
//! # File format (version 1)
//!
//! The file is JSON Lines. The first line is the header:
//!
//! ```text
//! {"app_version":"...","format_version":1,"record":"header","started_unix_s":...}
//! ```
//!
//! Each following line is one Console entry with a monotonic millisecond offset
//! from the recording start and its original level (`CMD`, `INFO`, `OK`,
//! `WARN`, or `ERROR`):
//!
//! ```text
//! {"level":"INFO","offset_ms":0,"record":"entry","text":"..."}
//! ```
//!
//! A recording that reaches a limit ends with one truncation marker:
//!
//! ```text
//! {"bytes":...,"entries":...,"reason":"max_entries","record":"truncated"}
//! ```
//!
//! Start time is Unix seconds (UTC). Offsets use a monotonic clock, so a system
//! clock change cannot make the log run backwards.
//!
//! # Bounds
//!
//! One recording is capped at [`SESSION_MAX_BYTES`] and
//! [`SESSION_MAX_ENTRIES`]. The writer reserves room for the truncation marker
//! before accepting the final entry, so the finished file stays inside the byte
//! bound. Replay applies the same limits before exposing text. The file name is
//! fixed under the configuration export directory; no user-supplied path is
//! read or written.

use std::fmt;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{Map, Value};

use crate::offsets_export::EXPORT_DIRECTORY;
use crate::state::ConsoleLevel;

/// Fixed file name written under the configuration export directory.
pub const SESSION_RECORDING_FILE_NAME: &str = "session.jsonl";

/// Current JSONL format version understood by writer and reader.
pub const SESSION_FORMAT_VERSION: u32 = 1;

/// Hard byte limit for one recording, including the header and marker.
pub const SESSION_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Hard entry limit for one recording, excluding the header and marker.
pub const SESSION_MAX_ENTRIES: usize = 100_000;

/// Room kept for the final truncation marker so it never exceeds the byte cap.
const TRUNCATION_MARKER_RESERVE: usize = 512;

/// Full path of the fixed session recording for one configuration root.
#[must_use]
pub fn session_recording_path(root: &Path) -> PathBuf {
    root.join(EXPORT_DIRECTORY)
        .join(SESSION_RECORDING_FILE_NAME)
}

/// One Console entry recovered from a recording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedEntry {
    /// Monotonic milliseconds since the recording started.
    pub offset_ms: u64,
    /// Original Console level.
    pub level: ConsoleLevel,
    /// Plain Console text, before any replay prefix.
    pub text: String,
}

/// Summary of a truncation marker in a recording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTruncation {
    /// Why the recorder stopped, for example `max_entries` or `max_bytes`.
    pub reason: String,
    /// Entries written before the marker.
    pub entries: u64,
    /// File bytes written before the marker.
    pub bytes: u64,
}

/// A validated recording read back from disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionReplay {
    /// Full path that was read.
    pub path: PathBuf,
    /// Format version from the header.
    pub format_version: u32,
    /// App version recorded in the header.
    pub app_version: String,
    /// Recording start time, Unix seconds (UTC).
    pub started_unix_s: u64,
    /// File size in bytes.
    pub bytes: usize,
    /// Entries in file order, with non-decreasing offsets.
    pub entries: Vec<RecordedEntry>,
    /// Final truncation marker, when the recorder hit a limit.
    pub truncated: Option<SessionTruncation>,
}

/// Last `LOAD RECORDING` result kept for the Console toolbar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionReplayStatus {
    /// Whether the recording was read and appended.
    pub ok: bool,
    /// Result line, for example `REPLAYED 6 ENTRIES`.
    pub text: String,
    /// Full file path, when one was known.
    pub path: Option<PathBuf>,
    /// Whether a job was active when replay ran. Replay is text only.
    pub job_active: bool,
}

/// Why recording could not start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionError {
    /// No platform configuration directory is available in this session.
    NoConfigRoot,
    /// The export directory or recording file could not be written.
    Write {
        /// Intended full path.
        path: PathBuf,
        /// Operating-system error text.
        message: String,
    },
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConfigRoot => f.write_str("the app configuration directory is unavailable"),
            Self::Write { path, message } => write!(f, "{}: {message}", path.display()),
        }
    }
}

impl std::error::Error for SessionError {}

/// Why a recording was refused by replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionReplayError {
    /// No platform configuration directory is available in this session.
    NoConfigRoot,
    /// The fixed recording file does not exist.
    Missing {
        /// Full file path.
        path: PathBuf,
    },
    /// The path exists but is not a regular file (a directory or a symlink).
    NotRegular {
        /// Full file path.
        path: PathBuf,
    },
    /// The file is larger than [`SESSION_MAX_BYTES`].
    Oversized {
        /// Full file path.
        path: PathBuf,
        /// Observed size in bytes.
        bytes: usize,
        /// Accepted limit in bytes.
        limit: usize,
    },
    /// The file could not be opened or read.
    Unreadable {
        /// Full file path.
        path: PathBuf,
        /// Operating-system error text.
        message: String,
    },
    /// A line does not match the version-1 shape.
    Malformed {
        /// Full file path.
        path: PathBuf,
        /// 1-based line number, or zero for a whole-file problem.
        line: usize,
        /// What is wrong.
        reason: String,
    },
    /// The header names a format version this build does not understand.
    WrongVersion {
        /// Full file path.
        path: PathBuf,
        /// Version found in the header.
        found: u32,
        /// Version expected by this build.
        expected: u32,
    },
    /// A non-empty file has no header line.
    NoHeader {
        /// Full file path.
        path: PathBuf,
    },
}

impl SessionReplayError {
    /// Full path involved in the failure, when one was known.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::NoConfigRoot => None,
            Self::Missing { path }
            | Self::NotRegular { path }
            | Self::Oversized { path, .. }
            | Self::Unreadable { path, .. }
            | Self::Malformed { path, .. }
            | Self::WrongVersion { path, .. }
            | Self::NoHeader { path } => Some(path),
        }
    }
}

impl fmt::Display for SessionReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConfigRoot => f.write_str("the app configuration directory is unavailable"),
            Self::Missing { path } => {
                write!(f, "no session recording at {}", path.display())
            }
            Self::NotRegular { path } => write!(
                f,
                "{} is not a regular file; only the fixed session recording is read",
                path.display()
            ),
            Self::Oversized { path, bytes, limit } => write!(
                f,
                "{} is {bytes} bytes; the replay limit is {limit} bytes",
                path.display()
            ),
            Self::Unreadable { path, message } => {
                write!(f, "{} could not be read: {message}", path.display())
            }
            Self::Malformed { path, line, reason } if *line == 0 => {
                write!(f, "{} is malformed: {reason}", path.display())
            }
            Self::Malformed { path, line, reason } => {
                write!(f, "{} line {line} is malformed: {reason}", path.display())
            }
            Self::WrongVersion {
                path,
                found,
                expected,
            } => write!(
                f,
                "{} has format version {found}; this build reads version {expected}",
                path.display()
            ),
            Self::NoHeader { path } => {
                write!(f, "{} has no session recording header", path.display())
            }
        }
    }
}

impl std::error::Error for SessionReplayError {}

/// Recorder lifecycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RecordingState {
    /// No recording in this app run.
    #[default]
    Idle,
    /// Entries are being appended.
    Recording,
    /// The operator stopped the recorder; the file is flushed and complete.
    Stopped,
    /// A bound was reached; the marker is written and no entry follows.
    Truncated,
}

impl RecordingState {
    /// Short uppercase label for the Console status row.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "NOT RECORDING",
            Self::Recording => "RECORDING",
            Self::Stopped => "STOPPED",
            Self::Truncated => "TRUNCATED",
        }
    }
}

/// Current recorder state plus its fixed limits, for the Console toolbar.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordingStatus {
    /// Recorder lifecycle.
    pub state: RecordingState,
    /// Full target path, when a recording has started in this app run.
    pub path: Option<PathBuf>,
    /// Entries written so far.
    pub entries: usize,
    /// Bytes written so far.
    pub bytes: usize,
    /// Final truncation marker, when a bound stopped the recorder.
    pub truncation: Option<SessionTruncation>,
    /// Last local write or flush problem, if any.
    pub last_error: Option<String>,
}

impl RecordingStatus {
    /// Whether entries are currently being appended.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.state == RecordingState::Recording
    }
}

/// Result of one [`SessionRecorder::record`] call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordOutcome {
    /// The recorder is idle, stopped, or truncated; the entry was not written.
    NotRecording,
    /// The entry was appended.
    Recorded,
    /// The entry was appended and the recorder then stopped at a limit.
    RecordedAndTruncated,
}

/// Append-only local recorder for Console entries.
///
/// The recorder owns one buffered file. [`Drop`] flushes it, so closing the app
/// without an explicit stop still commits buffered entries. It has no engine,
/// transport, or network handle, and its only inputs are a Console level and
/// text.
#[derive(Debug, Default)]
pub struct SessionRecorder {
    state: RecordingState,
    path: Option<PathBuf>,
    writer: Option<BufWriter<File>>,
    started_at: Option<Instant>,
    entries: usize,
    bytes: usize,
    last_offset_ms: u64,
    truncation: Option<SessionTruncation>,
    last_error: Option<String>,
}

impl SessionRecorder {
    /// Begin a new recording at the fixed export path.
    ///
    /// A second call while [`Self::is_recording`] is true is a no-op: it neither
    /// reopens nor truncates the current file. Starting again after a stop or a
    /// truncation deliberately replaces only the fixed `session.jsonl`, never
    /// another export.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::NoConfigRoot`] without a configuration root, or
    /// [`SessionError::Write`] when the directory or header cannot be written.
    pub fn start(
        &mut self,
        root: Option<&Path>,
        app_version: &str,
        started_unix_s: u64,
    ) -> Result<RecordingStatus, SessionError> {
        if self.state == RecordingState::Recording {
            return Ok(self.status());
        }
        let Some(root) = root else {
            return Err(SessionError::NoConfigRoot);
        };
        let path = session_recording_path(root);
        let write = (|| -> Result<BufWriter<File>, String> {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let file = File::create(&path).map_err(|error| error.to_string())?;
            Ok(BufWriter::new(file))
        })();
        let mut writer = match write {
            Ok(writer) => writer,
            Err(message) => return Err(SessionError::Write { path, message }),
        };
        let header = encode_header(SESSION_FORMAT_VERSION, app_version, started_unix_s);
        let header = format!("{header}\n");
        if let Err(error) = writer.write_all(header.as_bytes()) {
            return Err(SessionError::Write {
                path,
                message: error.to_string(),
            });
        }
        // Make even a crash-truncated recording a valid empty file: the header
        // is the version boundary replay validates first.
        if let Err(error) = writer.flush() {
            return Err(SessionError::Write {
                path,
                message: error.to_string(),
            });
        }
        self.state = RecordingState::Recording;
        self.path = Some(path);
        self.writer = Some(writer);
        self.started_at = Some(Instant::now());
        self.entries = 0;
        self.bytes = header.len();
        self.last_offset_ms = 0;
        self.truncation = None;
        self.last_error = None;
        Ok(self.status())
    }

    /// Current lifecycle and counters.
    #[must_use]
    pub fn status(&self) -> RecordingStatus {
        RecordingStatus {
            state: self.state,
            path: self.path.clone(),
            entries: self.entries,
            bytes: self.bytes,
            truncation: self.truncation.clone(),
            last_error: self.last_error.clone(),
        }
    }

    /// Whether entries are currently being appended.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.state == RecordingState::Recording
    }

    /// Flush one entry using the recorder's monotonic clock.
    ///
    /// Callers that need a deterministic offset use [`Self::record`] instead.
    pub fn record_now(&mut self, level: ConsoleLevel, text: &str) -> RecordOutcome {
        let offset_ms = self.started_at.map_or(0, |started| {
            u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
        });
        self.record(level, text, offset_ms)
    }

    /// Append one entry with an explicit monotonic millisecond offset.
    ///
    /// Offsets are clamped to at least the previous offset, so a caller cannot
    /// make the file run backwards. When a bound is reached, the entry is
    /// rejected (if there is no room) or written and followed by the marker; the
    /// recorder then refuses every later entry until a new start.
    pub fn record(&mut self, level: ConsoleLevel, text: &str, offset_ms: u64) -> RecordOutcome {
        if self.state != RecordingState::Recording {
            return RecordOutcome::NotRecording;
        }
        let offset_ms = offset_ms.max(self.last_offset_ms);
        if self.entries >= SESSION_MAX_ENTRIES {
            self.finish_truncated("max_entries");
            return RecordOutcome::NotRecording;
        }
        let line = encode_entry(offset_ms, level, text);
        let line = format!("{line}\n");
        let projected = self
            .bytes
            .saturating_add(line.len())
            .saturating_add(TRUNCATION_MARKER_RESERVE);
        if projected > SESSION_MAX_BYTES {
            self.finish_truncated("max_bytes");
            return RecordOutcome::NotRecording;
        }
        let Some(writer) = self.writer.as_mut() else {
            self.last_error = Some("the recording file is not open".to_string());
            self.state = RecordingState::Stopped;
            return RecordOutcome::NotRecording;
        };
        if let Err(error) = writer.write_all(line.as_bytes()) {
            self.last_error = Some(error.to_string());
            self.state = RecordingState::Stopped;
            self.writer = None;
            return RecordOutcome::NotRecording;
        }
        self.entries += 1;
        self.bytes = self.bytes.saturating_add(line.len());
        self.last_offset_ms = offset_ms;
        if self.entries >= SESSION_MAX_ENTRIES {
            self.finish_truncated("max_entries");
            return RecordOutcome::RecordedAndTruncated;
        }
        RecordOutcome::Recorded
    }

    /// Flush buffered entries and pause recording.
    ///
    /// Returns `false` when no recording was active, so a second STOP is a
    /// harmless no-op. A truncated recording is already flushed and closed.
    pub fn stop(&mut self) -> bool {
        if self.state != RecordingState::Recording {
            return false;
        }
        self.flush();
        self.writer = None;
        self.state = RecordingState::Stopped;
        true
    }

    /// Flush buffered entries without changing the lifecycle.
    pub fn flush(&mut self) {
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        if let Err(error) = writer.flush() {
            self.last_error = Some(error.to_string());
        }
    }

    /// Write the final marker, flush, and close the file.
    fn finish_truncated(&mut self, reason: &str) {
        let Some(writer) = self.writer.as_mut() else {
            self.state = RecordingState::Truncated;
            return;
        };
        let marker = encode_truncated(reason, self.entries, self.bytes);
        let mut line = format!("{marker}\n");
        let available = SESSION_MAX_BYTES.saturating_sub(self.bytes);
        if line.len() > available {
            // The reserve makes this unreachable for the writer's own bounds;
            // drop the marker fields rather than exceed the hard cap.
            line = format!(
                "{{\"record\":\"truncated\",\"reason\":\"{reason}\",\"entries\":{},\"bytes\":{}}}\n",
                self.entries, self.bytes
            );
            if line.len() > available {
                line.truncate(available.saturating_sub(2));
                line.push('\n');
            }
        }
        if let Err(error) = writer.write_all(line.as_bytes()) {
            self.last_error = Some(error.to_string());
        }
        self.bytes = self.bytes.saturating_add(line.len());
        if let Err(error) = writer.flush() {
            self.last_error = Some(error.to_string());
        }
        self.writer = None;
        self.truncation = Some(SessionTruncation {
            reason: reason.to_string(),
            entries: u64::try_from(self.entries).unwrap_or(u64::MAX),
            bytes: u64::try_from(self.bytes).unwrap_or(u64::MAX),
        });
        self.state = RecordingState::Truncated;
    }
}

impl Drop for SessionRecorder {
    fn drop(&mut self) {
        // A window close without STOP must not lose buffered Console text.
        self.flush();
    }
}

/// Read and validate the fixed recording under one configuration root.
///
/// Replay is read-only: this function only reads the one fixed file, applies
/// the byte and entry bounds, verifies the version and every line shape, and
/// returns plain entries. It cannot send, start, or change machine state.
///
/// # Errors
///
/// Returns [`SessionReplayError`] for a missing root or file, a non-regular
/// path, an oversized file, a read failure, a wrong format version, or a line
/// that does not match the version-1 shape.
pub fn read_session_recording(root: Option<&Path>) -> Result<SessionReplay, SessionReplayError> {
    let Some(root) = root else {
        return Err(SessionReplayError::NoConfigRoot);
    };
    let path = session_recording_path(root);
    read_recording_path(&path)
}

fn read_recording_path(path: &Path) -> Result<SessionReplay, SessionReplayError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            SessionReplayError::Missing {
                path: path.to_path_buf(),
            }
        } else {
            SessionReplayError::Unreadable {
                path: path.to_path_buf(),
                message: error.to_string(),
            }
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SessionReplayError::NotRegular {
            path: path.to_path_buf(),
        });
    }
    if metadata.len() > u64::try_from(SESSION_MAX_BYTES).unwrap_or(u64::MAX) {
        return Err(SessionReplayError::Oversized {
            path: path.to_path_buf(),
            bytes: usize::try_from(metadata.len()).unwrap_or(usize::MAX),
            limit: SESSION_MAX_BYTES,
        });
    }
    let file = File::open(path).map_err(|error| SessionReplayError::Unreadable {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let mut bytes = Vec::new();
    file.take(u64::try_from(SESSION_MAX_BYTES + 1).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .map_err(|error| SessionReplayError::Unreadable {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    if bytes.len() > SESSION_MAX_BYTES {
        return Err(SessionReplayError::Oversized {
            path: path.to_path_buf(),
            bytes: bytes.len(),
            limit: SESSION_MAX_BYTES,
        });
    }
    let text = String::from_utf8(bytes).map_err(|_| SessionReplayError::Malformed {
        path: path.to_path_buf(),
        line: 0,
        reason: "the file is not valid UTF-8".to_string(),
    })?;
    parse_recording(path, &text)
}

fn parse_recording(path: &Path, text: &str) -> Result<SessionReplay, SessionReplayError> {
    let mut format_version = None;
    let mut app_version = String::new();
    let mut started_unix_s = 0;
    let mut entries = Vec::new();
    let mut last_offset_ms = 0;
    let mut truncated = None;
    let max_lines = SESSION_MAX_ENTRIES.saturating_add(2);
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        if line > max_lines {
            return Err(malformed(
                path,
                line,
                format!("more than {max_lines} JSONL lines"),
            ));
        }
        if raw.trim().is_empty() {
            return Err(malformed(path, line, "blank line"));
        }
        let value: Value = serde_json::from_str(raw)
            .map_err(|error| malformed(path, line, format!("invalid JSON: {error}")))?;
        let object = value
            .as_object()
            .ok_or_else(|| malformed(path, line, "the line is not a JSON object"))?;
        match string_field(object, "record") {
            Some("header") => {
                if format_version.is_some() || !entries.is_empty() || truncated.is_some() {
                    return Err(malformed(path, line, "the header must be the first line"));
                }
                exact_fields(
                    object,
                    &["record", "format_version", "app_version", "started_unix_s"],
                )
                .map_err(|reason| malformed(path, line, reason))?;
                let found = u32_field(object, "format_version")
                    .map_err(|reason| malformed(path, line, reason))?;
                if found != SESSION_FORMAT_VERSION {
                    return Err(SessionReplayError::WrongVersion {
                        path: path.to_path_buf(),
                        found,
                        expected: SESSION_FORMAT_VERSION,
                    });
                }
                app_version = string_field(object, "app_version")
                    .ok_or_else(|| malformed(path, line, "app_version must be a string"))?
                    .to_string();
                if app_version.is_empty() {
                    return Err(malformed(path, line, "app_version is empty"));
                }
                started_unix_s = u64_field(object, "started_unix_s")
                    .map_err(|reason| malformed(path, line, reason))?;
                format_version = Some(found);
            }
            Some("entry") => {
                if format_version.is_none() {
                    return Err(malformed(path, line, "the entry precedes the header"));
                }
                if truncated.is_some() {
                    return Err(malformed(
                        path,
                        line,
                        "an entry follows the truncation marker",
                    ));
                }
                if entries.len() >= SESSION_MAX_ENTRIES {
                    return Err(malformed(
                        path,
                        line,
                        format!("more than {SESSION_MAX_ENTRIES} entries"),
                    ));
                }
                exact_fields(object, &["record", "offset_ms", "level", "text"])
                    .map_err(|reason| malformed(path, line, reason))?;
                let offset_ms = u64_field(object, "offset_ms")
                    .map_err(|reason| malformed(path, line, reason))?;
                if offset_ms < last_offset_ms {
                    return Err(malformed(
                        path,
                        line,
                        format!(
                            "offset_ms {offset_ms} is before the previous offset {last_offset_ms}"
                        ),
                    ));
                }
                let level = string_field(object, "level")
                    .and_then(level_from_label)
                    .ok_or_else(|| {
                        malformed(path, line, "level must be CMD, INFO, OK, WARN, or ERROR")
                    })?;
                let entry_text = string_field(object, "text")
                    .ok_or_else(|| malformed(path, line, "text must be a string"))?
                    .to_string();
                entries.push(RecordedEntry {
                    offset_ms,
                    level,
                    text: entry_text,
                });
                last_offset_ms = offset_ms;
            }
            Some("truncated") => {
                if format_version.is_none() {
                    return Err(malformed(path, line, "the marker precedes the header"));
                }
                if truncated.is_some() {
                    return Err(malformed(path, line, "duplicate truncation marker"));
                }
                exact_fields(object, &["record", "reason", "entries", "bytes"])
                    .map_err(|reason| malformed(path, line, reason))?;
                let reason = string_field(object, "reason")
                    .ok_or_else(|| malformed(path, line, "reason must be a string"))?;
                if reason.is_empty() {
                    return Err(malformed(path, line, "reason is empty"));
                }
                let marker_entries =
                    u64_field(object, "entries").map_err(|reason| malformed(path, line, reason))?;
                if marker_entries != u64::try_from(entries.len()).unwrap_or(u64::MAX) {
                    return Err(malformed(
                        path,
                        line,
                        "marker entry count does not match the file",
                    ));
                }
                let marker_bytes =
                    u64_field(object, "bytes").map_err(|reason| malformed(path, line, reason))?;
                truncated = Some(SessionTruncation {
                    reason: reason.to_string(),
                    entries: marker_entries,
                    bytes: marker_bytes,
                });
            }
            Some(other) => {
                return Err(malformed(
                    path,
                    line,
                    format!("unknown record type {other:?}"),
                ));
            }
            None => {
                return Err(malformed(path, line, "missing record type"));
            }
        }
    }
    let Some(format_version) = format_version else {
        return Err(SessionReplayError::NoHeader {
            path: path.to_path_buf(),
        });
    };
    Ok(SessionReplay {
        path: path.to_path_buf(),
        format_version,
        app_version,
        started_unix_s,
        bytes: text.len(),
        entries,
        truncated,
    })
}

fn malformed(path: &Path, line: usize, reason: impl Into<String>) -> SessionReplayError {
    SessionReplayError::Malformed {
        path: path.to_path_buf(),
        line,
        reason: reason.into(),
    }
}

fn encode_header(format_version: u32, app_version: &str, started_unix_s: u64) -> String {
    serde_json::json!({
        "record": "header",
        "format_version": format_version,
        "app_version": app_version,
        "started_unix_s": started_unix_s,
    })
    .to_string()
}

fn encode_entry(offset_ms: u64, level: ConsoleLevel, text: &str) -> String {
    serde_json::json!({
        "record": "entry",
        "offset_ms": offset_ms,
        "level": level.label(),
        "text": text,
    })
    .to_string()
}

fn encode_truncated(reason: &str, entries: usize, bytes: usize) -> String {
    serde_json::json!({
        "record": "truncated",
        "reason": reason,
        "entries": u64::try_from(entries).unwrap_or(u64::MAX),
        "bytes": u64::try_from(bytes).unwrap_or(u64::MAX),
    })
    .to_string()
}

fn string_field<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str)
}

fn u64_field(object: &Map<String, Value>, key: &str) -> Result<u64, String> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{key} must be an unsigned integer"))
}

fn u32_field(object: &Map<String, Value>, key: &str) -> Result<u32, String> {
    let value = u64_field(object, key)?;
    u32::try_from(value).map_err(|_| format!("{key} is out of range"))
}

/// Require exactly the named fields: every one present and no others.
fn exact_fields(object: &Map<String, Value>, expected: &[&str]) -> Result<(), String> {
    for key in expected {
        if !object.contains_key(*key) {
            return Err(format!("missing field {key}"));
        }
    }
    if let Some(extra) = object.keys().find(|key| !expected.contains(&key.as_str())) {
        return Err(format!("unknown field {extra}"));
    }
    Ok(())
}

fn level_from_label(label: &str) -> Option<ConsoleLevel> {
    match label {
        "CMD" => Some(ConsoleLevel::Command),
        "INFO" => Some(ConsoleLevel::Info),
        "OK" => Some(ConsoleLevel::Ok),
        "WARN" => Some(ConsoleLevel::Warn),
        "ERROR" => Some(ConsoleLevel::Error),
        _ => None,
    }
}

/// Fixed entries pushed once by `--demo-session-record`.
///
/// They are plain Console text, never protocol traffic: the demo flag records
/// them so the active indicator and file have a reproducible shape.
pub const DEMO_SESSION_LIVE_ENTRIES: [(ConsoleLevel, &str); 3] = [
    (
        ConsoleLevel::Info,
        "[MSG:Demo session recording - local text only]",
    ),
    (
        ConsoleLevel::Ok,
        "Recording demo marker; no command was sent",
    ),
    (
        ConsoleLevel::Warn,
        "Replay is read-only and never changes machine state",
    ),
];

/// Fixed entries written by `--demo-session-replay` before the normal replay.
pub const DEMO_SESSION_REPLAY_ENTRIES: [(ConsoleLevel, &str); 6] = [
    (ConsoleLevel::Info, "Grbl 1.1f ['$' for help]"),
    (ConsoleLevel::Command, "> $I+"),
    (ConsoleLevel::Info, "[VER:1.1f.20240604:]"),
    (ConsoleLevel::Ok, "ok"),
    (ConsoleLevel::Info, "<Idle|MPos:0.000,0.000,0.000|FS:0,0>"),
    (
        ConsoleLevel::Warn,
        "[MSG:Demo replay - simulated controller text only]",
    ),
];

/// Write the fixed demo recording through the normal recorder.
///
/// Two calls produce byte-identical files: start time, offsets, and entries are
/// constants. The replay flag uses this so a screenshot shows a real recording
/// read back by the normal validator.
///
/// # Errors
///
/// Returns [`SessionError`] when no configuration root is available or the
/// fixed file cannot be written.
pub fn write_demo_session_recording(root: Option<&Path>) -> Result<PathBuf, SessionError> {
    let mut recorder = SessionRecorder::default();
    let status = recorder.start(root, "demo", 1_700_000_000)?;
    for (index, (level, text)) in DEMO_SESSION_REPLAY_ENTRIES.iter().enumerate() {
        let offset_ms = u64::try_from(index).unwrap_or(0) * 250;
        recorder.record(*level, text, offset_ms);
    }
    recorder.stop();
    Ok(status
        .path
        .unwrap_or_else(|| session_recording_path(Path::new("."))))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "ghs-app-session-{label}-{}-{nanos}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn start(root: &TempRoot) -> SessionRecorder {
        let mut recorder = SessionRecorder::default();
        recorder
            .start(Some(&root.path), "test", 1_700_000_000)
            .expect("start recording");
        recorder
    }

    #[test]
    fn start_writes_a_header_and_entries_and_stop_flushes() {
        let dir = TempRoot::new("header");
        let mut recorder = start(&dir);
        assert_eq!(
            recorder.record(ConsoleLevel::Info, "first", 0),
            RecordOutcome::Recorded
        );
        assert_eq!(
            recorder.record(ConsoleLevel::Command, "> $I+", 5),
            RecordOutcome::Recorded
        );
        assert!(recorder.stop());
        assert_eq!(recorder.status().state, RecordingState::Stopped);

        let text = fs::read_to_string(session_recording_path(&dir.path)).expect("recording");
        let mut lines = text.lines();
        let header = lines.next().expect("header");
        assert!(header.contains("\"record\":\"header\""), "{header}");
        assert!(header.contains("\"format_version\":1"), "{header}");
        assert!(header.contains("\"app_version\":\"test\""), "{header}");
        assert!(header.contains("\"started_unix_s\":1700000000"), "{header}");
        assert!(text.contains("\"record\":\"entry\""), "{text}");
        assert!(text.contains("\"level\":\"CMD\""), "{text}");
        assert!(text.contains("\"text\":\"> $I+\""), "{text}");
        assert_eq!(text.lines().count(), 3);
    }

    #[test]
    fn offsets_and_levels_round_trip_in_order() {
        let dir = TempRoot::new("round-trip");
        let mut recorder = start(&dir);
        for (index, level) in [
            ConsoleLevel::Info,
            ConsoleLevel::Command,
            ConsoleLevel::Ok,
            ConsoleLevel::Warn,
            ConsoleLevel::Error,
        ]
        .into_iter()
        .enumerate()
        {
            let offset = u64::try_from(index).unwrap_or(0) * 3;
            recorder.record(level, &format!("line {index}"), offset);
        }
        recorder.stop();

        let replay = read_session_recording(Some(&dir.path)).expect("replay");
        assert_eq!(replay.format_version, SESSION_FORMAT_VERSION);
        assert_eq!(replay.app_version, "test");
        assert_eq!(replay.started_unix_s, 1_700_000_000);
        assert_eq!(
            replay.bytes,
            fs::metadata(&replay.path).expect("metadata").len() as usize
        );
        let levels: Vec<ConsoleLevel> = replay.entries.iter().map(|entry| entry.level).collect();
        assert_eq!(
            levels,
            vec![
                ConsoleLevel::Info,
                ConsoleLevel::Command,
                ConsoleLevel::Ok,
                ConsoleLevel::Warn,
                ConsoleLevel::Error
            ]
        );
        let offsets: Vec<u64> = replay.entries.iter().map(|entry| entry.offset_ms).collect();
        assert_eq!(offsets, vec![0, 3, 6, 9, 12]);
        assert_eq!(replay.entries[4].text, "line 4");
    }

    #[test]
    fn a_decreasing_offset_is_clamped_to_keep_the_log_monotonic() {
        let dir = TempRoot::new("monotonic");
        let mut recorder = start(&dir);
        recorder.record(ConsoleLevel::Info, "first", 50);
        recorder.record(ConsoleLevel::Info, "second", 20);
        recorder.stop();
        let replay = read_session_recording(Some(&dir.path)).expect("replay");
        assert_eq!(replay.entries[0].offset_ms, 50);
        assert_eq!(replay.entries[1].offset_ms, 50);
    }

    #[test]
    fn the_entry_cap_stops_recording_and_appends_a_marker() {
        let dir = TempRoot::new("entry-cap");
        let mut recorder = start(&dir);
        // A small text keeps the byte cap out of the way for this test.
        for index in 0..SESSION_MAX_ENTRIES {
            let outcome = recorder.record(
                ConsoleLevel::Info,
                "x",
                u64::try_from(index).unwrap_or(u64::MAX),
            );
            let expected = if index + 1 == SESSION_MAX_ENTRIES {
                RecordOutcome::RecordedAndTruncated
            } else {
                RecordOutcome::Recorded
            };
            assert_eq!(outcome, expected, "entry {index}");
        }
        assert_eq!(
            recorder.record(ConsoleLevel::Info, "after cap", 0),
            RecordOutcome::NotRecording,
            "the cap is final until a new start"
        );
        let status = recorder.status();
        assert_eq!(status.state, RecordingState::Truncated);
        assert_eq!(status.entries, SESSION_MAX_ENTRIES);
        let truncation = status.truncation.expect("truncation");
        assert_eq!(truncation.reason, "max_entries");

        let replay = read_session_recording(Some(&dir.path)).expect("replay");
        assert_eq!(replay.entries.len(), SESSION_MAX_ENTRIES);
        assert_eq!(
            replay
                .truncated
                .as_ref()
                .map(|marker| marker.reason.as_str()),
            Some("max_entries")
        );
        assert!(replay.bytes <= SESSION_MAX_BYTES);
    }

    #[test]
    fn the_byte_cap_rejects_an_oversized_entry_and_appends_a_marker() {
        let dir = TempRoot::new("byte-cap");
        let mut recorder = start(&dir);
        let huge = "y".repeat(SESSION_MAX_BYTES);
        assert_eq!(
            recorder.record(ConsoleLevel::Info, &huge, 0),
            RecordOutcome::NotRecording
        );
        let status = recorder.status();
        assert_eq!(status.state, RecordingState::Truncated);
        assert_eq!(status.entries, 0);
        assert_eq!(
            status
                .truncation
                .as_ref()
                .map(|marker| marker.reason.as_str()),
            Some("max_bytes")
        );
        let replay = read_session_recording(Some(&dir.path)).expect("replay");
        assert!(replay.entries.is_empty());
        assert!(replay.bytes <= SESSION_MAX_BYTES);
        assert_eq!(
            replay
                .truncated
                .as_ref()
                .map(|marker| marker.reason.as_str()),
            Some("max_bytes")
        );
    }

    #[test]
    fn start_and_stop_are_idempotent() {
        let dir = TempRoot::new("idempotent");
        let mut recorder = start(&dir);
        recorder.record(ConsoleLevel::Info, "first", 0);
        recorder.flush();
        let before = fs::read_to_string(session_recording_path(&dir.path)).expect("first read");
        recorder
            .start(Some(&dir.path), "test", 999)
            .expect("second start");
        assert!(recorder.is_recording());
        assert_eq!(recorder.status().entries, 1);
        assert!(recorder.stop());
        assert!(!recorder.stop());
        let after = fs::read_to_string(session_recording_path(&dir.path)).expect("second read");
        assert_eq!(before, after, "a second start must not rewrite the file");
    }

    #[test]
    fn dropping_an_active_recorder_flushes_buffered_entries() {
        let dir = TempRoot::new("drop-flush");
        let mut recorder = start(&dir);
        recorder.record(ConsoleLevel::Info, "closed with the window", 7);
        drop(recorder);
        let replay = read_session_recording(Some(&dir.path)).expect("flushed recording");
        assert_eq!(replay.entries.len(), 1);
        assert_eq!(replay.entries[0].offset_ms, 7);
        assert_eq!(replay.entries[0].text, "closed with the window");
    }

    #[test]
    fn the_fixed_path_stays_under_exports_and_unrelated_files_are_untouched() {
        let dir = TempRoot::new("paths");
        let exports = dir.path.join(EXPORT_DIRECTORY);
        fs::create_dir_all(&exports).expect("exports");
        fs::write(exports.join("console.log"), b"console snapshot").expect("console sentinel");
        fs::write(exports.join("offsets.nc"), b"offset snapshot").expect("offsets sentinel");
        fs::write(dir.path.join("app.json"), b"app config").expect("app sentinel");

        let mut recorder = start(&dir);
        recorder.record(ConsoleLevel::Info, "entry", 0);
        recorder.stop();

        let path = session_recording_path(&dir.path);
        assert!(path.starts_with(&exports));
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("session.jsonl")
        );
        assert_eq!(
            fs::read(exports.join("console.log")).expect("console"),
            b"console snapshot"
        );
        assert_eq!(
            fs::read(exports.join("offsets.nc")).expect("offsets"),
            b"offset snapshot"
        );
        assert_eq!(
            fs::read(dir.path.join("app.json")).expect("app config"),
            b"app config"
        );
    }

    #[test]
    fn replay_refuses_a_wrong_format_version() {
        let dir = TempRoot::new("version");
        let exports = dir.path.join(EXPORT_DIRECTORY);
        fs::create_dir_all(&exports).expect("exports");
        fs::write(
            session_recording_path(&dir.path),
            "{\"record\":\"header\",\"format_version\":99,\"app_version\":\"x\",\
             \"started_unix_s\":1}\n",
        )
        .expect("recording");
        let error = read_session_recording(Some(&dir.path)).expect_err("wrong version");
        assert!(
            matches!(
                error,
                SessionReplayError::WrongVersion {
                    found: 99,
                    expected: SESSION_FORMAT_VERSION,
                    ..
                }
            ),
            "{error}"
        );
        assert!(error.to_string().contains("version 99"), "{error}");
    }

    #[test]
    fn replay_refuses_malformed_lines() {
        let dir = TempRoot::new("malformed");
        let exports = dir.path.join(EXPORT_DIRECTORY);
        fs::create_dir_all(&exports).expect("exports");
        let header = "{\"record\":\"header\",\"format_version\":1,\
                      \"app_version\":\"x\",\"started_unix_s\":1}\n";
        let cases = [
            ("not json".to_string(), "invalid JSON"),
            (
                format!(
                    "{header}{{\"record\":\"entry\",\"offset_ms\":0,\"level\":\"LOUD\",\
                     \"text\":\"x\"}}\n"
                ),
                "level must be",
            ),
            (
                format!(
                    "{header}{{\"record\":\"entry\",\"offset_ms\":5,\"level\":\"INFO\",\
                     \"text\":\"a\"}}\n\
                     {{\"record\":\"entry\",\"offset_ms\":4,\"level\":\"INFO\",\"text\":\"b\"}}\n"
                ),
                "before the previous offset",
            ),
            (
                format!(
                    "{header}{{\"record\":\"entry\",\"offset_ms\":0,\"level\":\"INFO\",\
                     \"text\":\"x\",\"extra\":1}}\n"
                ),
                "unknown field extra",
            ),
            (
                format!("{header}{{\"record\":\"mystery\"}}\n"),
                "unknown record type",
            ),
        ];
        for (text, expected) in cases {
            fs::write(session_recording_path(&dir.path), text).expect("recording");
            let error = read_session_recording(Some(&dir.path)).expect_err(expected);
            assert!(
                error.to_string().contains(expected),
                "expected {expected:?}, got {error}"
            );
        }
    }

    #[test]
    fn replay_refuses_an_oversized_file_before_parsing() {
        let dir = TempRoot::new("oversized");
        let exports = dir.path.join(EXPORT_DIRECTORY);
        fs::create_dir_all(&exports).expect("exports");
        let path = session_recording_path(&dir.path);
        let file = File::create(&path).expect("create");
        file.set_len(u64::try_from(SESSION_MAX_BYTES + 1).unwrap_or(u64::MAX))
            .expect("sparse file");
        let error = read_session_recording(Some(&dir.path)).expect_err("oversized");
        assert!(
            matches!(error, SessionReplayError::Oversized { .. }),
            "{error}"
        );
        assert!(error.to_string().contains("replay limit"), "{error}");
    }

    #[test]
    fn replay_refuses_missing_and_non_regular_files() {
        let dir = TempRoot::new("missing");
        let error = read_session_recording(Some(&dir.path)).expect_err("missing");
        assert!(
            matches!(error, SessionReplayError::Missing { .. }),
            "{error}"
        );

        let exports = dir.path.join(EXPORT_DIRECTORY);
        fs::create_dir_all(exports.join(SESSION_RECORDING_FILE_NAME)).expect("directory");
        let error = read_session_recording(Some(&dir.path)).expect_err("directory");
        assert!(
            matches!(error, SessionReplayError::NotRegular { .. }),
            "{error}"
        );
        assert!(error.to_string().contains("not a regular file"), "{error}");

        let error = read_session_recording(None).expect_err("no root");
        assert_eq!(error, SessionReplayError::NoConfigRoot);
    }

    #[test]
    fn replay_of_a_recording_round_trips_quotes_newlines_and_backslashes() {
        let dir = TempRoot::new("escapes");
        let mut recorder = start(&dir);
        let text = "quote \" and backslash \\ and newline\nsecond line";
        recorder.record(ConsoleLevel::Error, text, 0);
        recorder.stop();
        let replay = read_session_recording(Some(&dir.path)).expect("replay");
        assert_eq!(replay.entries.len(), 1);
        assert_eq!(replay.entries[0].text, text);
    }

    #[test]
    fn a_recording_stops_recording_after_stop_until_a_new_start() {
        let dir = TempRoot::new("stop-gate");
        let mut recorder = start(&dir);
        recorder.record(ConsoleLevel::Info, "kept", 0);
        recorder.stop();
        assert_eq!(
            recorder.record(ConsoleLevel::Info, "dropped", 5),
            RecordOutcome::NotRecording
        );
        recorder
            .start(Some(&dir.path), "test", 1_700_000_001)
            .expect("restart");
        recorder.record(ConsoleLevel::Info, "new", 0);
        recorder.stop();
        let replay = read_session_recording(Some(&dir.path)).expect("replay");
        assert_eq!(replay.started_unix_s, 1_700_000_001);
        assert_eq!(replay.entries.len(), 1);
        assert_eq!(replay.entries[0].text, "new");
    }

    #[test]
    fn the_demo_recording_is_byte_identical_on_every_run() {
        let dir = TempRoot::new("demo");
        let first = write_demo_session_recording(Some(&dir.path)).expect("first");
        let first_text = fs::read(&first).expect("first bytes");
        let second = write_demo_session_recording(Some(&dir.path)).expect("second");
        let second_text = fs::read(&second).expect("second bytes");
        assert_eq!(first, second);
        assert_eq!(first_text, second_text);
        let replay = read_session_recording(Some(&dir.path)).expect("replay");
        let expected: Vec<(ConsoleLevel, &str)> = DEMO_SESSION_REPLAY_ENTRIES.to_vec();
        let actual: Vec<(ConsoleLevel, &str)> = replay
            .entries
            .iter()
            .map(|entry| (entry.level, entry.text.as_str()))
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(replay.entries.len(), 6);
        assert_eq!(
            error_for_missing_root(),
            SessionError::NoConfigRoot.to_string()
        );
    }

    fn error_for_missing_root() -> String {
        write_demo_session_recording(None)
            .expect_err("no root")
            .to_string()
    }
}
