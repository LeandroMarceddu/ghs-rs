//! Status, parser-state, offset and enumeration reports.
//!
//! Every report is capability-derived: fields such as `MPG`, `SD` and `SP<n>`
//! appear only when the profile advertises the feature.

use ghs_core::{AlarmCode, ErrorCode, MachineState, Position, Signals, Units, Wcs};
use ghs_proto::DistanceMode;

use super::Simulator;
use crate::util::{emit, format_gcode_number, format_position, format_position_mm};

impl Simulator {
    /// Controller welcome banner.
    ///
    /// This is not queued automatically: send it once on connect, or let a
    /// reset print it when banner output is enabled.
    #[must_use]
    pub fn banner(&self) -> Vec<u8> {
        let version = if self.profile.info.version.is_empty() {
            "1.1f"
        } else {
            &self.profile.info.version
        };
        format!("GrblHAL {version} ['$' for help]\r\n").into_bytes()
    }

    /// `$I` (short) or `$I+` (extended) identity report.
    pub(super) fn emit_info(&self, extended: bool, out: &mut Vec<u8>) {
        let lines: &[String] = if extended {
            &self.identity
        } else {
            &self.identity[..self.identity.len().min(3)]
        };
        for line in lines {
            emit(out, line);
        }
        emit(out, "ok");
    }

    /// Realtime status report; `full` selects the `0x87` complete form.
    pub(super) fn emit_status(&self, full: bool, out: &mut Vec<u8>) {
        let mut line = String::from("<");
        line.push_str(&self.state_word());
        line.push_str("|MPos:");
        line.push_str(&format_position(self.mpos, self.axes, self.units));
        line.push_str("|WPos:");
        line.push_str(&format_position(self.wpos(), self.axes, self.units));
        line.push_str("|WCO:");
        line.push_str(&format_position(self.wco(), self.axes, self.units));
        line.push_str(&format!("|Bf:{},{}", self.planner_free(), self.rx_buffer));
        line.push_str(&format!("|Ln:{}", self.line_number));
        line.push_str(&format!(
            "|FS:{},{}",
            format_gcode_number(self.units.from_mm(self.live_feed())),
            format_gcode_number(self.effective_rpm())
        ));
        line.push_str(&format!("|Pn:{}", self.pin_state()));
        line.push_str(&format!("|WCS:{}", self.wcs));
        line.push_str(&format!(
            "|Ov:{},{},{}",
            self.overrides.feed, self.overrides.rapid, self.overrides.spindle
        ));
        line.push_str(&format!("|A:{}", self.accessories()));
        if self.profile.info.has_mpg() {
            line.push_str(&format!("|MPG:{}", u8::from(self.mpg)));
        }
        line.push_str(&format!(
            "|H:{},{}",
            u8::from(self.homing_complete()),
            self.homed.bits()
        ));
        line.push_str(&format!("|T:{}", self.tool));
        if !full && self.check_mode {
            line.push_str("|$C:1");
        }
        if full {
            let dtg = self.motion.as_ref().map_or_else(
                || Position::new(self.axes),
                |motion| motion.target() - self.mpos,
            );
            line.push_str("|DTG:");
            line.push_str(&format_position(dtg, self.axes, self.units));
            line.push_str("|Sc:");
            line.push_str("|D:0");
            line.push_str("|P:0");
            line.push_str(&format!("|TLR:{}", u8::from(self.tlr.is_some())));
            line.push_str("|In:0");
            line.push_str("|FW:grblHAL");
            line.push_str(&format!("|$C:{}", u8::from(self.check_mode)));
        }
        if self.auto_report {
            line.push_str(&format!("|AR:{}", self.auto_report_interval.as_millis()));
        }
        for index in 1..usize::from(self.spindle_count) {
            let rpm = self.spindle_rpms.get(index).copied().unwrap_or(0.0);
            line.push_str(&format!("|SP{index}:{}", format_gcode_number(rpm)));
        }
        if self.profile.info.has_sd() {
            line.push_str("|SD:1");
        }
        line.push('>');
        emit(out, &line);
    }

    /// `$G` parser-state report.
    pub(super) fn emit_parser_state(&self, out: &mut Vec<u8>) {
        let units = if self.units == Units::Inch {
            "G20"
        } else {
            "G21"
        };
        let distance = match self.distance {
            DistanceMode::Absolute => "G90",
            DistanceMode::Incremental => "G91",
        };
        let spindle = match (self.effective_spindle_on(), self.spindle_clockwise) {
            (true, true) => "M3",
            (true, false) => "M4",
            _ => "M5",
        };
        let coolant = if self.flood {
            "M8"
        } else if self.mist {
            "M7"
        } else {
            "M9"
        };
        let tlo = if self.tlo.abs_max() > 0.0 {
            "G43.1"
        } else {
            "G49"
        };
        emit(
            out,
            &format!(
                "[GC:{} {} {} {} {} G94 {} G98 G50 {} {} T{} F{} S{}]",
                self.motion_mode,
                self.wcs,
                self.plane,
                units,
                distance,
                tlo,
                spindle,
                coolant,
                self.tool,
                format_gcode_number(self.units.from_mm(self.feed_rate)),
                format_gcode_number(self.spindle_rpm)
            ),
        );
        emit(out, "ok");
    }

    /// `$#` offset, home, tool-length and tool-table report.
    pub(super) fn emit_offsets(&self, out: &mut Vec<u8>) {
        for wcs in Wcs::ALL {
            emit(
                out,
                &format!(
                    "[{}:{}]",
                    wcs.as_str(),
                    format_position_mm(self.wcs_offsets[wcs.index()], self.axes)
                ),
            );
        }
        emit(
            out,
            &format!("[G28:{}]", format_position_mm(self.g28, self.axes)),
        );
        emit(
            out,
            &format!("[G30:{}]", format_position_mm(self.g30, self.axes)),
        );
        emit(
            out,
            &format!("[G92:{}]", format_position_mm(self.g92, self.axes)),
        );
        emit(
            out,
            &format!(
                "[HOME:{}:{}]",
                format_position_mm(self.home_position, self.axes),
                self.homed.bits()
            ),
        );
        emit(
            out,
            &format!("[TLO:{}]", format_position_mm(self.tlo, self.axes)),
        );
        emit(out, &format!("[TLR:{:.3}]", self.tlr.unwrap_or(0.0)));
        let (probe, success) = match self.probe_result {
            Some(result) => (result.position, u8::from(result.success)),
            None => (Position::new(self.axes), 0),
        };
        emit(
            out,
            &format!("[PRB:{}:{}]", format_position_mm(probe, self.axes), success),
        );
        for tool in &self.tool_table {
            emit(
                out,
                &format!(
                    "[T:{}|{}|{:.3}|0|{}|{}]",
                    tool.number,
                    format_position_mm(tool.offset, self.axes),
                    tool.radius_mm,
                    tool.name,
                    tool.pocket
                ),
            );
        }
        emit(out, "ok");
    }

    /// `$EA` alarm-code enumeration.
    pub(super) fn emit_alarm_codes(&self, out: &mut Vec<u8>) {
        for code in 1..=21 {
            let description = AlarmCode::new(code).description();
            if !description.is_empty() {
                emit(
                    out,
                    &format!("[ALARMCODE:{code}|{description}|{description}]"),
                );
            }
        }
        emit(out, "ok");
    }

    /// `$EE` error-code enumeration.
    pub(super) fn emit_error_codes(&self, out: &mut Vec<u8>) {
        for code in 1..=70 {
            let description = ErrorCode::new(code).description();
            if !description.is_empty() {
                emit(
                    out,
                    &format!("[ERRORCODE:{code}|{description}|{description}]"),
                );
            }
        }
        emit(out, "ok");
    }

    /// `$ES` setting metadata.
    pub(super) fn emit_setting_metadata(&self, out: &mut Vec<u8>) {
        for line in self.settings.metadata_lines() {
            emit(out, &line);
        }
        emit(out, "ok");
    }

    /// `$EG` setting groups.
    pub(super) fn emit_setting_groups(&self, out: &mut Vec<u8>) {
        for line in self.settings.group_lines() {
            emit(out, &line);
        }
        emit(out, "ok");
    }

    /// `$SED` long descriptions for every setting.
    pub(super) fn emit_setting_descriptions(&self, out: &mut Vec<u8>) {
        for line in self.settings.description_lines() {
            emit(out, &line);
        }
        emit(out, "ok");
    }

    /// `$PINS` pin mapping.
    pub(super) fn emit_pins(&self, out: &mut Vec<u8>) {
        let capability = self.profile.info.signals;
        let aux = self.profile.info.aux_io.unwrap_or_default();
        let mut index = 0_u16;
        for axis in self.axes.iter() {
            if capability.contains(Signals::axis_limit(axis)) {
                emit(out, &format!("[PIN:{index}|{} limit|input]", axis.as_str()));
                index = index.saturating_add(1);
            }
        }
        for (signal, name) in [
            (Signals::FEED_HOLD, "Feed hold"),
            (Signals::CYCLE_START, "Cycle start"),
            (Signals::DOOR, "Safety door"),
            (Signals::ESTOP, "E-stop"),
            (Signals::PROBE, "Probe"),
        ] {
            if capability.contains(signal) {
                emit(out, &format!("[PIN:{index}|{name}|input]"));
                index = index.saturating_add(1);
            }
        }
        for input in 0..aux.digital_in.min(crate::simulator::MAX_SIM_AUX_IO) {
            emit(out, &format!("[PIN:{index}|Aux input {input}|input]"));
            index = index.saturating_add(1);
        }
        for output in 0..aux.digital_out.min(crate::simulator::MAX_SIM_AUX_IO) {
            emit(out, &format!("[PIN:{index}|Aux output {output}|output]"));
            index = index.saturating_add(1);
        }
        emit(out, "ok");
    }

    /// `$PINSTATE` auxiliary pin levels.
    pub(super) fn emit_pin_state(&self, out: &mut Vec<u8>) {
        let inputs: String = self
            .aux_in
            .iter()
            .map(|state| u8::from(*state).to_string())
            .collect();
        let outputs: String = self
            .aux_out
            .iter()
            .map(|state| u8::from(*state).to_string())
            .collect();
        emit(
            out,
            &format!("[PINSTATE:{}|{inputs}|{outputs}]", self.pin_state()),
        );
        emit(out, "ok");
    }

    /// `$LIM` asserted limit switches.
    pub(super) fn emit_limits(&self, out: &mut Vec<u8>) {
        emit(out, &format!("[LIM:{}]", self.signals.axis_limits()));
        emit(out, "ok");
    }

    /// `$LEV` last control and limit events.
    pub(super) fn emit_events(&self, out: &mut Vec<u8>) {
        emit(out, &format!("[LEV:{}|{}]", self.lev_count, self.signals));
        emit(out, "ok");
    }

    /// `$PORTS` communication ports.
    pub(super) fn emit_ports(&self, out: &mut Vec<u8>) {
        emit(out, "[PORT:0|USB Serial|115200]");
        emit(out, "ok");
    }

    /// `$SPINDLES` / `$SPINDLESH` spindle enumeration.
    pub(super) fn emit_spindles(&self, detailed: bool, out: &mut Vec<u8>) {
        let description = self
            .profile
            .info
            .spindle
            .clone()
            .unwrap_or_else(|| "PWM".to_string());
        if detailed {
            emit(out, &format!("[SPINDLESH:{}]", self.spindle_count));
        }
        for index in 0..usize::from(self.spindle_count) {
            let rpm = self.spindle_rpms.get(index).copied().unwrap_or(0.0);
            let state = if self.effective_spindle_on() {
                "on"
            } else {
                "off"
            };
            if detailed {
                emit(
                    out,
                    &format!("[SPINDLE:{index}|{description}|{state}|{rpm:.3}]"),
                );
            } else {
                emit(out, &format!("[SPINDLE:{index}|{description}]"));
            }
        }
        emit(out, "ok");
    }

    /// `$HSS` homing switch state.
    pub(super) fn emit_homing_switch_state(&self, out: &mut Vec<u8>) {
        emit(out, &format!("[HSS:{}]", self.signals.axis_limits().bits()));
        emit(out, "ok");
    }

    /// `$HELP` text.
    pub(super) fn emit_help(&self, topic: &str, out: &mut Vec<u8>) {
        let topic = topic.trim();
        if topic.is_empty() {
            emit(out, "[HLP:grblHAL simulator]");
            emit(
                out,
                "[HLP:Use $HELP commands for the supported command list]",
            );
        } else if topic.eq_ignore_ascii_case("commands") {
            emit(
                out,
                "[HLP:$I $I+ $$ $+ $<n> $<n>=<v> $SED $G $# $N $H $HSS $X $SLP]",
            );
            emit(out, "[HLP:$C $B $S $O $J= $TLR $TPW $RST $EA $EE $ES $EG]");
            emit(
                out,
                "[HLP:$PINS $PINSTATE $LIM $LEV $PORTS $SPINDLES $SPINDLESH $REBOOT $RTC]",
            );
        } else {
            emit(out, &format!("[HLP:No help for {topic}]"));
        }
        emit(out, "ok");
    }

    /// `$RTC` simulated real-time clock.
    pub(super) fn emit_rtc(&self, out: &mut Vec<u8>) {
        emit(
            out,
            &format!("[RTC:{}]", crate::util::format_clock(self.clock)),
        );
        emit(out, "ok");
    }

    fn state_word(&self) -> String {
        if self.state == MachineState::Alarm {
            format!("Alarm:{}", self.alarm.unwrap_or(0))
        } else {
            self.state.as_str().to_string()
        }
    }

    fn live_feed(&self) -> f64 {
        self.motion
            .as_ref()
            .map_or(0.0, crate::motion::Motion::feed_mm_min)
    }

    fn effective_spindle_on(&self) -> bool {
        self.spindle_on && !self.spindle_stop
    }

    fn effective_rpm(&self) -> f64 {
        if self.effective_spindle_on() {
            self.spindle_rpm
        } else {
            0.0
        }
    }

    fn homing_complete(&self) -> bool {
        self.axes.difference(self.homed).is_empty()
    }

    fn pin_state(&self) -> Signals {
        let mut pins = self.signals;
        if self.state == MachineState::Door {
            pins.insert(Signals::DOOR);
        }
        if self.alarm == Some(10) {
            pins.insert(Signals::ESTOP);
        }
        if self.probe_result.is_some_and(|probe| probe.success) {
            pins.insert(Signals::PROBE);
        }
        pins
    }

    fn accessories(&self) -> String {
        let mut text = String::new();
        if self.effective_spindle_on() {
            text.push(if self.spindle_clockwise { 'S' } else { 'C' });
        }
        if self.flood {
            text.push('F');
        }
        if self.mist {
            text.push('M');
        }
        if self.tool_change.is_some() {
            text.push('T');
        }
        text
    }
}
