//! Pure controller-reset model for the Utility destructive reset card.
//!
//! The model owns the selected `$RST` target, the explicit two-step
//! confirmation, the backup acknowledgement, and the last result. It never
//! sends anything itself: [`ControllerReset::confirm`] accepts the caller's
//! sender and hands it exactly one [`Command::System`] reset command.
//!
//! Local profiles, app preferences, backup bundles, and saved height maps are
//! never written or deleted by this module. The controller reset is remote
//! controller state only; the operator is told that the local profile is not
//! changed automatically.

use ghs_engine::{Command, ResetTarget, SystemCommand};

/// Every `$RST` target, in increasing order of destruction.
pub const RESET_TARGETS: [ResetTarget; 4] = [
    ResetTarget::Offsets,
    ResetTarget::Settings,
    ResetTarget::DriverParameters,
    ResetTarget::All,
];

/// Warning shown on the card and in the reset confirmation sheet.
pub const LOCAL_PROFILE_NOTE: &str = "LOCAL PROFILE IS NOT CHANGED AUTOMATICALLY";

/// Exact command line for one target.
#[must_use]
pub fn command_text(target: ResetTarget) -> &'static str {
    match target {
        ResetTarget::Offsets => "$RST=#",
        ResetTarget::Settings => "$RST=$",
        ResetTarget::DriverParameters => "$RST=&",
        ResetTarget::All => "$RST=*",
    }
}

/// Build the exact reset command for one target.
#[must_use]
pub fn reset_command(target: ResetTarget) -> Command {
    Command::System(SystemCommand::Reset(target))
}

/// Operator-facing title for one target.
#[must_use]
pub fn target_title(target: ResetTarget) -> &'static str {
    match target {
        ResetTarget::Offsets => "OFFSETS AND CONTROLLER TOOL DATA",
        ResetTarget::Settings => "CORE SETTING DEFAULTS",
        ResetTarget::DriverParameters => "DRIVER AND PLUGIN DEFAULTS",
        ResetTarget::All => "ALL SETTINGS, PARAMETERS, STARTUP LINES, AND BUILD INFO",
    }
}

/// What the operator loses by confirming one target.
#[must_use]
pub fn consequence(target: ResetTarget) -> &'static str {
    match target {
        ResetTarget::Offsets => {
            "Offsets ($#) and controller tool data are reset. Work coordinate systems, G28/G30, \
             G92, TLO, and the controller's tool records may be lost. Local profile data, \
             backups, and saved height maps stay."
        }
        ResetTarget::Settings => {
            "Core controller settings return to their defaults. Offsets and controller tool data \
             stay. Export a backup before changing settings again."
        }
        ResetTarget::DriverParameters => {
            "Driver and plugin settings return to their defaults. Core settings, offsets, and \
             controller tool data stay."
        }
        ResetTarget::All => {
            "ALL settings, parameters, startup lines, and build info are reset. Offsets, tool \
             data, and controller parameters may be unrecoverable. This is the most destructive \
             reset and the controller will usually re-handshake."
        }
    }
}

/// Short loss summary shown on the compact card row.
///
/// The full wording is in [`consequence`] and is repeated in the confirmation
/// sheet before anything can be sent.
#[must_use]
pub fn summary(target: ResetTarget) -> &'static str {
    match target {
        ResetTarget::Offsets => {
            "Offsets and controller tool data. Local profile, backups, and saved maps stay."
        }
        ResetTarget::Settings => "Core setting defaults. Offsets and tool data stay.",
        ResetTarget::DriverParameters => {
            "Driver and plugin defaults. Core settings, offsets, and tool data stay."
        }
        ResetTarget::All => {
            "All settings, parameters, startup lines, and build info. Most destructive."
        }
    }
}

/// Whether this is the most destructive target.
#[must_use]
pub fn is_most_destructive(target: ResetTarget) -> bool {
    target == ResetTarget::All
}

/// Read-only reports requested after a successful reset.
///
/// The reset command itself is never part of this list. A driver/plugin reset
/// has no dedicated read-only report in the app, so it requests none.
#[must_use]
pub fn refresh_commands(target: ResetTarget) -> Vec<Command> {
    match target {
        ResetTarget::Offsets => vec![Command::System(SystemCommand::Offsets)],
        ResetTarget::Settings => vec![Command::System(SystemCommand::Settings)],
        ResetTarget::DriverParameters => Vec::new(),
        ResetTarget::All => vec![
            Command::System(SystemCommand::Offsets),
            Command::System(SystemCommand::Settings),
        ],
    }
}

/// Short description of the read-only refresh attempted after a reset.
#[must_use]
pub fn refresh_note(target: ResetTarget) -> &'static str {
    match target {
        ResetTarget::Offsets => "Requested read-only `$#` refresh.",
        ResetTarget::Settings => "Requested read-only `$$` settings dump.",
        ResetTarget::DriverParameters => "No read-only report is defined for driver parameters.",
        ResetTarget::All => "Requested read-only `$#` and `$$` refreshes.",
    }
}

/// Controller and app preconditions for a destructive `$RST`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResetGate {
    /// The app is running the `--mock` design-review shell.
    pub mock: bool,
    /// A live engine session is open.
    pub live: bool,
    /// The handshake finished and telemetry is live.
    pub ready: bool,
    /// The controller reports `Idle`.
    pub idle: bool,
    /// The controller reports `Alarm` or `Door`.
    pub alarm_or_door: bool,
    /// A job is running or paused.
    pub job_active: bool,
    /// A manual tool change is waiting.
    pub tool_change_pending: bool,
    /// A probe run or probe write is active.
    pub probe_active: bool,
    /// A height-map Apply or Revert is pending or loading.
    pub height_map_action_in_flight: bool,
    /// A pendant has taken the input stream (`MPG:1`).
    pub pendant_active: bool,
    /// A setting write is queued, writing, or awaiting read-back.
    pub settings_write_pending: bool,
    /// An offset write is pending or refreshing.
    pub offset_write_pending: bool,
}

impl ResetGate {
    /// An open gate for tests: live, ready, Idle, and no activity.
    #[cfg(test)]
    pub const READY_IDLE: Self = Self {
        mock: false,
        live: true,
        ready: true,
        idle: true,
        alarm_or_door: false,
        job_active: false,
        tool_change_pending: false,
        probe_active: false,
        height_map_action_in_flight: false,
        pendant_active: false,
        settings_write_pending: false,
        offset_write_pending: false,
    };

    /// First blocking reason, or `None` when the controller is reset-safe.
    #[must_use]
    pub fn refusal(self) -> Option<ResetRefusal> {
        if self.mock {
            Some(ResetRefusal::Mock)
        } else if !self.live {
            Some(ResetRefusal::NotLive)
        } else if !self.ready {
            Some(ResetRefusal::NotReady)
        } else if self.alarm_or_door {
            Some(ResetRefusal::AlarmOrDoor)
        } else if self.job_active {
            Some(ResetRefusal::JobActive)
        } else if self.tool_change_pending {
            Some(ResetRefusal::ToolChangePending)
        } else if self.probe_active {
            Some(ResetRefusal::ProbeActive)
        } else if self.height_map_action_in_flight {
            Some(ResetRefusal::HeightMapAction)
        } else if self.pendant_active {
            Some(ResetRefusal::PendantActive)
        } else if self.settings_write_pending {
            Some(ResetRefusal::SettingsWritePending)
        } else if self.offset_write_pending {
            Some(ResetRefusal::OffsetWritePending)
        } else if !self.idle {
            Some(ResetRefusal::NotIdle)
        } else {
            None
        }
    }
}

/// Why a controller reset cannot be requested or confirmed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetRefusal {
    /// `--mock` has no engine to send a command to.
    Mock,
    /// No live session is open.
    NotLive,
    /// The link is connecting, disconnected, or failed.
    NotReady,
    /// The controller reports `Alarm` or `Door`.
    AlarmOrDoor,
    /// A job is running or paused.
    JobActive,
    /// A manual tool change is waiting.
    ToolChangePending,
    /// A probe run or probe write is active.
    ProbeActive,
    /// A height-map Apply or Revert is in flight.
    HeightMapAction,
    /// A pendant owns the input stream.
    PendantActive,
    /// A setting write is pending.
    SettingsWritePending,
    /// An offset write is pending.
    OffsetWritePending,
    /// The controller reports a state other than `Idle`.
    NotIdle,
    /// A backup export is currently writing.
    BackupExportInFlight,
    /// No successful export or explicit data-loss acknowledgement exists.
    BackupNotAcknowledged,
}

impl ResetRefusal {
    /// Operator-facing reason shown in the card and console.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            ResetRefusal::Mock => "MOCK MODE: controller reset needs a live controller",
            ResetRefusal::NotLive => "NOT LIVE: connect the controller before reset",
            ResetRefusal::NotReady => {
                "LINK NOT READY: reset is disabled until the handshake completes"
            }
            ResetRefusal::AlarmOrDoor => {
                "ALARM OR DOOR: clear the alarm or door state before reset"
            }
            ResetRefusal::JobActive => "JOB ACTIVE: stop or finish the job before reset",
            ResetRefusal::ToolChangePending => {
                "TOOL CHANGE PENDING: finish or stop the tool change before reset"
            }
            ResetRefusal::ProbeActive => "PROBE ACTIVE: finish or abort the probe run before reset",
            ResetRefusal::HeightMapAction => {
                "HEIGHT MAP BUSY: finish the Apply or Revert before reset"
            }
            ResetRefusal::PendantActive => {
                "PENDANT ACTIVE (MPG:1): release the pendant before reset"
            }
            ResetRefusal::SettingsWritePending => {
                "SETTINGS WRITE PENDING: finish or discard it before reset"
            }
            ResetRefusal::OffsetWritePending => {
                "OFFSET WRITE PENDING: finish the write and refresh before reset"
            }
            ResetRefusal::NotIdle => {
                "NOT IDLE: reset is available only while the controller reports Idle"
            }
            ResetRefusal::BackupExportInFlight => {
                "BACKUP EXPORT IN FLIGHT: wait for the export to finish"
            }
            ResetRefusal::BackupNotAcknowledged => {
                "BACKUP NOT ACKNOWLEDGED: export a backup or accept data loss first"
            }
        }
    }
}

/// Last reset result shown on the Utility card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetResult {
    /// Exactly one reset command was accepted by the sender.
    Sent,
    /// A gate or backup check refused the request; nothing was sent.
    Refused,
    /// The sender rejected the one reset command.
    Failed,
}

impl ResetResult {
    /// Short uppercase label for the card.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            ResetResult::Sent => "SENT",
            ResetResult::Refused => "REFUSED",
            ResetResult::Failed => "FAILED",
        }
    }
}

/// Last backup-export attempt shown next to the reset card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackupExportResult {
    /// The bundle was written to this path.
    Exported(String),
    /// The export failed with this message.
    Failed(String),
}

impl BackupExportResult {
    /// Operator-facing text.
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            BackupExportResult::Exported(path) => path,
            BackupExportResult::Failed(message) => message,
        }
    }

    /// Whether the attempt succeeded.
    #[must_use]
    pub fn ok(&self) -> bool {
        matches!(self, BackupExportResult::Exported(_))
    }
}

/// Result of confirming the controller-reset sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResetOutcome {
    /// Exactly one reset command was accepted for this target.
    Sent {
        /// Target whose reset command was sent.
        target: ResetTarget,
    },
    /// A gate or backup check refused the confirmation; nothing was sent.
    Refused(ResetRefusal),
    /// The sender rejected the one reset command.
    Failed(String),
    /// No confirmation sheet was open; nothing was sent.
    NotConfirmed,
}

/// Pure state for the Utility `CONTROLLER RESET` card.
#[derive(Clone, Debug, PartialEq)]
pub struct ControllerReset {
    selected: ResetTarget,
    confirm_target: Option<ResetTarget>,
    export_succeeded: bool,
    loss_accepted: bool,
    export_in_flight: bool,
    export_result: Option<BackupExportResult>,
    last_result: Option<ResetResult>,
    last_target: Option<ResetTarget>,
    error: Option<String>,
}

impl Default for ControllerReset {
    fn default() -> Self {
        Self {
            selected: ResetTarget::Offsets,
            confirm_target: None,
            export_succeeded: false,
            loss_accepted: false,
            export_in_flight: false,
            export_result: None,
            last_result: None,
            last_target: None,
            error: None,
        }
    }
}

impl ControllerReset {
    /// Select a target. This never opens a confirmation and never sends.
    pub fn select(&mut self, target: ResetTarget) {
        self.selected = target;
    }

    /// Target captured by the open confirmation, when one is open.
    #[must_use]
    pub fn confirm_target(&self) -> Option<ResetTarget> {
        self.confirm_target
    }

    /// Whether the confirmation sheet is open.
    #[must_use]
    pub fn is_confirm_open(&self) -> bool {
        self.confirm_target.is_some()
    }

    /// Whether a backup was successfully exported in this session.
    #[must_use]
    pub fn export_succeeded_this_session(&self) -> bool {
        self.export_succeeded
    }

    /// Whether the operator explicitly accepted data loss without a backup.
    #[must_use]
    pub fn loss_accepted(&self) -> bool {
        self.loss_accepted
    }

    /// Toggle the explicit `I HAVE A BACKUP OR ACCEPT DATA LOSS` choice.
    pub fn toggle_loss_accepted(&mut self) {
        self.loss_accepted = !self.loss_accepted;
    }

    /// Set the explicit data-loss acknowledgement.
    pub fn set_loss_accepted(&mut self, accepted: bool) {
        self.loss_accepted = accepted;
    }

    /// Whether a reset may be confirmed with the current acknowledgement.
    #[must_use]
    pub fn backup_acknowledged(&self) -> bool {
        self.export_succeeded || self.loss_accepted
    }

    /// Whether a backup export is currently writing.
    #[must_use]
    pub fn backup_export_in_flight(&self) -> bool {
        self.export_in_flight
    }

    /// Last backup-export attempt.
    #[must_use]
    pub fn export_result(&self) -> Option<&BackupExportResult> {
        self.export_result.as_ref()
    }

    /// Last reset result.
    #[must_use]
    pub fn last_result(&self) -> Option<ResetResult> {
        self.last_result
    }

    /// Target of the last reset attempt.
    #[must_use]
    pub fn last_target(&self) -> Option<ResetTarget> {
        self.last_target
    }

    /// Last reset error or refusal text.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Mark the start of a backup export. Reset requests are refused until
    /// [`Self::backup_export_succeeded`] or [`Self::backup_export_failed`].
    pub fn begin_backup_export(&mut self) {
        self.export_in_flight = true;
        self.export_result = None;
    }

    /// Record a successful backup export and acknowledge the backup.
    pub fn backup_export_succeeded(&mut self, path: impl Into<String>) {
        self.export_in_flight = false;
        self.export_succeeded = true;
        self.export_result = Some(BackupExportResult::Exported(path.into()));
    }

    /// Record a failed backup export. A previous successful export in this
    /// session remains acknowledged.
    pub fn backup_export_failed(&mut self, message: impl Into<String>) {
        self.export_in_flight = false;
        self.export_result = Some(BackupExportResult::Failed(message.into()));
    }

    /// Clear the session acknowledgement and every transient field.
    pub fn reset_session(&mut self) {
        *self = Self::default();
    }

    /// First gate or backup reason a reset cannot proceed.
    #[must_use]
    pub fn refusal(&self, gate: ResetGate) -> Option<ResetRefusal> {
        self.gate_refusal(gate)
            .or_else(|| self.backup_acknowledgement_refusal())
    }

    /// First controller/app gate reason, ignoring the backup acknowledgement.
    #[must_use]
    pub fn gate_refusal(&self, gate: ResetGate) -> Option<ResetRefusal> {
        gate.refusal()
    }

    /// Backup acknowledgement or export-in-flight refusal.
    #[must_use]
    pub fn backup_acknowledgement_refusal(&self) -> Option<ResetRefusal> {
        if self.export_in_flight {
            Some(ResetRefusal::BackupExportInFlight)
        } else if !self.backup_acknowledged() {
            Some(ResetRefusal::BackupNotAcknowledged)
        } else {
            None
        }
    }

    /// Open the confirmation for the selected target.
    ///
    /// Opening sends nothing. The exact target is captured so a later
    /// confirmation can only send the command the operator saw.
    ///
    /// # Errors
    ///
    /// Returns the first gate or backup refusal. No sheet opens and nothing is
    /// sent.
    pub fn request_confirm(&mut self, gate: ResetGate) -> Result<(), ResetRefusal> {
        self.confirm_target = None;
        if let Some(refusal) = self.refusal(gate) {
            self.record_refused(refusal, self.selected);
            return Err(refusal);
        }
        self.confirm_target = Some(self.selected);
        self.error = None;
        Ok(())
    }

    /// Close the confirmation without sending anything.
    pub fn cancel_confirm(&mut self) {
        self.confirm_target = None;
    }

    /// Consume the open confirmation and hand exactly one reset command to
    /// `send`.
    ///
    /// The gate and backup acknowledgement are checked again at this moment.
    /// A refusal, a missing confirmation, or a sender rejection sends nothing.
    pub fn confirm(
        &mut self,
        gate: ResetGate,
        send: impl FnOnce(Command) -> Result<(), String>,
    ) -> ResetOutcome {
        let Some(target) = self.confirm_target.take() else {
            return ResetOutcome::NotConfirmed;
        };
        if let Some(refusal) = self.refusal(gate) {
            self.record_refused(refusal, target);
            return ResetOutcome::Refused(refusal);
        }
        match send(reset_command(target)) {
            Ok(()) => {
                self.last_result = Some(ResetResult::Sent);
                self.last_target = Some(target);
                self.error = None;
                ResetOutcome::Sent { target }
            }
            Err(message) => {
                self.last_result = Some(ResetResult::Failed);
                self.last_target = Some(target);
                self.error = Some(message.clone());
                ResetOutcome::Failed(message)
            }
        }
    }

    fn record_refused(&mut self, refusal: ResetRefusal, target: ResetTarget) {
        self.last_result = Some(ResetResult::Refused);
        self.last_target = Some(target);
        self.error = Some(refusal.message().to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorded_send(sent: &mut Vec<Command>) -> impl FnMut(Command) -> Result<(), String> + '_ {
        move |command| {
            sent.push(command);
            Ok(())
        }
    }

    fn acknowledged() -> ControllerReset {
        let mut model = ControllerReset::default();
        model.set_loss_accepted(true);
        model
    }

    #[test]
    fn every_target_has_the_exact_reset_command() {
        let expected = [
            (ResetTarget::Offsets, "$RST=#"),
            (ResetTarget::Settings, "$RST=$"),
            (ResetTarget::DriverParameters, "$RST=&"),
            (ResetTarget::All, "$RST=*"),
        ];
        for (target, line) in expected {
            assert_eq!(command_text(target), line);
            assert_eq!(
                reset_command(target),
                Command::System(SystemCommand::Reset(target))
            );
            assert_eq!(
                SystemCommand::Reset(target)
                    .encode()
                    .expect("reset encodes"),
                line
            );
        }
        assert_eq!(RESET_TARGETS.len(), expected.len());
    }

    #[test]
    fn opening_a_confirmation_sends_nothing_and_captures_the_target() {
        let mut model = acknowledged();
        let sent: Vec<Command> = Vec::new();
        model.select(ResetTarget::Settings);
        assert!(model.request_confirm(ResetGate::READY_IDLE).is_ok());
        assert_eq!(model.confirm_target(), Some(ResetTarget::Settings));
        assert!(sent.is_empty());
        assert!(model.last_result().is_none());
    }

    #[test]
    fn confirmation_is_required_and_cancel_sends_nothing() {
        let mut model = acknowledged();
        let mut sent = Vec::new();
        assert_eq!(
            model.confirm(ResetGate::READY_IDLE, recorded_send(&mut sent)),
            ResetOutcome::NotConfirmed
        );
        assert!(sent.is_empty());

        model.select(ResetTarget::Offsets);
        model.request_confirm(ResetGate::READY_IDLE).expect("open");
        model.cancel_confirm();
        assert!(!model.is_confirm_open());
        assert_eq!(
            model.confirm(ResetGate::READY_IDLE, recorded_send(&mut sent)),
            ResetOutcome::NotConfirmed
        );
        assert!(sent.is_empty());
    }

    #[test]
    fn an_unacknowledged_backup_blocks_the_confirmation() {
        let mut model = ControllerReset::default();
        assert_eq!(
            model.request_confirm(ResetGate::READY_IDLE),
            Err(ResetRefusal::BackupNotAcknowledged)
        );
        assert!(!model.is_confirm_open());
        assert_eq!(model.last_result(), Some(ResetResult::Refused));
        assert_eq!(model.last_target(), Some(ResetTarget::Offsets));
        assert!(model.error().is_some_and(|text| text.contains("BACKUP")));
    }

    #[test]
    fn a_backup_in_flight_blocks_the_confirmation() {
        let mut model = acknowledged();
        model.begin_backup_export();
        assert!(model.backup_export_in_flight());
        assert_eq!(
            model.request_confirm(ResetGate::READY_IDLE),
            Err(ResetRefusal::BackupExportInFlight)
        );
        assert!(!model.is_confirm_open());

        model.backup_export_succeeded("backup.json");
        assert!(!model.backup_export_in_flight());
        assert!(model.backup_acknowledged());
        assert!(model.request_confirm(ResetGate::READY_IDLE).is_ok());
    }

    #[test]
    fn every_gate_refusal_sends_nothing_on_confirm() {
        let cases = [
            (
                ResetGate {
                    mock: true,
                    ..ResetGate::READY_IDLE
                },
                ResetRefusal::Mock,
            ),
            (
                ResetGate {
                    live: false,
                    ..ResetGate::READY_IDLE
                },
                ResetRefusal::NotLive,
            ),
            (
                ResetGate {
                    ready: false,
                    ..ResetGate::READY_IDLE
                },
                ResetRefusal::NotReady,
            ),
            (
                ResetGate {
                    alarm_or_door: true,
                    ..ResetGate::READY_IDLE
                },
                ResetRefusal::AlarmOrDoor,
            ),
            (
                ResetGate {
                    job_active: true,
                    ..ResetGate::READY_IDLE
                },
                ResetRefusal::JobActive,
            ),
            (
                ResetGate {
                    tool_change_pending: true,
                    ..ResetGate::READY_IDLE
                },
                ResetRefusal::ToolChangePending,
            ),
            (
                ResetGate {
                    probe_active: true,
                    ..ResetGate::READY_IDLE
                },
                ResetRefusal::ProbeActive,
            ),
            (
                ResetGate {
                    height_map_action_in_flight: true,
                    ..ResetGate::READY_IDLE
                },
                ResetRefusal::HeightMapAction,
            ),
            (
                ResetGate {
                    pendant_active: true,
                    ..ResetGate::READY_IDLE
                },
                ResetRefusal::PendantActive,
            ),
            (
                ResetGate {
                    settings_write_pending: true,
                    ..ResetGate::READY_IDLE
                },
                ResetRefusal::SettingsWritePending,
            ),
            (
                ResetGate {
                    offset_write_pending: true,
                    ..ResetGate::READY_IDLE
                },
                ResetRefusal::OffsetWritePending,
            ),
            (
                ResetGate {
                    idle: false,
                    ..ResetGate::READY_IDLE
                },
                ResetRefusal::NotIdle,
            ),
        ];

        for (gate, refusal) in cases {
            let mut model = acknowledged();
            model.request_confirm(ResetGate::READY_IDLE).expect("open");
            let mut sent = Vec::new();
            assert_eq!(
                model.confirm(gate, recorded_send(&mut sent)),
                ResetOutcome::Refused(refusal)
            );
            assert!(sent.is_empty(), "{refusal:?} must not send");
            assert_eq!(model.last_result(), Some(ResetResult::Refused));
        }
    }

    #[test]
    fn one_confirmation_sends_exactly_one_reset_and_then_only_the_refresh() {
        for target in RESET_TARGETS {
            let mut model = acknowledged();
            model.select(target);
            model.request_confirm(ResetGate::READY_IDLE).expect("open");
            let mut sent = Vec::new();
            let outcome = model.confirm(ResetGate::READY_IDLE, recorded_send(&mut sent));
            assert_eq!(outcome, ResetOutcome::Sent { target });
            sent.extend(refresh_commands(target));
            assert_eq!(sent[0], reset_command(target));
            assert_eq!(
                &sent[1..],
                refresh_commands(target).as_slice(),
                "{target:?} refresh plan"
            );
            assert_eq!(
                sent.iter()
                    .filter(|command| matches!(command, Command::System(SystemCommand::Reset(_))))
                    .count(),
                1
            );
        }
    }

    #[test]
    fn a_sender_rejection_is_failed_once_and_never_retried() {
        let mut model = acknowledged();
        model.select(ResetTarget::Offsets);
        model.request_confirm(ResetGate::READY_IDLE).expect("open");
        let mut sent = Vec::new();
        let outcome = model.confirm(ResetGate::READY_IDLE, |command| {
            sent.push(command);
            Err("engine is busy".to_string())
        });
        assert_eq!(outcome, ResetOutcome::Failed("engine is busy".to_string()));
        assert_eq!(model.last_result(), Some(ResetResult::Failed));
        assert_eq!(model.error(), Some("engine is busy"));
        assert_eq!(sent.len(), 1);

        assert_eq!(
            model.confirm(ResetGate::READY_IDLE, recorded_send(&mut sent)),
            ResetOutcome::NotConfirmed
        );
        assert_eq!(sent.len(), 1, "a rejected reset is never retried");
    }

    #[test]
    fn the_full_reset_is_visibly_the_most_destructive() {
        assert!(is_most_destructive(ResetTarget::All));
        for target in [
            ResetTarget::Offsets,
            ResetTarget::Settings,
            ResetTarget::DriverParameters,
        ] {
            assert!(!is_most_destructive(target));
        }
        let text = consequence(ResetTarget::All).to_lowercase();
        assert!(text.contains("all settings"));
        assert!(text.contains("startup lines"));
        assert!(text.contains("build info"));
        assert!(text.contains("most destructive"));
        assert!(consequence(ResetTarget::Offsets).contains("tool data"));
        assert!(consequence(ResetTarget::Settings).contains("Core controller settings"));
        assert!(consequence(ResetTarget::DriverParameters).contains("Driver and plugin"));
    }
}
