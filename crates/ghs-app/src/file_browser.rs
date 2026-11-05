//! Touch-friendly in-app program file browser and bounded source reader.
//!
//! The browser lists one directory at a time: visible subdirectories and files
//! whose extension is `.nc`, `.gcode`, `.tap`, or `.ngc` (ASCII
//! case-insensitive). Hidden entries and symlinks are skipped, so navigation
//! can never loop through a link or surprise the operator with a dotfile.
//!
//! This module never sends a command and never parses G-code. The selected
//! file text is validated here and handed to
//! `ghs_engine::Command::LoadProgram` by the shell; the engine owns parsing and
//! emits `JobLoaded`. Loading never starts a job, applies a height map, or
//! changes offsets or settings.

use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Largest program file accepted from the file browser: 20 MB.
///
/// The engine accepts more, but a touch panel must stay responsive; the bound
/// also rejects accidental disk images and logs.
pub const MAX_PROGRAM_BYTES: u64 = 20 * 1024 * 1024;

/// Extensions shown by the browser and accepted by [`read_program`].
pub const PROGRAM_EXTENSIONS: [&str; 4] = ["nc", "gcode", "tap", "ngc"];

/// One visible directory entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserEntry {
    /// Display name (file or directory name only).
    pub name: String,
    /// Absolute path used to enter or load the entry.
    pub path: PathBuf,
    /// `true` for a directory.
    pub is_dir: bool,
    /// File length in bytes; zero for a directory.
    pub size: u64,
}

impl BrowserEntry {
    /// Short kind label for the row badge: `DIR` or the upper-case extension.
    #[must_use]
    pub fn kind_label(&self) -> String {
        if self.is_dir {
            return "DIR".to_string();
        }
        self.path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_uppercase)
            .unwrap_or_else(|| "FILE".to_string())
    }
}

/// Why a directory could not be listed or entered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowseError {
    /// The path is not a directory.
    NotADirectory(String),
    /// A file was typed into the path field but its extension is not a program.
    NotAProgramFile(String),
    /// The filesystem returned an error.
    Io(String),
}

impl fmt::Display for BrowseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrowseError::NotADirectory(path) => {
                write!(formatter, "'{path}' is not a readable folder")
            }
            BrowseError::NotAProgramFile(name) => write!(
                formatter,
                "'{name}' is not a .nc, .gcode, .tap, or .ngc program"
            ),
            BrowseError::Io(message) => formatter.write_str(message),
        }
    }
}

/// Why a selected program file was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramFileError {
    /// The path is not a regular file.
    NotAFile(String),
    /// The extension is outside [`PROGRAM_EXTENSIONS`].
    UnsupportedExtension(String),
    /// The file is larger than [`MAX_PROGRAM_BYTES`].
    TooLarge {
        /// Actual length in bytes.
        bytes: u64,
        /// Configured limit in bytes.
        limit: u64,
    },
    /// The bytes contain a NUL, so this is not a text program.
    Binary,
    /// The bytes are not valid UTF-8.
    NotUtf8,
    /// The file has no non-whitespace text.
    Empty,
    /// The file could not be read.
    Io(String),
}

impl fmt::Display for ProgramFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProgramFileError::NotAFile(path) => write!(formatter, "'{path}' is not a regular file"),
            ProgramFileError::UnsupportedExtension(name) => write!(
                formatter,
                "'{name}' is not a .nc, .gcode, .tap, or .ngc program"
            ),
            ProgramFileError::TooLarge { bytes, limit } => write!(
                formatter,
                "file is {}; the limit for the file browser is {}",
                format_size(*bytes),
                format_size(*limit)
            ),
            ProgramFileError::Binary => {
                formatter.write_str("file contains binary data, not a G-code program")
            }
            ProgramFileError::NotUtf8 => formatter.write_str("file is not valid UTF-8 text"),
            ProgramFileError::Empty => formatter.write_str("file is empty"),
            ProgramFileError::Io(message) => formatter.write_str(message),
        }
    }
}

/// One validated program file ready for `Command::LoadProgram`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramFile {
    /// File name shown in the job header and program list.
    pub name: String,
    /// Complete UTF-8 source text.
    pub source: String,
}

impl ProgramFile {
    /// Source length in bytes.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.source.len()
    }
}

/// True when the path ends in a supported program extension (ASCII
/// case-insensitive).
#[must_use]
pub fn is_program_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            PROGRAM_EXTENSIONS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

/// Start directory for the first browser open: `$HOME`, else the current
/// directory, else the filesystem root.
#[must_use]
pub fn default_root() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Human size for rows and messages.
#[must_use]
pub fn format_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    let value = bytes as f64;
    if value >= MIB {
        format!("{:.1} MB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KB", value / KIB)
    } else {
        format!("{bytes} B")
    }
}

/// Abbreviate a path from the left when it is longer than `max_chars`.
///
/// The tail carries the current folder and file name, so it is the useful part
/// on a narrow touch panel.
#[must_use]
pub fn compact_path(path: &Path, max_chars: usize) -> String {
    let text = path.to_string_lossy().into_owned();
    let count = text.chars().count();
    if max_chars == 0 || count <= max_chars {
        return text;
    }
    let keep = max_chars.saturating_sub(3);
    let skip = count - keep;
    let tail: String = text.chars().skip(skip).collect();
    format!("...{tail}")
}

/// List one directory, without recursion.
///
/// Entries are sorted with directories first and then case-insensitively by
/// name. Hidden names (leading `.`) and symlinks are skipped.
///
/// # Errors
///
/// Returns [`BrowseError`] when the path is not a directory or cannot be read.
pub fn list_directory(directory: &Path) -> Result<Vec<BrowserEntry>, BrowseError> {
    if !directory.is_dir() {
        return Err(BrowseError::NotADirectory(display_path(directory)));
    }
    let read_dir = fs::read_dir(directory)
        .map_err(|error| BrowseError::Io(format!("{}: {error}", display_path(directory))))?;
    let mut entries = Vec::new();
    for entry in read_dir {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        // Symlinks are skipped instead of followed: a link can form a cycle and
        // makes the visible tree differ from the real one.
        if file_type.is_symlink() {
            continue;
        }
        let is_dir = file_type.is_dir();
        if !is_dir && !is_program_file(&entry.path()) {
            continue;
        }
        let size = if is_dir {
            0
        } else {
            entry.metadata().map(|metadata| metadata.len()).unwrap_or(0)
        };
        entries.push(BrowserEntry {
            name,
            path: entry.path(),
            is_dir,
            size,
        });
    }
    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(entries)
}

/// Read and validate a program file.
///
/// The size is checked from metadata before reading and again after the bounded
/// read, so a file that grows during the read cannot cross the limit.
///
/// # Errors
///
/// Returns [`ProgramFileError`] for an unsupported extension, a non-file, an
/// oversized file, binary or non-UTF-8 bytes, and an empty file.
pub fn read_program(path: &Path) -> Result<ProgramFile, ProgramFileError> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| display_path(path));
    if !is_program_file(path) {
        return Err(ProgramFileError::UnsupportedExtension(name));
    }
    let metadata =
        fs::metadata(path).map_err(|error| ProgramFileError::Io(format!("{name}: {error}")))?;
    if !metadata.is_file() {
        return Err(ProgramFileError::NotAFile(display_path(path)));
    }
    if metadata.len() > MAX_PROGRAM_BYTES {
        return Err(ProgramFileError::TooLarge {
            bytes: metadata.len(),
            limit: MAX_PROGRAM_BYTES,
        });
    }
    let file =
        fs::File::open(path).map_err(|error| ProgramFileError::Io(format!("{name}: {error}")))?;
    let mut bytes = Vec::new();
    file.take(MAX_PROGRAM_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ProgramFileError::Io(format!("{name}: {error}")))?;
    if bytes.len() as u64 > MAX_PROGRAM_BYTES {
        return Err(ProgramFileError::TooLarge {
            bytes: bytes.len() as u64,
            limit: MAX_PROGRAM_BYTES,
        });
    }
    if bytes.contains(&0) {
        return Err(ProgramFileError::Binary);
    }
    let text = String::from_utf8(bytes).map_err(|_| ProgramFileError::NotUtf8)?;
    // A UTF-8 BOM is not G-code; dropping it keeps the first word on line one.
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    if text.trim().is_empty() {
        return Err(ProgramFileError::Empty);
    }
    Ok(ProgramFile {
        name,
        source: text.to_string(),
    })
}

/// Validate the paths from an external OS file drop.
///
/// Exactly one regular file is accepted. The file extension, size, and text
/// are validated later by [`read_program`], so a dropped text file that is not
/// a program gets the same clear refusal as a browser selection. Directories,
/// multiple items, and empty drops are refused before any read.
///
/// # Errors
///
/// Returns a short message for the console or inline error when the drop does
/// not contain exactly one regular file.
pub fn dropped_program_path(paths: &[PathBuf]) -> Result<&Path, String> {
    match paths {
        [] => Err("Drop one .nc, .gcode, .tap, or .ngc program file".to_string()),
        [path] => {
            if path.is_file() {
                Ok(path)
            } else {
                Err(format!(
                    "'{}' is not a regular file; drop one program file",
                    display_path(path)
                ))
            }
        }
        _ => Err(format!(
            "Drop exactly one program file; {} items were dropped",
            paths.len()
        )),
    }
}

/// State of one open file browser.
///
/// The model is pure data so navigation, filtering, and selection can be tested
/// without a window. The shell owns the GPUI input, scroll handle, and modal.
#[derive(Clone, Debug)]
pub struct FileBrowser {
    cwd: PathBuf,
    entries: Arc<[BrowserEntry]>,
    selected: Option<usize>,
    path_draft: String,
    error: Option<String>,
}

impl FileBrowser {
    /// Open `start` (a directory, or a file whose folder is opened).
    ///
    /// A path that cannot be resolved falls back to the current directory; a
    /// folder that cannot be read still opens and reports the error in the
    /// modal.
    #[must_use]
    pub fn open(start: &Path) -> Self {
        let (cwd, preselected) = locate(start);
        let mut browser = Self {
            cwd,
            entries: Arc::from(Vec::new()),
            selected: None,
            path_draft: String::new(),
            error: None,
        };
        browser.reload();
        if let Some(path) = preselected {
            browser.select_path(&path);
        }
        browser
    }

    /// Current directory.
    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Current directory as display text.
    #[must_use]
    pub fn path_display(&self) -> String {
        display_path(&self.cwd)
    }

    /// Replace the path field draft. Navigation happens on [`Self::commit_path`].
    pub fn set_path_draft(&mut self, draft: impl Into<String>) {
        self.path_draft = draft.into();
    }

    /// Visible entries.
    #[must_use]
    pub fn entries(&self) -> &[BrowserEntry] {
        &self.entries
    }

    /// Shared entry list for the virtualized list closure.
    #[must_use]
    pub fn entries_arc(&self) -> Arc<[BrowserEntry]> {
        Arc::clone(&self.entries)
    }

    /// Selected entry index, if a file is selected.
    #[must_use]
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// Selected file, if any.
    #[must_use]
    pub fn selected_entry(&self) -> Option<&BrowserEntry> {
        self.selected
            .and_then(|index| self.entries.get(index))
            .filter(|entry| !entry.is_dir)
    }

    /// Current error text shown by the modal.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Set an error supplied by the load path.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    /// Number of visible program files.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.entries.iter().filter(|entry| !entry.is_dir).count()
    }

    /// Select one visible file. Directories are not selectable.
    ///
    /// Returns `true` when the selection changed.
    pub fn select(&mut self, index: usize) -> bool {
        let Some(entry) = self.entries.get(index) else {
            return false;
        };
        if entry.is_dir {
            return false;
        }
        self.selected = Some(index);
        self.error = None;
        true
    }

    /// Select a file by path when it is visible in the current directory.
    pub fn select_path(&mut self, path: &Path) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.path == path) else {
            return false;
        };
        self.select(index)
    }

    /// Enter a visible directory.
    ///
    /// Returns `true` when the current directory changed.
    pub fn enter(&mut self, index: usize) -> bool {
        let Some(entry) = self.entries.get(index) else {
            return false;
        };
        if !entry.is_dir {
            return false;
        }
        self.cwd = entry.path.clone();
        self.reload();
        true
    }

    /// Move to the parent directory. Returns `false` at the filesystem root.
    pub fn up(&mut self) -> bool {
        let Some(parent) = self.cwd.parent() else {
            return false;
        };
        let parent = parent.to_path_buf();
        if parent == self.cwd {
            return false;
        }
        self.cwd = parent;
        self.reload();
        true
    }

    /// Navigate to a directory typed into the path field, or to the folder of a
    /// supported program file and select it.
    ///
    /// On failure the current directory and entries stay unchanged and the
    /// error explains why.
    pub fn navigate_to(&mut self, path: &Path) -> bool {
        let path = if path.as_os_str().is_empty() {
            return false;
        } else {
            path
        };
        let Ok(metadata) = fs::metadata(path) else {
            self.error = Some(format!("'{}' was not found", display_path(path)));
            return false;
        };
        if metadata.is_dir() {
            let directory = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            self.cwd = directory;
            self.reload();
            return true;
        }
        if metadata.is_file() && is_program_file(path) {
            let Some(parent) = path.parent() else {
                self.error = Some(format!("'{}' has no parent folder", display_path(path)));
                return false;
            };
            let directory = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
            let file = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            self.cwd = directory;
            self.reload();
            if self.select_path(&file) {
                return true;
            }
            self.error = Some(format!(
                "'{}' is not visible in {}",
                display_path(path),
                self.path_display()
            ));
            return false;
        }
        self.error = Some(BrowseError::NotAProgramFile(display_path(path)).to_string());
        false
    }

    /// Navigate to the current [`Self::path_draft`].
    pub fn commit_path(&mut self) -> bool {
        let draft = self.path_draft.trim().to_string();
        if draft.is_empty() {
            self.error = Some("Enter a folder path first".to_string());
            return false;
        }
        self.navigate_to(Path::new(&draft))
    }

    fn reload(&mut self) {
        match list_directory(&self.cwd) {
            Ok(entries) => {
                self.entries = Arc::from(entries);
                self.error = None;
            }
            Err(error) => {
                self.entries = Arc::from(Vec::new());
                self.error = Some(error.to_string());
            }
        }
        self.selected = None;
        self.path_draft = self.path_display();
    }
}

/// Resolve an initial path: a directory stays, a program file opens its parent
/// and is preselected later, and an unreadable path falls back to the current
/// directory.
fn locate(start: &Path) -> (PathBuf, Option<PathBuf>) {
    if start.is_file()
        && let Some(parent) = start.parent()
    {
        let directory = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
        let preselected = is_program_file(start)
            .then(|| fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf()));
        return (directory, preselected);
    }
    let directory = if start.is_dir() {
        fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf())
    } else {
        std::env::current_dir()
            .ok()
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| PathBuf::from("/"))
    };
    (directory, None)
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::state::{AppState, ConnectionState, ProgramLoadRefusal};
    use ghs_engine::{Command, JobState};

    const SOURCE: &str = "\
G21 G90 G54 G17\n\
\n\
G0 Z5\n\
G0 X0 Y0\n\
M3 S12000\n\
G1 Z-1 F200\n\
G1 X40 F800\n\
M5\n\
M30\n";

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    /// Small self-contained temp root. Every test gets its own directory and
    /// drops it at the end; no user path is ever touched.
    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn new(label: &str) -> Self {
            let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ghs-file-browser-{label}-{}-{serial}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("temp root");
            Self { path }
        }

        fn dir(&self, name: &str) -> PathBuf {
            let path = self.path.join(name);
            fs::create_dir_all(&path).expect("temp dir");
            path
        }

        fn file(&self, name: &str, contents: &[u8]) -> PathBuf {
            let path = self.path.join(name);
            fs::write(&path, contents).expect("temp file");
            path
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn live_state() -> AppState {
        let mut state = AppState::new();
        state.begin_live("test transport");
        state.connection = ConnectionState::Ready;
        state
    }

    #[test]
    fn extension_filter_is_case_insensitive_and_ignores_other_files() {
        for name in ["a.nc", "B.GCODE", "c.Tap", "d.ngc"] {
            assert!(is_program_file(Path::new(name)), "{name} must be visible");
        }
        for name in ["e.txt", "f", "g.nc.bak"] {
            assert!(!is_program_file(Path::new(name)), "{name} must be hidden");
        }

        let root = TempRoot::new("filter");
        root.file("part.nc", b"G21\n");
        root.file("part.GCODE", b"G21\n");
        root.file("part.tap", b"G21\n");
        root.file("part.ngc", b"G21\n");
        root.file("notes.txt", b"not a program");
        root.file(".hidden.nc", b"G21\n");
        let entries = list_directory(&root.path).expect("list");
        let files: Vec<&str> = entries
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(
            files.len(),
            4,
            "only the four program files remain: {files:?}"
        );
        assert!(files.contains(&"part.GCODE"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_listed_or_followed() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new("symlink");
        let outside = root.dir("outside");
        let target = root.file("target.nc", b"G21\n");
        symlink(&outside, root.path.join("loop")).expect("directory symlink");
        symlink(&target, root.path.join("link.nc")).expect("file symlink");

        let entries = list_directory(&root.path).expect("list");
        assert!(
            !entries.iter().any(|entry| entry.name == "loop"),
            "a directory symlink must not be listed"
        );
        assert!(
            !entries.iter().any(|entry| entry.name == "link.nc"),
            "a file symlink must not be listed"
        );
        assert!(entries.iter().any(|entry| entry.name == "target.nc"));
    }

    #[test]
    fn directory_enter_and_up_change_the_current_path() {
        let root = TempRoot::new("enter");
        let parts = root.dir("parts");
        root.file("top.nc", b"G21\n");
        root.file("parts/inside.nc", b"G21\n");

        let mut browser = FileBrowser::open(&root.path);
        let index = browser
            .entries()
            .iter()
            .position(|entry| entry.is_dir && entry.name == "parts")
            .expect("parts folder is listed");
        assert!(browser.enter(index));
        assert_eq!(browser.cwd(), parts.as_path());
        assert_eq!(browser.file_count(), 1);
        assert_eq!(browser.entries()[0].name, "inside.nc");

        assert!(browser.up());
        assert_eq!(browser.cwd(), root.path.as_path());
        assert!(browser.entries().iter().any(|entry| entry.name == "top.nc"));
        let file_index = browser
            .entries()
            .iter()
            .position(|entry| entry.name == "top.nc")
            .expect("top.nc is listed");
        assert!(!browser.enter(file_index), "a file cannot be entered");
    }

    #[test]
    fn up_stops_at_the_filesystem_root() {
        let mut browser = FileBrowser::open(Path::new("/"));
        assert!(!browser.up());
        assert_eq!(browser.cwd(), Path::new("/"));
    }

    #[test]
    fn path_display_is_absolute_and_compact_path_keeps_the_tail() {
        let root = TempRoot::new("path");
        let browser = FileBrowser::open(&root.path);
        let root_name = root.path.file_name().expect("root name").to_string_lossy();
        let shown = browser.path_display();
        assert!(
            Path::new(&shown).is_absolute(),
            "path must be absolute: {shown}"
        );
        assert!(shown.ends_with(root_name.as_ref()));

        let long = Path::new("/a/bb/ccc/part.nc");
        let compact = compact_path(long, 12);
        assert!(compact.starts_with("..."));
        assert!(compact.ends_with("part.nc"));
        assert_eq!(compact.chars().count(), 12);
    }

    #[test]
    fn tapping_a_file_selects_it_and_tapping_a_directory_does_not() {
        let root = TempRoot::new("select");
        root.dir("parts");
        root.file("selected.nc", SOURCE.as_bytes());

        let mut browser = FileBrowser::open(&root.path);
        let dir_index = browser
            .entries()
            .iter()
            .position(|entry| entry.is_dir)
            .expect("a directory");
        assert!(
            !browser.select(dir_index),
            "directories are entered, not selected"
        );
        assert!(browser.selected_entry().is_none());

        let file_index = browser
            .entries()
            .iter()
            .position(|entry| entry.name == "selected.nc")
            .expect("a program file");
        assert!(browser.select(file_index));
        let selected = browser.selected_entry().expect("selection");
        assert_eq!(selected.name, "selected.nc");
        assert_eq!(browser.selected_index(), Some(file_index));
    }

    #[test]
    fn typed_path_navigates_to_a_directory_and_selects_a_file() {
        let root = TempRoot::new("typed");
        let parts = root.dir("parts");
        let file = root.file("part.nc", b"G21\n");
        fs::write(parts.join("part.nc"), b"G21\n").expect("folder program");

        let mut browser = FileBrowser::open(&root.path);
        browser.set_path_draft(parts.to_string_lossy().to_string());
        assert!(browser.commit_path());
        assert_eq!(browser.cwd(), parts.as_path());

        browser.set_path_draft(file.to_string_lossy().to_string());
        // The file lives in the root, so this navigates back and selects it.
        assert!(browser.commit_path());
        assert_eq!(browser.cwd(), root.path.as_path());
        assert_eq!(
            browser.selected_entry().map(|entry| entry.name.as_str()),
            Some("part.nc")
        );
    }

    #[test]
    fn non_utf8_file_is_refused() {
        let root = TempRoot::new("utf8");
        let file = root.file("binary.nc", &[0x47, 0x32, 0x31, 0xFF, 0xFE, 0x0A]);
        let error = read_program(&file).expect_err("invalid UTF-8 must be refused");
        assert_eq!(error, ProgramFileError::NotUtf8);
        assert!(error.to_string().contains("UTF-8"));
    }

    #[test]
    fn binary_nul_bytes_are_refused_before_utf8_validation() {
        let root = TempRoot::new("nul");
        let file = root.file("image.nc", b"G21\n\0\0\0");
        assert_eq!(
            read_program(&file).expect_err("binary must be refused"),
            ProgramFileError::Binary
        );
    }

    #[test]
    fn empty_file_is_refused() {
        let root = TempRoot::new("empty");
        let file = root.file("empty.nc", b" \n\t\n");
        let error = read_program(&file).expect_err("empty program must be refused");
        assert_eq!(error, ProgramFileError::Empty);
        assert_eq!(error.to_string(), "file is empty");
    }

    #[test]
    fn oversized_file_is_refused_from_metadata() {
        let root = TempRoot::new("large");
        let file = root.file("huge.nc", b"");
        let handle = fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .expect("open");
        handle.set_len(MAX_PROGRAM_BYTES + 1).expect("sparse file");
        let error = read_program(&file).expect_err("oversized program must be refused");
        assert!(
            matches!(
                error,
                ProgramFileError::TooLarge {
                    bytes,
                    limit: MAX_PROGRAM_BYTES
                } if bytes == MAX_PROGRAM_BYTES + 1
            ),
            "unexpected error: {error:?}"
        );
        assert!(error.to_string().contains("limit"));
    }

    #[test]
    fn a_program_file_round_trips_its_name_and_exact_source() {
        let root = TempRoot::new("roundtrip");
        let file = root.file("Part.NC", SOURCE.as_bytes());
        let program = read_program(&file).expect("valid program");
        assert_eq!(program.name, "Part.NC");
        assert_eq!(program.source, SOURCE);
        assert_eq!(program.byte_len(), SOURCE.len());
    }

    #[test]
    fn a_drop_accepts_exactly_one_regular_file() {
        let root = TempRoot::new("drop-one");
        let file = root.file("part.nc", SOURCE.as_bytes());
        let accepted = dropped_program_path(std::slice::from_ref(&file)).expect("accepted");
        assert_eq!(accepted, file.as_path());
    }

    #[test]
    fn a_drop_refuses_empty_multiple_and_directory_payloads() {
        let root = TempRoot::new("drop-refusals");
        let first = root.file("first.nc", SOURCE.as_bytes());
        let second = root.file("second.nc", SOURCE.as_bytes());
        let folder = root.dir("parts");

        let empty = dropped_program_path(&[]).expect_err("empty drop");
        assert!(empty.contains("one"), "{empty}");

        let multiple = dropped_program_path(&[first.clone(), second]).expect_err("multiple files");
        assert!(multiple.contains("exactly one"), "{multiple}");

        let directory = dropped_program_path(&[folder]).expect_err("directory drop");
        assert!(directory.contains("not a regular file"), "{directory}");
    }

    #[test]
    fn load_command_is_exactly_one_load_program_and_never_starts() {
        let mut state = live_state();
        let program = ProgramFile {
            name: "part.nc".to_string(),
            source: SOURCE.to_string(),
        };
        let command = state
            .program_load(&program.name, &program.source)
            .expect("ready, idle load is allowed");
        assert_eq!(
            command,
            Command::LoadProgram {
                name: "part.nc".to_string(),
                source: SOURCE.to_string(),
            }
        );
        assert!(!matches!(
            command,
            Command::StartProgram { .. } | Command::Realtime(_) | Command::Mdi(_)
        ));
        assert_eq!(state.job.state, JobState::Idle);
        assert!(!state.job.is_active());
        assert!(
            !state.job.has_program(),
            "a load command does not install a program"
        );
    }

    #[test]
    fn load_is_refused_while_a_job_is_active_and_keeps_the_job() {
        let mut state = live_state();
        let old = ghs_gcode::parse_program(SOURCE).expect("old parses");
        state
            .job
            .loaded("old.nc".to_string(), std::sync::Arc::new(old), 9);
        state.job.started(1, 9);

        let refusal = state
            .program_load("new.nc", SOURCE)
            .expect_err("an active job must refuse a new program");
        assert_eq!(refusal, ProgramLoadRefusal::JobActive);
        assert_eq!(state.job.name, "old.nc");
        assert!(state.job.is_active());
        assert!(refusal.message().contains("JOB ACTIVE"));
    }

    #[test]
    fn load_is_refused_in_mock_mode_and_before_the_link_is_ready() {
        let mut mock = AppState::new();
        assert_eq!(
            mock.program_load("part.nc", SOURCE),
            Err(ProgramLoadRefusal::Mock)
        );
        assert!(mock.program_load_refusal().is_some());

        let mut connecting = live_state();
        connecting.connection = ConnectionState::Connecting;
        assert_eq!(
            connecting.program_load("part.nc", SOURCE),
            Err(ProgramLoadRefusal::NotReady)
        );
    }

    #[test]
    fn exact_source_is_remembered_for_the_height_map_original() {
        let mut state = live_state();
        let program = ProgramFile {
            name: "part.nc".to_string(),
            source: SOURCE.to_string(),
        };
        state
            .program_load(&program.name, &program.source)
            .expect("load is allowed");

        // This is what `live.rs` does when the engine's JobLoaded arrives.
        let parsed = ghs_gcode::parse_program(&program.source).expect("program parses");
        state
            .height_map
            .on_job_loaded(&program.name, &parsed, false);
        assert!(state.height_map.original().is_none(), "not a probe run yet");
        state.height_map.on_probe_run_started();

        let original = state
            .height_map
            .original()
            .expect("the current program is kept for Apply");
        assert!(original.exact, "the app registered the exact bytes it sent");
        assert_eq!(original.source, SOURCE, "blank lines survive exactly");
        assert_eq!(original.name, "part.nc");
    }

    #[test]
    fn a_rejected_load_can_discard_its_pending_exact_source() {
        let mut state = live_state();
        state
            .program_load("rejected.nc", SOURCE)
            .expect("load is allowed before the engine hears it");
        state.height_map.discard_exact_source("rejected.nc");

        let program = ghs_gcode::parse_program("G21\nG0 X1\nM30\n").expect("other parses");
        state.height_map.on_job_loaded("other.nc", &program, false);
        state.height_map.on_probe_run_started();
        let original = state.height_map.original().expect("original");
        assert!(
            !original.exact,
            "a stale exact source must not be adopted by another program"
        );
    }
}
