//! The deterministic grblHAL simulator engine.
//!
//! The engine is sans-IO: [`Simulator::feed`] consumes a byte chunk and returns
//! every byte the controller would have written, and [`Simulator::tick`]
//! advances simulated time. No thread, socket, serial port, or wall clock is
//! involved, so a test is fully reproducible.
//!
//! Behaviour is derived from the [`ControllerProfile`] capability record, never
//! from a profile name. `flexihal` is a strict subset of `generic-full`, and the
//! same code serves both.

mod commands;
mod report;

use std::time::Duration;

use ghs_core::{
    Axes, ControllerInfo, ControllerProfile, MachineState, Overrides, Position, ProbeResult,
    Signals, Units, Wcs,
};
use ghs_proto::{DistanceMode, Plane, RealtimeCommand};

use crate::gcode::{self, is_code};
use crate::motion::{self, Motion, MotionKind};
use crate::profile::{self, SimProfile};
use crate::settings::SettingsTable;
use crate::util::emit;

/// Longest input line the simulator accepts, in bytes.
const MAX_LINE_LEN: usize = 1024;

/// Maximum number of automatic status reports emitted by one tick.
const MAX_STATUS_REPORTS_PER_TICK: usize = 16;

/// Auto-report interval used when the controller enables it without `$481`.
const DEFAULT_AUTO_REPORT: Duration = Duration::from_millis(500);

/// Highest number of auxiliary IO lines the simulator models per direction.
const MAX_SIM_AUX_IO: u16 = 64;

/// One simulated controller-side tool-table entry.
#[derive(Debug, Clone)]
struct SimTool {
    number: u32,
    offset: Position,
    radius_mm: f64,
    name: String,
    pocket: u32,
}

/// A pending manual tool change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToolChange {
    requested: u32,
    acknowledged: bool,
}

/// Plan collected from the G and M words of one line before it is applied.
#[derive(Debug, Default)]
struct LinePlan {
    motion: Option<f64>,
    home: Option<f64>,
    machine_coords: bool,
    g92: bool,
    g10: bool,
    set_tlo: bool,
    clear_tlo: bool,
    tool_change: bool,
    pause: bool,
    end_program: bool,
    spindle: Option<(bool, bool)>,
    coolant: Option<(bool, bool)>,
}

/// Immutable view of the simulated controller state for assertions.
///
/// Positions are in millimetres; `feed_rate_mm_min` is the live feed rate and
/// `programmed_feed_mm_min` the modal `F` value.
#[derive(Debug, Clone, PartialEq)]
pub struct SimSnapshot {
    /// Profile name the simulator was built from.
    pub profile: String,
    /// Axes modelled by the controller.
    pub axes: Axes,
    /// Current machine state.
    pub state: MachineState,
    /// Active alarm code, if any.
    pub alarm: Option<u16>,
    /// Latched sticky error code, if any.
    pub error: Option<u16>,
    /// Machine position (`MPos`) in millimetres.
    pub mpos: Position,
    /// Work position (`WPos`) in millimetres.
    pub wpos: Position,
    /// Active work coordinate offset (`WCO`) in millimetres.
    pub wco: Position,
    /// Active work coordinate system.
    pub wcs: Wcs,
    /// Active length unit mode.
    pub units: Units,
    /// Active distance mode.
    pub distance_mode: DistanceMode,
    /// Feed, rapid and spindle overrides.
    pub overrides: Overrides,
    /// Live feed rate in millimetres per minute.
    pub feed_rate_mm_min: f64,
    /// Modal `F` value in millimetres per minute.
    pub programmed_feed_mm_min: f64,
    /// Effective spindle speed in RPM.
    pub spindle_rpm: f64,
    /// Whether the spindle is running.
    pub spindle_on: bool,
    /// Whether the spindle runs clockwise.
    pub spindle_clockwise: bool,
    /// Flood coolant state.
    pub flood: bool,
    /// Mist coolant state.
    pub mist: bool,
    /// Accessory fan state.
    pub fan: bool,
    /// Current tool number.
    pub tool: u32,
    /// Whether a manual tool change is pending.
    pub tool_change_pending: bool,
    /// Whether the pending tool change was acknowledged with `0xA3`.
    pub tool_change_acknowledged: bool,
    /// Whether a pendant has taken over the input stream.
    pub mpg_active: bool,
    /// Whether the virtual probe-connected signal is asserted.
    pub probe_connected: bool,
    /// Last probe result.
    pub probe_result: Option<ProbeResult>,
    /// Axes known to be homed.
    pub homed: Axes,
    /// Whether every modelled axis is homed.
    pub homing_complete: bool,
    /// Optional-stop toggle state.
    pub optional_stop: bool,
    /// Single-block toggle state.
    pub single_block: bool,
    /// Block-delete toggle state.
    pub block_delete: bool,
    /// Check-mode state.
    pub check_mode: bool,
    /// Auto-reporting state.
    pub auto_report: bool,
    /// Asserted input signals.
    pub signals: Signals,
    /// Free planner blocks reported in `Bf`.
    pub planner_free: u32,
    /// Free serial receive-buffer bytes reported in `Bf`.
    pub rx_free: u32,
    /// Controller-reported line number.
    pub line_number: u32,
    /// Stored startup lines.
    pub startup_lines: [String; 2],
    /// Simulated time since construction.
    pub clock: Duration,
}

/// A deterministic, capability-configured grblHAL simulator.
///
/// ```
/// use ghs_sim::{SimProfile, Simulator};
///
/// let mut sim = Simulator::new(SimProfile::FlexiHal);
/// assert!(sim.banner().starts_with(b"GrblHAL 1.1f"));
/// let status = sim.feed(b"?");
/// assert!(status.starts_with(b"<Idle"));
/// ```
#[derive(Debug)]
pub struct Simulator {
    profile: ControllerProfile,
    axes: Axes,
    identity: Vec<String>,
    planner_blocks: u32,
    rx_buffer: u32,
    spindle_count: u8,
    settings: SettingsTable,
    line: Vec<u8>,
    line_overflow: bool,
    last_terminator: Option<u8>,
    escaped: bool,
    state: MachineState,
    alarm: Option<u16>,
    error: Option<u16>,
    mpos: Position,
    wcs: Wcs,
    wcs_offsets: [Position; 9],
    g92: Position,
    g28: Position,
    g30: Position,
    home_position: Position,
    homed: Axes,
    tlo: Position,
    tlr: Option<f64>,
    tool: u32,
    tool_change: Option<ToolChange>,
    tool_table: Vec<SimTool>,
    motion: Option<Motion>,
    hold_after_motion: bool,
    units: Units,
    distance: DistanceMode,
    plane: Plane,
    motion_mode: String,
    feed_rate: f64,
    spindle_rpm: f64,
    spindle_on: bool,
    spindle_clockwise: bool,
    spindle_stop: bool,
    spindle_rpms: Vec<f64>,
    flood: bool,
    mist: bool,
    fan: bool,
    overrides: Overrides,
    optional_stop: bool,
    single_block: bool,
    block_delete: bool,
    check_mode: bool,
    mpg: bool,
    auto_report: bool,
    auto_report_interval: Duration,
    auto_report_accum: Duration,
    signals: Signals,
    aux_in: Vec<bool>,
    aux_out: Vec<bool>,
    probe_connected: bool,
    probe_result: Option<ProbeResult>,
    line_number: u32,
    startup_lines: [String; 2],
    clock: Duration,
    banner_enabled: bool,
    lev_count: u32,
}

impl Simulator {
    /// Build a simulator for a named profile, with the welcome banner enabled.
    #[must_use]
    pub fn new(profile: SimProfile) -> Self {
        Self::from_profile(profile.controller_profile())
    }

    /// Build a simulator for a named profile without a startup banner.
    ///
    /// Use this to test the defensive handshake: a controller may answer a
    /// status request before (or instead of) printing a banner, and a pendant
    /// may already be in control.
    #[must_use]
    pub fn new_without_banner(profile: SimProfile) -> Self {
        let mut simulator = Self::new(profile);
        simulator.set_banner_enabled(false);
        simulator
    }

    /// Build a simulator from any capability record.
    ///
    /// Every behavioural feature is derived from the record: axes, buffer
    /// sizes, tool-table entries, spindle count, and the standard capability
    /// helpers (`SD`, `ATC`, `MPG`, `PC`, `TC`, `HOME`, `ENUMS`, `SED`, `RT+`).
    #[must_use]
    pub fn from_profile(profile: ControllerProfile) -> Self {
        let info = &profile.info;
        let axes = profile::axes_of(info);
        let planner_blocks = info.planner_blocks.unwrap_or(64).clamp(1, 4096);
        let rx_buffer = info.rx_buffer_size.unwrap_or(1024).clamp(64, 65_536);
        let tool_entries = info.tool_table_entries.unwrap_or(0).min(64);
        let spindle_count = profile::spindle_count(info);
        let identity = profile::identity_lines(info, spindle_count);
        let settings = SettingsTable::for_axes(axes);
        let aux = info.aux_io.unwrap_or_default();
        let tool_table = (0..tool_entries)
            .map(|number| SimTool {
                number: u32::try_from(number).unwrap_or(0),
                offset: Position::new(axes),
                radius_mm: 0.0,
                name: String::new(),
                pocket: u32::try_from(number).unwrap_or(0),
            })
            .collect();

        Simulator {
            profile,
            axes,
            identity,
            planner_blocks: u32::try_from(planner_blocks).unwrap_or(64),
            rx_buffer: u32::try_from(rx_buffer).unwrap_or(1024),
            spindle_count,
            settings,
            line: Vec::new(),
            line_overflow: false,
            last_terminator: None,
            escaped: false,
            state: MachineState::Idle,
            alarm: None,
            error: None,
            mpos: Position::new(axes),
            wcs: Wcs::G54,
            wcs_offsets: [Position::new(axes); 9],
            g92: Position::new(axes),
            g28: Position::new(axes),
            g30: Position::new(axes),
            home_position: Position::new(axes),
            homed: Axes::empty(),
            tlo: Position::new(axes),
            tlr: None,
            tool: 0,
            tool_change: None,
            tool_table,
            motion: None,
            hold_after_motion: false,
            units: Units::Mm,
            distance: DistanceMode::Absolute,
            plane: Plane::XY,
            motion_mode: "G0".to_string(),
            feed_rate: 0.0,
            spindle_rpm: 0.0,
            spindle_on: false,
            spindle_clockwise: true,
            spindle_stop: false,
            spindle_rpms: vec![0.0; usize::from(spindle_count)],
            flood: false,
            mist: false,
            fan: false,
            overrides: Overrides::DEFAULT,
            optional_stop: false,
            single_block: false,
            block_delete: false,
            check_mode: false,
            mpg: false,
            auto_report: false,
            auto_report_interval: DEFAULT_AUTO_REPORT,
            auto_report_accum: Duration::ZERO,
            signals: Signals::NONE,
            aux_in: vec![false; usize::from(aux.digital_in.min(MAX_SIM_AUX_IO))],
            aux_out: vec![false; usize::from(aux.digital_out.min(MAX_SIM_AUX_IO))],
            probe_connected: false,
            probe_result: None,
            line_number: 0,
            startup_lines: std::array::from_fn(|_| String::new()),
            clock: Duration::ZERO,
            banner_enabled: true,
            lev_count: 0,
        }
    }

    /// Enable or disable the welcome banner.
    ///
    /// The banner is printed by [`Simulator::banner`] and again on a soft
    /// reset. Disabling it models a controller that stays silent on connect.
    pub fn set_banner_enabled(&mut self, enabled: bool) {
        self.banner_enabled = enabled;
    }

    /// Returns `true` when a reset prints the welcome banner.
    #[must_use]
    pub fn banner_enabled(&self) -> bool {
        self.banner_enabled
    }

    /// Profile name of the controller.
    #[must_use]
    pub fn profile_name(&self) -> &str {
        &self.profile.name
    }

    /// Capability record the simulator was built from.
    #[must_use]
    pub fn capabilities(&self) -> &ControllerInfo {
        &self.profile.info
    }

    /// Current value of one simulated setting.
    #[must_use]
    pub fn setting(&self, number: u16) -> Option<f64> {
        self.settings.value(number)
    }

    /// Snapshot of the controller state for assertions.
    #[must_use]
    pub fn snapshot(&self) -> SimSnapshot {
        let live_feed = self.motion.as_ref().map_or(0.0, Motion::feed_mm_min);
        let spindle_on = self.spindle_on && !self.spindle_stop;
        SimSnapshot {
            profile: self.profile.name.clone(),
            axes: self.axes,
            state: self.state,
            alarm: self.alarm,
            error: self.error,
            mpos: self.mpos,
            wpos: self.wpos(),
            wco: self.wco(),
            wcs: self.wcs,
            units: self.units,
            distance_mode: self.distance,
            overrides: self.overrides,
            feed_rate_mm_min: live_feed,
            programmed_feed_mm_min: self.feed_rate,
            spindle_rpm: if spindle_on { self.spindle_rpm } else { 0.0 },
            spindle_on,
            spindle_clockwise: self.spindle_clockwise,
            flood: self.flood,
            mist: self.mist,
            fan: self.fan,
            tool: self.tool,
            tool_change_pending: self.tool_change.is_some(),
            tool_change_acknowledged: self.tool_change.is_some_and(|change| change.acknowledged),
            mpg_active: self.mpg,
            probe_connected: self.probe_connected,
            probe_result: self.probe_result,
            homed: self.homed,
            homing_complete: self.axes.difference(self.homed).is_empty(),
            optional_stop: self.optional_stop,
            single_block: self.single_block,
            block_delete: self.block_delete,
            check_mode: self.check_mode,
            auto_report: self.auto_report,
            signals: self.signals,
            planner_free: self.planner_free(),
            rx_free: self.rx_buffer,
            line_number: self.line_number,
            startup_lines: self.startup_lines.clone(),
            clock: self.clock,
        }
    }

    /// Consume an arbitrary byte chunk and return all output it produced.
    ///
    /// Partial lines survive across calls; `CR`, `LF`, `CR LF` and `LF CR` all
    /// terminate one line. Realtime bytes are handled immediately even when
    /// they arrive in the middle of a line.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for &byte in bytes {
            if self.escaped {
                self.escaped = false;
                if byte == 0x14 {
                    self.soft_reset(&mut out);
                    continue;
                }
            }
            if byte == 0x1B {
                self.escaped = true;
                continue;
            }
            if let Some(command) = RealtimeCommand::from_byte(byte) {
                self.realtime(command, &mut out);
                continue;
            }
            match byte {
                b'\r' => {
                    if self.last_terminator == Some(b'\n') {
                        self.last_terminator = None;
                        continue;
                    }
                    self.end_line(&mut out);
                    self.last_terminator = Some(b'\r');
                }
                b'\n' => {
                    if self.last_terminator == Some(b'\r') {
                        self.last_terminator = None;
                        continue;
                    }
                    self.end_line(&mut out);
                    self.last_terminator = Some(b'\n');
                }
                _ => {
                    self.last_terminator = None;
                    if self.line.len() >= MAX_LINE_LEN {
                        self.line_overflow = true;
                    } else if !self.line_overflow {
                        self.line.push(byte);
                    }
                }
            }
        }
        out
    }

    /// Advance simulated time by `elapsed` and return pending output.
    ///
    /// Motion, the auto-report timer and the controller clock advance. Held,
    /// alarmed, sleeping and door-open states freeze motion.
    pub fn tick(&mut self, elapsed: Duration) -> Vec<u8> {
        let mut out = Vec::new();
        self.clock = self.clock.saturating_add(elapsed);

        if !matches!(
            self.state,
            MachineState::Hold | MachineState::Door | MachineState::Alarm | MachineState::Sleep
        ) {
            let mut finished = false;
            if let Some(motion) = self.motion.as_mut() {
                finished = motion.advance(elapsed);
                self.mpos = motion.position();
            }
            if finished {
                let mut probe = None;
                if let Some(motion) = self.motion.take() {
                    self.mpos = motion.target();
                    if motion.kind() == MotionKind::Probe {
                        probe = Some(motion.target());
                    }
                }
                self.finish_hold_or_idle();
                if let Some(position) = probe {
                    self.record_probe(position, &mut out);
                }
            }
        }

        self.advance_auto_report(elapsed, &mut out);
        out
    }

    /// Raise an alarm asynchronously, as a hard limit or E-stop would.
    ///
    /// Returns the `ALARM:<n>` line the controller emits. Motion, spindle and
    /// coolant stop immediately and the controller locks until `$X`, `$H` or a
    /// reset clears the alarm.
    pub fn raise_alarm(&mut self, code: u16) -> Vec<u8> {
        let mut out = Vec::new();
        self.raise_alarm_to(code, &mut out);
        out
    }

    /// Assert or release a simulated input signal, for example a limit switch.
    ///
    /// The signal is reflected in `Pn:` and in `$LIM`. Asserting a limit while
    /// hard limits are enabled makes the next motion alarm.
    pub fn set_signal(&mut self, signal: Signals, asserted: bool) {
        let current = self.signals.bits();
        let next = if asserted {
            current | signal.bits()
        } else {
            current & !signal.bits()
        };
        if next != current {
            self.lev_count = self.lev_count.saturating_add(1);
            self.signals = Signals::from_bits(next);
        }
    }

    /// Simulated machine position.
    #[must_use]
    pub fn position(&self) -> Position {
        self.mpos
    }

    // --- internals -------------------------------------------------------

    fn end_line(&mut self, out: &mut Vec<u8>) {
        let line = std::mem::take(&mut self.line);
        let overflow = std::mem::take(&mut self.line_overflow);
        if overflow {
            tracing::debug!(limit = MAX_LINE_LEN, "discarding an overlong input line");
            self.emit_error_only(11, out);
            return;
        }
        let Ok(text) = String::from_utf8(line) else {
            tracing::debug!("discarding an input line that is not valid UTF-8");
            self.emit_error_only(1, out);
            return;
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            self.error = None;
            emit(out, "ok");
            return;
        }
        if let Some(command) = trimmed.strip_prefix('$') {
            self.error = None;
            self.system_command(command.trim(), out);
            return;
        }
        self.process_gcode_line(trimmed, out);
    }

    fn process_gcode_line(&mut self, text: &str, out: &mut Vec<u8>) {
        let mut text = text;
        if let Some(rest) = text.strip_prefix('/') {
            if self.block_delete {
                emit(out, "ok");
                return;
            }
            text = rest;
        }
        if let Some(code) = self.error {
            self.emit_error_only(code, out);
            return;
        }
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
            _ => {}
        }
        if self.tool_change.is_some() {
            self.emit_error_only(40, out);
            return;
        }
        self.execute_gcode(text, out);
    }

    fn execute_gcode(&mut self, text: &str, out: &mut Vec<u8>) {
        let words = match gcode::parse_line(text) {
            Ok(words) => words,
            Err(()) => {
                self.fail(1, out);
                return;
            }
        };
        self.settle_motion(out);

        let mut plan = LinePlan::default();
        for word in &words {
            match word.letter {
                'G' => {
                    if !word.has_value {
                        self.fail(1, out);
                        return;
                    }
                    if !self.plan_g_word(word.value, &mut plan) {
                        self.fail(20, out);
                        return;
                    }
                }
                'M' => {
                    if !word.has_value {
                        self.fail(1, out);
                        return;
                    }
                    if !self.plan_m_word(word.value, &mut plan) {
                        self.fail(20, out);
                        return;
                    }
                }
                _ => {}
            }
        }

        let mut axis_words: Vec<(ghs_core::Axis, f64)> = Vec::new();
        let mut params: Vec<(char, f64)> = Vec::new();
        let mut feed_value = None;
        let mut speed_value = None;
        let mut tool_value = None;

        for word in &words {
            match word.letter {
                'G' | 'M' => {}
                'N' => {
                    if word.has_value && word.value >= 0.0 && word.value.fract() == 0.0 {
                        self.line_number = u32::try_from(word.value as i64).unwrap_or(0);
                    }
                }
                'F' => {
                    if !word.has_value {
                        self.fail(1, out);
                        return;
                    }
                    feed_value = Some(word.value);
                }
                'S' => {
                    if !word.has_value {
                        self.fail(1, out);
                        return;
                    }
                    speed_value = Some(word.value);
                }
                'T' => {
                    if !word.has_value || word.value < 0.0 || word.value.fract() != 0.0 {
                        self.fail(23, out);
                        return;
                    }
                    if word.value > 9999.0 {
                        self.fail(38, out);
                        return;
                    }
                    tool_value = Some(u32::try_from(word.value as i64).unwrap_or(0));
                }
                'I' | 'J' | 'K' | 'R' | 'P' | 'Q' | 'L' => {
                    if !word.has_value {
                        self.fail(1, out);
                        return;
                    }
                    params.push((word.letter, word.value));
                }
                _ if word.is_axis() => {
                    if !word.has_value {
                        self.fail(1, out);
                        return;
                    }
                    let Some(axis) = ghs_core::Axis::from_letter(word.letter) else {
                        self.fail(20, out);
                        return;
                    };
                    if !self.axes.contains(axis) {
                        self.fail(20, out);
                        return;
                    }
                    axis_words.push((axis, word.value));
                }
                _ => {
                    self.fail(20, out);
                    return;
                }
            }
        }

        let scale = self.units.mm_per_unit();
        if let Some(feed) = feed_value {
            if feed < 0.0 {
                self.fail(4, out);
                return;
            }
            self.feed_rate = feed * scale;
        }
        if let Some(speed) = speed_value {
            if speed < 0.0 {
                self.fail(4, out);
                return;
            }
            self.spindle_rpm = speed;
            if let Some(first) = self.spindle_rpms.first_mut() {
                *first = speed;
            }
        }
        if let Some(tool) = tool_value {
            self.tool = tool;
        }
        if let Some((on, clockwise)) = plan.spindle {
            self.spindle_on = on;
            self.spindle_clockwise = clockwise;
        }
        if let Some((mist, flood)) = plan.coolant {
            self.mist = mist;
            self.flood = flood;
        }
        if plan.set_tlo {
            for (axis, raw) in &axis_words {
                self.tlo.set(*axis, raw * scale);
            }
        }
        if plan.clear_tlo {
            self.tlo = Position::new(self.axes);
        }

        if plan.end_program {
            self.spindle_on = false;
            self.flood = false;
            self.mist = false;
            self.finish_hold_or_idle();
            emit(out, "ok");
            return;
        }

        if plan.tool_change {
            self.begin_tool_change(out);
            return;
        }

        if plan.pause {
            self.state = MachineState::Hold;
            self.hold_after_motion = false;
            emit(out, "ok");
            return;
        }

        if plan.g92 {
            let offset = self.wcs_offsets[self.wcs.index()];
            for (axis, raw) in &axis_words {
                let value = raw * scale;
                self.g92
                    .set(*axis, self.mpos.get(*axis) - offset.get(*axis) - value);
            }
        }

        if plan.g10 {
            if self.apply_g10(&params, &axis_words, scale, out) {
                emit(out, "ok");
            }
            return;
        }

        if let Some(code) = plan.home {
            let is_g28 = is_code(code, 28.0);
            let mut target = if is_g28 { self.g28 } else { self.g30 };
            for (axis, raw) in &axis_words {
                target.set(*axis, raw * scale);
            }
            if !axis_words.is_empty() {
                if is_g28 {
                    self.g28 = target;
                } else {
                    self.g30 = target;
                }
            }
            if self.check_mode {
                emit(out, "ok");
                return;
            }
            self.start_motion(target, MotionKind::Rapid, None, out);
            emit(out, "ok");
            return;
        }

        if let Some(code) = plan.motion {
            let mut target = self.mpos;
            if plan.machine_coords {
                for (axis, raw) in &axis_words {
                    target.set(*axis, raw * scale);
                }
            } else if self.distance == DistanceMode::Incremental {
                for (axis, raw) in &axis_words {
                    target.set(*axis, target.get(*axis) + raw * scale);
                }
            } else {
                let mut work = self.wpos();
                for (axis, raw) in &axis_words {
                    work.set(*axis, raw * scale);
                }
                target = work + self.wco();
            }

            let is_arc = is_code(code, 2.0) || is_code(code, 3.0);
            if is_arc
                && !params
                    .iter()
                    .any(|(letter, _)| matches!(letter, 'I' | 'J' | 'K' | 'R'))
            {
                self.fail(35, out);
                return;
            }
            let kind = if is_code(code, 0.0) {
                MotionKind::Rapid
            } else if is_code(code, 38.2) || is_code(code, 38.3) {
                MotionKind::Probe
            } else {
                MotionKind::Feed
            };
            if self.check_mode {
                emit(out, "ok");
                return;
            }
            if self.settings.value(20).unwrap_or(0.0) > 0.0 && self.outside_travel(target) {
                self.raise_alarm_to(2, out);
                return;
            }
            if self.settings.value(21).unwrap_or(0.0) > 0.0
                && !self.signals.axis_limits().is_empty()
            {
                self.raise_alarm_to(1, out);
                return;
            }
            if kind != MotionKind::Rapid && self.feed_rate <= 0.0 {
                self.fail(22, out);
                return;
            }
            self.start_motion(target, kind, None, out);
            emit(out, "ok");
            return;
        }

        emit(out, "ok");
    }

    /// Apply one G word to the line plan. Returns `false` for an unknown code.
    fn plan_g_word(&mut self, value: f64, plan: &mut LinePlan) -> bool {
        if is_code(value, 0.0) || is_code(value, 1.0) || is_code(value, 2.0) || is_code(value, 3.0)
        {
            plan.motion = Some(value);
            self.motion_mode = format!("G{}", crate::util::format_gcode_number(value));
            return true;
        }
        if is_code(value, 38.2) || is_code(value, 38.3) {
            plan.motion = Some(value);
            return true;
        }
        if is_code(value, 4.0) {
            return true;
        }
        if is_code(value, 17.0) {
            self.plane = Plane::XY;
            return true;
        }
        if is_code(value, 18.0) {
            self.plane = Plane::ZX;
            return true;
        }
        if is_code(value, 19.0) {
            self.plane = Plane::YZ;
            return true;
        }
        if is_code(value, 20.0) {
            self.units = Units::Inch;
            return true;
        }
        if is_code(value, 21.0) {
            self.units = Units::Mm;
            return true;
        }
        if is_code(value, 28.0) {
            plan.home = Some(28.0);
            return true;
        }
        if is_code(value, 30.0) {
            plan.home = Some(30.0);
            return true;
        }
        if is_code(value, 53.0) {
            plan.machine_coords = true;
            return true;
        }
        if let Some(wcs) = wcs_from_code(value) {
            self.wcs = wcs;
            return true;
        }
        if is_code(value, 90.0) {
            self.distance = DistanceMode::Absolute;
            return true;
        }
        if is_code(value, 91.0) {
            self.distance = DistanceMode::Incremental;
            return true;
        }
        if is_code(value, 92.0) {
            plan.g92 = true;
            return true;
        }
        if is_code(value, 10.0) {
            plan.g10 = true;
            return true;
        }
        if is_code(value, 43.0) || is_code(value, 43.1) {
            plan.set_tlo = true;
            return true;
        }
        if is_code(value, 49.0) {
            plan.clear_tlo = true;
            return true;
        }
        if is_code(value, 40.0)
            || is_code(value, 50.0)
            || is_code(value, 51.0)
            || is_code(value, 80.0)
            || is_code(value, 94.0)
            || is_code(value, 98.0)
            || is_code(value, 99.0)
        {
            return true;
        }
        false
    }

    /// Apply one M word to the line plan. Returns `false` for an unknown code.
    fn plan_m_word(&mut self, value: f64, plan: &mut LinePlan) -> bool {
        if is_code(value, 0.0) {
            plan.pause = true;
            return true;
        }
        if is_code(value, 1.0) {
            plan.pause = self.optional_stop;
            return true;
        }
        if is_code(value, 2.0) || is_code(value, 30.0) {
            plan.end_program = true;
            return true;
        }
        if is_code(value, 3.0) {
            plan.spindle = Some((true, true));
            return true;
        }
        if is_code(value, 4.0) {
            plan.spindle = Some((true, false));
            return true;
        }
        if is_code(value, 5.0) {
            plan.spindle = Some((false, true));
            return true;
        }
        if is_code(value, 6.0) {
            plan.tool_change = true;
            return true;
        }
        if is_code(value, 7.0) {
            plan.coolant = Some((true, false));
            return true;
        }
        if is_code(value, 8.0) {
            plan.coolant = Some((false, true));
            return true;
        }
        if is_code(value, 9.0) {
            plan.coolant = Some((false, false));
            return true;
        }
        if is_code(value, 48.0) || is_code(value, 49.0) {
            return true;
        }
        false
    }

    fn apply_g10(
        &mut self,
        params: &[(char, f64)],
        axis_words: &[(ghs_core::Axis, f64)],
        scale: f64,
        out: &mut Vec<u8>,
    ) -> bool {
        let Some(l_word) = params
            .iter()
            .find(|(letter, _)| *letter == 'L')
            .map(|(_, v)| *v)
        else {
            self.fail(20, out);
            return false;
        };
        let Some(p_word) = params
            .iter()
            .find(|(letter, _)| *letter == 'P')
            .map(|(_, v)| *v)
        else {
            self.fail(20, out);
            return false;
        };
        let offset_mode = if is_code(l_word, 2.0) {
            true
        } else if is_code(l_word, 20.0) {
            false
        } else {
            self.fail(20, out);
            return false;
        };
        if p_word < 1.0 || p_word > 9.0 || p_word.fract() != 0.0 {
            self.fail(20, out);
            return false;
        }
        let index = (p_word as usize).saturating_sub(1);
        let mut system = self.wcs_offsets[index];
        for (axis, raw) in axis_words {
            let value = raw * scale;
            let offset = if offset_mode {
                value
            } else {
                self.mpos.get(*axis) - value
            };
            system.set(*axis, offset);
        }
        self.wcs_offsets[index] = system;
        true
    }

    fn begin_tool_change(&mut self, out: &mut Vec<u8>) {
        if !(self.profile.info.manual_tool_change() || self.profile.info.has_atc()) {
            self.fail(20, out);
            return;
        }
        if self.tool_change.is_some() {
            self.emit_error_only(40, out);
            return;
        }
        self.tool_change = Some(ToolChange {
            requested: self.tool,
            acknowledged: false,
        });
        self.state = MachineState::Tool;
        emit(out, "ok");
    }

    fn start_motion(
        &mut self,
        target: Position,
        kind: MotionKind,
        feed_override: Option<f64>,
        out: &mut Vec<u8>,
    ) {
        let feed = feed_override.unwrap_or_else(|| {
            if kind == MotionKind::Rapid {
                self.rapid_feed_rate()
            } else {
                self.feed_rate
            }
        });
        let duration = if kind == MotionKind::Rapid {
            motion::duration_for_rapid(self.mpos, target, self.axes, |axis| {
                self.axis_max_rate(axis)
            })
        } else {
            motion::duration_for_feed(self.mpos, target, feed)
        };
        if duration.is_zero() || target == self.mpos {
            self.mpos = target;
            if kind == MotionKind::Probe {
                self.record_probe(target, out);
            }
            return;
        }
        self.motion = Some(Motion::new(
            self.mpos, target, self.axes, kind, feed, duration,
        ));
        self.state = if kind == MotionKind::Jog {
            MachineState::Jog
        } else {
            MachineState::Run
        };
        self.hold_after_motion = self.single_block && kind != MotionKind::Jog;
    }

    /// Complete the in-flight block at its current interpolated position.
    fn settle_motion(&mut self, out: &mut Vec<u8>) {
        if let Some(motion) = self.motion.take() {
            self.mpos = if motion.is_finished() {
                motion.target()
            } else {
                motion.position()
            };
            if motion.kind() == MotionKind::Probe && motion.is_finished() {
                self.record_probe(motion.target(), out);
            }
        }
        if matches!(
            self.state,
            MachineState::Run | MachineState::Jog | MachineState::Home
        ) {
            self.finish_hold_or_idle();
        }
    }

    fn finish_hold_or_idle(&mut self) {
        if self.tool_change.is_some() {
            self.state = MachineState::Tool;
        } else if self.hold_after_motion {
            self.state = MachineState::Hold;
        } else {
            self.state = MachineState::Idle;
        }
        self.hold_after_motion = false;
    }

    fn record_probe(&mut self, position: Position, out: &mut Vec<u8>) {
        let result = ProbeResult::new(true, position);
        self.probe_result = Some(result);
        emit(
            out,
            &format!(
                "[PRB:{}:1]",
                crate::util::format_position_mm(position, self.axes)
            ),
        );
    }

    fn fail(&mut self, code: u16, out: &mut Vec<u8>) {
        self.error = Some(code);
        self.emit_error_only(code, out);
    }

    fn emit_error_only(&self, code: u16, out: &mut Vec<u8>) {
        emit(out, &format!("error:{code}"));
    }

    fn raise_alarm_to(&mut self, code: u16, out: &mut Vec<u8>) {
        if let Some(motion) = self.motion.take() {
            self.mpos = motion.position();
        }
        self.hold_after_motion = false;
        self.tool_change = None;
        self.spindle_on = false;
        self.flood = false;
        self.mist = false;
        self.state = MachineState::Alarm;
        self.alarm = Some(code);
        emit(out, &format!("ALARM:{code}"));
    }

    fn soft_reset(&mut self, out: &mut Vec<u8>) {
        if let Some(motion) = self.motion.take() {
            self.mpos = motion.position();
        }
        self.line.clear();
        self.line_overflow = false;
        self.last_terminator = None;
        self.escaped = false;
        self.tool_change = None;
        self.hold_after_motion = false;
        self.mpg = false;
        self.check_mode = false;
        self.overrides = Overrides::DEFAULT;
        self.spindle_on = false;
        self.spindle_stop = false;
        self.flood = false;
        self.mist = false;
        self.fan = false;
        self.error = None;
        self.auto_report_accum = Duration::ZERO;
        if self.alarm == Some(11) {
            self.state = MachineState::Alarm;
        } else {
            self.alarm = None;
            self.state = MachineState::Idle;
        }
        if self.banner_enabled {
            out.extend_from_slice(&self.banner());
        }
        for (index, line) in self.startup_lines.iter().enumerate() {
            if !line.is_empty() {
                emit(out, &format!(">N{index}={line}:ok"));
            }
        }
    }

    fn stop_operation(&mut self) {
        if let Some(motion) = self.motion.take() {
            self.mpos = motion.position();
        }
        self.hold_after_motion = false;
        self.tool_change = None;
        self.error = None;
        if self.state != MachineState::Alarm {
            self.state = MachineState::Idle;
        }
    }

    fn realtime(&mut self, command: RealtimeCommand, out: &mut Vec<u8>) {
        match command {
            RealtimeCommand::StatusReport => self.emit_status(false, out),
            RealtimeCommand::FullStatusReport => self.emit_status(true, out),
            RealtimeCommand::ParserStateReport => self.emit_parser_state(out),
            RealtimeCommand::CycleStart => self.cycle_start(),
            RealtimeCommand::FeedHold => self.feed_hold(),
            RealtimeCommand::JogCancel => self.cancel_jog(),
            RealtimeCommand::SafetyDoor => self.safety_door(),
            RealtimeCommand::SoftReset => self.soft_reset(out),
            RealtimeCommand::Reboot => self.soft_reset(out),
            RealtimeCommand::Stop | RealtimeCommand::Exit => self.stop_operation(),
            RealtimeCommand::OptionalStopToggle => self.optional_stop = !self.optional_stop,
            RealtimeCommand::SingleBlockToggle => self.single_block = !self.single_block,
            RealtimeCommand::Fan0Toggle => self.fan = !self.fan,
            RealtimeCommand::MpgModeToggle => {
                if self.profile.info.has_mpg() {
                    self.mpg = !self.mpg;
                }
            }
            RealtimeCommand::AutoReportToggle => self.toggle_auto_report(),
            RealtimeCommand::FeedOverrideReset => self.overrides.feed = 100,
            RealtimeCommand::FeedOverrideCoarsePlus => self.adjust_feed_override(10),
            RealtimeCommand::FeedOverrideCoarseMinus => self.adjust_feed_override(-10),
            RealtimeCommand::FeedOverrideFinePlus => self.adjust_feed_override(1),
            RealtimeCommand::FeedOverrideFineMinus => self.adjust_feed_override(-1),
            RealtimeCommand::RapidOverrideReset => self.overrides.rapid = 100,
            RealtimeCommand::RapidOverrideMedium => self.overrides.rapid = 50,
            RealtimeCommand::RapidOverrideLow => self.overrides.rapid = 25,
            RealtimeCommand::RapidOverrideExtraLow => self.overrides.rapid = 5,
            RealtimeCommand::SpindleOverrideReset => self.overrides.spindle = 100,
            RealtimeCommand::SpindleOverrideCoarsePlus => self.adjust_spindle_override(10),
            RealtimeCommand::SpindleOverrideCoarseMinus => self.adjust_spindle_override(-10),
            RealtimeCommand::SpindleOverrideFinePlus => self.adjust_spindle_override(1),
            RealtimeCommand::SpindleOverrideFineMinus => self.adjust_spindle_override(-1),
            RealtimeCommand::SpindleStopToggle => self.spindle_stop = !self.spindle_stop,
            RealtimeCommand::SoftEStop => self.raise_alarm_to(10, out),
            RealtimeCommand::FloodToggle => self.flood = !self.flood,
            RealtimeCommand::MistToggle => self.mist = !self.mist,
            RealtimeCommand::ToolChangeAck => {
                if let Some(change) = self.tool_change.as_mut() {
                    change.acknowledged = true;
                }
            }
            RealtimeCommand::ProbeConnectedToggle => {
                if self.profile.info.has_probe_connected_signal() {
                    self.probe_connected = !self.probe_connected;
                }
            }
            RealtimeCommand::PidReport
            | RealtimeCommand::ProgramDemarcation
            | RealtimeCommand::Macro(_) => {}
        }
    }

    fn cycle_start(&mut self) {
        match self.state {
            MachineState::Hold | MachineState::Door => {
                self.state = self.resume_state();
            }
            MachineState::Tool => {
                let acknowledged = self.tool_change.is_some_and(|change| change.acknowledged);
                if acknowledged {
                    if let Some(change) = self.tool_change.take() {
                        self.tool = change.requested;
                    }
                    self.state = MachineState::Idle;
                }
            }
            _ => {}
        }
    }

    fn feed_hold(&mut self) {
        if matches!(
            self.state,
            MachineState::Run | MachineState::Jog | MachineState::Home
        ) {
            self.state = MachineState::Hold;
        }
    }

    fn cancel_jog(&mut self) {
        let is_jog = self
            .motion
            .as_ref()
            .is_some_and(|motion| motion.kind() == MotionKind::Jog);
        if !is_jog {
            return;
        }
        if let Some(motion) = self.motion.take() {
            self.mpos = motion.position();
        }
        self.state = if self.tool_change.is_some() {
            MachineState::Tool
        } else {
            MachineState::Idle
        };
    }

    fn safety_door(&mut self) {
        if self.state == MachineState::Door {
            self.state = self.resume_state();
        } else if !matches!(
            self.state,
            MachineState::Alarm | MachineState::Sleep | MachineState::Home
        ) {
            self.spindle_on = false;
            self.flood = false;
            self.mist = false;
            self.state = MachineState::Door;
        }
    }

    fn resume_state(&self) -> MachineState {
        match self.motion.as_ref().map(Motion::kind) {
            Some(MotionKind::Jog) => MachineState::Jog,
            Some(_) => MachineState::Run,
            None => MachineState::Idle,
        }
    }

    fn advance_auto_report(&mut self, elapsed: Duration, out: &mut Vec<u8>) {
        if !self.auto_report
            || self.auto_report_interval.is_zero()
            || self.state == MachineState::Sleep
        {
            return;
        }
        self.auto_report_accum = self.auto_report_accum.saturating_add(elapsed);
        let mut emitted = 0;
        while self.auto_report_accum >= self.auto_report_interval
            && emitted < MAX_STATUS_REPORTS_PER_TICK
        {
            self.auto_report_accum -= self.auto_report_interval;
            self.emit_status(false, out);
            emitted += 1;
        }
        if emitted == MAX_STATUS_REPORTS_PER_TICK {
            self.auto_report_accum = Duration::ZERO;
        }
    }

    fn toggle_auto_report(&mut self) {
        self.auto_report = !self.auto_report;
        if self.auto_report {
            let milliseconds = self.settings.value(481).unwrap_or(0.0);
            self.auto_report_interval = if milliseconds > 0.0 {
                Duration::from_millis(u64::try_from(milliseconds as i64).unwrap_or(500))
            } else {
                DEFAULT_AUTO_REPORT
            };
            self.auto_report_accum = Duration::ZERO;
        }
    }

    fn adjust_feed_override(&mut self, delta: i16) {
        let next = i32::from(self.overrides.feed) + i32::from(delta);
        self.overrides.feed = u16::try_from(next.clamp(10, 200)).unwrap_or(100);
    }

    fn adjust_spindle_override(&mut self, delta: i16) {
        let next = i32::from(self.overrides.spindle) + i32::from(delta);
        self.overrides.spindle = u16::try_from(next.clamp(10, 200)).unwrap_or(100);
    }

    fn wco(&self) -> Position {
        self.wcs_offsets[self.wcs.index()] + self.g92
    }

    fn wpos(&self) -> Position {
        self.mpos - self.wco()
    }

    fn planner_free(&self) -> u32 {
        if self.motion.is_some() {
            self.planner_blocks.saturating_sub(1)
        } else {
            self.planner_blocks
        }
    }

    fn axis_max_rate(&self, axis: ghs_core::Axis) -> f64 {
        let number = 110 + u16::try_from(axis.index()).unwrap_or(0);
        self.settings.value(number).unwrap_or(5000.0)
    }

    fn rapid_feed_rate(&self) -> f64 {
        self.axes
            .iter()
            .map(|axis| self.axis_max_rate(axis))
            .fold(0.0_f64, f64::max)
    }

    fn outside_travel(&self, target: Position) -> bool {
        for axis in self.axes.iter() {
            let limit = self
                .settings
                .value(130 + u16::try_from(axis.index()).unwrap_or(0))
                .unwrap_or(0.0);
            if target.get(axis) > limit || target.get(axis) < -limit {
                return true;
            }
        }
        false
    }
}

fn wcs_from_code(value: f64) -> Option<Wcs> {
    Wcs::ALL
        .into_iter()
        .find(|wcs| is_code(value, wcs_code(*wcs)))
}

fn wcs_code(wcs: Wcs) -> f64 {
    match wcs {
        Wcs::G54 => 54.0,
        Wcs::G55 => 55.0,
        Wcs::G56 => 56.0,
        Wcs::G57 => 57.0,
        Wcs::G58 => 58.0,
        Wcs::G59 => 59.0,
        Wcs::G59_1 => 59.1,
        Wcs::G59_2 => 59.2,
        Wcs::G59_3 => 59.3,
    }
}
