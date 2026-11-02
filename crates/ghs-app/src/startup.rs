//! Startup-line (`$N0`/`$N1`) read and write model.
//!
//! A startup line is executed by the controller on every reset and can command
//! motion, the spindle, or coolant. The model therefore never edits a line
//! optimistically and never writes one by itself:
//!
//! 1. `$N`/`$N<n>` reports fill [`StartupLineRow::stored`].
//! 2. [`StartupLinesModel::set_draft`] only changes a local draft.
//! 3. Apply is explicit and gated by [`StartupWriteGate`].
//! 4. The shell sends `$N<n>=<line>`; its ack is a plain `ok`, so the model
//!    immediately asks for `$N<n>` again and compares the read-back with the
//!    requested text.
//! 5. Only a matching read-back becomes `VERIFIED`. A different value is
//!    `MISMATCH`; a rejection, alarm, disconnect, or refused read is `FAILED`.
//!    The previous controller value stays visible in `stored` in every case.
//!
//! The model owns no GPUI types and no clock, so the state machine is testable
//! without a window or an engine thread.

use ghs_engine::{Command, StartupLine, SystemCommand};

/// Largest startup line accepted by the protocol encoder, in bytes.
pub const MAX_STARTUP_TEXT: usize = 256;

/// Startup-line indices shown by the editor.
pub const STARTUP_INDICES: [u8; 2] = [0, 1];

/// Read/write state of one startup line.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum StartupStatus {
    /// The draft matches the last controller value.
    #[default]
    Synced,
    /// A local edit is waiting for Apply.
    Edited,
    /// Apply was confirmed; the write is waiting to be sent.
    Queued,
    /// The write was accepted by the engine and is being streamed.
    Writing,
    /// A `$N<n>` read-back was requested.
    Verifying,
    /// The read-back matched the requested text.
    Verified,
    /// The read-back differs from the requested text; holds the reported text.
    Mismatch(String),
    /// The write or read-back failed; holds the reason.
    Failed(String),
}

impl StartupStatus {
    /// Short row label.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            StartupStatus::Synced => "",
            StartupStatus::Edited => "PENDING",
            StartupStatus::Queued => "QUEUED",
            StartupStatus::Writing => "WRITING",
            StartupStatus::Verifying => "VERIFYING",
            StartupStatus::Verified => "VERIFIED",
            StartupStatus::Mismatch(_) => "MISMATCH",
            StartupStatus::Failed(_) => "FAILED",
        }
    }

    /// Whether a write or read-back is in progress.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            StartupStatus::Queued | StartupStatus::Writing | StartupStatus::Verifying
        )
    }

    /// Whether the status needs the operator's attention.
    #[must_use]
    pub fn is_problem(&self) -> bool {
        matches!(self, StartupStatus::Mismatch(_) | StartupStatus::Failed(_))
    }
}

/// One `$N<n>` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupLineRow {
    /// Startup-line index, 0 for `$N0`.
    pub index: u8,
    /// Last text reported by the controller.
    pub stored: String,
    /// Local edit; sent only after an explicit confirmation.
    pub draft: String,
    /// Read/write state.
    pub status: StartupStatus,
    /// Detail for a pending, verified, mismatched, or failed row.
    pub note: Option<String>,
    /// Whether the controller has answered for this index.
    pub reported: bool,
}

impl StartupLineRow {
    fn new(index: u8) -> Self {
        Self {
            index,
            stored: String::new(),
            draft: String::new(),
            status: StartupStatus::Synced,
            note: None,
            reported: false,
        }
    }

    /// `$N0`-style title.
    #[must_use]
    pub fn title(&self) -> String {
        format!("$N{}", self.index)
    }

    /// Whether the draft differs from the last controller value.
    #[must_use]
    pub fn has_edit(&self) -> bool {
        self.draft != self.stored
    }

    /// Whether Apply would clear the line.
    #[must_use]
    pub fn is_clear(&self) -> bool {
        self.draft.is_empty()
    }
}

/// Machine/connection gate for a startup-line write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct StartupWriteGate {
    /// A live engine session (not `--mock`).
    pub live: bool,
    /// The engine handshake finished and the link is ready.
    pub ready: bool,
    /// The controller reports `Idle`.
    pub idle: bool,
    /// The controller reports `Alarm` or `Door`.
    pub alarm_or_door: bool,
    /// A job is running or paused.
    pub job_active: bool,
}

impl StartupWriteGate {
    /// Why a write is refused, or `None` when it is allowed.
    #[must_use]
    pub fn blocked_reason(&self) -> Option<&'static str> {
        if !self.live {
            Some("mock mode: no controller")
        } else if !self.ready {
            Some("not connected")
        } else if self.alarm_or_door {
            Some("alarm or door")
        } else if self.job_active {
            Some("job active")
        } else if !self.idle {
            Some("controller not Idle")
        } else {
            None
        }
    }
}

/// One confirmed write handed to the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupWrite {
    /// Startup-line index.
    pub index: u8,
    /// Exact text to store; empty clears the line.
    pub line: String,
}

/// One request the shell should send through the engine gateway.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupRequest {
    /// `$N<index>=<line>`.
    Write {
        /// Startup-line index.
        index: u8,
        /// Exact text.
        line: String,
    },
    /// `$N<index>` read-back.
    Read {
        /// Startup-line index.
        index: u8,
    },
}

/// The guarded `Command::System` for a confirmed startup-line write.
#[must_use]
pub fn write_command(write: &StartupWrite) -> Command {
    Command::System(SystemCommand::WriteStartupLine {
        index: write.index,
        line: write.line.clone(),
    })
}

/// The guarded `Command::System` for a startup-line read-back.
#[must_use]
pub fn read_command(index: u8) -> Command {
    Command::System(SystemCommand::ReadStartupLine(index))
}

/// One console notice produced by a write result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupNotice {
    /// Whether the notice reports success.
    pub ok: bool,
    /// Human-readable text.
    pub text: String,
}

/// `$N0`/`$N1` editor state.
#[derive(Debug)]
pub struct StartupLinesModel {
    rows: Vec<StartupLineRow>,
    inflight: Option<StartupWrite>,
    last_result: Option<String>,
    last_result_ok: bool,
    notice: Option<StartupNotice>,
}

impl Default for StartupLinesModel {
    fn default() -> Self {
        Self::new()
    }
}

impl StartupLinesModel {
    /// Two empty rows, not yet reported by a controller.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: STARTUP_INDICES
                .iter()
                .copied()
                .map(StartupLineRow::new)
                .collect(),
            inflight: None,
            last_result: None,
            last_result_ok: false,
            notice: None,
        }
    }

    /// Editor rows in index order.
    #[must_use]
    pub fn rows(&self) -> &[StartupLineRow] {
        &self.rows
    }

    /// One row by index.
    #[must_use]
    pub fn row(&self, index: u8) -> Option<&StartupLineRow> {
        self.rows.iter().find(|row| row.index == index)
    }

    /// Whether at least one row has a controller report.
    #[must_use]
    pub fn reported(&self) -> bool {
        self.rows.iter().any(|row| row.reported)
    }

    /// Whether every row has a controller report.
    #[must_use]
    pub fn fully_reported(&self) -> bool {
        self.rows.iter().all(|row| row.reported)
    }

    /// Whether a confirmed write is waiting for the controller.
    #[must_use]
    pub fn inflight_index(&self) -> Option<u8> {
        self.inflight.as_ref().map(|write| write.index)
    }

    /// Number of rows with a local edit or an unfinished write.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.has_edit() || row.status.is_busy())
            .count()
    }

    /// Last write result for the section footer.
    #[must_use]
    pub fn last_result(&self) -> Option<&str> {
        self.last_result.as_deref()
    }

    /// Whether the last write result was a success.
    #[must_use]
    pub fn last_result_ok(&self) -> bool {
        self.last_result_ok
    }

    /// Remove the pending console notice, if any.
    pub fn take_notice(&mut self) -> Option<StartupNotice> {
        self.notice.take()
    }

    /// Replace the controller-reported values.
    ///
    /// A report that arrives for the index being verified is the read-back and
    /// is compared with the requested text. Other local edits survive a report;
    /// they are never silently replaced by a refresh.
    pub fn apply_reported(&mut self, lines: &[StartupLine]) -> bool {
        let mut changed = false;
        for line in lines {
            let Some(position) = self.rows.iter().position(|row| row.index == line.index) else {
                continue;
            };
            let requested = self
                .inflight
                .as_ref()
                .filter(|write| write.index == line.index)
                .map(|write| write.line.clone());
            let Some(row) = self.rows.get_mut(position) else {
                continue;
            };
            if !row.reported {
                row.reported = true;
                changed = true;
            }
            let verifying = row.status == StartupStatus::Verifying;
            let stored_changed = row.stored != line.line;
            row.stored.clone_from(&line.line);
            if verifying {
                changed = true;
                match requested {
                    Some(requested) if requested == line.line => {
                        row.draft.clone_from(&line.line);
                        row.status = StartupStatus::Verified;
                        let note = "read-back matches the requested line".to_string();
                        row.note = Some(note);
                        let text = format!("$N{} VERIFIED ({})", row.index, line.line);
                        self.last_result = Some(text.clone());
                        self.last_result_ok = true;
                        self.notice = Some(StartupNotice { ok: true, text });
                        self.inflight = None;
                    }
                    Some(requested) => {
                        // Keep the attempted text in the draft so the operator
                        // can see exactly what was refused; the controller's
                        // value remains in `stored`.
                        let note = format!(
                            "controller reports \"{}\"; requested \"{}\"",
                            line.line, requested
                        );
                        row.status = StartupStatus::Mismatch(line.line.clone());
                        row.note = Some(note.clone());
                        let text = format!("$N{} MISMATCH: {note}", row.index);
                        self.last_result = Some(text.clone());
                        self.last_result_ok = false;
                        self.notice = Some(StartupNotice { ok: false, text });
                        self.inflight = None;
                    }
                    None => {
                        row.status = StartupStatus::Synced;
                        row.draft.clone_from(&line.line);
                        row.note = None;
                    }
                }
            } else if matches!(row.status, StartupStatus::Synced | StartupStatus::Verified) {
                if row.draft != line.line {
                    row.draft.clone_from(&line.line);
                    changed = true;
                }
                if row.status != StartupStatus::Synced {
                    row.status = StartupStatus::Synced;
                    changed = true;
                }
                if row.note.take().is_some() {
                    changed = true;
                }
            }
            if stored_changed {
                changed = true;
            }
        }
        changed
    }

    /// Set one row's local draft. Nothing is sent.
    ///
    /// Empty text is a valid explicit clear. The text is validated before it is
    /// stored so a bad character never reaches a confirmation sheet.
    ///
    /// # Errors
    ///
    /// Returns a message for an unknown index, an over-long line, a control
    /// character, non-ASCII text, or an edit while that line is being written.
    pub fn set_draft(&mut self, index: u8, text: impl Into<String>) -> Result<(), String> {
        let text = text.into();
        validate_text(&text)?;
        let Some(row) = self.rows.iter_mut().find(|row| row.index == index) else {
            return Err(format!("$N{index} is not a startup line"));
        };
        if row.status.is_busy() {
            return Err(format!(
                "$N{index} is being written; wait for the read-back"
            ));
        }
        row.draft = text;
        row.note = None;
        row.status = if row.draft == row.stored {
            StartupStatus::Synced
        } else {
            StartupStatus::Edited
        };
        Ok(())
    }

    /// Validate a confirmed write without changing any state.
    ///
    /// The shell uses this to open the confirmation sheet for exactly the text
    /// that will be sent.
    ///
    /// # Errors
    ///
    /// Returns the gate reason, a missing report, an unchanged line, a busy
    /// row, another write in progress, or a validation message.
    pub fn preview_apply(&self, index: u8, gate: StartupWriteGate) -> Result<StartupWrite, String> {
        if let Some(reason) = gate.blocked_reason() {
            return Err(reason.to_string());
        }
        if let Some(write) = &self.inflight {
            return Err(format!(
                "a startup-line write is already in progress ($N{})",
                write.index
            ));
        }
        let Some(row) = self.row(index) else {
            return Err(format!("$N{index} is not a startup line"));
        };
        if !row.reported {
            return Err("startup lines have not been read from the controller yet".to_string());
        }
        if row.status.is_busy() {
            return Err(format!("$N{index} is already being written"));
        }
        validate_text(&row.draft)?;
        if row.draft == row.stored {
            return Err("the startup line is unchanged".to_string());
        }
        Ok(StartupWrite {
            index,
            line: row.draft.clone(),
        })
    }

    /// Confirm and queue one write after the operator accepted the warning.
    ///
    /// # Errors
    ///
    /// Returns the same reasons as [`StartupLinesModel::preview_apply`].
    pub fn begin_apply(
        &mut self,
        index: u8,
        gate: StartupWriteGate,
    ) -> Result<StartupWrite, String> {
        let write = self.preview_apply(index, gate)?;
        if let Some(row) = self.rows.iter_mut().find(|row| row.index == index) {
            row.status = StartupStatus::Queued;
            row.note = None;
        }
        self.inflight = Some(write.clone());
        self.last_result = Some(format!("$N{index} queued; confirming the write"));
        self.last_result_ok = false;
        Ok(write)
    }

    /// The next request the shell should send, if any.
    ///
    /// A queued write is sent first; after it is accepted the same row is read
    /// back. While the engine reports `Busy` the request is returned again on
    /// the next poll, without changing the state.
    #[must_use]
    pub fn next_request(&self) -> Option<StartupRequest> {
        let write = self.inflight.as_ref()?;
        let row = self.row(write.index)?;
        match row.status {
            StartupStatus::Queued => Some(StartupRequest::Write {
                index: write.index,
                line: write.line.clone(),
            }),
            StartupStatus::Writing => Some(StartupRequest::Read { index: write.index }),
            _ => None,
        }
    }

    /// Record that the write command was accepted by the engine.
    pub fn write_sent(&mut self, index: u8) -> bool {
        self.transition(index, StartupStatus::Queued, StartupStatus::Writing)
    }

    /// The engine was busy; keep the write queued for the next poll.
    pub fn write_busy(&mut self, index: u8) -> bool {
        self.row(index)
            .is_some_and(|row| row.index == index && row.status == StartupStatus::Queued)
    }

    /// Record that the read-back command was accepted by the engine.
    pub fn read_sent(&mut self, index: u8) -> bool {
        self.transition(index, StartupStatus::Writing, StartupStatus::Verifying)
    }

    /// The engine was busy; keep asking for the read-back.
    #[cfg(test)]
    pub fn read_busy(&mut self, index: u8) -> bool {
        self.row(index)
            .is_some_and(|row| row.index == index && row.status == StartupStatus::Writing)
    }

    /// Fail the in-flight write and keep the previous controller value.
    ///
    /// Returns `true` when a pending row was affected.
    pub fn failed(&mut self, index: u8, message: impl Into<String>) -> bool {
        let message = message.into();
        let inflight = self
            .inflight
            .as_ref()
            .is_some_and(|write| write.index == index);
        let busy = self.row(index).is_some_and(|row| row.status.is_busy());
        if !inflight && !busy {
            return false;
        }
        if inflight {
            self.inflight = None;
        }
        let Some(row) = self.rows.iter_mut().find(|row| row.index == index) else {
            return false;
        };
        row.status = StartupStatus::Failed(message.clone());
        row.note = Some(message.clone());
        let text = format!("$N{index} FAILED: {message}");
        self.last_result = Some(text.clone());
        self.last_result_ok = false;
        self.notice = Some(StartupNotice { ok: false, text });
        true
    }

    /// Fail the in-flight write after a controller `error:<n>`.
    pub fn on_controller_error(&mut self, code: u16) -> bool {
        let Some(index) = self.inflight_index() else {
            return false;
        };
        self.failed(index, format!("controller error:{code}"))
    }

    /// Fail the in-flight write after an alarm.
    pub fn on_alarm(&mut self, code: u16) -> bool {
        let Some(index) = self.inflight_index() else {
            return false;
        };
        self.failed(index, format!("ALARM:{code}"))
    }

    /// Fail a pending write because the link closed.
    pub fn on_disconnected(&mut self) -> bool {
        let Some(index) = self.inflight_index() else {
            return false;
        };
        self.failed(index, "disconnected")
    }

    /// Stop a queued write because the machine left the allowed state.
    pub fn cancel_pending(&mut self, reason: &str) -> bool {
        let Some(index) = self.inflight_index() else {
            return false;
        };
        self.failed(index, reason)
    }

    /// Drop every report, edit, and result for a new session.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    fn transition(&mut self, index: u8, from: StartupStatus, to: StartupStatus) -> bool {
        let Some(row) = self.rows.iter_mut().find(|row| row.index == index) else {
            return false;
        };
        if row.status != from {
            return false;
        }
        row.status = to;
        true
    }
}

/// Validate startup-line text for the protocol encoder and the simulator.
///
/// The limits mirror `ghs_proto::command::MAX_STARTUP_LINE` and
/// `validate_line`: at most 256 bytes, ASCII, no control characters (a tab is
/// allowed by the encoder). Empty text is valid and clears the line.
///
/// # Errors
///
/// Returns a human-readable message naming the first rejected byte.
pub fn validate_text(text: &str) -> Result<(), String> {
    if text.len() > MAX_STARTUP_TEXT {
        return Err(format!(
            "startup line is {} bytes; the limit is {MAX_STARTUP_TEXT}",
            text.len()
        ));
    }
    for (index, ch) in text.char_indices() {
        if ch == '\t' {
            continue;
        }
        if ch.is_control() {
            return Err(format!(
                "control character U+{:04X} at byte {index} is not allowed",
                u32::from(ch)
            ));
        }
        if !ch.is_ascii() {
            return Err(format!(
                "non-ASCII character at byte {index} is not allowed"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> StartupWriteGate {
        StartupWriteGate {
            live: true,
            ready: true,
            idle: true,
            alarm_or_door: false,
            job_active: false,
        }
    }

    fn reported(model: &mut StartupLinesModel, zero: &str, one: &str) -> bool {
        model.apply_reported(&[
            StartupLine {
                index: 0,
                line: zero.to_string(),
            },
            StartupLine {
                index: 1,
                line: one.to_string(),
            },
        ])
    }

    fn loaded() -> StartupLinesModel {
        let mut model = StartupLinesModel::new();
        assert!(reported(&mut model, "G21 G90", ""));
        model.take_notice();
        model
    }

    /// Drive the model the way the shell does: send each request, then feed the
    /// read-back. `busy_reads` makes the first N read attempts return Busy.
    fn pump(model: &mut StartupLinesModel, busy_reads: usize, read_back: &str) -> Vec<Command> {
        let mut sent = Vec::new();
        let mut busy = 0;
        for _ in 0..16 {
            match model.next_request() {
                Some(StartupRequest::Write { index, line }) => {
                    sent.push(write_command(&StartupWrite { index, line }));
                    assert!(model.write_sent(index));
                }
                Some(StartupRequest::Read { index }) => {
                    if busy < busy_reads {
                        busy += 1;
                        assert!(model.read_busy(index));
                        continue;
                    }
                    sent.push(read_command(index));
                    assert!(model.read_sent(index));
                    model.apply_reported(&[StartupLine {
                        index,
                        line: read_back.to_string(),
                    }]);
                    model.take_notice();
                    return sent;
                }
                None => break,
            }
        }
        sent
    }

    #[test]
    fn reports_load_rows_and_mark_them_reported() {
        let mut model = StartupLinesModel::new();
        assert!(!model.reported());
        assert!(!model.fully_reported());
        assert_eq!(model.rows().len(), 2);
        assert!(model.rows().iter().all(|row| row.draft.is_empty()));

        assert!(reported(&mut model, "G21 G90", "M3 S0"));
        assert!(model.reported());
        assert!(model.fully_reported());
        assert_eq!(model.row(0).expect("row 0").stored, "G21 G90");
        assert_eq!(model.row(0).expect("row 0").draft, "G21 G90");
        assert_eq!(model.row(1).expect("row 1").stored, "M3 S0");
        assert_eq!(model.row(0).expect("row 0").status, StartupStatus::Synced);

        // A second report replaces the controller value, not the rows.
        assert!(reported(&mut model, "G17 G21", ""));
        assert_eq!(model.row(0).expect("row 0").draft, "G17 G21");
        assert_eq!(model.row(1).expect("row 1").stored, "");
    }

    #[test]
    fn a_local_edit_is_never_sent_until_apply() {
        let mut model = loaded();
        model.set_draft(0, "G21 G90 G54").expect("valid local edit");
        let row = model.row(0).expect("row 0");
        assert_eq!(row.stored, "G21 G90");
        assert_eq!(row.draft, "G21 G90 G54");
        assert_eq!(row.status, StartupStatus::Edited);
        assert_eq!(model.next_request(), None, "editing must not send anything");

        // A controller refresh keeps the local edit.
        assert!(!reported(&mut model, "G21 G90", ""));
        assert_eq!(model.row(0).expect("row 0").draft, "G21 G90 G54");
        assert_eq!(model.row(0).expect("row 0").status, StartupStatus::Edited);
    }

    #[test]
    fn apply_is_gated_by_connection_machine_and_job_state() {
        let mut model = loaded();
        model.set_draft(0, "G21 G90 G54").expect("edit");

        let blocked = [
            (
                StartupWriteGate {
                    live: false,
                    ..gate()
                },
                "mock mode",
            ),
            (
                StartupWriteGate {
                    ready: false,
                    ..gate()
                },
                "not connected",
            ),
            (
                StartupWriteGate {
                    alarm_or_door: true,
                    ..gate()
                },
                "alarm or door",
            ),
            (
                StartupWriteGate {
                    job_active: true,
                    ..gate()
                },
                "job active",
            ),
            (
                StartupWriteGate {
                    idle: false,
                    ..gate()
                },
                "not Idle",
            ),
        ];
        for (gate, expected) in blocked {
            let error = model
                .begin_apply(0, gate)
                .expect_err("the gate must refuse the write");
            assert!(error.contains(expected), "{error}");
            assert_eq!(model.inflight_index(), None);
            assert_eq!(model.row(0).expect("row 0").status, StartupStatus::Edited);
        }

        let write = model.begin_apply(0, gate()).expect("idle write is allowed");
        assert_eq!(model.inflight_index(), Some(0));
        assert_eq!(model.row(0).expect("row 0").status, StartupStatus::Queued);
        assert_eq!(
            write_command(&write),
            Command::System(SystemCommand::WriteStartupLine {
                index: 0,
                line: "G21 G90 G54".into()
            })
        );
    }

    #[test]
    fn apply_sends_the_exact_startup_command_only_after_confirmation() {
        let mut model = loaded();
        model.set_draft(1, "M3 S0").expect("edit");
        assert_eq!(model.next_request(), None);

        // The confirmation sheet validates first; nothing is queued or sent.
        let preview = model.preview_apply(1, gate()).expect("preview");
        assert_eq!(preview.line, "M3 S0");
        assert_eq!(model.next_request(), None);

        let write = model.begin_apply(1, gate()).expect("confirm");
        let command = write_command(&write);
        assert_eq!(
            command,
            Command::System(SystemCommand::WriteStartupLine {
                index: 1,
                line: "M3 S0".into()
            })
        );
        let Command::System(system) = &command else {
            panic!("startup writes must use the guarded system path");
        };
        assert_eq!(system.encode().expect("encodes"), "$N1=M3 S0");
    }

    #[test]
    fn busy_read_back_retries_and_eventually_verifies() {
        let mut model = loaded();
        model.set_draft(0, "G21 G90 G54").expect("edit");
        model.begin_apply(0, gate()).expect("confirm");

        let sent = pump(&mut model, 3, "G21 G90 G54");
        assert_eq!(sent.len(), 2, "one write and one read-back");
        assert!(matches!(
            &sent[1],
            Command::System(SystemCommand::ReadStartupLine(0))
        ));
        assert_eq!(model.row(0).expect("row 0").status, StartupStatus::Verified);
        assert_eq!(model.row(0).expect("row 0").stored, "G21 G90 G54");
        assert_eq!(model.row(0).expect("row 0").draft, "G21 G90 G54");
        assert!(model.last_result_ok());
        assert!(model.last_result().unwrap_or_default().contains("VERIFIED"));
        assert_eq!(model.next_request(), None);
        assert_eq!(
            model.pending_count(),
            0,
            "a verified row is no longer pending"
        );
    }

    #[test]
    fn a_mismatch_surfaces_the_read_back_and_keeps_the_old_value() {
        let mut model = loaded();
        model.set_draft(0, "G21 G90 G54").expect("edit");
        model.begin_apply(0, gate()).expect("confirm");

        let sent = pump(&mut model, 0, "G21");
        assert_eq!(sent.len(), 2);
        let row = model.row(0).expect("row 0");
        assert_eq!(row.stored, "G21", "the controller's value stays visible");
        assert_eq!(row.draft, "G21 G90 G54", "the attempt stays visible");
        assert!(matches!(row.status, StartupStatus::Mismatch(_)));
        assert_eq!(row.status.label(), "MISMATCH");
        assert!(
            row.note
                .as_deref()
                .unwrap_or_default()
                .contains("G21 G90 G54")
        );
        assert!(!model.last_result_ok());
        assert!(model.last_result().unwrap_or_default().contains("MISMATCH"));
        assert_eq!(model.inflight_index(), None);
    }

    #[test]
    fn a_controller_error_or_alarm_fails_the_write_and_keeps_the_old_value() {
        let mut model = loaded();
        model.set_draft(0, "G21 G90 G54").expect("edit");
        model.begin_apply(0, gate()).expect("confirm");
        assert!(model.on_controller_error(14));
        let row = model.row(0).expect("row 0");
        assert_eq!(row.stored, "G21 G90", "the previous value stays visible");
        assert_eq!(row.draft, "G21 G90 G54");
        assert_eq!(
            row.status,
            StartupStatus::Failed("controller error:14".into())
        );
        assert_eq!(row.status.label(), "FAILED");
        assert!(!model.last_result_ok());
        assert!(model.last_result().unwrap_or_default().contains("error:14"));
        assert_eq!(model.next_request(), None);

        // A retry can be edited and confirmed again.
        model.set_draft(0, "G21 G90 G54").expect("retry edit");
        model.begin_apply(0, gate()).expect("retry");
        assert!(model.on_alarm(1));
        assert!(matches!(
            model.row(0).expect("row 0").status,
            StartupStatus::Failed(ref message) if message == "ALARM:1"
        ));
    }

    #[test]
    fn a_disconnect_fails_a_pending_write() {
        let mut model = loaded();
        model.set_draft(1, "M3 S0").expect("edit");
        model.begin_apply(1, gate()).expect("confirm");
        assert!(model.on_disconnected());
        assert_eq!(
            model.row(1).expect("row 1").status,
            StartupStatus::Failed("disconnected".into())
        );
        assert_eq!(model.inflight_index(), None);
        assert!(!model.on_disconnected(), "nothing is left to fail");
    }

    #[test]
    fn an_empty_line_is_an_explicit_clear() {
        let mut model = loaded();
        model.set_draft(0, "").expect("clear the line");
        let row = model.row(0).expect("row 0");
        assert!(row.is_clear());
        assert_eq!(row.stored, "G21 G90");
        assert_eq!(row.status, StartupStatus::Edited);

        model.begin_apply(0, gate()).expect("confirm the clear");
        let sent = pump(&mut model, 0, "");
        assert_eq!(
            sent[0],
            Command::System(SystemCommand::WriteStartupLine {
                index: 0,
                line: String::new()
            })
        );
        assert_eq!(model.row(0).expect("row 0").status, StartupStatus::Verified);
        assert_eq!(model.row(0).expect("row 0").stored, "");
    }

    #[test]
    fn text_is_bounded_ascii_and_free_of_control_characters() {
        assert!(validate_text("").is_ok());
        assert!(validate_text("G21 G90\tG54").is_ok());
        assert!(validate_text(&"A".repeat(MAX_STARTUP_TEXT)).is_ok());
        assert!(
            validate_text(&"A".repeat(MAX_STARTUP_TEXT + 1))
                .unwrap_err()
                .contains("limit")
        );
        for bad in ["G21\nG90", "G21\r", "G0\0X0", "M3 S0\u{7f}", "G1 X10\u{1b}"] {
            let error = validate_text(bad).expect_err("control character");
            assert!(error.contains("control character"), "{error}");
        }
        assert!(
            validate_text("G1 X10 ; \u{00e9}")
                .unwrap_err()
                .contains("non-ASCII")
        );

        let mut model = loaded();
        assert!(model.set_draft(0, "G21\nG90").is_err());
        assert_eq!(
            model.row(0).expect("row 0").draft,
            "G21 G90",
            "a rejected edit leaves the previous text"
        );
    }
}
