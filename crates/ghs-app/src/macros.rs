//! User-defined macros: validation, the profile save gate, and the explicit
//! confirm/run handoff.
//!
//! A macro is stored in the active controller profile. This module never
//! touches a transport: the shell receives a typed [`Command::Mdi`] only after
//! an explicit RUN click (and, for a macro marked `confirm`, a second explicit
//! confirmation). Loading a profile, connecting, or importing a backup never
//! arms or sends a macro.
//!
//! Multi-line commands, `$` system commands, and parameters are intentionally
//! outside this pass. They are rejected with a clear message instead of being
//! forwarded to the engine, whose MDI path has stricter limits.

use ghs_config::Macro;
use ghs_engine::Command;

/// Maximum length of a macro name in characters, matching `ghs-config`.
pub const MAX_MACRO_NAME_CHARS: usize = 128;
/// Maximum length of one macro command in bytes.
///
/// A macro is one MDI line. The engine and the controller may impose a smaller
/// receive-buffer limit; that rejection is surfaced to the operator and is not
/// retried.
pub const MAX_MACRO_COMMAND_BYTES: usize = 256;

/// Connection and machine gate mirrored from the engine's MDI rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacroRunGate {
    /// A live engine link exists (not mock review mode).
    pub live: bool,
    /// The engine handshake completed.
    pub ready: bool,
    /// Current controller state label (`Idle`, `Alarm`, `Door`, ...).
    pub machine_state: &'static str,
    /// A job is running or paused.
    pub job_active: bool,
}

impl MacroRunGate {
    /// Why a macro may not run right now, or `None` when it may.
    #[must_use]
    pub fn blocked_reason(self) -> Option<&'static str> {
        if !self.live {
            return Some("mock mode: no controller");
        }
        if !self.ready {
            return Some("not connected");
        }
        if matches!(self.machine_state, "Alarm" | "Door") {
            return Some("alarm or door");
        }
        if self.job_active {
            return Some("job active");
        }
        if self.machine_state != "Idle" {
            return Some("controller not Idle");
        }
        None
    }
}

/// What an explicit RUN click produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroRunRequest {
    /// The macro requires confirmation; the exact record is captured.
    Confirm(Macro),
    /// The macro does not require confirmation and may be sent once.
    Send(Macro),
}

/// Pending confirmation plus the last refusal or send error.
#[derive(Debug, Default)]
pub struct MacroModel {
    pending: Option<Macro>,
    last_error: Option<String>,
}

impl MacroModel {
    /// An empty model.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop a pending confirmation and the last error.
    ///
    /// Called on a new connection or a profile change so a sheet captured on
    /// one controller can never run against another.
    pub fn reset(&mut self) {
        self.pending = None;
        self.last_error = None;
    }

    /// Macro waiting for confirmation, if any.
    #[must_use]
    pub fn pending(&self) -> Option<&Macro> {
        self.pending.as_ref()
    }

    /// Last gate refusal or send error, for the Utility footer.
    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Handle one explicit RUN click.
    ///
    /// A macro with `confirm` set is captured but no command is produced until
    /// [`MacroModel::confirm`]. A direct macro returns [`MacroRunRequest::Send`]
    /// exactly once.
    ///
    /// # Errors
    ///
    /// Returns the gate reason, an unknown name, or a stale record that no
    /// longer satisfies the current app policy (for example an old multi-line
    /// command).
    pub fn request_run(
        &mut self,
        macros: &[Macro],
        name: &str,
        gate: MacroRunGate,
    ) -> Result<MacroRunRequest, String> {
        if let Some(reason) = gate.blocked_reason() {
            let message = format!("Macro run refused: {reason}");
            self.last_error = Some(message.clone());
            return Err(message);
        }
        let Some(mac) = macros.iter().find(|record| record.name == name).cloned() else {
            let message = format!("No macro named '{name}' is stored in this profile");
            self.last_error = Some(message.clone());
            return Err(message);
        };
        if let Err(message) = validate_for_run(&mac) {
            let message = format!("Macro '{}' cannot run: {message}", mac.name);
            self.last_error = Some(message.clone());
            return Err(message);
        }
        self.last_error = None;
        self.pending = None;
        if mac.confirm {
            self.pending = Some(mac.clone());
            Ok(MacroRunRequest::Confirm(mac))
        } else {
            Ok(MacroRunRequest::Send(mac))
        }
    }

    /// Produce the one MDI command for the captured confirmation.
    ///
    /// The pending record is consumed before the gate is checked again, so a
    /// second press can never send a duplicate.
    ///
    /// # Errors
    ///
    /// Returns a message when no macro is waiting or the gate has closed since
    /// the sheet opened.
    pub fn confirm(&mut self, gate: MacroRunGate) -> Result<Command, String> {
        let Some(mac) = self.pending.take() else {
            return Err("No macro run is waiting for confirmation".to_string());
        };
        if let Some(reason) = gate.blocked_reason() {
            let message = format!("Macro '{}' not sent: {reason}", mac.name);
            self.last_error = Some(message.clone());
            return Err(message);
        }
        Ok(Command::Mdi(mac.command))
    }

    /// Close the confirmation sheet without producing a command.
    ///
    /// Returns the macro that was waiting, if any.
    pub fn cancel(&mut self) -> Option<Macro> {
        self.pending.take()
    }

    /// Record an engine or transport rejection. There is deliberately no retry.
    pub fn send_failed(&mut self, message: impl Into<String>) {
        self.last_error = Some(message.into());
    }

    /// Clear the last error after the engine accepted the command.
    pub fn sent(&mut self) {
        self.last_error = None;
    }
}

/// Validate and normalize a macro name for one profile.
///
/// Names are trimmed and unique within the profile. `original` is the stored
/// name when editing, so a record does not collide with itself.
///
/// # Errors
///
/// Returns a message for an empty, over-long, or control-character name, or a
/// duplicate in `macros`.
pub fn validate_name(
    name: &str,
    original: Option<&str>,
    macros: &[Macro],
) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("a macro name is required".to_string());
    }
    if trimmed.chars().count() > MAX_MACRO_NAME_CHARS {
        return Err(format!(
            "the macro name exceeds the {MAX_MACRO_NAME_CHARS}-character limit"
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err("the macro name must not contain control characters".to_string());
    }
    let original = original.map(str::trim);
    if macros
        .iter()
        .any(|record| record.name.trim() == trimmed && Some(record.name.trim()) != original)
    {
        return Err(format!(
            "a macro named '{trimmed}' already exists in this profile"
        ));
    }
    Ok(trimmed.to_string())
}

/// Validate and normalize one macro command.
///
/// This pass accepts one MDI G-code line only. `$` system commands and
/// newlines are rejected before they can reach the engine.
///
/// # Errors
///
/// Returns a message for an empty, over-long, multi-line, `$`, or
/// control-character command.
pub fn validate_command(command: &str) -> Result<String, String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err("a macro command is required".to_string());
    }
    if trimmed.len() > MAX_MACRO_COMMAND_BYTES {
        return Err(format!(
            "the macro command exceeds the {MAX_MACRO_COMMAND_BYTES}-byte limit"
        ));
    }
    if trimmed.contains(['\n', '\r']) {
        return Err(
            "the macro command must be a single MDI line; multi-line macros are not supported in \
             this pass"
                .to_string(),
        );
    }
    if trimmed.chars().any(char::is_control) {
        return Err("the macro command must not contain control characters".to_string());
    }
    if trimmed.contains('$') {
        return Err(
            "system ($) commands are not supported in user macros in this pass".to_string(),
        );
    }
    Ok(trimmed.to_string())
}

/// Validate a stored macro before it is sent.
///
/// This rejects a record loaded from an older profile that does not meet the
/// current single-line/no-`$`/bounded policy, instead of forwarding it.
///
/// # Errors
///
/// Returns the name or command validation message.
pub fn validate_for_run(mac: &Macro) -> Result<(), String> {
    validate_name(&mac.name, Some(mac.name.as_str()), &[])?;
    validate_command(&mac.command)?;
    Ok(())
}

/// Build a validated record for the macro editor.
///
/// # Errors
///
/// Returns the name or command validation message.
pub fn build_macro(
    name: &str,
    command: &str,
    confirm: bool,
    original: Option<&str>,
    macros: &[Macro],
) -> Result<Macro, String> {
    let name = validate_name(name, original, macros)?;
    let command = validate_command(command)?;
    Ok(Macro {
        name,
        command,
        confirm,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Macro {
        Macro {
            name: "Safe Z".into(),
            command: "G53 G0 Z0".into(),
            confirm: true,
        }
    }

    fn ready_gate() -> MacroRunGate {
        MacroRunGate {
            live: true,
            ready: true,
            machine_state: "Idle",
            job_active: false,
        }
    }

    #[test]
    fn names_are_unique_trimmed_and_bounded() {
        let macros = vec![sample()];
        assert_eq!(
            validate_name("  Park  ", None, &macros),
            Ok("Park".to_string())
        );
        assert!(validate_name("", None, &macros).is_err());
        assert!(validate_name("   ", None, &macros).is_err());
        assert!(validate_name(&"x".repeat(MAX_MACRO_NAME_CHARS + 1), None, &macros).is_err());
        assert!(validate_name("Bad\nName", None, &macros).is_err());
        assert!(validate_name("Bad\u{7}Name", None, &macros).is_err());

        let error = validate_name(" Safe Z ", None, &macros).expect_err("duplicate");
        assert!(error.contains("already exists"), "{error}");
        assert_eq!(
            validate_name("Safe Z", Some("Safe Z"), &macros),
            Ok("Safe Z".to_string()),
            "editing a record must not collide with itself"
        );
        assert!(validate_name("Safe Z", Some("Other"), &macros).is_err());
    }

    #[test]
    fn commands_are_single_line_bounded_and_never_system_commands() {
        assert_eq!(validate_command("  G0 X1 Y2  "), Ok("G0 X1 Y2".to_string()));
        for (input, needle) in [
            ("", "required"),
            ("   ", "required"),
            ("$H", "system"),
            ("G0 X0 (cost $5)", "system"),
            ("G0 X0\nG0 Y0", "single MDI line"),
            ("G0 X0\rG0 Y0", "single MDI line"),
            ("G0 X0\tY0", "control"),
            ("G0 X0\u{7}", "control"),
        ] {
            let error = validate_command(input).expect_err(input);
            assert!(error.contains(needle), "{input:?}: {error}");
        }
        let long = "G".repeat(MAX_MACRO_COMMAND_BYTES + 1);
        assert!(validate_command(&long).is_err());
    }

    #[test]
    fn a_confirmed_macro_is_captured_and_produces_one_command() {
        let macros = vec![sample()];
        let mut model = MacroModel::new();
        let request = model
            .request_run(&macros, "Safe Z", ready_gate())
            .expect("request");
        assert!(matches!(&request, MacroRunRequest::Confirm(mac) if mac.name == "Safe Z"));
        assert_eq!(model.pending().map(|mac| mac.name.as_str()), Some("Safe Z"));

        let command = model.confirm(ready_gate()).expect("confirmed command");
        assert_eq!(command, Command::Mdi("G53 G0 Z0".into()));
        assert!(model.pending().is_none());
        assert!(
            model.confirm(ready_gate()).is_err(),
            "a second press must not send a duplicate"
        );
    }

    #[test]
    fn cancelling_or_a_closed_gate_produces_no_command() {
        let macros = vec![sample()];
        let mut model = MacroModel::new();
        model
            .request_run(&macros, "Safe Z", ready_gate())
            .expect("request");
        assert!(model.cancel().is_some());
        assert!(model.confirm(ready_gate()).is_err());

        model
            .request_run(&macros, "Safe Z", ready_gate())
            .expect("request again");
        let error = model
            .confirm(MacroRunGate {
                machine_state: "Alarm",
                ..ready_gate()
            })
            .expect_err("closed gate");
        assert!(error.contains("alarm or door"), "{error}");
        assert!(model.pending().is_none());
    }

    #[test]
    fn a_direct_macro_is_sent_once_without_a_confirmation() {
        let mut mac = sample();
        mac.confirm = false;
        let macros = vec![mac];
        let mut model = MacroModel::new();
        let request = model
            .request_run(&macros, "Safe Z", ready_gate())
            .expect("request");
        assert!(matches!(request, MacroRunRequest::Send(_)));
        assert!(model.pending().is_none());
        assert!(model.confirm(ready_gate()).is_err());
    }

    #[test]
    fn every_run_gate_has_a_clear_reason() {
        let cases = [
            (
                MacroRunGate {
                    live: false,
                    ..ready_gate()
                },
                "mock",
            ),
            (
                MacroRunGate {
                    ready: false,
                    ..ready_gate()
                },
                "not connected",
            ),
            (
                MacroRunGate {
                    machine_state: "Alarm",
                    ..ready_gate()
                },
                "alarm or door",
            ),
            (
                MacroRunGate {
                    machine_state: "Door",
                    ..ready_gate()
                },
                "alarm or door",
            ),
            (
                MacroRunGate {
                    job_active: true,
                    ..ready_gate()
                },
                "job active",
            ),
            (
                MacroRunGate {
                    machine_state: "Run",
                    ..ready_gate()
                },
                "not Idle",
            ),
        ];
        for (gate, needle) in cases {
            let reason = gate.blocked_reason().expect("blocked");
            assert!(reason.contains(needle), "{reason}");
            let mut model = MacroModel::new();
            let error = model
                .request_run(&[sample()], "Safe Z", gate)
                .expect_err("request refused");
            assert!(error.contains(needle), "{error}");
            assert!(model.pending().is_none());
            assert!(model.last_error().is_some());
        }
        assert_eq!(ready_gate().blocked_reason(), None);
    }

    #[test]
    fn a_send_rejection_is_recorded_and_never_requeued() {
        let macros = vec![sample()];
        let mut model = MacroModel::new();
        model
            .request_run(&macros, "Safe Z", ready_gate())
            .expect("request");
        let command = model.confirm(ready_gate()).expect("command");
        model.send_failed(format!("engine refused {command:?}"));
        assert!(
            model
                .last_error()
                .is_some_and(|text| text.contains("engine refused"))
        );
        assert!(model.pending().is_none());
        assert!(model.confirm(ready_gate()).is_err());
        model.sent();
        assert!(model.last_error().is_none());
    }

    #[test]
    fn a_stale_multi_line_record_is_refused_before_the_engine() {
        let stale = Macro {
            name: "Old".into(),
            command: "G0 X0\nG0 Y0".into(),
            confirm: false,
        };
        let mut model = MacroModel::new();
        let error = model
            .request_run(&[stale], "Old", ready_gate())
            .expect_err("old record");
        assert!(error.contains("single MDI line"), "{error}");
    }
}
