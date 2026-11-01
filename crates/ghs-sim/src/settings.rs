//! The simulator's typed settings table and its `$ES`/`$EG`/`$SED` metadata.
//!
//! The table models the common grbl and grblHAL settings a milling sender
//! uses: step pulse timing, step/direction masks, limits, homing, spindle RPM,
//! status reporting and the per-axis steps/rate/acceleration/travel quartet.
//! Every setting carries full `$ES` metadata so the settings UI can be
//! generated from the controller instead of hardcoded.

use std::collections::BTreeMap;

use ghs_core::{Axes, Axis};

use crate::util::{decimals_for, format_number};

/// Group id for general settings.
pub(crate) const GROUP_GENERAL: u16 = 0;
/// Group id for stepper settings.
pub(crate) const GROUP_STEPPER: u16 = 1;
/// Group id for limit and homing settings.
pub(crate) const GROUP_LIMITS: u16 = 2;
/// Group id for spindle settings.
pub(crate) const GROUP_SPINDLE: u16 = 3;
/// Group id for per-axis settings.
pub(crate) const GROUP_AXES: u16 = 4;

/// Data type of a simulated setting, matching the `$ES` type codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingKind {
    /// Type 0: boolean checkbox.
    Boolean,
    /// Type 1: bitfield checkbox column.
    Bitfield,
    /// Type 4: axis mask checkbox column.
    AxisMask,
    /// Type 5: integer field.
    Integer,
    /// Type 6: decimal field.
    Decimal,
}

impl SettingKind {
    /// Wire type code for `$ES`.
    pub(crate) const fn type_code(self) -> u8 {
        match self {
            SettingKind::Boolean => 0,
            SettingKind::Bitfield => 1,
            SettingKind::AxisMask => 4,
            SettingKind::Integer => 5,
            SettingKind::Decimal => 6,
        }
    }

    /// Returns `true` when the setting accepts whole numbers only.
    pub(crate) const fn is_integer(self) -> bool {
        !matches!(self, SettingKind::Decimal)
    }
}

/// One simulated controller setting and its metadata.
#[derive(Debug, Clone)]
pub(crate) struct SettingDef {
    /// Setting number, the `<n>` in `$<n>`.
    pub number: u16,
    /// Group id from `$EG`.
    pub group: u16,
    /// Human-readable name.
    pub name: String,
    /// Unit label, absent when the setting is unitless.
    pub unit: Option<&'static str>,
    /// Value type.
    pub kind: SettingKind,
    /// `$ES` display format.
    pub format: &'static str,
    /// Smallest accepted value.
    pub min: f64,
    /// Largest accepted value.
    pub max: f64,
    /// Default value.
    pub default: f64,
    /// Whether a reboot is required after a change.
    pub reboot: bool,
    /// Longer `$SED` description.
    pub description: String,
}

impl SettingDef {
    fn decimals(&self) -> usize {
        if self.kind == SettingKind::Decimal {
            decimals_for(self.format)
        } else {
            0
        }
    }

    /// Format a value the way `$$` and `$<n>` report it.
    #[must_use]
    pub(crate) fn format_value(&self, value: f64) -> String {
        format_number(value, self.decimals())
    }

    fn format_bound(&self, value: f64) -> String {
        self.format_value(value)
    }

    /// Render the `[SETTING:...]` metadata record.
    #[must_use]
    pub(crate) fn metadata_line(&self) -> String {
        format!(
            "[SETTING:{}|{}|{}|{}|{}|{}|{}|{}|{}|0]",
            self.number,
            self.group,
            self.name,
            self.unit.unwrap_or("N/A"),
            self.kind.type_code(),
            self.format,
            self.format_bound(self.min),
            self.format_bound(self.max),
            u8::from(self.reboot)
        )
    }

    /// Render the `[SETTINGDESC:...]` long-description record.
    #[must_use]
    pub(crate) fn description_line(&self) -> String {
        format!("[SETTINGDESC:{}|{}]", self.number, self.description)
    }
}

/// A rejected setting write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingError {
    /// The setting number is not in the profile's table.
    Unknown,
    /// The value is not a finite number.
    NotANumber,
    /// An integer setting received a fractional value.
    NotInteger,
    /// The value is outside the `$ES` range.
    OutOfRange,
}

/// The simulated controller settings: definitions plus current values.
#[derive(Debug, Clone)]
pub(crate) struct SettingsTable {
    defs: Vec<SettingDef>,
    values: BTreeMap<u16, f64>,
}

impl SettingsTable {
    /// Build the table for a controller with `axes` axes.
    ///
    /// Per-axis settings use the grbl numbering convention: `$100` is the
    /// steps-per-millimetre setting for the first axis, `$101` the second and so
    /// on, with `$110` starting the maximum-rate block.
    #[must_use]
    pub(crate) fn for_axes(axes: Axes) -> Self {
        let mut defs = vec![
            SettingDef {
                number: 0,
                group: GROUP_STEPPER,
                name: "Step pulse time".to_string(),
                unit: Some("microseconds"),
                kind: SettingKind::Integer,
                format: "#####0",
                min: 3.0,
                max: 1000.0,
                default: 10.0,
                reboot: true,
                description: "Time each step pulse is held on the stepper drivers.".to_string(),
            },
            SettingDef {
                number: 1,
                group: GROUP_STEPPER,
                name: "Step idle delay".to_string(),
                unit: Some("milliseconds"),
                kind: SettingKind::Integer,
                format: "#####0",
                min: 0.0,
                max: 255.0,
                default: 25.0,
                reboot: false,
                description: "Delay before the drivers are disabled after the last motion."
                    .to_string(),
            },
            SettingDef {
                number: 2,
                group: GROUP_STEPPER,
                name: "Step pulse invert mask".to_string(),
                unit: None,
                kind: SettingKind::Bitfield,
                format: "#####0",
                min: 0.0,
                max: 255.0,
                default: 0.0,
                reboot: true,
                description: "Axes whose step pulse polarity is inverted.".to_string(),
            },
            SettingDef {
                number: 3,
                group: GROUP_STEPPER,
                name: "Step direction invert mask".to_string(),
                unit: None,
                kind: SettingKind::Bitfield,
                format: "#####0",
                min: 0.0,
                max: 255.0,
                default: 0.0,
                reboot: true,
                description: "Axes whose direction signal is inverted.".to_string(),
            },
            SettingDef {
                number: 4,
                group: GROUP_STEPPER,
                name: "Invert step enable pin".to_string(),
                unit: None,
                kind: SettingKind::Boolean,
                format: "#####0",
                min: 0.0,
                max: 1.0,
                default: 0.0,
                reboot: true,
                description: "Invert the stepper driver enable pin.".to_string(),
            },
            SettingDef {
                number: 5,
                group: GROUP_LIMITS,
                name: "Invert limit pins".to_string(),
                unit: None,
                kind: SettingKind::Boolean,
                format: "#####0",
                min: 0.0,
                max: 1.0,
                default: 0.0,
                reboot: true,
                description: "Invert the limit switch inputs.".to_string(),
            },
            SettingDef {
                number: 6,
                group: GROUP_LIMITS,
                name: "Invert probe pin".to_string(),
                unit: None,
                kind: SettingKind::Boolean,
                format: "#####0",
                min: 0.0,
                max: 1.0,
                default: 0.0,
                reboot: true,
                description: "Invert the probe input.".to_string(),
            },
            SettingDef {
                number: 10,
                group: GROUP_GENERAL,
                name: "Status report mask".to_string(),
                unit: None,
                kind: SettingKind::Bitfield,
                format: "#####0",
                min: 0.0,
                max: 255.0,
                default: 3.0,
                reboot: false,
                description: "Fields included in periodic status reports.".to_string(),
            },
            SettingDef {
                number: 20,
                group: GROUP_LIMITS,
                name: "Soft limits enable".to_string(),
                unit: None,
                kind: SettingKind::Boolean,
                format: "#####0",
                min: 0.0,
                max: 1.0,
                default: 0.0,
                reboot: false,
                description: "Reject motion outside the configured maximum travel.".to_string(),
            },
            SettingDef {
                number: 21,
                group: GROUP_LIMITS,
                name: "Hard limits enable".to_string(),
                unit: None,
                kind: SettingKind::Boolean,
                format: "#####0",
                min: 0.0,
                max: 1.0,
                default: 0.0,
                reboot: false,
                description: "Stop with an alarm when a limit switch is asserted.".to_string(),
            },
            SettingDef {
                number: 22,
                group: GROUP_LIMITS,
                name: "Homing cycle enable".to_string(),
                unit: None,
                kind: SettingKind::Boolean,
                format: "#####0",
                min: 0.0,
                max: 1.0,
                default: 1.0,
                reboot: false,
                description: "Enable the homing cycle.".to_string(),
            },
            SettingDef {
                number: 23,
                group: GROUP_LIMITS,
                name: "Homing direction invert mask".to_string(),
                unit: None,
                kind: SettingKind::AxisMask,
                format: "#####0",
                min: 0.0,
                max: 511.0,
                default: 0.0,
                reboot: false,
                description: "Axes that home in the positive direction.".to_string(),
            },
            SettingDef {
                number: 24,
                group: GROUP_LIMITS,
                name: "Homing feed rate".to_string(),
                unit: Some("mm/min"),
                kind: SettingKind::Decimal,
                format: "#####0.000",
                min: 1.0,
                max: 100_000.0,
                default: 500.0,
                reboot: false,
                description: "Feed rate for the final homing approach.".to_string(),
            },
            SettingDef {
                number: 25,
                group: GROUP_LIMITS,
                name: "Homing seek rate".to_string(),
                unit: Some("mm/min"),
                kind: SettingKind::Decimal,
                format: "#####0.000",
                min: 1.0,
                max: 100_000.0,
                default: 1500.0,
                reboot: false,
                description: "Fast seek rate for the homing cycle.".to_string(),
            },
            SettingDef {
                number: 26,
                group: GROUP_LIMITS,
                name: "Homing switch debounce".to_string(),
                unit: Some("milliseconds"),
                kind: SettingKind::Integer,
                format: "#####0",
                min: 0.0,
                max: 1000.0,
                default: 250.0,
                reboot: false,
                description: "Debounce interval for homing switch signals.".to_string(),
            },
            SettingDef {
                number: 27,
                group: GROUP_LIMITS,
                name: "Homing pull-off".to_string(),
                unit: Some("mm"),
                kind: SettingKind::Decimal,
                format: "#####0.000",
                min: 0.0,
                max: 1000.0,
                default: 1.0,
                reboot: false,
                description: "Distance to move away from a homing switch.".to_string(),
            },
            SettingDef {
                number: 30,
                group: GROUP_SPINDLE,
                name: "Maximum spindle speed".to_string(),
                unit: Some("RPM"),
                kind: SettingKind::Decimal,
                format: "#####0.000",
                min: 0.0,
                max: 100_000.0,
                default: 24_000.0,
                reboot: false,
                description: "Highest programmed spindle speed.".to_string(),
            },
            SettingDef {
                number: 31,
                group: GROUP_SPINDLE,
                name: "Minimum spindle speed".to_string(),
                unit: Some("RPM"),
                kind: SettingKind::Decimal,
                format: "#####0.000",
                min: 0.0,
                max: 100_000.0,
                default: 0.0,
                reboot: false,
                description: "Lowest programmed spindle speed.".to_string(),
            },
            SettingDef {
                number: 32,
                group: GROUP_SPINDLE,
                name: "Laser mode".to_string(),
                unit: None,
                kind: SettingKind::Boolean,
                format: "#####0",
                min: 0.0,
                max: 1.0,
                default: 0.0,
                reboot: false,
                description: "Enable laser mode spindle behaviour.".to_string(),
            },
            SettingDef {
                number: 481,
                group: GROUP_GENERAL,
                name: "Auto report interval".to_string(),
                unit: Some("milliseconds"),
                kind: SettingKind::Integer,
                format: "#####0",
                min: 0.0,
                max: 60_000.0,
                default: 0.0,
                reboot: false,
                description: "Automatic status report interval; zero disables auto reporting."
                    .to_string(),
            },
        ];

        for (index, axis) in axes.iter().enumerate() {
            let number = 100 + u16::try_from(index).unwrap_or(0);
            let letter = axis.as_str();
            defs.push(SettingDef {
                number,
                group: GROUP_AXES,
                name: format!("{letter} steps per millimetre"),
                unit: Some("step/mm"),
                kind: SettingKind::Decimal,
                format: "#####0.000",
                min: 1.0,
                max: 10_000.0,
                default: 250.0,
                reboot: true,
                description: format!("Steps per millimetre for the {letter} axis."),
            });
            defs.push(SettingDef {
                number: 110 + u16::try_from(index).unwrap_or(0),
                group: GROUP_AXES,
                name: format!("{letter} maximum rate"),
                unit: Some("mm/min"),
                kind: SettingKind::Decimal,
                format: "#####0.000",
                min: 1.0,
                max: 100_000.0,
                default: 5000.0,
                reboot: false,
                description: format!("Maximum velocity of the {letter} axis."),
            });
            defs.push(SettingDef {
                number: 120 + u16::try_from(index).unwrap_or(0),
                group: GROUP_AXES,
                name: format!("{letter} acceleration"),
                unit: Some("mm/sec^2"),
                kind: SettingKind::Decimal,
                format: "#####0.000",
                min: 1.0,
                max: 10_000.0,
                default: 500.0,
                reboot: false,
                description: format!("Acceleration of the {letter} axis."),
            });
            defs.push(SettingDef {
                number: 130 + u16::try_from(index).unwrap_or(0),
                group: GROUP_AXES,
                name: format!("{letter} maximum travel"),
                unit: Some(if matches!(axis, Axis::A | Axis::B | Axis::C) {
                    "degrees"
                } else {
                    "mm"
                }),
                kind: SettingKind::Decimal,
                format: "#####0.000",
                min: 1.0,
                max: 1_000_000.0,
                default: default_travel(axis),
                reboot: false,
                description: format!("Maximum travel of the {letter} axis from the home position."),
            });
        }
        defs.sort_by_key(|def| def.number);

        let values = defs.iter().map(|def| (def.number, def.default)).collect();
        SettingsTable { defs, values }
    }

    /// Current value of one setting.
    #[must_use]
    pub(crate) fn value(&self, number: u16) -> Option<f64> {
        self.values.get(&number).copied()
    }

    /// Definition of one setting.
    #[must_use]
    pub(crate) fn def(&self, number: u16) -> Option<&SettingDef> {
        self.defs.iter().find(|def| def.number == number)
    }

    /// Every definition, in ascending number order.
    #[cfg(test)]
    pub(crate) fn defs(&self) -> impl Iterator<Item = &SettingDef> {
        self.defs.iter()
    }

    /// Render `$<n>=<value>` for one setting.
    #[must_use]
    pub(crate) fn line(&self, number: u16) -> Option<String> {
        let def = self.def(number)?;
        let value = self.value(number)?;
        Some(format!("${number}={}", def.format_value(value)))
    }

    /// Render `$<n>=<value> (<name>)` for `$$=<n>`.
    #[must_use]
    pub(crate) fn detail_line(&self, number: u16) -> Option<String> {
        let def = self.def(number)?;
        let value = self.value(number)?;
        Some(format!(
            "${number}={} ({})",
            def.format_value(value),
            def.name
        ))
    }

    /// Render every `$<n>=<value>` line.
    #[must_use]
    pub(crate) fn all_lines(&self) -> Vec<String> {
        self.defs
            .iter()
            .filter_map(|def| self.line(def.number))
            .collect()
    }

    /// Apply a setting write.
    ///
    /// # Errors
    ///
    /// Returns [`SettingError`] for an unknown setting, a malformed number, a
    /// fractional value for an integer setting, or a value outside the `$ES`
    /// range. The stored value is unchanged on error.
    pub(crate) fn set_text(&mut self, number: u16, text: &str) -> Result<f64, SettingError> {
        let Some(def) = self.def(number) else {
            return Err(SettingError::Unknown);
        };
        let Ok(value) = text.trim().parse::<f64>() else {
            return Err(SettingError::NotANumber);
        };
        if !value.is_finite() {
            return Err(SettingError::NotANumber);
        }
        if def.kind.is_integer() && value.fract() != 0.0 {
            return Err(SettingError::NotInteger);
        }
        if value < def.min || value > def.max {
            return Err(SettingError::OutOfRange);
        }
        self.values.insert(number, value);
        Ok(value)
    }

    /// Restore every setting to its default.
    pub(crate) fn reset(&mut self) {
        for def in &self.defs {
            self.values.insert(def.number, def.default);
        }
    }

    /// `[SETTING:...]` metadata for every setting.
    #[must_use]
    pub(crate) fn metadata_lines(&self) -> Vec<String> {
        self.defs.iter().map(SettingDef::metadata_line).collect()
    }

    /// `[SETTINGGROUP:...]` records for the table's groups.
    #[must_use]
    pub(crate) fn group_lines(&self) -> Vec<String> {
        [
            (GROUP_GENERAL, "N/A", "General"),
            (GROUP_STEPPER, "0", "Stepper"),
            (GROUP_LIMITS, "0", "Limits"),
            (GROUP_SPINDLE, "0", "Spindle"),
            (GROUP_AXES, "0", "Axes"),
        ]
        .into_iter()
        .map(|(id, parent, name)| format!("[SETTINGGROUP:{id}|{parent}|{name}]"))
        .collect()
    }

    /// `[SETTINGDESC:...]` descriptions for every setting.
    #[must_use]
    pub(crate) fn description_lines(&self) -> Vec<String> {
        self.defs.iter().map(SettingDef::description_line).collect()
    }

    /// Description line for one setting.
    #[must_use]
    pub(crate) fn description_line(&self, number: u16) -> Option<String> {
        self.def(number).map(SettingDef::description_line)
    }
}

fn default_travel(axis: Axis) -> f64 {
    match axis {
        Axis::X | Axis::U => 1200.0,
        Axis::Y | Axis::V => 900.0,
        Axis::Z | Axis::W => 200.0,
        Axis::A | Axis::B | Axis::C => 360.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_covers_common_grbl_settings() {
        let table = SettingsTable::for_axes(Axes::xyz());
        for number in [
            0, 1, 2, 3, 4, 5, 6, 10, 20, 21, 22, 23, 24, 25, 26, 27, 30, 31, 32, 481,
        ] {
            assert!(table.def(number).is_some(), "missing ${number}");
            assert!(table.line(number).is_some());
        }
        for number in [100, 101, 102, 110, 111, 112, 120, 121, 122, 130, 131, 132] {
            assert!(table.def(number).is_some(), "missing ${number}");
        }
        assert!(table.def(103).is_none());
        assert_eq!(table.line(100).as_deref(), Some("$100=250.000"));
        assert_eq!(table.line(110).as_deref(), Some("$110=5000.000"));
    }

    #[test]
    fn six_axis_tables_extend_the_axis_block() {
        let table = SettingsTable::for_axes(Axes::parse("XYZABC").expect("valid"));
        for number in [103, 104, 105, 113, 114, 115, 123, 124, 125, 133, 134, 135] {
            assert!(table.def(number).is_some(), "missing ${number}");
        }
        assert!(table.def(106).is_none());
    }

    #[test]
    fn writes_validate_type_and_range() {
        let mut table = SettingsTable::for_axes(Axes::xyz());
        assert_eq!(table.set_text(100, "300.000"), Ok(300.0));
        assert_eq!(table.value(100), Some(300.0));
        assert_eq!(table.line(100).as_deref(), Some("$100=300.000"));

        assert_eq!(table.set_text(23, "1.5"), Err(SettingError::NotInteger));
        assert_eq!(table.set_text(100, "0"), Err(SettingError::OutOfRange));
        assert_eq!(table.set_text(100, "abc"), Err(SettingError::NotANumber));
        assert_eq!(table.set_text(999, "1"), Err(SettingError::Unknown));
        assert_eq!(table.value(100), Some(300.0));
    }

    #[test]
    fn metadata_matches_the_setting_record_shape() {
        let table = SettingsTable::for_axes(Axes::xyz());
        let metadata = table.metadata_lines();
        assert_eq!(metadata.len(), table.defs().count());
        let line = metadata
            .iter()
            .find(|line| line.starts_with("[SETTING:100|"))
            .expect("steps per mm metadata");
        assert_eq!(
            line,
            "[SETTING:100|4|X steps per millimetre|step/mm|6|#####0.000|1.000|10000.000|1|0]"
        );
        let boolean = metadata
            .iter()
            .find(|line| line.starts_with("[SETTING:22|"))
            .expect("homing metadata");
        assert!(boolean.contains("|0|"));
        assert!(boolean.ends_with("|0|0]"));
    }

    #[test]
    fn reset_restores_defaults() {
        let mut table = SettingsTable::for_axes(Axes::xyz());
        table.set_text(110, "9000").expect("valid");
        table.reset();
        assert_eq!(table.value(110), Some(5000.0));
    }

    #[test]
    fn group_and_description_lines_cover_the_table() {
        let table = SettingsTable::for_axes(Axes::xyz());
        assert_eq!(table.group_lines().len(), 5);
        assert_eq!(table.description_lines().len(), table.defs().count());
        assert!(table.description_line(100).is_some_and(|line| {
            line.starts_with("[SETTINGDESC:100|Steps per millimetre for the X")
        }));
    }
}
