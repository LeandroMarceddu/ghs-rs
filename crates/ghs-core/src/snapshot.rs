//! Immutable machine snapshot published to the UI.

use serde::{Deserialize, Serialize};

use crate::axis::Axes;
use crate::codes::AlarmCode;
use crate::overrides::{FeedSpindle, Overrides};
use crate::position::Position;
use crate::probe::ProbeResult;
use crate::signals::{AuxPins, Signals};
use crate::state::{MachineState, RunSubstate};
use crate::wcs::Wcs;

/// Free space reported in the grblHAL `Bf:` status field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BufferState {
    /// Free planner blocks.
    pub planner_free: u32,
    /// Free serial receive-buffer bytes.
    pub rx_free: u32,
}

/// Lifecycle of the job currently loaded in the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum JobState {
    /// No job has been started.
    #[default]
    Idle,
    /// Lines are being streamed.
    Running,
    /// Streaming is paused by feed hold or the user.
    Paused,
    /// Every line completed successfully.
    Completed,
    /// The job was stopped or failed.
    Aborted,
    /// The link was lost or the controller alarmed mid-job.
    Interrupted,
}

/// Job progress information shown in the UI.
///
/// This is application-side bookkeeping: it is timestamp-free and contains no
/// wall-clock data so snapshots stay deterministic and cheap to clone.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct JobInfo {
    /// Program name or path, when a job is loaded.
    pub name: Option<String>,
    /// Current job lifecycle state.
    pub state: JobState,
    /// Controller-reported line number for the current line, from `Ln:`.
    pub line: Option<u32>,
    /// Total lines in the loaded program, zero when unknown.
    pub total_lines: u32,
    /// Lines acknowledged by the controller, saturating at `total_lines`.
    pub completed_lines: u32,
    /// Progress in the range `0.0..=1.0`; zero when unknown.
    pub progress: f32,
    /// Whether the job is paused.
    pub paused: bool,
    /// Whether the job was interrupted by an alarm, E-stop or link loss.
    pub interrupted: bool,
}

impl JobInfo {
    /// Set progress from the completed/total line counts, clamped to
    /// `0.0..=1.0`.
    pub fn update_progress(&mut self) {
        self.progress = if self.total_lines == 0 {
            0.0
        } else {
            let completed = self.completed_lines.min(self.total_lines);
            (completed as f32 / self.total_lines as f32).clamp(0.0, 1.0)
        };
    }

    /// Returns `true` when a job is loaded and can still make progress.
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self.state, JobState::Running | JobState::Paused)
    }
}

/// Immutable snapshot of the controller and job state.
///
/// The engine publishes this through shared telemetry and an `Arc` snapshot;
/// the UI reads the latest value at frame time. Positions are in millimetres.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MachineSnapshot {
    /// Current machine state.
    pub state: MachineState,
    /// Optional run substate (feed hold pending or probing).
    pub substate: RunSubstate,
    /// Machine position.
    pub mpos: Position,
    /// Work position.
    pub wpos: Position,
    /// Active work coordinate offset (`WCO`).
    pub wco: Position,
    /// Active work coordinate system.
    pub wcs: Wcs,
    /// Feed, rapid and spindle overrides.
    pub overrides: Overrides,
    /// Live feed rate and spindle state.
    pub feed_spindle: FeedSpindle,
    /// Asserted input signals from `Pn:`.
    pub signals: Signals,
    /// Auxiliary IO state, when reported.
    pub pins: AuxPins,
    /// Planner and serial buffer free space.
    pub buffer: BufferState,
    /// Controller-reported line number, from `Ln:`.
    pub line_number: Option<u32>,
    /// Job progress information.
    pub job: JobInfo,
    /// Most recent probe result.
    pub probe: Option<ProbeResult>,
    /// Active alarm, when the controller reported one.
    pub alarm: Option<AlarmCode>,
    /// Whether homing is complete (`H:1`), when the controller reported it.
    pub homing_complete: Option<bool>,
    /// Axes known to be homed, when the controller reported a mask.
    pub homed_axes: Axes,
}

impl MachineSnapshot {
    /// Returns `true` when the controller is in an alarm state or reported an
    /// alarm code.
    #[must_use]
    pub fn is_alarm(&self) -> bool {
        self.state == MachineState::Alarm || self.alarm.is_some_and(|alarm| !alarm.is_none())
    }

    /// Returns `true` when a job is streaming.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.state == MachineState::Run || self.job.state == JobState::Running
    }

    /// Returns `true` when a position is known and finite.
    #[must_use]
    pub fn has_position(&self) -> bool {
        !self.mpos.axes().is_empty() && self.mpos.is_finite()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axis::Axis;

    #[test]
    fn default_snapshot_is_safe_and_empty() {
        let snapshot = MachineSnapshot::default();
        assert_eq!(snapshot.state, MachineState::Unknown);
        assert_eq!(snapshot.substate, RunSubstate::None);
        assert_eq!(snapshot.wcs, Wcs::G54);
        assert_eq!(snapshot.overrides, Overrides::DEFAULT);
        assert!(!snapshot.is_running());
        assert!(!snapshot.is_alarm());
        assert!(snapshot.probe.is_none());
        assert!(snapshot.alarm.is_none());
        assert!(snapshot.pins.is_empty());
    }

    #[test]
    fn alarm_detection_covers_state_and_code() {
        let mut snapshot = MachineSnapshot {
            state: MachineState::Alarm,
            ..MachineSnapshot::default()
        };
        assert!(snapshot.is_alarm());
        snapshot.state = MachineState::Idle;
        assert!(!snapshot.is_alarm());
        snapshot.alarm = Some(AlarmCode::new(1));
        assert!(snapshot.is_alarm());
    }

    #[test]
    fn job_progress_is_clamped() {
        let mut job = JobInfo {
            total_lines: 10,
            completed_lines: 5,
            ..JobInfo::default()
        };
        job.update_progress();
        assert_eq!(job.progress, 0.5);
        job.completed_lines = 50;
        job.update_progress();
        assert_eq!(job.progress, 1.0);
        job.total_lines = 0;
        job.update_progress();
        assert_eq!(job.progress, 0.0);
        assert!(!job.is_active());
        job.state = JobState::Running;
        assert!(job.is_active());
    }

    #[test]
    fn position_presence_requires_axes() {
        let mut snapshot = MachineSnapshot::default();
        assert!(snapshot.has_position());
        snapshot.mpos = Position::new(Axes::empty());
        assert!(!snapshot.has_position());
        snapshot.mpos = Position::xyz(1.0, 2.0, 3.0);
        assert!(snapshot.has_position());
        assert_eq!(snapshot.mpos[Axis::Z], 3.0);
    }
}
