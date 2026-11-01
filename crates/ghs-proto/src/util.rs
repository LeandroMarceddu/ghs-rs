//! Small shared parsing helpers.

use ghs_core::{AXIS_COUNT, Axes, Axis, Position};

/// Truncate `text` to at most `max` characters, after trimming whitespace.
pub(crate) fn bounded(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= max {
        trimmed.to_string()
    } else {
        trimmed.chars().take(max).collect()
    }
}

/// Parse a comma-separated position with a variable axis count.
///
/// Missing axes are not invented: the mask is set from the number of values, in
/// wire order. NaNs, infinities, empty fields and more than nine axes are
/// rejected.
pub(crate) fn parse_position(value: &str) -> Result<Position, &'static str> {
    if value.is_empty() {
        return Err("no coordinates");
    }
    let mut values = Vec::with_capacity(AXIS_COUNT);
    for part in value.split(',') {
        if values.len() == AXIS_COUNT {
            return Err("too many axes");
        }
        let part = part.trim();
        if part.is_empty() {
            return Err("empty coordinate");
        }
        let parsed = part.parse::<f64>().map_err(|_| "expected a number")?;
        if !parsed.is_finite() {
            return Err("coordinate is not finite");
        }
        values.push(parsed);
    }
    let axes = Axes::from_iter(Axis::ALL.into_iter().take(values.len()));
    Position::from_values(axes, &values).ok_or("too many axes")
}

/// Parse a finite `f64`.
pub(crate) fn parse_f64(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

/// Parse an unsigned 32-bit integer.
pub(crate) fn parse_u32(text: &str) -> Option<u32> {
    text.trim().parse::<u32>().ok()
}

/// Parse an unsigned 16-bit integer.
pub(crate) fn parse_u16(text: &str) -> Option<u16> {
    text.trim().parse::<u16>().ok()
}

/// Parse a signed 32-bit integer.
pub(crate) fn parse_i32(text: &str) -> Option<i32> {
    text.trim().parse::<i32>().ok()
}

/// Parse grblHAL's `0`/`1` booleans.
pub(crate) fn parse_bool01(text: &str) -> Option<bool> {
    match text.trim() {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

/// Read the leading decimal digits of `text` as a `u16`.
///
/// Used for `error:<n>` and `ALARM:<n>` lines, which some builds decorate with
/// trailing text.
pub(crate) fn leading_u16(text: &str) -> Option<u16> {
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u16>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_variable_length() {
        let xyz = parse_position("1.0,2.0,3.0").expect("valid");
        assert_eq!(xyz.to_string(), "X1.000 Y2.000 Z3.000");
        let five = parse_position("1,2,3,4,5").expect("valid");
        assert_eq!(five.axes().to_string(), "XYZAB");
        assert_eq!(
            parse_position("1,2,3,4,5,6,7,8,9")
                .expect("valid")
                .axes()
                .len(),
            9
        );
    }

    #[test]
    fn malformed_positions_are_rejected() {
        assert!(parse_position("").is_err());
        assert!(parse_position("1,,3").is_err());
        assert!(parse_position("1,two,3").is_err());
        assert!(parse_position("NaN,1,2").is_err());
        assert!(parse_position("inf,1,2").is_err());
        assert!(parse_position("1,2,3,4,5,6,7,8,9,10").is_err());
    }

    #[test]
    fn numbers_and_booleans() {
        assert_eq!(parse_f64(" 1.5 "), Some(1.5));
        assert_eq!(parse_f64("NaN"), None);
        assert_eq!(parse_u32("42"), Some(42));
        assert_eq!(parse_u32("-1"), None);
        assert_eq!(parse_u16("70000"), None);
        assert_eq!(parse_i32("-3"), Some(-3));
        assert_eq!(parse_bool01("1"), Some(true));
        assert_eq!(parse_bool01("0"), Some(false));
        assert_eq!(parse_bool01("true"), None);
    }

    #[test]
    fn leading_digits_stop_at_text() {
        assert_eq!(leading_u16("3"), Some(3));
        assert_eq!(leading_u16("3: bad"), Some(3));
        assert_eq!(leading_u16("bad"), None);
        assert_eq!(leading_u16("99999"), None);
    }

    #[test]
    fn bounded_truncates_characters() {
        assert_eq!(bounded("  hello  ", 10), "hello");
        assert_eq!(bounded(&"x".repeat(20), 5), "xxxxx");
    }
}
