//! Status-report merging into the immutable machine snapshot.
//!
//! grblHAL sends many fields only on change. Always merge into the previous
//! snapshot instead of replacing it, or the DRO would reset whenever the
//! controller omits `WPos`, `WCO`, `Ov`, or `H`.

use ghs_core::{MachineSnapshot, MachineState};
use ghs_proto::StatusReport;

/// Merge one status report into the previous snapshot.
///
/// Fields the report does not carry keep their previous value. A status that
/// leaves the `Alarm` state clears the stored alarm code.
#[must_use]
pub(crate) fn apply_status(previous: &MachineSnapshot, report: &StatusReport) -> MachineSnapshot {
    let mut next = previous.clone();
    next.state = report.state;
    next.substate = report.substate;

    if let Some(wco) = report.wco {
        next.wco = wco;
    }
    if let Some(mpos) = report.mpos {
        next.mpos = mpos;
    }
    if let Some(wpos) = report.wpos {
        next.wpos = wpos;
    }

    // Keep the DRO pair consistent when the controller reports a new offset and
    // only one of the two positions, as change-only reports sometimes do.
    if let Some(wco) = report.wco {
        match (report.mpos.is_some(), report.wpos.is_some()) {
            (true, false) => next.wpos = next.mpos - wco,
            (false, true) => next.mpos = next.wpos + wco,
            _ => {}
        }
    }

    if let Some(wcs) = report.wcs {
        next.wcs = wcs;
    }
    if let Some(overrides) = report.overrides {
        next.overrides = overrides;
    }
    if let Some(feed) = report.feed {
        next.feed_spindle.feed = feed;
    }
    if let Some(rpm) = report.spindle_rpm {
        next.feed_spindle.spindle_rpm = rpm;
    }
    if let Some(accessory) = &report.accessory {
        next.feed_spindle.spindle_on = accessory.spindle_on;
        next.feed_spindle.clockwise = accessory.clockwise;
    }
    if let Some(pins) = report.pins {
        next.signals = pins;
    }
    if let Some(buffer) = report.buffer {
        next.buffer = buffer;
    }
    if let Some(line) = report.line_number {
        next.line_number = Some(line);
        next.job.line = Some(line);
    }
    if let Some(homing) = report.homing {
        next.homing_complete = Some(homing.complete);
        if let Some(mask) = homing.homed_axes {
            next.homed_axes = mask;
        }
    }
    if let Some(probe) = report.probe {
        next.probe = Some(probe);
    }

    if report.state == MachineState::Alarm {
        if let Some(alarm) = report.alarm {
            next.alarm = Some(alarm);
        }
    } else {
        next.alarm = None;
    }

    next
}

#[cfg(test)]
mod tests {
    use ghs_core::{Axes, MachineState, Position, Signals, Wcs};
    use ghs_proto::parse_status;

    use super::*;

    fn merge(previous: &MachineSnapshot, line: &str) -> MachineSnapshot {
        let report = parse_status(line).expect("valid status");
        apply_status(previous, &report)
    }

    #[test]
    fn positions_and_offsets_are_kept_when_omitted() {
        let first = merge(
            &MachineSnapshot::default(),
            "<Idle|MPos:1.000,2.000,3.000|WPos:11.000,12.000,13.000|WCO:10.000,10.000,10.000|FS:0,0>",
        );
        assert_eq!(first.mpos, Position::xyz(1.0, 2.0, 3.0));
        assert_eq!(first.wpos, Position::xyz(11.0, 12.0, 13.0));
        assert_eq!(first.wco, Position::xyz(10.0, 10.0, 10.0));

        let second = merge(&first, "<Run|FS:1200,18000|Ov:125,50,90|WCS:G55>");
        assert_eq!(second.state, MachineState::Run);
        assert_eq!(second.mpos, first.mpos);
        assert_eq!(second.wpos, first.wpos);
        assert_eq!(second.wco, first.wco);
        assert_eq!(second.feed_spindle.feed, 1200.0);
        assert_eq!(second.feed_spindle.spindle_rpm, 18000.0);
        assert_eq!(second.overrides.feed, 125);
        assert_eq!(second.overrides.rapid, 50);
        assert_eq!(second.overrides.spindle, 90);
        assert_eq!(second.wcs, Wcs::G55);
    }

    #[test]
    fn a_new_offset_with_one_position_keeps_the_pair_consistent() {
        let base = merge(
            &MachineSnapshot::default(),
            "<Idle|MPos:0,0,0|WPos:0,0,0|WCO:0,0,0>",
        );
        let moved = merge(
            &base,
            "<Idle|WPos:5.000,6.000,7.000|WCO:10.000,20.000,30.000>",
        );
        assert_eq!(moved.mpos, Position::xyz(15.0, 26.0, 37.0));
        assert_eq!(moved.wpos, Position::xyz(5.0, 6.0, 7.0));

        let moved = merge(
            &base,
            "<Idle|MPos:15.000,26.000,37.000|WCO:10.000,20.000,30.000>",
        );
        assert_eq!(moved.wpos, Position::xyz(5.0, 6.0, 7.0));
    }

    #[test]
    fn accessories_signals_and_homing_merge() {
        let first = merge(
            &MachineSnapshot::default(),
            "<Idle|Pn:XZP|A:SF|H:1,7|T:3|TLR:1|WCS:G54>",
        );
        assert_eq!(
            first.signals,
            Signals::X_LIMIT | Signals::Z_LIMIT | Signals::PROBE
        );
        assert!(first.feed_spindle.spindle_on);
        assert!(first.feed_spindle.clockwise);
        assert_eq!(first.homing_complete, Some(true));
        assert_eq!(first.homed_axes, Axes::xyz());

        let second = merge(&first, "<Run|A:CM>");
        assert!(second.feed_spindle.spindle_on);
        assert!(!second.feed_spindle.clockwise);
        assert_eq!(second.signals, first.signals);
        assert_eq!(second.homing_complete, Some(true));
    }

    #[test]
    fn alarm_set_and_clear() {
        let alarm = merge(&MachineSnapshot::default(), "<Alarm:1|MPos:0,0,0>");
        assert_eq!(alarm.state, MachineState::Alarm);
        assert!(alarm.is_alarm());
        assert_eq!(alarm.alarm.map(|code| code.code()), Some(1));

        let idle = merge(&alarm, "<Idle|MPos:0,0,0>");
        assert_eq!(idle.state, MachineState::Idle);
        assert!(!idle.is_alarm());
        assert!(idle.alarm.is_none());
    }

    #[test]
    fn extra_axes_are_preserved() {
        let report = parse_status("<Idle|MPos:1,2,3,4,5,6,7,8,9>").expect("valid");
        let merged = apply_status(&MachineSnapshot::default(), &report);
        assert_eq!(merged.mpos.axes().len(), 9);
    }

    #[test]
    fn malformed_fields_do_not_erase_the_previous_snapshot() {
        let first = merge(&MachineSnapshot::default(), "<Idle|MPos:1,2,3|FS:100,2000>");
        let second = merge(&first, "<Idle|MPos:bad|FS:bad>");
        assert_eq!(second.mpos, first.mpos);
        assert_eq!(second.feed_spindle.feed, first.feed_spindle.feed);
    }

    #[test]
    fn line_number_updates_job_info() {
        let snapshot = merge(&MachineSnapshot::default(), "<Run|Ln:42|MPos:0,0,0>");
        assert_eq!(snapshot.line_number, Some(42));
        assert_eq!(snapshot.job.line, Some(42));
    }
}
