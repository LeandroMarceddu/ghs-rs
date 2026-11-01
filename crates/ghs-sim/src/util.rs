//! Formatting helpers shared by the simulator.

use std::time::Duration;

use ghs_core::{Axes, Position, Units};

/// Append one controller line with the grblHAL `CR LF` terminator.
pub(crate) fn emit(out: &mut Vec<u8>, line: &str) {
    out.extend_from_slice(line.as_bytes());
    out.extend_from_slice(b"\r\n");
}

/// Number of decimal digits declared by a `$ES` format string such as
/// `#####0.000`.
pub(crate) fn decimals_for(format: &str) -> usize {
    format.split_once('.').map_or(0, |(_, fraction)| {
        fraction.chars().take_while(char::is_ascii_digit).count()
    })
}

/// Format a finite value with a fixed number of decimals.
///
/// Non-finite values print as `0`; the simulator never stores them, but a
/// formatting helper must not produce `NaN` text on the wire.
pub(crate) fn format_number(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    format!("{value:.decimals$}")
}

/// Format a position as the comma-separated value list grblHAL uses in
/// `MPos`, `WPos`, `WCO` and `DTG`.
pub(crate) fn format_position(position: Position, axes: Axes, units: Units) -> String {
    let mut text = String::new();
    for (index, axis) in axes.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push_str(&format_number(units.from_mm(position.get(axis)), 3));
    }
    text
}

/// Format a position in millimetres as a comma-separated value list.
pub(crate) fn format_position_mm(position: Position, axes: Axes) -> String {
    format_position(position, axes, Units::Mm)
}

/// Format a G-code value without needless trailing zeros, matching grblHAL's
/// `$G` output style (`F1000`, `S12000`, `X1.5`).
pub(crate) fn format_gcode_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let rounded = (value * 1000.0).round() / 1000.0;
    if (rounded - rounded.round()).abs() < 1e-9 {
        return format!("{:.0}", rounded.round());
    }
    let mut text = format!("{rounded:.3}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

/// Deterministic controller clock text for `$RTC`.
///
/// The simulated clock starts at `2026-01-01T00:00:00Z` and advances only with
/// [`crate::Simulator::tick`], so a test never depends on wall-clock time.
pub(crate) fn format_clock(elapsed: Duration) -> String {
    let total = elapsed.as_secs();
    let seconds = total % 60;
    let minutes = (total / 60) % 60;
    let hours = (total / 3_600) % 24;
    let day = 1 + (total / 86_400) % 28;
    format!("2026-01-{day:02} {hours:02}:{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghs_core::{Axis, Position};

    #[test]
    fn positions_are_comma_separated_in_report_units() {
        let position = Position::xyz(1.0, -2.5, 25.4);
        assert_eq!(
            format_position(position, Axes::xyz(), Units::Mm),
            "1.000,-2.500,25.400"
        );
        assert_eq!(
            format_position(position, Axes::xyz(), Units::Inch),
            "0.039,-0.098,1.000"
        );
    }

    #[test]
    fn gcode_numbers_drop_trailing_zeros() {
        assert_eq!(format_gcode_number(0.0), "0");
        assert_eq!(format_gcode_number(-2.0), "-2");
        assert_eq!(format_gcode_number(1.5), "1.5");
        assert_eq!(format_gcode_number(1.25), "1.25");
        assert_eq!(format_gcode_number(1.234_499), "1.234");
        assert_eq!(format_gcode_number(f64::NAN), "0");
    }

    #[test]
    fn clock_is_deterministic_and_wraps() {
        assert_eq!(format_clock(Duration::ZERO), "2026-01-01 00:00:00");
        assert_eq!(
            format_clock(Duration::from_secs(3_661)),
            "2026-01-01 01:01:01"
        );
    }

    #[test]
    fn axis_order_is_preserved() {
        let mut position = Position::new(Axes::parse("XZA").expect("valid"));
        position.set(Axis::X, 1.0);
        position.set(Axis::Z, 2.0);
        position.set(Axis::A, 3.0);
        assert_eq!(
            format_position_mm(position, Axes::parse("XZA").expect("valid")),
            "1.000,2.000,3.000"
        );
    }
}
