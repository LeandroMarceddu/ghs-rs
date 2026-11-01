//! grblHAL realtime status report parsing.

use ghs_core::{
    AlarmCode, Axes, BufferState, MachineState, Overrides, Position, ProbeResult, RunSubstate,
    Signals, Wcs,
};

use crate::error::ParseError;
use crate::offsets::parse_probe_payload;
use crate::util::{
    bounded, parse_bool01, parse_f64, parse_i32, parse_position, parse_u16, parse_u32,
};

/// Maximum length kept for a free-form status field.
const MAX_FIELD_LEN: usize = 128;

/// One status field that was present but could not be decoded.
///
/// Malformed fields never fail the whole report: the rest of the status is
/// still useful, and the UI can surface the problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedField {
    /// Field key, for example `"WPos"`.
    pub key: String,
    /// Raw field value.
    pub value: String,
    /// Why the value was rejected.
    pub reason: &'static str,
}

/// Accessory state from the `A:` status field.
///
/// Letters follow the grblHAL report: `S` spindle clockwise, `C` spindle
/// counter-clockwise, `E` spindle encoder error, `F` flood, `M` mist and `T` a
/// pending tool change.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessoryState {
    /// The spindle is running.
    pub spindle_on: bool,
    /// The spindle runs clockwise (`M3`); `false` for counter-clockwise (`M4`).
    pub clockwise: bool,
    /// The spindle encoder reported an error.
    pub encoder_error: bool,
    /// Flood coolant is on.
    pub flood: bool,
    /// Mist coolant is on.
    pub mist: bool,
    /// A manual tool change is pending.
    pub tool_change_pending: bool,
    /// Raw accessory letters, for forward compatibility.
    pub raw: String,
}

impl AccessoryState {
    /// Decode an `A:` value.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        let clockwise = value.contains('S');
        let counter_clockwise = value.contains('C');
        AccessoryState {
            spindle_on: clockwise || counter_clockwise,
            clockwise,
            encoder_error: value.contains('E'),
            flood: value.contains('F'),
            mist: value.contains('M'),
            tool_change_pending: value.contains('T'),
            raw: bounded(value, 32),
        }
    }
}

/// Homing state from the `H:` status field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HomingStatus {
    /// Whether homing is complete.
    pub complete: bool,
    /// Axes known to be homed, when the controller reported a mask.
    pub homed_axes: Option<Axes>,
}

/// SD card state from the `SD:` status field.
///
/// grblHAL uses one field for two purposes; both forms are preserved.
#[derive(Debug, Clone, PartialEq)]
pub enum SdStatus {
    /// Mount state: `0` not mounted, `1` mounted, `2` not mounted with
    /// auto-detect, `3` mounted with auto-detect.
    Mount(u8),
    /// Streaming progress: percentage of blocks read and the file name.
    Progress {
        /// Percentage of blocks read, as reported.
        percent: f32,
        /// File being streamed.
        file: String,
    },
    /// Streaming was temporarily suspended.
    Pending,
    /// An unrecognised value, kept verbatim.
    Raw(String),
}

impl SdStatus {
    /// Decode an `SD:` value.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        let trimmed = value.trim();
        if trimmed.eq_ignore_ascii_case("pending") {
            return SdStatus::Pending;
        }
        if let Some((percent, file)) = trimmed.split_once(',')
            && let Some(percent) = parse_f64(percent)
        {
            #[allow(clippy::cast_possible_truncation)]
            let percent = percent as f32;
            return SdStatus::Progress {
                percent,
                file: bounded(file, MAX_FIELD_LEN),
            };
        }
        if let Ok(mount) = trimmed.parse::<u8>()
            && mount <= 3
        {
            return SdStatus::Mount(mount);
        }
        SdStatus::Raw(bounded(trimmed, MAX_FIELD_LEN))
    }
}

/// A complete realtime status report, `<...>`.
///
/// Fields that are only sent on change are optional. Unknown fields are kept in
/// [`StatusReport::unknown`] and malformed fields in
/// [`StatusReport::malformed`]; neither can make parsing fail.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatusReport {
    /// Machine state from the first field.
    pub state: MachineState,
    /// Domain substate (`Run:1` feed hold pending, `Run:2` probing).
    pub substate: RunSubstate,
    /// Raw numeric substate from any state (`Hold:<n>`, `Door:<n>`,
    /// `Alarm:<n>`, `Run:<n>`).
    pub substate_code: Option<u16>,
    /// Active alarm code from `Alarm:<n>`.
    pub alarm: Option<AlarmCode>,
    /// Machine position (`MPos`).
    pub mpos: Option<Position>,
    /// Work position (`WPos`).
    pub wpos: Option<Position>,
    /// Work coordinate offset (`WCO`).
    pub wco: Option<Position>,
    /// Distance to go in the current block (`DTG`).
    pub dtg: Option<Position>,
    /// Planner and serial buffer free space (`Bf`).
    pub buffer: Option<BufferState>,
    /// Controller line number (`Ln`).
    pub line_number: Option<u32>,
    /// Current feed rate in the reported units per minute (`FS`/`F`).
    pub feed: Option<f64>,
    /// Programmed spindle speed (`FS`).
    pub spindle_rpm: Option<f64>,
    /// Actual spindle speed, when the controller reports a third `FS` value.
    pub spindle_actual_rpm: Option<f64>,
    /// Additional spindle data fields such as `SP0:<value>`.
    pub spindle_data: Vec<(String, String)>,
    /// Asserted input signals (`Pn`).
    pub pins: Option<Signals>,
    /// Active work coordinate system (`WCS`).
    pub wcs: Option<Wcs>,
    /// Override percentages (`Ov`).
    pub overrides: Option<Overrides>,
    /// Accessory state (`A`).
    pub accessory: Option<AccessoryState>,
    /// Pendant control flag (`MPG:1` means the pendant owns the input stream).
    pub mpg: Option<bool>,
    /// Homing state (`H`).
    pub homing: Option<HomingStatus>,
    /// Lathe radius/diameter mode (`D`).
    pub diameter_mode: Option<bool>,
    /// Current tool id (`T`).
    pub tool: Option<u32>,
    /// Probe id from the `P:` field.
    pub probe_id: Option<u32>,
    /// Probe protection flag from the `P:` field.
    pub probe_protect: bool,
    /// Axes with scaling active (`Sc`).
    pub scaled_axes: Option<Axes>,
    /// Auto-reporting state and interval (`AR`).
    pub auto_report: Option<String>,
    /// Tool length reference set (`TLR`).
    pub tlo_reference_set: Option<bool>,
    /// Result of the last `M66` wait-for-input (`In`).
    pub m66_result: Option<i32>,
    /// Firmware name (`FW`).
    pub firmware: Option<String>,
    /// Check mode / blocking event flag (`$C`).
    pub check_mode: Option<bool>,
    /// SD card state (`SD`).
    pub sd: Option<SdStatus>,
    /// Probe coordinates when the build includes them in the status report.
    pub probe: Option<ProbeResult>,
    /// Unrecognised fields, preserving key and value.
    pub unknown: Vec<(String, String)>,
    /// Fields that were recognised but could not be decoded.
    pub malformed: Vec<MalformedField>,
}

impl StatusReport {
    /// Returns `true` when a pendant has taken over the input stream.
    #[must_use]
    pub fn is_mpg_controlled(&self) -> bool {
        self.mpg == Some(true)
    }

    /// Returns `true` when the controller is in check mode.
    #[must_use]
    pub fn is_check_mode(&self) -> bool {
        self.state == MachineState::Check || self.check_mode == Some(true)
    }

    /// Machine position, falling back to the work position plus `WCO`.
    #[must_use]
    pub fn machine_position(&self) -> Option<Position> {
        self.mpos
            .or_else(|| self.wpos.map(|wpos| wpos + self.wco.unwrap_or_default()))
    }
}

/// Parse a `<...>` realtime status report.
///
/// # Errors
///
/// Returns [`ParseError::Expected`] when the line is not enclosed in angle
/// brackets. Field-level problems are recorded in
/// [`StatusReport::malformed`] instead.
pub fn parse_status(line: &str) -> Result<StatusReport, ParseError> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .ok_or(ParseError::Expected(
            "a status report enclosed in '<' and '>'",
        ))?;

    let mut fields = inner.split('|');
    let head = fields.next().unwrap_or_default();
    let (state, substate_code) = MachineState::parse_with_code(head);

    let mut report = StatusReport {
        state,
        substate: RunSubstate::from_report(state, substate_code),
        substate_code,
        ..StatusReport::default()
    };
    if state == MachineState::Alarm {
        report.alarm = substate_code.map(AlarmCode::new);
    }

    for field in fields {
        if field.is_empty() {
            continue;
        }
        let Some((key, value)) = field.split_once(':') else {
            // `AR` without a value means auto-reporting is off.
            if field.eq_ignore_ascii_case("AR") {
                report.auto_report = Some(String::new());
            } else {
                report
                    .unknown
                    .push((bounded(field, MAX_FIELD_LEN), String::new()));
            }
            continue;
        };
        report.apply_field(key, value);
    }

    Ok(report)
}

impl StatusReport {
    fn apply_field(&mut self, key: &str, value: &str) {
        match key {
            "MPos" => self.mpos = self.position_field(key, value),
            "WPos" => self.wpos = self.position_field(key, value),
            "WCO" => self.wco = self.position_field(key, value),
            "DTG" => self.dtg = self.position_field(key, value),
            "Bf" => self.buffer = self.buffer_field(key, value),
            "Ln" => self.line_number = self.u32_field(key, value),
            "FS" => self.feed_spindle_field(key, value),
            "F" => self.feed = self.f64_field(key, value),
            "Pn" => self.pins = Some(Signals::parse(value)),
            "WCS" => match Wcs::parse(value) {
                Some(wcs) => self.wcs = Some(wcs),
                None => self.malformed(key, value, "expected G54..G59.3"),
            },
            "Ov" => self.overrides = self.overrides_field(key, value),
            "A" => self.accessory = Some(AccessoryState::parse(value)),
            "MPG" => self.mpg = self.bool_field(key, value),
            "H" => self.homing = self.homing_field(key, value),
            "D" => self.diameter_mode = self.bool_field(key, value),
            "T" => self.tool = self.u32_field(key, value),
            "P" => self.probe_field(key, value),
            "Sc" => match Axes::parse(value) {
                Ok(axes) => self.scaled_axes = Some(axes),
                Err(_) => self.malformed(key, value, "expected axis letters"),
            },
            "AR" => self.auto_report = Some(bounded(value, MAX_FIELD_LEN)),
            "TLR" => self.tlo_reference_set = self.bool_field(key, value),
            "In" => self.m66_result = self.i32_field(key, value),
            "FW" => self.firmware = Some(bounded(value, MAX_FIELD_LEN)),
            "$C" => self.check_mode = self.bool_field(key, value),
            "SD" => self.sd = Some(SdStatus::parse(value)),
            "PRB" => match parse_probe_payload(value) {
                Some(probe) => self.probe = Some(probe),
                None => self.malformed(key, value, "expected probe coordinates"),
            },
            _ if key.starts_with("SP") && key.len() > 2 => {
                if self.spindle_data.len() < 4 {
                    self.spindle_data
                        .push((bounded(key, 16), bounded(value, MAX_FIELD_LEN)));
                }
            }
            _ => self
                .unknown
                .push((bounded(key, MAX_FIELD_LEN), bounded(value, MAX_FIELD_LEN))),
        }
    }

    fn position_field(&mut self, key: &str, value: &str) -> Option<Position> {
        match parse_position(value) {
            Ok(position) => Some(position),
            Err(reason) => {
                self.malformed(key, value, reason);
                None
            }
        }
    }

    fn buffer_field(&mut self, key: &str, value: &str) -> Option<BufferState> {
        let mut parts = value.split(',');
        let planner = parts.next().and_then(parse_u32);
        let rx = parts.next().and_then(parse_u32);
        match (planner, rx) {
            (Some(planner_free), Some(rx_free)) => Some(BufferState {
                planner_free,
                rx_free,
            }),
            _ => {
                self.malformed(key, value, "expected two integers");
                None
            }
        }
    }

    fn feed_spindle_field(&mut self, key: &str, value: &str) {
        let mut parts = value.split(',');
        let Some(feed) = parts.next().and_then(parse_f64) else {
            self.malformed(key, value, "expected a feed rate");
            return;
        };
        self.feed = Some(feed);
        if let Some(spindle) = parts.next() {
            match parse_f64(spindle) {
                Some(rpm) => self.spindle_rpm = Some(rpm),
                None => self.malformed(key, value, "expected a spindle speed"),
            }
        }
        if let Some(actual) = parts.next() {
            self.spindle_actual_rpm = parse_f64(actual);
        }
    }

    fn overrides_field(&mut self, key: &str, value: &str) -> Option<Overrides> {
        let mut parts = value.split(',');
        let feed = parts.next().and_then(parse_u16);
        let rapid = parts.next().and_then(parse_u16);
        let spindle = parts.next().and_then(parse_u16);
        match (feed, rapid, spindle) {
            (Some(feed), Some(rapid), Some(spindle)) => Some(Overrides::new(feed, rapid, spindle)),
            _ => {
                self.malformed(key, value, "expected three percentages");
                None
            }
        }
    }

    fn homing_field(&mut self, key: &str, value: &str) -> Option<HomingStatus> {
        let mut parts = value.split(',');
        let Some(complete) = parts.next().and_then(parse_bool01) else {
            self.malformed(key, value, "expected 0 or 1");
            return None;
        };
        let homed_axes = parts
            .next()
            .and_then(|mask| mask.trim().parse::<u16>().ok())
            .map(Axes::from_bits_retain);
        Some(HomingStatus {
            complete,
            homed_axes,
        })
    }

    fn probe_field(&mut self, key: &str, value: &str) {
        let mut parts = value.split(',');
        let Some(id) = parts.next().and_then(parse_u32) else {
            self.malformed(key, value, "expected a probe id");
            return;
        };
        self.probe_id = Some(id);
        self.probe_protect = parts
            .next()
            .is_some_and(|flag| flag.trim().eq_ignore_ascii_case("P"));
    }

    fn bool_field(&mut self, key: &str, value: &str) -> Option<bool> {
        match parse_bool01(value) {
            Some(value) => Some(value),
            None => {
                self.malformed(key, value, "expected 0 or 1");
                None
            }
        }
    }

    fn u32_field(&mut self, key: &str, value: &str) -> Option<u32> {
        match parse_u32(value) {
            Some(value) => Some(value),
            None => {
                self.malformed(key, value, "expected a non-negative integer");
                None
            }
        }
    }

    fn i32_field(&mut self, key: &str, value: &str) -> Option<i32> {
        match parse_i32(value) {
            Some(value) => Some(value),
            None => {
                self.malformed(key, value, "expected an integer");
                None
            }
        }
    }

    fn f64_field(&mut self, key: &str, value: &str) -> Option<f64> {
        match parse_f64(value) {
            Some(value) => Some(value),
            None => {
                self.malformed(key, value, "expected a number");
                None
            }
        }
    }

    fn malformed(&mut self, key: &str, value: &str, reason: &'static str) {
        self.malformed.push(MalformedField {
            key: bounded(key, 32),
            value: bounded(value, MAX_FIELD_LEN),
            reason,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghs_core::Axis;

    #[test]
    fn parses_minimal_status() {
        let report = parse_status("<Idle|MPos:0.000,0.000,0.000>").expect("valid");
        assert_eq!(report.state, MachineState::Idle);
        assert_eq!(report.substate, RunSubstate::None);
        assert_eq!(report.mpos, Some(Position::xyz(0.0, 0.0, 0.0)));
        assert!(report.wpos.is_none());
        assert!(report.unknown.is_empty());
        assert!(report.malformed.is_empty());
    }

    #[test]
    fn parses_full_status_with_every_documented_field() {
        let line = concat!(
            "<Run:2|MPos:1.000,2.000,3.000|WPos:11.000,12.000,13.000|",
            "DTG:0.500,0.000,0.000|Bf:15,64|Ln:42|FS:1200.5,18000,17950|",
            "SP0:18000|Pn:XZP|WCO:10.000,10.000,10.000|WCS:G55|",
            "Ov:125,50,90|A:SFM|Sc:XY|AR:1000|MPG:1|H:1,77|D:1|T:3|",
            "P:1,P|TLR:1|In:7|FW:grblHAL|$C:1|SD:Pending|",
            "PRB:1.000,2.000,-3.000:1|X-EXTRA:anything>"
        );
        let report = parse_status(line).expect("valid");

        assert_eq!(report.state, MachineState::Run);
        assert_eq!(report.substate, RunSubstate::Probing);
        assert_eq!(report.substate_code, Some(2));
        assert_eq!(report.mpos, Some(Position::xyz(1.0, 2.0, 3.0)));
        assert_eq!(report.wpos, Some(Position::xyz(11.0, 12.0, 13.0)));
        assert_eq!(report.wco, Some(Position::xyz(10.0, 10.0, 10.0)));
        assert_eq!(
            report.dtg,
            Some(Position::from_values(Axes::xyz(), &[0.5, 0.0, 0.0]).expect("valid"))
        );
        assert_eq!(
            report.buffer,
            Some(BufferState {
                planner_free: 15,
                rx_free: 64
            })
        );
        assert_eq!(report.line_number, Some(42));
        assert_eq!(report.feed, Some(1200.5));
        assert_eq!(report.spindle_rpm, Some(18000.0));
        assert_eq!(report.spindle_actual_rpm, Some(17950.0));
        assert_eq!(
            report.spindle_data,
            vec![("SP0".to_string(), "18000".to_string())]
        );
        assert_eq!(
            report.pins,
            Some(Signals::X_LIMIT | Signals::Z_LIMIT | Signals::PROBE)
        );
        assert_eq!(report.wcs, Some(Wcs::G55));
        assert_eq!(report.overrides, Some(Overrides::new(125, 50, 90)));
        let accessory = report.accessory.as_ref().expect("accessory");
        assert!(accessory.spindle_on);
        assert!(accessory.clockwise);
        assert!(accessory.flood);
        assert!(accessory.mist);
        assert!(!accessory.tool_change_pending);
        assert_eq!(report.scaled_axes, Axes::parse("XY").ok());
        assert_eq!(report.auto_report.as_deref(), Some("1000"));
        assert_eq!(report.mpg, Some(true));
        assert!(report.is_mpg_controlled());
        assert_eq!(
            report.homing,
            Some(HomingStatus {
                complete: true,
                homed_axes: Some(Axes::from_bits_retain(77))
            })
        );
        assert_eq!(report.diameter_mode, Some(true));
        assert_eq!(report.tool, Some(3));
        assert_eq!(report.probe_id, Some(1));
        assert!(report.probe_protect);
        assert_eq!(report.tlo_reference_set, Some(true));
        assert_eq!(report.m66_result, Some(7));
        assert_eq!(report.firmware.as_deref(), Some("grblHAL"));
        assert_eq!(report.check_mode, Some(true));
        assert!(matches!(report.sd, Some(SdStatus::Pending)));
        assert_eq!(
            report.probe,
            Some(ProbeResult::new(true, Position::xyz(1.0, 2.0, -3.0)))
        );
        assert_eq!(
            report.unknown,
            vec![("X-EXTRA".to_string(), "anything".to_string())]
        );
        assert!(report.malformed.is_empty());
    }

    #[test]
    fn parses_hold_and_door_substates() {
        let hold = parse_status("<Hold:1|MPos:0,0,0>").expect("valid");
        assert_eq!(hold.state, MachineState::Hold);
        assert_eq!(hold.substate_code, Some(1));
        assert_eq!(hold.substate, RunSubstate::None);

        let door = parse_status("<Door:3>").expect("valid");
        assert_eq!(door.state, MachineState::Door);
        assert_eq!(door.substate_code, Some(3));

        let tool = parse_status("<Tool|MPos:0,0,0>").expect("valid");
        assert_eq!(tool.state, MachineState::Tool);
    }

    #[test]
    fn alarm_substate_becomes_an_alarm_code() {
        let report = parse_status("<Alarm:1|MPos:0,0,0>").expect("valid");
        assert_eq!(report.state, MachineState::Alarm);
        assert_eq!(report.substate_code, Some(1));
        assert_eq!(report.alarm, Some(AlarmCode::new(1)));
        assert_eq!(
            report.alarm.map(|alarm| alarm.to_string()).as_deref(),
            Some("alarm 1: hard limit triggered")
        );
    }

    #[test]
    fn malformed_fields_are_reported_not_fatal() {
        let report =
            parse_status("<Idle|MPos:1,two,3|Bf:x,y|Ln:-1|FS:fast,0|Ov:1,2|H:7|D:2|T:x|P:x|SD:9>")
                .expect("valid status");
        assert!(report.mpos.is_none());
        assert!(report.buffer.is_none());
        assert!(report.line_number.is_none());
        assert!(report.feed.is_none());
        assert!(report.overrides.is_none());
        assert!(report.homing.is_none());
        assert!(report.diameter_mode.is_none());
        assert!(report.tool.is_none());
        assert!(report.probe_id.is_none());
        assert_eq!(report.malformed.len(), 9);
        assert!(report.malformed.iter().any(|field| field.key == "MPos"));
        assert!(matches!(report.sd, Some(SdStatus::Raw(_))));
    }

    #[test]
    fn unknown_fields_and_letters_never_fail() {
        let report =
            parse_status("<Idle|FUTURE:value|Pn:XYZQ?!|AR|NoColon>").expect("valid status");
        assert_eq!(
            report.unknown,
            vec![
                ("FUTURE".to_string(), "value".to_string()),
                ("NoColon".to_string(), String::new()),
            ]
        );
        assert_eq!(report.auto_report.as_deref(), Some(""));
        assert_eq!(
            report.pins,
            Some(Signals::X_LIMIT | Signals::Y_LIMIT | Signals::Z_LIMIT | Signals::SINGLE_BLOCK)
        );
    }

    #[test]
    fn variable_axis_counts_are_preserved() {
        let report = parse_status("<Idle|MPos:1,2,3,4,5,6,7,8,9|WPos:1,2>").expect("valid");
        let mpos = report.mpos.expect("mpos");
        assert_eq!(mpos.axes().len(), 9);
        assert_eq!(mpos.get(Axis::W), 9.0);
        let wpos = report.wpos.expect("wpos");
        assert_eq!(wpos.axes().to_string(), "XY");
    }

    #[test]
    fn sd_status_forms() {
        assert_eq!(SdStatus::parse("0"), SdStatus::Mount(0));
        assert_eq!(SdStatus::parse("3"), SdStatus::Mount(3));
        assert_eq!(
            SdStatus::parse("42.5,part.nc"),
            SdStatus::Progress {
                percent: 42.5,
                file: "part.nc".to_string()
            }
        );
        assert_eq!(SdStatus::parse("pending"), SdStatus::Pending);
        assert!(matches!(SdStatus::parse("9"), SdStatus::Raw(_)));
    }

    #[test]
    fn rejects_lines_without_brackets() {
        assert!(parse_status("Idle").is_err());
        assert!(parse_status("<Idle").is_err());
        assert!(parse_status("").is_err());
    }

    #[test]
    fn machine_position_falls_back_to_wpos_plus_wco() {
        let report = parse_status("<Idle|WPos:1,2,3|WCO:10,20,30>").expect("valid");
        assert_eq!(
            report.machine_position(),
            Some(Position::xyz(11.0, 22.0, 33.0))
        );
    }

    #[test]
    fn accessory_states() {
        let cw = AccessoryState::parse("SF");
        assert!(cw.spindle_on && cw.clockwise && cw.flood && !cw.mist);
        let ccw = AccessoryState::parse("CMT");
        assert!(ccw.spindle_on && !ccw.clockwise && ccw.mist && ccw.tool_change_pending);
        let off = AccessoryState::parse("");
        assert!(!off.spindle_on);
    }
}
