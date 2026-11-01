//! Controller profiles used by the simulator.
//!
//! A profile is pure data: the identity and capability record the controller
//! would report in its `$I`/`$I+` response. The simulator engine derives every
//! behaviour from that record, so `flexihal` is simply a smaller data set than
//! `generic-full`; no code path knows either name.
//!
//! Two pieces of simulator-only behaviour are encoded in the same record:
//!
//! - the number of spindles, from a `SPINDLES=<n>` token in `NEWOPT`
//!   (the reference board omits it and therefore has one spindle);
//! - the controller-side tool table size, from the `OPT` line.

use ghs_core::{Axes, ControllerInfo, ControllerProfile};

/// Highest spindle count the simulator models.
pub const MAX_SIM_SPINDLES: u8 = 4;

/// A named simulator profile.
///
/// The enum only selects which capability data to load. Every behavioural
/// decision in the engine is made from the resulting [`ControllerProfile`], so
/// adding a board means adding data, not a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimProfile {
    /// The reference FlexiHAL build recorded in
    /// `docs/hardware/flexihal-1.1f.20240604.md`.
    FlexiHal,
    /// A superset profile with more axes, SD, MPG, ATC, PC and a tool table.
    GenericFull,
}

impl SimProfile {
    /// Profile name as reported by [`ControllerProfile::name`].
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            SimProfile::FlexiHal => "flexihal",
            SimProfile::GenericFull => "generic-full",
        }
    }

    /// Build the capability record for this profile.
    #[must_use]
    pub fn controller_profile(self) -> ControllerProfile {
        match self {
            SimProfile::FlexiHal => profile_flexihal(),
            SimProfile::GenericFull => profile_generic_full(),
        }
    }
}

/// The recorded FlexiHAL `$I+` identity lines.
///
/// These are byte-for-byte the lines in
/// `fixtures/controller/flexihal-1.1f.20240604.txt`, so a `$I+` request against
/// the simulator reproduces the recorded controller exactly.
const FLEXIHAL_LINES: &[&str] = &[
    "[VER:1.1f.20240604:]",
    "[OPT:VNMSL+2,64,1024,3,0]",
    "[AXS:3:XYZ]",
    "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
    "[FIRMWARE:grblHAL]",
    "[SIGNALS:HSDEP]",
    "[NVS STORAGE:*EEPROM]",
    "[FREE MEMORY:80K]",
    "[DRIVER:STM32F446]",
    "[DRIVER VERSION:240428]",
    "[BOARD:FlexiHAL_20240604_210624]",
    "[AUX IO:4,4,0,0]",
    "[PLUGIN:Bootloader Entry v0.02]",
    "[PLUGIN:MODBUS v0.16]",
    "[PLUGIN:HUANYANG VFD v0.11]",
    "[PLUGIN:HUANYANG P2A VFD v0.10]",
    "[PLUGIN:Durapulse VFD GS20 v0.05]",
    "[PLUGIN:Yalang VFD YL620A v0.02]",
    "[PLUGIN:MODVFD v0.03]",
    "[PLUGIN:H-100 VFD v0.03]",
    "[PLUGIN:Nowforever VFD v0.01]",
    "[PLUGIN:KEYPAD v1.4 Jog2K]",
    "[PLUGIN:Macro plugin v0.02]",
    "[PLUGIN:Probe Protection v0.01]",
    "[PLUGIN:Persistent tool v0.02]",
    "[PLUGIN:RGB Indicator Lights v1.0]",
    "[SPINDLE:Huanyang v1]",
];

/// The `generic-full` identity lines.
///
/// The profile is a strict superset of [`profile_flexihal`]: more axes, an SD
/// card and YModem, ATC, an MPG input, a probe-connected signal, two spindles
/// and an eight-entry controller tool table.
const GENERIC_FULL_LINES: &[&str] = &[
    "[VER:1.1f.20240604:]",
    "[OPT:VNMSL+2,128,2048,6,8]",
    "[AXS:6:XYZABC]",
    "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED,SD,YM,ATC=1,MPG,PC,OS,BD,SS,SPINDLES=2]",
    "[FIRMWARE:grblHAL]",
    "[SIGNALS:XYZABCDRHSELTMFQPIJK]",
    "[NVS STORAGE:*EEPROM]",
    "[FREE MEMORY:256K]",
    "[DRIVER:STM32F446]",
    "[DRIVER VERSION:240428]",
    "[BOARD:GenericFull_SIM]",
    "[AUX IO:8,8,2,2]",
    "[PLUGIN:MODBUS v0.16]",
    "[PLUGIN:SDCARD v1.0]",
    "[PLUGIN:Probe Protection v0.01]",
    "[SPINDLE:PWM v1]",
    "[SPINDLE:PWM v1]",
];

/// Capability record for the reference FlexiHAL build.
///
/// Three axes, 1024-byte RX buffer, 64 planner blocks, no SD/ATC/MPG/PC and no
/// controller tool table. The `$I+` reply regenerated from this record is
/// identical to the recorded fixture.
#[must_use]
pub fn profile_flexihal() -> ControllerProfile {
    ControllerProfile::from_info_lines(SimProfile::FlexiHal.name(), FLEXIHAL_LINES)
}

/// Capability record for the `generic-full` superset profile.
///
/// Six axes, 2048-byte RX buffer, 128 planner blocks, SD/YM, ATC, MPG, PC, two
/// spindles and eight controller tool-table entries.
#[must_use]
pub fn profile_generic_full() -> ControllerProfile {
    ControllerProfile::from_info_lines(SimProfile::GenericFull.name(), GENERIC_FULL_LINES)
}

/// Axes the simulator should model for a capability record.
///
/// A hand-written profile without `AXS` still gets the default three axes so
/// the engine can always produce a status report.
#[must_use]
pub(crate) fn axes_of(info: &ControllerInfo) -> Axes {
    if info.axes.is_empty() {
        Axes::xyz()
    } else {
        info.axes
    }
}

/// Number of spindles declared by the `SPINDLES=<n>` `NEWOPT` token.
#[must_use]
pub(crate) fn spindle_count(info: &ControllerInfo) -> u8 {
    info.new_options
        .iter()
        .find_map(|option| option.strip_prefix("SPINDLES="))
        .and_then(|value| value.trim().parse::<u8>().ok())
        .unwrap_or(1)
        .clamp(1, MAX_SIM_SPINDLES)
}

/// Rebuild the `$I+` identity lines from a capability record.
///
/// The reconstruction is lossless for records produced by
/// [`ControllerProfile::from_info_lines`]: the recorded FlexiHAL fixture round
/// trips byte for byte. Lines are emitted in the same order the controller
/// prints them.
#[must_use]
pub(crate) fn identity_lines(info: &ControllerInfo, spindles: u8) -> Vec<String> {
    let axes = axes_of(info);
    let mut lines = Vec::new();
    lines.push(format!(
        "[VER:{}.{}:{}]",
        info.version, info.build_info, info.info
    ));
    lines.push(format!(
        "[OPT:{},{},{},{},{}]",
        info.options,
        info.planner_blocks.unwrap_or(64),
        info.rx_buffer_size.unwrap_or(1024),
        axes.len(),
        info.tool_table_entries.unwrap_or(0)
    ));
    lines.push(format!("[AXS:{}:{}]", axes.len(), axes));
    lines.push(format!("[NEWOPT:{}]", info.new_options.join(",")));
    if let Some(firmware) = &info.firmware {
        lines.push(format!("[FIRMWARE:{firmware}]"));
    }
    lines.push(format!("[SIGNALS:{}]", info.signals));
    if let Some(nvs) = &info.nvs {
        lines.push(format!("[NVS STORAGE:*{nvs}]"));
    }
    if let Some(free) = info.free_memory_kb {
        lines.push(format!("[FREE MEMORY:{free}K]"));
    }
    if let Some(driver) = &info.driver {
        lines.push(format!("[DRIVER:{driver}]"));
    }
    if let Some(version) = &info.driver_version {
        lines.push(format!("[DRIVER VERSION:{version}]"));
    }
    if let Some(options) = &info.driver_options {
        lines.push(format!("[DRIVER OPTIONS:{options}]"));
    }
    if let Some(board) = &info.board {
        lines.push(format!("[BOARD:{board}]"));
    }
    if let Some(aux) = info.aux_io {
        lines.push(format!(
            "[AUX IO:{},{},{},{}]",
            aux.digital_in, aux.digital_out, aux.analog_in, aux.analog_out
        ));
    }
    for plugin in &info.plugins {
        lines.push(format!("[PLUGIN:{plugin}]"));
    }
    if let Some(spindle) = &info.spindle {
        for _ in 0..spindles {
            lines.push(format!("[SPINDLE:{spindle}]"));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flexihal_profile_matches_the_recorded_capabilities() {
        let profile = profile_flexihal();
        let info = profile.capabilities();
        assert_eq!(profile.name, "flexihal");
        assert_eq!(info.version, "1.1f");
        assert_eq!(info.build_info, "20240604");
        assert_eq!(info.axes.to_string(), "XYZ");
        assert_eq!(info.rx_buffer_size, Some(1024));
        assert_eq!(info.planner_blocks, Some(64));
        assert_eq!(info.tool_table_entries, Some(0));
        assert_eq!(info.new_options.len(), 7);
        assert!(info.has_sed());
        assert!(!info.has_sd());
        assert!(!info.has_mpg());
        assert!(!info.has_atc());
        assert!(!info.has_probe_connected_signal());
        assert_eq!(info.signals.to_string(), "HSDEP");
        assert_eq!(spindle_count(info), 1);
        assert_eq!(identity_lines(info, 1), FLEXIHAL_LINES);
    }

    #[test]
    fn generic_full_is_a_strict_superset() {
        let flexihal = profile_flexihal();
        let full = profile_generic_full();
        assert!(flexihal.is_subset_of(&full));
        assert!(!full.is_subset_of(&flexihal));
        assert_eq!(full.capabilities().axes.to_string(), "XYZABC");
        assert_eq!(full.capabilities().rx_buffer_size, Some(2048));
        assert_eq!(full.capabilities().planner_blocks, Some(128));
        assert_eq!(full.capabilities().tool_table_entries, Some(8));
        assert!(full.capabilities().has_sd());
        assert!(full.capabilities().has_mpg());
        assert!(full.capabilities().has_atc());
        assert!(full.capabilities().has_probe_connected_signal());
        assert_eq!(spindle_count(full.capabilities()), 2);
        assert_eq!(axes_of(full.capabilities()).to_string(), "XYZABC");
    }

    #[test]
    fn profiles_without_axis_data_fall_back_to_xyz() {
        let info = ControllerInfo::new();
        assert_eq!(axes_of(&info), Axes::xyz());
        assert_eq!(spindle_count(&info), 1);
    }
}
