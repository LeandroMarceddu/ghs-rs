//! Named controller capability profiles.
//!
//! A [`ControllerProfile`] is data: it names a controller and carries the
//! capability record parsed from its `$I`/`$I+` reply. Product code branches on
//! the capability helpers in [`ControllerInfo`], never on a profile name, so a
//! new board is added by supplying data rather than a code path.
//!
//! Behavioural profile data (latency, simulated position, settings defaults)
//! belongs to the simulator, which depends on this crate; it is deliberately
//! not embedded here.

use serde::{Deserialize, Serialize};

use crate::info::{ControllerInfo, parse_controller_info};

/// A named capability profile for a grblHAL controller.
///
/// The reference board is one profile among many; it is not special-cased in
/// the protocol or domain layers.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ControllerProfile {
    /// Profile name, for example `"flexihal"` or `"generic-full"`.
    pub name: String,
    /// Capabilities parsed from the controller's identity report.
    pub info: ControllerInfo,
}

impl ControllerProfile {
    /// Build a profile from a name and capability record.
    #[must_use]
    pub fn new(name: impl Into<String>, info: ControllerInfo) -> Self {
        ControllerProfile {
            name: name.into(),
            info,
        }
    }

    /// Build a profile by parsing `$I`/`$I+` lines.
    #[must_use]
    pub fn from_info_lines<S: AsRef<str>>(name: impl Into<String>, lines: &[S]) -> Self {
        ControllerProfile::new(name, parse_controller_info(lines))
    }

    /// Capabilities of this profile.
    #[must_use]
    pub const fn capabilities(&self) -> &ControllerInfo {
        &self.info
    }

    /// Returns `true` when every capability of `self` is also present in
    /// `other`.
    ///
    /// Axis sets, input signals, options, auxiliary IO counts and tool-table
    /// entries are compared; a missing numeric capability is treated as zero.
    /// Buffer sizes are performance resources, not features, so they are not
    /// part of the subset relation. This lets a test assert that a restricted
    /// board is a subset of a full profile without naming either.
    #[must_use]
    pub fn is_subset_of(&self, other: &Self) -> bool {
        let axes_ok = self.info.axes.difference(other.info.axes).is_empty();
        let signals_ok = self.info.signals.bits() & !other.info.signals.bits() == 0;
        let options_ok = self
            .info
            .new_options
            .iter()
            .all(|option| other.info.new_options.contains(option));
        let aux_ok = match (self.info.aux_io, other.info.aux_io) {
            (None, _) => true,
            (Some(own), Some(other)) => {
                own.digital_in <= other.digital_in
                    && own.digital_out <= other.digital_out
                    && own.analog_in <= other.analog_in
                    && own.analog_out <= other.analog_out
            }
            (Some(_), None) => false,
        };
        let tools_ok =
            self.info.tool_table_entries.unwrap_or(0) <= other.info.tool_table_entries.unwrap_or(0);

        axes_ok && signals_ok && options_ok && aux_ok && tools_ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLEXIHAL: &[&str] = &[
        "[VER:1.1f.20240604:]",
        "[OPT:VNMSL+2,64,1024,3,0]",
        "[AXS:3:XYZ]",
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
        "[FIRMWARE:grblHAL]",
        "[SIGNALS:HSDEP]",
        "[AUX IO:4,4,0,0]",
    ];

    const GENERIC_FULL: &[&str] = &[
        "[VER:1.1f.20240604:]",
        "[OPT:VNMSL+2,128,2048,6,8]",
        "[AXS:6:XYZABC]",
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED,SD,YM,ATC=1,MPG,PC,OS,BD,SS]",
        "[FIRMWARE:grblHAL]",
        "[SIGNALS:XZYABCDRHSELTMFQPIJK]",
        "[AUX IO:8,8,2,2]",
    ];

    #[test]
    fn named_profiles_are_pure_data() {
        let flexihal = ControllerProfile::from_info_lines("flexihal", FLEXIHAL);
        assert_eq!(flexihal.name, "flexihal");
        assert_eq!(flexihal.capabilities().rx_buffer_size, Some(1024));
        assert_eq!(flexihal.capabilities().planner_blocks, Some(64));
        assert!(flexihal.capabilities().has_sed());
        assert!(!flexihal.capabilities().has_sd());
    }

    #[test]
    fn restricted_profile_is_a_subset_of_the_full_profile() {
        let flexihal = ControllerProfile::from_info_lines("flexihal", FLEXIHAL);
        let full = ControllerProfile::from_info_lines("generic-full", GENERIC_FULL);
        assert!(flexihal.is_subset_of(&full));
        assert!(!full.is_subset_of(&flexihal));
        assert!(flexihal.is_subset_of(&flexihal));
    }

    #[test]
    fn subset_detects_missing_options_and_axes() {
        let mut board = ControllerProfile::from_info_lines("board", FLEXIHAL);
        let full = ControllerProfile::from_info_lines("generic-full", GENERIC_FULL);
        board.info.new_options.push("LATHE".into());
        assert!(!board.is_subset_of(&full));

        let mut wider = ControllerProfile::from_info_lines("wider", FLEXIHAL);
        wider.info.axes = crate::Axes::parse("XYZUVW").expect("valid");
        assert!(!wider.is_subset_of(&full));
    }
}
