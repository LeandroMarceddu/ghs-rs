//! Local `offsets.nc` export and validated, operator-confirmed restore.
//!
//! Export is local only: it reads the last live `$#` snapshot from
//! [`OffsetModel`] and writes one `G10 L2 P<n>` line per reported work
//! coordinate system under the app configuration directory. Nothing is sent to
//! the controller and no job is loaded.
//!
//! Restore validates a fixed file before it is offered. The file may contain
//! comments, blank lines, and `G10 L2 P<n> <axis><value>` lines only. The
//! explicit `LOAD AS PROGRAM` action re-reads and re-validates the file, then
//! hands the engine exactly one [`Command::LoadProgram`]. It never sends `G10`
//! itself, never starts the program, and never restores automatically. The
//! operator must press Start in the Job workspace, where the engine's usual
//! Idle/ready safety gateway remains the final authority.
//!
//! Only work-coordinate offsets are exported. `G92` is non-persistent,
//! `G28`/`G30` are stored positions rather than work offsets, and `HOME`,
//! `TLO`, `TLR`, `G51`, and `PRB` are not restore targets for this file. The
//! generated header states that omission in the file itself.

use std::fs;
use std::path::{Path, PathBuf};

use ghs_engine::{Axis as CoreAxis, Command, Wcs};

use crate::offsets::{MAX_OFFSET_MM, OffsetModel};

/// Fixed file name written under the configuration export directory.
pub const OFFSETS_FILE_NAME: &str = "offsets.nc";

/// Export subdirectory, shared with the height-map CSV and backup bundle.
pub const EXPORT_DIRECTORY: &str = "exports";

/// Largest accepted offsets file. A real export has nine command lines.
pub const MAX_OFFSETS_FILE_BYTES: usize = 64 * 1024;

/// Largest accepted line count, including comments and blank lines.
pub const MAX_OFFSETS_FILE_LINES: usize = 512;

/// Header written at the top of every export.
///
/// The comments deliberately avoid nested parentheses so the file is valid
/// G-code. "OMITTED" names the non-work-offset parameters the export leaves
/// out; the parser never accepts any of them as a command.
const HEADER: &str = "\
(ghs-rs work offset backup)
(Source: live controller work offset report. Values are millimetres.)
(Only G54 through G59.3 are included, one G10 L2 P<n> line per reported WCS.)
(OMITTED: G92 non-persistent, G28 and G30 stored positions, HOME, TLO, TLR, G51 scaling, PRB.)
(Not a full machine backup. Running this file overwrites the stored WCS offsets it lists.)
(Sets G21; every value in this file is millimetres.)
";

const G21_PREAMBLE: &str = "G21\n";

/// Full path of the fixed offsets backup for one configuration root.
#[must_use]
pub fn offsets_export_path(root: &Path) -> PathBuf {
    root.join(EXPORT_DIRECTORY).join(OFFSETS_FILE_NAME)
}

/// One parsed `G10 L2` command with normalized preview text.
#[derive(Clone, Debug, PartialEq)]
pub struct OffsetLine {
    /// 1-based line number in the source file.
    pub file_line: usize,
    /// Normalized command text, for example
    /// `G10 L2 P1 X-357.000 Y-757.000 Z-148.306`.
    pub text: String,
    /// Coordinate system selected by `P`.
    pub wcs: Wcs,
    /// X, Y and Z values in millimetres; missing axis words stay `None`.
    pub values_mm: [Option<f64>; 3],
}

/// A validated offsets file held for the preview sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct OffsetsFile {
    path: PathBuf,
    source: String,
    lines: Vec<OffsetLine>,
    wcs: Vec<Wcs>,
}

impl OffsetsFile {
    /// File that was read and validated.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Exact source text that `LOAD AS PROGRAM` hands to the engine.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Every accepted `G10 L2` line, in file order.
    #[must_use]
    pub fn lines(&self) -> &[OffsetLine] {
        &self.lines
    }

    /// Distinct work coordinate systems named by the file, in first-seen
    /// order. A file that repeats a system lists it once here.
    #[must_use]
    pub fn wcs(&self) -> &[Wcs] {
        &self.wcs
    }
}

/// Successful local export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OffsetsExport {
    /// Full path that was written.
    pub path: PathBuf,
    /// Number of `G10 L2` lines written.
    pub wcs_lines: usize,
}

/// Short result line for the offsets backup panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OffsetsBackupStatus {
    /// Whether the last action succeeded.
    pub ok: bool,
    /// Human-readable message; export messages include the full path.
    pub text: String,
}

/// Export result and validation preview state for the Offsets workspace.
///
/// This model never talks to a transport. It produces a validated
/// [`Command::LoadProgram`] only from [`OffsetsBackupModel::load_program`],
/// which is called by the explicit sheet button. There is no start command in
/// this module.
#[derive(Debug, Default)]
pub struct OffsetsBackupModel {
    preview: Option<OffsetsFile>,
    status: Option<OffsetsBackupStatus>,
}

impl OffsetsBackupModel {
    /// Validated file waiting in the preview sheet, if any.
    #[must_use]
    pub fn preview(&self) -> Option<&OffsetsFile> {
        self.preview.as_ref()
    }

    /// Result of the last export, validation, cancel, or load action.
    #[must_use]
    pub fn status(&self) -> Option<&OffsetsBackupStatus> {
        self.status.as_ref()
    }

    /// Build and write the fixed `offsets.nc` from the live `$#` snapshot.
    ///
    /// No command is sent and no job is loaded. The write is refused before it
    /// touches the file when `$#` has not reported a work offset or a value is
    /// non-finite or outside [`MAX_OFFSET_MM`].
    ///
    /// # Errors
    ///
    /// Returns the build, directory, or write message. The same message is
    /// stored in [`OffsetsBackupModel::status`].
    pub fn export(
        &mut self,
        model: &OffsetModel,
        root: Option<&Path>,
    ) -> Result<OffsetsExport, String> {
        let Some(root) = root else {
            let message = "the app configuration directory is unavailable".to_string();
            self.status = Some(OffsetsBackupStatus {
                ok: false,
                text: format!("Offsets export refused: {message}"),
            });
            return Err(message);
        };
        let path = offsets_export_path(root);
        let text = match build_offsets_nc(model) {
            Ok(text) => text,
            Err(message) => {
                self.status = Some(OffsetsBackupStatus {
                    ok: false,
                    text: format!("Offsets export refused: {message}"),
                });
                return Err(message);
            }
        };
        let wcs_lines = text
            .lines()
            .filter(|line| line.starts_with("G10 L2 P"))
            .count();
        if let Err(error) = write_offsets_nc(root, &text) {
            let message = format!("{}: {error}", path.display());
            self.status = Some(OffsetsBackupStatus {
                ok: false,
                text: format!("Offsets export failed: {message}"),
            });
            return Err(message);
        }
        let record = OffsetsExport { path, wcs_lines };
        self.status = Some(OffsetsBackupStatus {
            ok: true,
            text: format!(
                "Offsets exported to {} ({wcs_lines} WCS lines, millimetres)",
                record.path.display()
            ),
        });
        Ok(record)
    }

    /// Read and validate the fixed offsets file for the preview sheet.
    ///
    /// This is local validation only. It never loads or starts a program.
    ///
    /// # Errors
    ///
    /// Returns a path-qualified I/O or validation message. A refusal closes any
    /// previous preview, so a stale sheet cannot be confirmed.
    pub fn open_preview(&mut self, root: Option<&Path>) -> Result<(), String> {
        let Some(root) = root else {
            let message = "the app configuration directory is unavailable".to_string();
            self.preview = None;
            self.status = Some(OffsetsBackupStatus {
                ok: false,
                text: format!("Offsets file not loaded: {message}"),
            });
            return Err(message);
        };
        let path = offsets_export_path(root);
        match read_offsets_file(&path) {
            Ok(file) => {
                let lines = file.lines().len();
                self.preview = Some(file);
                self.status = Some(OffsetsBackupStatus {
                    ok: true,
                    text: format!(
                        "Offsets file validated: {} ({lines} G10 L2 lines). Nothing loaded or \
                         started.",
                        path.display()
                    ),
                });
                Ok(())
            }
            Err(error) => {
                self.preview = None;
                let message = format!("{}: {error}", path.display());
                self.status = Some(OffsetsBackupStatus {
                    ok: false,
                    text: format!("Offsets file not loaded: {message}"),
                });
                Err(message)
            }
        }
    }

    /// Close the preview sheet without doing anything else.
    ///
    /// The operator-facing CANCEL action records a status line; this silent
    /// variant is used when the connection goes away.
    pub fn close_preview(&mut self) {
        self.preview = None;
    }

    /// Close the preview sheet after CANCEL. Nothing is loaded or sent.
    pub fn cancel(&mut self) {
        self.preview = None;
        self.status = Some(OffsetsBackupStatus {
            ok: true,
            text: "Offset restore cancelled; nothing was loaded or sent".to_string(),
        });
    }

    /// Record an engine rejection after the preview was consumed.
    pub fn load_failed(&mut self, message: impl Into<String>) {
        self.status = Some(OffsetsBackupStatus {
            ok: false,
            text: format!("Offsets program not loaded: {}", message.into()),
        });
    }

    /// Re-validate the previewed file and produce the one load command.
    ///
    /// Only the explicit `LOAD AS PROGRAM` action calls this. The file is read
    /// again so a change between the preview and the confirmation is validated
    /// and refused rather than silently loaded. The returned command is always
    /// [`Command::LoadProgram`]; this method can never start the job.
    ///
    /// # Errors
    ///
    /// Returns a message when no preview is open, the file changed and is no
    /// longer valid, or the file cannot be read.
    pub fn load_program(&mut self) -> Result<Command, String> {
        let Some(preview) = self.preview.as_ref() else {
            let message = "no validated offsets file is open".to_string();
            self.status = Some(OffsetsBackupStatus {
                ok: false,
                text: format!("Offsets program not loaded: {message}"),
            });
            return Err(message);
        };
        let path = preview.path().to_path_buf();
        let previewed_source = preview.source().to_string();
        let file = read_offsets_file(&path);
        // The confirmation is consumed either way; a failed re-read cannot be
        // retried by pressing the same stale sheet again.
        self.preview = None;
        match file {
            Ok(file) if file.source() != previewed_source => {
                let message = format!(
                    "{}: the file changed since the preview; open it again",
                    path.display()
                );
                self.status = Some(OffsetsBackupStatus {
                    ok: false,
                    text: format!("Offsets program not loaded: {message}"),
                });
                Err(message)
            }
            Ok(file) => {
                let lines = file.lines().len();
                let source = file.source().to_string();
                self.status = Some(OffsetsBackupStatus {
                    ok: true,
                    text: format!(
                        "Offsets program loaded from {} ({lines} lines); not started. Review it \
                         in Job and press Start only in Idle with a ready controller.",
                        path.display()
                    ),
                });
                Ok(Command::LoadProgram {
                    name: OFFSETS_FILE_NAME.to_string(),
                    source,
                })
            }
            Err(error) => {
                let message = format!("{}: {error}", path.display());
                self.status = Some(OffsetsBackupStatus {
                    ok: false,
                    text: format!("Offsets program not loaded: {message}"),
                });
                Err(message)
            }
        }
    }
}

/// Build the export text from the last live `$#` snapshot.
///
/// One line is emitted per reported work coordinate system, in `$#` order, in
/// millimetres regardless of the display unit. `G92`, `G28`/`G30`, `HOME`,
/// `TLO`, `TLR`, `G51`, and `PRB` are never emitted as commands.
///
/// # Errors
///
/// Returns a message when `$#` has not reported, no work coordinate system was
/// reported, or a reported value is non-finite or outside [`MAX_OFFSET_MM`].
pub fn build_offsets_nc(model: &OffsetModel) -> Result<String, String> {
    if !model.reported() {
        return Err("$# has not reported the offsets yet; refresh them before exporting".into());
    }
    let mut text = String::from(HEADER);
    text.push_str(G21_PREAMBLE);
    let mut wcs_lines = 0usize;
    for wcs in Wcs::ALL {
        let Some(position) = model.wcs_position(wcs) else {
            continue;
        };
        let values = [
            position.get(CoreAxis::X),
            position.get(CoreAxis::Y),
            position.get(CoreAxis::Z),
        ];
        for (label, value) in ["X", "Y", "Z"].into_iter().zip(values) {
            if !value.is_finite() {
                return Err(format!(
                    "the {} {label} offset is not finite; export refused",
                    wcs.as_str()
                ));
            }
            if value.abs() > MAX_OFFSET_MM {
                return Err(format!(
                    "the {} {label} offset is outside +/-{MAX_OFFSET_MM:.0} mm; export refused",
                    wcs.as_str()
                ));
            }
        }
        text.push_str(&format!(
            "G10 L2 P{} X{} Y{} Z{}\n",
            wcs.index() + 1,
            format_mm(values[0]),
            format_mm(values[1]),
            format_mm(values[2])
        ));
        wcs_lines += 1;
    }
    if wcs_lines == 0 {
        return Err("$# did not report any work coordinate offsets; export refused".into());
    }
    Ok(text)
}

/// Write one export under `root/exports/offsets.nc`, creating the directory.
///
/// # Errors
///
/// Returns a directory or write message.
pub fn write_offsets_nc(root: &Path, text: &str) -> Result<PathBuf, String> {
    let path = offsets_export_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&path, text.as_bytes()).map_err(|error| error.to_string())?;
    Ok(path)
}

/// Read and validate an offsets file from disk.
///
/// # Errors
///
/// Returns a size, I/O, UTF-8, or line-validation message.
pub fn read_offsets_file(path: &Path) -> Result<OffsetsFile, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("the path is not a regular file".to_string());
    }
    if metadata.len() > MAX_OFFSETS_FILE_BYTES as u64 {
        return Err(format!(
            "the file is larger than {MAX_OFFSETS_FILE_BYTES} bytes"
        ));
    }
    let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
    parse_offsets_nc(path.to_path_buf(), source)
}

/// Validate offsets source text without touching the controller.
///
/// Accepted content is limited to blank lines, comments, and
/// `G10 L2 P<n> <axis><value>` commands. Every other word, system command,
/// motion, `G92`, `G28`, `G30`, unknown code, malformed value, out-of-range
/// value, and duplicate axis word is refused with the 1-based file line.
///
/// # Errors
///
/// Returns the first validation message.
pub fn parse_offsets_nc(path: PathBuf, source: String) -> Result<OffsetsFile, String> {
    if source.len() > MAX_OFFSETS_FILE_BYTES {
        return Err(format!(
            "the file is larger than {MAX_OFFSETS_FILE_BYTES} bytes"
        ));
    }
    if source.lines().count() > MAX_OFFSETS_FILE_LINES {
        return Err(format!(
            "the file has more than {MAX_OFFSETS_FILE_LINES} lines"
        ));
    }
    let mut lines = Vec::new();
    let mut saw_g21 = false;
    for (index, raw) in source.lines().enumerate() {
        let file_line = index + 1;
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("G21") {
            if saw_g21 {
                return Err(format!("line {file_line}: duplicate G21 preamble"));
            }
            saw_g21 = true;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("G20") {
            return Err(format!(
                "line {file_line}: G20 is not allowed; offsets must be in G21 millimetres"
            ));
        }
        if let Some(line) = parse_offsets_line(raw, file_line)? {
            lines.push(line);
        }
    }
    if !saw_g21 {
        return Err(
            "missing G21 millimetre preamble; refusing an offsets file whose units \
                    are not explicit"
                .to_string(),
        );
    }
    if lines.is_empty() {
        return Err("no G10 L2 lines were found".to_string());
    }
    let mut wcs = Vec::new();
    for line in &lines {
        if !wcs.contains(&line.wcs) {
            wcs.push(line.wcs);
        }
    }
    Ok(OffsetsFile {
        path,
        source,
        lines,
        wcs,
    })
}

fn parse_offsets_line(raw: &str, file_line: usize) -> Result<Option<OffsetLine>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.starts_with('(') {
        check_comment(trimmed).map_err(|error| format!("line {file_line}: {error}"))?;
        return Ok(None);
    }
    if trimmed.starts_with(';') {
        return Ok(None);
    }
    let code =
        split_trailing_comment(trimmed).map_err(|error| format!("line {file_line}: {error}"))?;
    if code.is_empty() {
        return Ok(None);
    }
    let words = scan_words(code).map_err(|error| format!("line {file_line}: {error}"))?;
    let (wcs, values) =
        parse_g10_words(&words).map_err(|error| format!("line {file_line}: {error}"))?;
    Ok(Some(OffsetLine {
        file_line,
        text: canonical_text(wcs, &values),
        wcs,
        values_mm: values,
    }))
}

/// Validate a full-line comment, or the parenthesized tail of a command.
fn check_comment(text: &str) -> Result<(), String> {
    if !text.ends_with(')') {
        return Err("unterminated '(' comment".to_string());
    }
    let inner = &text[1..text.len() - 1];
    if inner.contains('(') || inner.contains(')') {
        return Err("nested parentheses are not allowed in a comment".to_string());
    }
    Ok(())
}

/// Strip an optional trailing `(...)` or `;` comment.
fn split_trailing_comment(text: &str) -> Result<&str, String> {
    let Some(index) = text.find(['(', ';']) else {
        return Ok(text);
    };
    let tail = &text[index..];
    if tail.starts_with('(') {
        check_comment(tail)?;
    }
    Ok(text[..index].trim())
}

/// A scanned G-code word with its numeric value.
struct Word {
    letter: char,
    value: f64,
}

/// Split compact or spaced G-code words.
///
/// Only a leading letter and a decimal number are accepted. Exponents and
/// identifiers are rejected rather than guessed.
fn scan_words(code: &str) -> Result<Vec<Word>, String> {
    let bytes = code.as_bytes();
    let mut words = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if !byte.is_ascii_alphabetic() {
            return Err(format!("unexpected character '{}'", byte as char));
        }
        let letter = (byte as char).to_ascii_uppercase();
        index += 1;
        let start = index;
        while index < bytes.len() && matches!(bytes[index], b'0'..=b'9' | b'+' | b'-' | b'.') {
            index += 1;
        }
        if start == index {
            return Err(format!("word {letter} has no numeric value"));
        }
        let value_text = &code[start..index];
        let value: f64 = value_text
            .parse()
            .map_err(|_| format!("'{value_text}' is not a number"))?;
        if !value.is_finite() {
            return Err(format!("word {letter} must be finite"));
        }
        words.push(Word { letter, value });
    }
    if words.is_empty() {
        return Err("the line has no command".to_string());
    }
    Ok(words)
}

/// Validate the allowed word set and extract one work-offset command.
fn parse_g10_words(words: &[Word]) -> Result<(Wcs, [Option<f64>; 3]), String> {
    let mut g: Option<f64> = None;
    let mut l: Option<f64> = None;
    let mut p: Option<f64> = None;
    let mut values = [None, None, None];
    for word in words {
        match word.letter {
            'G' => {
                if g.replace(word.value).is_some() {
                    return Err("a line may carry only one G word".to_string());
                }
            }
            'L' => {
                if l.replace(word.value).is_some() {
                    return Err("a line may carry only one L word".to_string());
                }
            }
            'P' => {
                if p.replace(word.value).is_some() {
                    return Err("a line may carry only one P word".to_string());
                }
            }
            'X' | 'Y' | 'Z' => {
                let axis = match word.letter {
                    'X' => 0,
                    'Y' => 1,
                    _ => 2,
                };
                if values[axis].replace(word.value).is_some() {
                    return Err(format!("the {} axis word is repeated", word.letter));
                }
                if word.value.abs() > MAX_OFFSET_MM {
                    return Err(format!(
                        "the {} value is outside +/-{MAX_OFFSET_MM:.0} mm",
                        word.letter
                    ));
                }
            }
            other => {
                return Err(format!(
                    "word '{other}' is not allowed; only G10 L2 P X Y Z are accepted"
                ));
            }
        }
    }
    if g != Some(10.0) {
        return Err("only G10 L2 commands are accepted".to_string());
    }
    if l != Some(2.0) {
        return Err("only G10 L2 commands are accepted".to_string());
    }
    let Some(p) = p else {
        return Err("G10 L2 requires a P word selecting G54 through G59.3".to_string());
    };
    if p.fract() != 0.0 || !(1.0..=9.0).contains(&p) {
        return Err("P must be 1 through 9 (G54 through G59.3)".to_string());
    }
    if values.iter().all(Option::is_none) {
        return Err("G10 L2 requires at least one axis word".to_string());
    }
    let wcs = Wcs::from_index(p as usize - 1)
        .ok_or_else(|| "P must be 1 through 9 (G54 through G59.3)".to_string())?;
    Ok((wcs, values))
}

fn canonical_text(wcs: Wcs, values: &[Option<f64>; 3]) -> String {
    let mut text = format!("G10 L2 P{}", wcs.index() + 1);
    for (index, label) in ["X", "Y", "Z"].into_iter().enumerate() {
        if let Some(value) = values[index] {
            text.push_str(&format!(" {label}{}", format_mm(value)));
        }
    }
    text
}

/// Format one millimetre value without a negative zero.
fn format_mm(value: f64) -> String {
    let text = format!("{value:.3}");
    if text == "-0.000" {
        "0.000".to_string()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use ghs_engine::{Offsets, Position, ProbeResult};

    use super::*;
    use crate::state::{Axis, Units};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "ghs-app-offsets-{label}-{}-{nanos}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn model(units: Units) -> OffsetModel {
        let mut model = OffsetModel::default();
        model.set_context(units, &[Axis::X, Axis::Y, Axis::Z], 0);
        model
    }

    fn all_wcs_offsets() -> Offsets {
        let mut offsets = Offsets::default();
        for wcs in Wcs::ALL {
            let seed = f64::from(u32::try_from(wcs.index()).expect("index")) * 10.0;
            offsets.wcs[wcs.index()] = Some(Position::xyz(seed + 1.0, seed + 2.0, seed + 3.0));
        }
        offsets
    }

    fn full_offsets() -> Offsets {
        let mut offsets = all_wcs_offsets();
        offsets.g28 = Some(Position::xyz(1.0, 1.0, -2.0));
        offsets.g30 = Some(Position::xyz(2.0, 2.0, 2.0));
        offsets.g92 = Some(Position::xyz(0.5, 0.0, 0.0));
        offsets.home = Some(Position::xyz(-299.0, -353.0, -2.0));
        offsets.tlo = Some(vec![0.0, 0.0, 3.0]);
        offsets.tlr = Some(12.5);
        offsets.scaling = Some(Position::xyz(1.0, 2.0, 3.0));
        offsets.probe = Some(ProbeResult::new(true, Position::xyz(-1.0, 2.0, -3.0)));
        offsets
    }

    fn command_lines(text: &str) -> Vec<&str> {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('('))
            .collect()
    }

    #[test]
    fn export_covers_every_reported_wcs_with_exact_g10_l2_lines() {
        let mut model = model(Units::Millimetre);
        model.apply_engine(&full_offsets());
        let text = build_offsets_nc(&model).expect("export text");
        let lines = command_lines(&text);
        assert_eq!(lines.len(), 10);
        assert_eq!(lines[0], "G21");
        for (index, wcs) in Wcs::ALL.into_iter().enumerate() {
            let seed = f64::from(u32::try_from(index).expect("index")) * 10.0;
            let expected = format!(
                "G10 L2 P{} X{:.3} Y{:.3} Z{:.3}",
                index + 1,
                seed + 1.0,
                seed + 2.0,
                seed + 3.0
            );
            assert_eq!(lines[index + 1], expected, "{}", wcs.as_str());
        }
        assert!(text.starts_with("(ghs-rs work offset backup)\n"));
        assert!(text.contains("G21"));
        ghs_gcode::parse_program(&text).expect("the engine parser accepts the export");
    }

    #[test]
    fn export_excludes_non_wcs_parameters_and_refuses_without_dollar_hash() {
        let reported = {
            let mut model = model(Units::Millimetre);
            model.apply_engine(&full_offsets());
            build_offsets_nc(&model).expect("export")
        };
        for line in command_lines(&reported) {
            if line == "G21" {
                continue;
            }
            let words: Vec<&str> = line.split_whitespace().collect();
            assert_eq!(words.first().copied(), Some("G10"), "{line}");
            assert_eq!(words.get(1).copied(), Some("L2"), "{line}");
            for word in &words[2..] {
                assert!(
                    matches!(word.chars().next(), Some('P' | 'X' | 'Y' | 'Z')),
                    "unexpected word {word} in {line}"
                );
            }
            for forbidden in ["G92", "G28", "G30", "HOME", "TLO", "TLR", "G51", "PRB"] {
                assert!(!line.contains(forbidden), "{forbidden} in {line}");
            }
        }
        assert!(reported.contains("OMITTED: G92"));
        assert!(reported.contains("G28 and G30"));
        assert!(reported.contains("TLO, TLR, G51 scaling, PRB"));

        let mut blank = model(Units::Millimetre);
        let error = build_offsets_nc(&blank).expect_err("no $# snapshot");
        assert!(error.contains("$#"), "{error}");
        blank.apply_engine(&Offsets::default());
        let error = build_offsets_nc(&blank).expect_err("no reported WCS");
        assert!(error.contains("did not report"), "{error}");
    }

    #[test]
    fn export_writes_and_reads_the_fixed_file_under_a_temporary_root() {
        let dir = TempDir::new("file");
        let mut model = model(Units::Millimetre);
        model.apply_engine(&all_wcs_offsets());
        let mut backup = OffsetsBackupModel::default();
        let export = backup
            .export(&model, Some(&dir.path))
            .expect("write export");
        let expected_path = dir.path.join("exports").join("offsets.nc");
        assert_eq!(export.path, expected_path);
        assert_eq!(export.wcs_lines, 9);
        assert!(expected_path.exists());

        let read = read_offsets_file(&expected_path).expect("read back");
        assert_eq!(read.path(), expected_path);
        assert_eq!(read.lines().len(), 9);
        assert_eq!(read.wcs(), &Wcs::ALL);
        assert_eq!(read.lines()[0].wcs, Wcs::G54);
        assert_eq!(read.lines()[8].wcs, Wcs::G59_3);
        assert_eq!(read.lines()[0].values_mm, [Some(1.0), Some(2.0), Some(3.0)]);
        assert!(backup.status().is_some_and(|status| status.ok));
    }

    #[test]
    fn export_uses_millimetres_in_inch_display_mode() {
        let mut model = model(Units::Inch);
        let mut offsets = Offsets::default();
        offsets.wcs[Wcs::G54.index()] = Some(Position::xyz(25.4, -50.8, 0.0));
        model.apply_engine(&offsets);
        assert!((model.wcs_row(0).expect("row").values[0].expect("X") - 1.0).abs() < 1.0e-9);
        let text = build_offsets_nc(&model).expect("export");
        assert!(text.contains("G10 L2 P1 X25.400 Y-50.800 Z0.000"), "{text}");
    }

    fn write_file(dir: &TempDir, source: &str) -> PathBuf {
        let path = dir.path.join("exports").join("offsets.nc");
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(&path, source).expect("write");
        path
    }

    const VALID: &str = "\
(hand-made work offsets)

G21

G10 L2 P1 X-357 Y-757 Z-148.306 (G54)
g10 l2 p9 x1.5 y-2.5
; G92 and G28 are comments here, not commands
";

    #[test]
    fn preview_accepts_a_valid_file_and_lists_every_line() {
        let dir = TempDir::new("valid");
        let path = write_file(&dir, VALID);
        let file = read_offsets_file(&path).expect("valid file");
        assert_eq!(file.lines().len(), 2);
        assert_eq!(file.lines()[0].file_line, 5);
        assert_eq!(file.lines()[0].wcs, Wcs::G54);
        assert_eq!(
            file.lines()[0].text,
            "G10 L2 P1 X-357.000 Y-757.000 Z-148.306"
        );
        assert_eq!(file.lines()[1].wcs, Wcs::G59_3);
        assert_eq!(file.lines()[1].text, "G10 L2 P9 X1.500 Y-2.500");
        assert_eq!(file.wcs(), &[Wcs::G54, Wcs::G59_3]);
        assert_eq!(file.source(), VALID);
    }

    #[test]
    fn preview_rejects_motion_system_g92_g28_g30_and_malformed_lines() {
        let rejected = [
            ("G0 X1", "only G10 L2"),
            ("G1 X1 F100", "only G10 L2"),
            ("G2 X1 Y1 I1 J0", "only G10 L2"),
            ("G92 X0", "only G10 L2"),
            ("G92.1", "only G10 L2"),
            ("G28", "only G10 L2"),
            ("G30", "only G10 L2"),
            ("$H", "unexpected character"),
            ("M3 S1000", "only G10 L2"),
            ("G10 L20 P1 X1", "only G10 L2"),
            ("G10 L2 P0 X1", "P must be 1 through 9"),
            ("G10 L2 P10 X1", "P must be 1 through 9"),
            ("G10 L2 P1", "at least one axis"),
            ("G10 L2 X1", "requires a P word"),
            ("G10 L2 P1 Q1", "not allowed"),
            ("G10 L2 P1 X1 X2", "repeated"),
            ("G10 L2 P1 X100000.001", "outside"),
            ("G10 L2 P1 Xnan", "value"),
            ("G10 L2 P1 X1 (open", "unterminated"),
            ("G20\nG10 L2 P1 X1", "G20"),
            ("G21\nG21\nG10 L2 P1 X1", "duplicate G21"),
            ("(nested (comment))", "nested"),
            ("hello", "no numeric value"),
            ("123", "unexpected character"),
        ];
        for (source, needle) in rejected {
            let error =
                parse_offsets_nc(PathBuf::from("test.nc"), source.to_string()).expect_err(source);
            assert!(
                error.contains(needle),
                "for '{source}' expected '{needle}', got '{error}'"
            );
        }
        let error = parse_offsets_nc(PathBuf::from("test.nc"), "G10 L2 P1 X1\n".into())
            .expect_err("missing G21 must be refused");
        assert!(error.contains("missing G21"), "{error}");
        assert!(parse_offsets_nc(PathBuf::from("test.nc"), "(only a comment)\n\n".into()).is_err());
    }

    #[test]
    fn restore_loads_only_after_confirmation_and_never_starts() {
        let dir = TempDir::new("restore");
        let source = "G21\nG10 L2 P1 X1 Y2 Z3\n";
        write_file(&dir, source);
        let mut backup = OffsetsBackupModel::default();
        let error = backup.load_program().expect_err("nothing previewed");
        assert!(error.contains("no validated"), "{error}");

        backup
            .open_preview(Some(&dir.path))
            .expect("validate fixed file");
        assert!(backup.preview().is_some());
        let command = backup.load_program().expect("explicit load");
        match command {
            Command::LoadProgram {
                name,
                source: loaded,
            } => {
                assert_eq!(name, OFFSETS_FILE_NAME);
                assert_eq!(loaded, source);
                ghs_gcode::parse_program(&loaded).expect("the loaded source parses");
            }
            other => panic!("restore must only load a program, got {other:?}"),
        }
        assert!(backup.preview().is_none(), "the confirmation is consumed");
        assert!(
            backup
                .status()
                .is_some_and(|status| status.ok && status.text.contains("not started"))
        );
        assert!(
            backup.load_program().is_err(),
            "a second press must not reuse the same confirmation"
        );
    }

    #[test]
    fn cancel_sends_nothing_and_restore_revalidates_the_file() {
        let dir = TempDir::new("cancel");
        let path = write_file(&dir, "G21\nG10 L2 P1 X1\n");
        let mut backup = OffsetsBackupModel::default();
        backup
            .open_preview(Some(&dir.path))
            .expect("validate fixed file");
        backup.cancel();
        assert!(backup.preview().is_none());
        assert!(
            backup
                .status()
                .is_some_and(|status| status.text.contains("cancelled")),
            "cancel records a no-op status"
        );
        assert!(
            backup.load_program().is_err(),
            "cancel leaves nothing to load"
        );

        backup.open_preview(Some(&dir.path)).expect("reopen");
        fs::write(&path, "G0 X10\n").expect("replace with motion");
        let error = backup
            .load_program()
            .expect_err("a changed invalid file is refused");
        assert!(error.contains("only G10 L2"), "{error}");
        assert!(backup.preview().is_none(), "the stale sheet is consumed");

        fs::write(&path, "G21\nG10 L2 P1 X1\n").expect("valid replacement");
        backup
            .open_preview(Some(&dir.path))
            .expect("reopen valid file");
        fs::write(&path, "G21\nG10 L2 P1 X2\n").expect("changed but still valid");
        let error = backup
            .load_program()
            .expect_err("a valid but unseen change is refused");
        assert!(error.contains("changed since the preview"), "{error}");
        assert!(backup.preview().is_none(), "the stale sheet is consumed");
    }

    #[test]
    fn non_finite_and_out_of_range_values_are_refused() {
        for value in [f64::NAN, f64::INFINITY, MAX_OFFSET_MM + 1.0] {
            let mut model = model(Units::Millimetre);
            let mut offsets = Offsets::default();
            offsets.wcs[Wcs::G54.index()] = Some(Position::xyz(value, 0.0, 0.0));
            model.apply_engine(&offsets);
            assert!(build_offsets_nc(&model).is_err(), "{value} must be refused");
        }
        let mut backup = OffsetsBackupModel::default();
        let dir = TempDir::new("range");
        let error = backup
            .export(&model(Units::Millimetre), Some(&dir.path))
            .expect_err("no $# report");
        assert!(error.contains("$#"), "{error}");
        assert!(!dir.path.join("exports").join("offsets.nc").exists());
    }
}
