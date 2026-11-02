//! Bounded, user-initiated console-log export.
//!
//! The Console keeps a bounded ring buffer in [`AppState`](crate::state::AppState).
//! `SAVE LOG` writes one snapshot of that buffer to
//! `<app config dir>/exports/console.log`. The export is local only: it opens no
//! network connection, starts no background writer, and sends nothing to the
//! controller.
//!
//! The ring buffer is already bounded; [`CONSOLE_LOG_MAX_BYTES`] makes the file
//! bound explicit as well. If a line cannot fit, the export stops cleanly and
//! records how many matching lines were skipped.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::height_map_library::format_unix_utc_seconds;
use crate::offsets_export::EXPORT_DIRECTORY;
use crate::state::{CONSOLE_LINE_LIMIT, ConsoleCounts, ConsoleFilter, ConsoleLine};

/// Fixed file name written under the configuration export directory.
pub const CONSOLE_LOG_FILE_NAME: &str = "console.log";

/// Hard byte limit for one exported log file.
///
/// The in-app ring buffer holds at most [`CONSOLE_LINE_LIMIT`] lines, so a
/// normal export is far smaller. The limit is a backstop for unusually long
/// protocol lines: the writer never produces a file larger than this.
pub const CONSOLE_LOG_MAX_BYTES: usize = 64 * 1024;

/// Reserved room for the final truncation marker.
const TRUNCATION_MARKER_RESERVE: usize = 128;

/// Full path of the fixed console log for one configuration root.
#[must_use]
pub fn console_log_path(root: &Path) -> PathBuf {
    root.join(EXPORT_DIRECTORY).join(CONSOLE_LOG_FILE_NAME)
}

/// Rendered log text plus what actually fit inside the byte limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleLogText {
    /// Complete file text, including the header and any truncation marker.
    pub text: String,
    /// Matching lines written below the header.
    pub lines: usize,
    /// Matching lines omitted because of [`CONSOLE_LOG_MAX_BYTES`].
    pub skipped: usize,
}

/// One successful snapshot written to disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleLogExport {
    /// Full path that was written.
    pub path: PathBuf,
    /// Filter used for this snapshot.
    pub filter: ConsoleFilter,
    /// Whether the snapshot contains only the active filter's lines.
    pub filtered: bool,
    /// Lines currently held by the ring buffer, before filtering.
    pub ring_lines: usize,
    /// Matching lines written below the header.
    pub lines: usize,
    /// Matching lines omitted at the size limit.
    pub skipped: usize,
    /// File size in bytes.
    pub bytes: usize,
}

/// Last `SAVE LOG` result kept for the Console toolbar.
///
/// The text names the result and the exported scope; `path` carries the full
/// fixed path when one was known, so the UI can show it without truncation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleLogStatus {
    /// Whether the snapshot was written.
    pub ok: bool,
    /// Result line, for example `SAVED 18 LINES (FULL BUFFER)`.
    pub text: String,
    /// Full target path, or `None` when no configuration root was available.
    pub path: Option<PathBuf>,
}

/// Why a console-log export could not be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsoleLogError {
    /// No platform configuration directory is available in this session.
    NoConfigRoot,
    /// The export directory or file could not be written.
    Write {
        /// Intended full path.
        path: PathBuf,
        /// Operating-system error text.
        message: String,
    },
}

impl ConsoleLogError {
    /// Full path involved in the failure, when one was known.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::NoConfigRoot => None,
            Self::Write { path, .. } => Some(path),
        }
    }
}

impl fmt::Display for ConsoleLogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConfigRoot => f.write_str("the app configuration directory is unavailable"),
            Self::Write { path, message } => write!(f, "{}: {message}", path.display()),
        }
    }
}

impl std::error::Error for ConsoleLogError {}

/// Render the header and matching lines without touching the filesystem.
///
/// The header records the generation time (UTC), the active filter, the ring
/// buffer state, per-level counts, and both limits. Every exported line is
/// plain text: `HH:MM LEVEL text`, where the level is one of `CMD`, `INFO`,
/// `OK`, `WARN`, or `ERROR`.
#[must_use]
pub fn render_console_log(
    lines: &[ConsoleLine],
    filter: ConsoleFilter,
    generated_unix_s: u64,
) -> ConsoleLogText {
    let counts = ConsoleCounts::from_lines(lines);
    let filtered = filter != ConsoleFilter::All;
    let scope = if filtered {
        format!("filtered view; ring buffer has {} lines", counts.all)
    } else {
        "full ring buffer".to_string()
    };
    let mut text = format!(
        "ghs-rs console log\n\
         Generated: {generated}\n\
         Filter: {label} ({scope})\n\
         Ring buffer limit: {line_limit} lines\n\
         Export limit: {byte_limit} bytes\n\
         Levels in ring buffer: ALL {all} | CMD {cmd} | INFO {info} | OK {ok} | WARN {warn} | \
         ERROR {error}\n\
         Line times are UTC (HH:MM). Format: HH:MM LEVEL text\n\
         ----\n",
        generated = format_unix_utc_seconds(generated_unix_s),
        label = filter.label(),
        line_limit = CONSOLE_LINE_LIMIT,
        byte_limit = CONSOLE_LOG_MAX_BYTES,
        all = counts.all,
        cmd = counts.command,
        info = counts.info,
        ok = counts.ok,
        warn = counts.warn,
        error = counts.error,
    );

    let matching = counts.count(filter);
    // Keep room for the truncation marker so the final file still respects the
    // limit even when the last line is large.
    let mut written = 0;
    for line in lines.iter().filter(|line| filter.matches(line.level)) {
        let rendered = format!("{} {:<5} {}\n", line.time, line.level.label(), line.text);
        if text.len() + rendered.len() + TRUNCATION_MARKER_RESERVE > CONSOLE_LOG_MAX_BYTES {
            break;
        }
        text.push_str(&rendered);
        written += 1;
    }
    let skipped = matching.saturating_sub(written);
    if skipped > 0 {
        text.push_str(&format!(
            "[export truncated: {skipped} matching lines did not fit in \
             {CONSOLE_LOG_MAX_BYTES} bytes]\n"
        ));
    }
    ConsoleLogText {
        text,
        lines: written,
        skipped,
    }
}

/// Write one snapshot of the ring buffer to the fixed export path.
///
/// The caller supplies the configuration root. `None` is a refusal, not a
/// fallback to the current directory. The export directory is created when
/// missing; the file is replaced in one plain write.
///
/// # Errors
///
/// Returns [`ConsoleLogError::NoConfigRoot`] without a root, or
/// [`ConsoleLogError::Write`] when the directory or file cannot be written.
pub fn write_console_log(
    lines: &[ConsoleLine],
    filter: ConsoleFilter,
    root: Option<&Path>,
    generated_unix_s: u64,
) -> Result<ConsoleLogExport, ConsoleLogError> {
    let Some(root) = root else {
        return Err(ConsoleLogError::NoConfigRoot);
    };
    let path = console_log_path(root);
    let rendered = render_console_log(lines, filter, generated_unix_s);
    let write = (|| -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&path, rendered.text.as_bytes()).map_err(|error| error.to_string())
    })();
    if let Err(message) = write {
        return Err(ConsoleLogError::Write { path, message });
    }
    Ok(ConsoleLogExport {
        path,
        filter,
        filtered: filter != ConsoleFilter::All,
        ring_lines: lines.len(),
        lines: rendered.lines,
        skipped: rendered.skipped,
        bytes: rendered.text.len(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::state::ConsoleLevel;

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
                "ghs-app-console-log-{label}-{}-{nanos}-{}",
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

    fn line(level: ConsoleLevel, text: &str) -> ConsoleLine {
        ConsoleLine {
            time: "10:42".to_string(),
            level,
            text: text.to_string(),
        }
    }

    fn buffer() -> Vec<ConsoleLine> {
        vec![
            line(ConsoleLevel::Info, "[VER:1.1f.20240604:]"),
            line(ConsoleLevel::Command, "> $I+"),
            line(ConsoleLevel::Ok, "ok"),
            line(ConsoleLevel::Warn, "Disconnected; telemetry is stale"),
            line(ConsoleLevel::Error, "error:9"),
        ]
    }

    #[test]
    fn log_text_has_a_timestamped_header_and_levels() {
        let rendered = render_console_log(&buffer(), ConsoleFilter::All, 1_700_000_000);
        assert!(rendered.text.starts_with("ghs-rs console log\n"));
        assert!(
            rendered.text.contains("Generated: 2023-11-14 22:13:20 UTC"),
            "{}",
            rendered.text
        );
        assert!(rendered.text.contains("Filter: ALL (full ring buffer)"));
        assert!(rendered.text.contains("Levels in ring buffer: ALL 5"));
        assert!(rendered.text.contains("10:42 CMD   > $I+\n"));
        assert!(rendered.text.contains("10:42 INFO  [VER:1.1f.20240604:]\n"));
        assert!(rendered.text.contains("10:42 OK    ok\n"));
        assert!(rendered.text.contains("10:42 WARN  Disconnected"));
        assert!(rendered.text.contains("10:42 ERROR error:9\n"));
        assert_eq!(rendered.lines, 5);
        assert_eq!(rendered.skipped, 0);
    }

    #[test]
    fn filtered_log_is_labeled_and_contains_only_matching_lines() {
        let rendered = render_console_log(&buffer(), ConsoleFilter::Warn, 1_700_000_000);
        assert!(
            rendered
                .text
                .contains("Filter: WARN (filtered view; ring buffer has 5 lines)"),
            "{}",
            rendered.text
        );
        assert!(rendered.text.contains("10:42 WARN  Disconnected"));
        assert!(!rendered.text.contains("> $I+"));
        assert!(!rendered.text.contains("[VER:"));
        assert_eq!(rendered.lines, 1);
    }

    #[test]
    fn fixed_export_path_lives_under_exports() {
        let dir = TempRoot::new("path");
        let path = console_log_path(&dir.path);
        assert!(path.starts_with(&dir.path));
        assert!(path.ends_with(Path::new("exports").join(CONSOLE_LOG_FILE_NAME)));
        assert!(!path.exists(), "building a path must not create the file");

        let export = write_console_log(&buffer(), ConsoleFilter::All, Some(&dir.path), 0)
            .expect("write log");
        assert_eq!(export.path, path);
        assert_eq!(
            export.bytes,
            fs::metadata(&path).expect("metadata").len() as usize
        );
        assert!(
            fs::read_to_string(&path)
                .expect("read log")
                .contains("[VER:1.1f.20240604:]")
        );
    }

    #[test]
    fn missing_config_root_is_refused_without_writing() {
        let error =
            write_console_log(&buffer(), ConsoleFilter::All, None, 0).expect_err("no config root");
        assert_eq!(error, ConsoleLogError::NoConfigRoot);
        assert!(error.to_string().contains("configuration directory"));
        assert!(error.path().is_none());
    }

    #[test]
    fn a_write_failure_reports_the_full_path_and_reason() {
        let dir = TempRoot::new("failure");
        // A regular file where the exports directory belongs makes the write
        // fail deterministically even when the test runs as root.
        fs::write(dir.path.join(EXPORT_DIRECTORY), b"not a directory").expect("blocking file");
        let error = write_console_log(&buffer(), ConsoleFilter::All, Some(&dir.path), 0)
            .expect_err("write must fail");
        let ConsoleLogError::Write { path, message } = &error else {
            panic!("expected a write error: {error}");
        };
        assert_eq!(path, &console_log_path(&dir.path));
        assert!(!message.is_empty());
        assert!(
            error.to_string().contains("console.log"),
            "error must name the target: {error}"
        );
    }

    #[test]
    fn oversized_export_is_truncated_at_the_explicit_limit() {
        let huge = ConsoleLine {
            time: "10:42".to_string(),
            level: ConsoleLevel::Info,
            // A single protocol line larger than the file budget.
            text: "x".repeat(CONSOLE_LOG_MAX_BYTES + 1),
        };
        let rendered = render_console_log(&[huge], ConsoleFilter::All, 0);
        assert_eq!(rendered.lines, 0);
        assert_eq!(rendered.skipped, 1);
        assert!(
            rendered.text.len() <= CONSOLE_LOG_MAX_BYTES,
            "{} > {}",
            rendered.text.len(),
            CONSOLE_LOG_MAX_BYTES
        );
        assert!(rendered.text.contains("export truncated: 1 matching lines"));
    }

    #[test]
    fn skipped_count_reports_lines_that_did_not_fit() {
        let mut lines = Vec::new();
        // Each line is ~2 KiB, so only a few dozen fit in 64 KiB.
        for index in 0..200 {
            lines.push(ConsoleLine {
                time: "10:42".to_string(),
                level: ConsoleLevel::Info,
                text: format!("line {index:03} {}", "y".repeat(2_000)),
            });
        }
        let rendered = render_console_log(&lines, ConsoleFilter::All, 0);
        assert!(rendered.lines > 0);
        assert!(rendered.skipped > 0);
        assert_eq!(rendered.lines + rendered.skipped, 200);
        assert!(rendered.text.len() <= CONSOLE_LOG_MAX_BYTES);
        assert!(rendered.text.contains(&format!(
            "export truncated: {} matching lines",
            rendered.skipped
        )));
    }
}
