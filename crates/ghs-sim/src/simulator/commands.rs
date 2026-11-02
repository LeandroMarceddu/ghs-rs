//! `$` system command handling.
//!
//! Commands are capability-gated: the enumeration commands are only available
//! when the profile advertises `ENUMS`/`SED`, and MPG-specific behaviour is
//! derived from the `MPG` token.

use ghs_core::{Axes, Axis, MachineState, Position};

use super::Simulator;
use crate::gcode;
use crate::motion::MotionKind;
use crate::settings::SettingError;
use crate::util::emit;

/// Longest startup line the simulator stores.
const MAX_STARTUP_LINE: usize = 256;

impl Simulator {
    /// Dispatch one line with the leading `$` removed.
    pub(super) fn system_command(&mut self, command: &str, out: &mut Vec<u8>) {
        let trimmed = command.trim();
        let upper = trimmed.to_ascii_uppercase();
        match upper.as_str() {
            "I" => {
                self.emit_info(false, out);
                return;
            }
            "I+" => {
                self.emit_info(true, out);
                return;
            }
            "G" => {
                self.emit_parser_state(out);
                return;
            }
            "#" => {
                self.emit_offsets(out);
                return;
            }
            "$" => {
                self.emit_all_settings(out);
                return;
            }
            "+" => {
                self.emit_all_settings(out);
                return;
            }
            "N" => {
                self.emit_startup_lines(out);
                return;
            }
            "H" => {
                self.home(None, out);
                return;
            }
            "HSS" => {
                self.emit_homing_switch_state(out);
                return;
            }
            "X" => {
                self.unlock(out);
                return;
            }
            "SLP" => {
                self.sleep(out);
                return;
            }
            "C" => {
                self.toggle_check_mode(out);
                return;
            }
            "B" => {
                self.block_delete = !self.block_delete;
                emit(out, "ok");
                return;
            }
            "S" => {
                self.single_block = !self.single_block;
                emit(out, "ok");
                return;
            }
            "O" => {
                self.optional_stop = !self.optional_stop;
                emit(out, "ok");
                return;
            }
            "TLR" => {
                self.set_tool_length_reference(out);
                return;
            }
            "TPW" => {
                self.probe_tool_plate(out);
                return;
            }
            "HELP" => {
                self.emit_help("", out);
                return;
            }
            "EA" | "EAG" => {
                if self.enums_available() {
                    self.emit_alarm_codes(out);
                } else {
                    self.fail(3, out);
                }
                return;
            }
            "EE" | "EEG" => {
                if self.enums_available() {
                    self.emit_error_codes(out);
                } else {
                    self.fail(3, out);
                }
                return;
            }
            "ES" | "ESG" => {
                if self.enums_available() {
                    self.emit_setting_metadata(out);
                } else {
                    self.fail(3, out);
                }
                return;
            }
            "EG" | "ESH" => {
                if self.enums_available() {
                    self.emit_setting_groups(out);
                } else {
                    self.fail(3, out);
                }
                return;
            }
            "E" => {
                if self.enums_available() {
                    self.emit_alarm_codes(out);
                    self.emit_error_codes(out);
                    self.emit_setting_metadata(out);
                    self.emit_setting_groups(out);
                } else {
                    self.fail(3, out);
                }
                return;
            }
            "PINS" => {
                self.emit_pins(out);
                return;
            }
            "PINSTATE" => {
                self.emit_pin_state(out);
                return;
            }
            "LIM" => {
                self.emit_limits(out);
                return;
            }
            "LEV" => {
                self.emit_events(out);
                return;
            }
            "PORTS" => {
                self.emit_ports(out);
                return;
            }
            "SPINDLES" => {
                self.emit_spindles(false, out);
                return;
            }
            "SPINDLESH" => {
                self.emit_spindles(true, out);
                return;
            }
            "REBOOT" => {
                self.soft_reset(out);
                return;
            }
            "RTC" => {
                self.emit_rtc(out);
                return;
            }
            "SR" => {
                emit(out, "[SR:0]");
                emit(out, "ok");
                return;
            }
            "SDS" => {
                emit(out, "[SDS:ok]");
                emit(out, "ok");
                return;
            }
            "DWNGRD" => {
                emit(out, "ok");
                return;
            }
            _ => {}
        }

        if let Some(topic) = strip_prefix_ci(trimmed, "HELP ") {
            self.emit_help(topic, out);
            return;
        }
        if let Some(value) = strip_prefix_ci(trimmed, "I=") {
            let _ = value;
            emit(out, "ok");
            return;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "$=") {
            self.emit_setting_details(rest, out);
            return;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "SED=") {
            self.emit_setting_description(rest, out);
            return;
        }
        if upper == "SED" {
            if self.profile.info.has_sed() {
                self.emit_setting_descriptions(out);
            } else {
                self.fail(3, out);
            }
            return;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "J=") {
            self.jog(rest, out);
            return;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "N") {
            self.startup_line_command(rest, out);
            return;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "RST=") {
            self.reset_command(rest, out);
            return;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "H") {
            self.home(Some(rest), out);
            return;
        }
        if let Ok(number) = trimmed.parse::<u16>() {
            self.read_setting(number, out);
            return;
        }
        if let Some((number, value)) = trimmed.split_once('=')
            && let Ok(number) = number.trim().parse::<u16>()
        {
            self.write_setting(number, value, out);
            return;
        }
        self.fail(3, out);
    }

    fn enums_available(&self) -> bool {
        self.profile.info.has_new_option("ENUMS")
    }

    fn emit_all_settings(&mut self, out: &mut Vec<u8>) {
        for line in self.settings.all_lines() {
            emit(out, &line);
        }
        emit(out, "ok");
    }

    fn emit_setting_details(&mut self, text: &str, out: &mut Vec<u8>) {
        let Ok(number) = text.trim().parse::<u16>() else {
            self.fail(3, out);
            return;
        };
        match self.settings.detail_line(number) {
            Some(line) => {
                emit(out, &line);
                emit(out, "ok");
            }
            None => self.fail(3, out),
        }
    }

    fn emit_setting_description(&mut self, text: &str, out: &mut Vec<u8>) {
        if !self.profile.info.has_sed() {
            self.fail(3, out);
            return;
        }
        let Ok(number) = text.trim().parse::<u16>() else {
            self.fail(3, out);
            return;
        };
        match self.settings.description_line(number) {
            Some(line) => {
                emit(out, &line);
                emit(out, "ok");
            }
            None => self.fail(3, out),
        }
    }

    fn read_setting(&mut self, number: u16, out: &mut Vec<u8>) {
        match self.settings.line(number) {
            Some(line) => {
                emit(out, &line);
                emit(out, "ok");
            }
            None => self.fail(3, out),
        }
    }

    fn write_setting(&mut self, number: u16, value: &str, out: &mut Vec<u8>) {
        if self.state != MachineState::Idle {
            self.fail(8, out);
            return;
        }
        match self.settings.set_text(number, value) {
            Ok(stored) => {
                if number == 481 && self.auto_report {
                    self.auto_report_interval = if stored > 0.0 {
                        std::time::Duration::from_millis(
                            u64::try_from(stored as i64).unwrap_or(500),
                        )
                    } else {
                        super::DEFAULT_AUTO_REPORT
                    };
                }
                emit(out, "ok");
            }
            Err(SettingError::Unknown) => self.fail(3, out),
            Err(SettingError::NotANumber) => self.fail(2, out),
            Err(SettingError::NotInteger) => self.fail(23, out),
            Err(SettingError::OutOfRange) => self.fail(39, out),
        }
    }

    fn emit_startup_lines(&mut self, out: &mut Vec<u8>) {
        for (index, line) in self.startup_lines.iter().enumerate() {
            emit(out, &format!("$N{index}={line}"));
        }
        emit(out, "ok");
    }

    fn startup_line_command(&mut self, rest: &str, out: &mut Vec<u8>) {
        if rest.is_empty() {
            self.emit_startup_lines(out);
            return;
        }
        if let Some((index, content)) = rest.split_once('=') {
            let Ok(index) = index.trim().parse::<usize>() else {
                self.fail(3, out);
                return;
            };
            if index > 1 {
                self.fail(3, out);
                return;
            }
            if content.len() > MAX_STARTUP_LINE || !content.is_ascii() {
                self.fail(14, out);
                return;
            }
            if self.state != MachineState::Idle {
                self.fail(8, out);
                return;
            }
            self.startup_lines[index] = content.to_string();
            emit(out, "ok");
            return;
        }
        let Ok(index) = rest.trim().parse::<usize>() else {
            self.fail(3, out);
            return;
        };
        if index > 1 {
            self.fail(3, out);
            return;
        }
        emit(out, &format!("$N{index}={}", self.startup_lines[index]));
        emit(out, "ok");
    }

    fn reset_command(&mut self, arg: &str, out: &mut Vec<u8>) {
        if self.state != MachineState::Idle {
            self.fail(8, out);
            return;
        }
        match arg.trim() {
            "*" => {
                self.settings.reset();
                self.reset_offsets();
                self.reset_tools();
            }
            "$" => {
                self.settings.reset();
                self.startup_lines = std::array::from_fn(|_| String::new());
            }
            "#" => self.reset_offsets(),
            "&" => self.reset_tools(),
            _ => {
                self.fail(3, out);
                return;
            }
        }
        emit(out, "ok");
    }

    fn reset_offsets(&mut self) {
        self.wcs = ghs_core::Wcs::G54;
        self.wcs_offsets = [Position::new(self.axes); 9];
        self.g92 = Position::new(self.axes);
        self.g28 = Position::new(self.axes);
        self.g30 = Position::new(self.axes);
        self.tlo = Position::new(self.axes);
        self.tlr = None;
        self.probe_result = None;
        self.clear_probe_contact();
    }

    fn reset_tools(&mut self) {
        self.tool = 0;
        self.tool_table = (0..self.tool_table.len())
            .map(|number| super::SimTool {
                number: u32::try_from(number).unwrap_or(0),
                offset: Position::new(self.axes),
                radius_mm: 0.0,
                name: String::new(),
                pocket: u32::try_from(number).unwrap_or(0),
            })
            .collect();
    }

    fn unlock(&mut self, out: &mut Vec<u8>) {
        // Unlock is a recovery action, not a reset: the last PRB and the live
        // probe input are left as they were. The operator retracts after
        // unlocking, which opens the input without losing the result.
        let was_locked =
            self.alarm.is_some() || matches!(self.state, MachineState::Alarm | MachineState::Door);
        self.alarm = None;
        self.error = None;
        if matches!(self.state, MachineState::Alarm | MachineState::Door) {
            self.state = MachineState::Idle;
        }
        if was_locked {
            emit(out, "[MSG:Caution: Unlocked]");
        }
        emit(out, "ok");
    }

    fn sleep(&mut self, out: &mut Vec<u8>) {
        if self.state != MachineState::Idle {
            self.emit_error_only(8, out);
            return;
        }
        self.spindle_on = false;
        self.flood = false;
        self.mist = false;
        self.state = MachineState::Sleep;
        emit(out, "ok");
    }

    fn toggle_check_mode(&mut self, out: &mut Vec<u8>) {
        match self.state {
            MachineState::Idle => {
                self.state = MachineState::Check;
                self.check_mode = true;
            }
            MachineState::Check => {
                self.state = MachineState::Idle;
                self.check_mode = false;
            }
            _ => {
                self.emit_error_only(8, out);
                return;
            }
        }
        emit(out, "ok");
    }

    fn set_tool_length_reference(&mut self, out: &mut Vec<u8>) {
        let value = self.tlo.get(Axis::Z);
        self.tlr = Some(value);
        emit(out, "ok");
    }

    fn probe_tool_plate(&mut self, out: &mut Vec<u8>) {
        self.tlr = Some(self.tlo.get(Axis::Z));
        self.probe_result = Some(ghs_core::ProbeResult::new(true, self.mpos));
        self.probe_triggered = true;
        emit(out, "ok");
    }

    fn home(&mut self, letters: Option<&str>, out: &mut Vec<u8>) {
        if !self.profile.info.homing() || self.settings.value(22).unwrap_or(0.0) == 0.0 {
            self.fail(5, out);
            return;
        }
        if matches!(
            self.state,
            MachineState::Run | MachineState::Jog | MachineState::Home
        ) {
            self.emit_error_only(8, out);
            return;
        }
        if self.tool_change.is_some() {
            self.emit_error_only(40, out);
            return;
        }

        let mut targets = self.axes;
        if let Some(letters) = letters {
            let letters = letters.trim();
            if !letters.is_empty() {
                let mut mask = Axes::empty();
                for letter in letters.chars() {
                    let Some(axis) = Axis::from_letter(letter) else {
                        self.emit_error_only(20, out);
                        return;
                    };
                    if !self.axes.contains(axis) {
                        self.emit_error_only(20, out);
                        return;
                    }
                    mask.insert(axis);
                }
                targets = mask;
            }
        }

        self.settle_motion(out);
        self.clear_probe_contact();
        for axis in targets.iter() {
            self.mpos.set(axis, self.home_position.get(axis));
        }
        self.homed = self.homed.union(targets);
        self.alarm = None;
        self.error = None;
        self.spindle_on = false;
        self.flood = false;
        self.mist = false;
        self.state = MachineState::Idle;
        emit(out, "ok");
    }

    fn jog(&mut self, text: &str, out: &mut Vec<u8>) {
        match self.state {
            MachineState::Alarm => {
                self.emit_error_only(9, out);
                return;
            }
            MachineState::Door => {
                self.emit_error_only(13, out);
                return;
            }
            MachineState::Sleep => return,
            MachineState::Run | MachineState::Home | MachineState::Check => {
                self.emit_error_only(8, out);
                return;
            }
            _ => {}
        }

        let Ok(words) = gcode::parse_line(text) else {
            self.emit_error_only(1, out);
            return;
        };
        let mut incremental = true;
        let mut scale = self.units.mm_per_unit();
        let mut axis_words: Vec<(Axis, f64)> = Vec::new();
        let mut feed_value = None;
        for word in &words {
            match word.letter {
                'G' => {
                    if gcode::is_code(word.value, 91.0) {
                        incremental = true;
                    } else if gcode::is_code(word.value, 90.0) {
                        incremental = false;
                    } else if gcode::is_code(word.value, 20.0) {
                        // Jog units are local to the $J line; they do not
                        // change the controller's modal unit state.
                        scale = 25.4;
                    } else if gcode::is_code(word.value, 21.0) {
                        scale = 1.0;
                    } else {
                        self.emit_error_only(20, out);
                        return;
                    }
                }
                'F' => {
                    if !word.has_value {
                        self.emit_error_only(1, out);
                        return;
                    }
                    if word.value < 0.0 {
                        self.emit_error_only(4, out);
                        return;
                    }
                    feed_value = Some(word.value);
                }
                _ if word.is_axis() => {
                    if !word.has_value {
                        self.emit_error_only(1, out);
                        return;
                    }
                    let Some(axis) = Axis::from_letter(word.letter) else {
                        self.emit_error_only(20, out);
                        return;
                    };
                    if !self.axes.contains(axis) {
                        self.emit_error_only(20, out);
                        return;
                    }
                    axis_words.push((axis, word.value));
                }
                _ => {
                    self.emit_error_only(20, out);
                    return;
                }
            }
        }

        let target = if incremental {
            let mut position = self.mpos;
            for (axis, raw) in &axis_words {
                position.set(*axis, position.get(*axis) + raw * scale);
            }
            position
        } else {
            let mut work = self.wpos();
            for (axis, raw) in &axis_words {
                work.set(*axis, raw * scale);
            }
            work + self.wco()
        };
        let feed = feed_value.map_or(self.feed_rate, |value| value * scale);
        if feed <= 0.0 {
            self.emit_error_only(22, out);
            return;
        }

        self.settle_motion(out);
        self.start_motion(target, MotionKind::Jog, Some(feed), out);
        emit(out, "ok");
    }
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        text.get(prefix.len()..)
    } else {
        None
    }
}
