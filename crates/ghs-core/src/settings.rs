//! Controller settings and their typed values.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::axis::{Axes, Axis};

/// Maximum length kept for a free-form setting value.
const MAX_STRING_VALUE: usize = 512;

/// One value of an axis-array setting such as `$100=250.000,250.000,250.000`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisValues {
    /// Axes the values belong to, in wire order.
    pub axes: Axes,
    /// Numeric values, one per axis in `axes`, in wire order.
    pub values: Vec<f64>,
}

impl AxisValues {
    /// Build a per-axis value list, returning `None` when the length does not
    /// match the axis mask or the list is longer than the nine modelled axes.
    #[must_use]
    pub fn new(axes: Axes, values: Vec<f64>) -> Option<Self> {
        if values.len() != axes.len() || values.len() > Axes::all().len() {
            return None;
        }
        Some(AxisValues { axes, values })
    }

    /// Build a value list from comma-separated numbers, assigning them to the
    /// first axes in wire order.
    #[must_use]
    pub fn from_values(values: Vec<f64>) -> Option<Self> {
        if values.is_empty() || values.len() > Axis::ALL.len() {
            return None;
        }
        let axes = Axes::from_iter(Axis::ALL.into_iter().take(values.len()));
        Some(AxisValues { axes, values })
    }

    /// Number of values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns `true` when there are no values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Value for one axis, when it is present.
    #[must_use]
    pub fn get(&self, axis: Axis) -> Option<f64> {
        let index = self.axes.iter().position(|candidate| candidate == axis)?;
        self.values.get(index).copied()
    }

    /// Iterate `(axis, value)` pairs in wire order.
    pub fn iter(&self) -> impl Iterator<Item = (Axis, f64)> + '_ {
        self.axes.iter().zip(self.values.iter().copied())
    }
}

impl fmt::Display for AxisValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, value) in self.values.iter().enumerate() {
            if index > 0 {
                f.write_str(",")?;
            }
            write!(f, "{value}")?;
        }
        Ok(())
    }
}

/// A typed controller setting value.
///
/// The parser cannot know a setting's declared type without `$SED`/`$ES`
/// metadata, so [`SettingValue::parse`] uses the wire form: integer text is
/// [`SettingValue::Int`], decimal text is [`SettingValue::Float`], a
/// comma-separated numeric list is [`SettingValue::AxisArray`], `true`/`false`
/// is [`SettingValue::Bool`], and everything else is a string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SettingValue {
    /// Floating-point value.
    Float(f64),
    /// Integer value.
    Int(i64),
    /// Boolean value.
    Bool(bool),
    /// Free-form string value.
    String(String),
    /// One value per axis.
    AxisArray(AxisValues),
}

impl SettingValue {
    /// Parse a setting value from its wire representation.
    ///
    /// Never fails: unrecognised text becomes a bounded
    /// [`SettingValue::String`].
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let text = text.trim();
        if text.is_empty() {
            return SettingValue::String(String::new());
        }

        if text.contains(',') {
            let numbers: Option<Vec<f64>> = text
                .split(',')
                .map(|part| {
                    part.trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite())
                })
                .collect();
            if let Some(numbers) = numbers
                && let Some(axis_values) = AxisValues::from_values(numbers)
            {
                return SettingValue::AxisArray(axis_values);
            }
        }

        if text.eq_ignore_ascii_case("true") {
            return SettingValue::Bool(true);
        }
        if text.eq_ignore_ascii_case("false") {
            return SettingValue::Bool(false);
        }
        if let Ok(value) = text.parse::<i64>() {
            return SettingValue::Int(value);
        }
        if let Ok(value) = text.parse::<f64>()
            && value.is_finite()
        {
            return SettingValue::Float(value);
        }

        let bounded: String = text.chars().take(MAX_STRING_VALUE).collect();
        SettingValue::String(bounded)
    }

    /// Numeric value, when the setting can be interpreted as a number.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            SettingValue::Float(value) => Some(*value),
            SettingValue::Int(value) => Some(*value as f64),
            SettingValue::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            SettingValue::String(_) | SettingValue::AxisArray(_) => None,
        }
    }

    /// Boolean value, when the setting can be interpreted as one.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            SettingValue::Bool(value) => Some(*value),
            SettingValue::Int(value) => Some(*value != 0),
            SettingValue::Float(value) => Some(*value != 0.0),
            SettingValue::String(_) | SettingValue::AxisArray(_) => None,
        }
    }

    /// String value, when the setting is textual.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            SettingValue::String(value) => Some(value),
            _ => None,
        }
    }

    /// Per-axis values, when the setting is an axis array.
    #[must_use]
    pub fn as_axis_values(&self) -> Option<&AxisValues> {
        match self {
            SettingValue::AxisArray(values) => Some(values),
            _ => None,
        }
    }
}

impl fmt::Display for SettingValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingValue::Float(value) => write!(f, "{value}"),
            SettingValue::Int(value) => write!(f, "{value}"),
            SettingValue::Bool(value) => f.write_str(if *value { "1" } else { "0" }),
            SettingValue::String(value) => f.write_str(value),
            SettingValue::AxisArray(values) => values.fmt(f),
        }
    }
}

impl From<f64> for SettingValue {
    fn from(value: f64) -> Self {
        SettingValue::Float(value)
    }
}

impl From<i64> for SettingValue {
    fn from(value: i64) -> Self {
        SettingValue::Int(value)
    }
}

impl From<bool> for SettingValue {
    fn from(value: bool) -> Self {
        SettingValue::Bool(value)
    }
}

impl From<String> for SettingValue {
    fn from(value: String) -> Self {
        SettingValue::String(value)
    }
}

impl From<&str> for SettingValue {
    fn from(value: &str) -> Self {
        SettingValue::String(value.to_string())
    }
}

/// A controller setting value plus optional metadata from `$SED`/`$ES`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setting {
    /// Setting number, the `<n>` in `$<n>=<value>`.
    pub number: u16,
    /// Current value.
    pub value: SettingValue,
    /// Human-readable name or description.
    pub description: Option<String>,
    /// Unit label, for example `"mm/min"`.
    pub unit: Option<String>,
    /// Group identifier or name from the controller's setting groups.
    pub group: Option<String>,
}

impl Setting {
    /// Build a setting from a number and value.
    #[must_use]
    pub fn new(number: u16, value: SettingValue) -> Self {
        Setting {
            number,
            value,
            description: None,
            unit: None,
            group: None,
        }
    }

    /// Build a setting with a description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Encode the write form, `$<n>=<value>`, without a line terminator.
    #[must_use]
    pub fn to_command(&self) -> String {
        format!("${}={}", self.number, self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scalar_values() {
        assert_eq!(SettingValue::parse("250.000"), SettingValue::Float(250.0));
        assert_eq!(SettingValue::parse("-1"), SettingValue::Int(-1));
        assert_eq!(SettingValue::parse("1"), SettingValue::Int(1));
        assert_eq!(SettingValue::parse("true"), SettingValue::Bool(true));
        assert_eq!(SettingValue::parse("FALSE"), SettingValue::Bool(false));
        assert_eq!(
            SettingValue::parse("custom string"),
            SettingValue::String("custom string".into())
        );
        assert_eq!(
            SettingValue::parse("  "),
            SettingValue::String(String::new())
        );
    }

    #[test]
    fn parse_axis_arrays() {
        let value = SettingValue::parse("250.000,250.000,250.000");
        let axes = value.as_axis_values().expect("axis array");
        assert_eq!(axes.axes.to_string(), "XYZ");
        assert_eq!(axes.len(), 3);
        assert_eq!(axes.get(Axis::Y), Some(250.0));
        assert_eq!(value.to_string(), "250,250,250");
    }

    #[test]
    fn malformed_axis_array_falls_back_to_string() {
        let value = SettingValue::parse("1.0,oops,3.0");
        assert!(matches!(value, SettingValue::String(_)));
    }

    #[test]
    fn value_accessors() {
        assert_eq!(SettingValue::Float(1.5).as_f64(), Some(1.5));
        assert_eq!(SettingValue::Int(4).as_f64(), Some(4.0));
        assert_eq!(SettingValue::Bool(true).as_f64(), Some(1.0));
        assert_eq!(SettingValue::Float(0.0).as_bool(), Some(false));
        assert_eq!(SettingValue::Int(2).as_bool(), Some(true));
        assert_eq!(SettingValue::String("x".into()).as_str(), Some("x"));
        assert_eq!(SettingValue::String("x".into()).as_f64(), None);
    }

    #[test]
    fn setting_encodes_write_command() {
        let setting =
            Setting::new(100, SettingValue::Float(250.0)).with_description("X steps per mm");
        assert_eq!(setting.to_command(), "$100=250");
        assert_eq!(setting.description.as_deref(), Some("X steps per mm"));
        assert_eq!(
            Setting::new(22, SettingValue::Bool(true)).to_command(),
            "$22=1"
        );
    }

    #[test]
    fn long_strings_are_bounded() {
        let long = "x".repeat(2000);
        let value = SettingValue::parse(&long);
        assert_eq!(value.as_str().map(str::len), Some(MAX_STRING_VALUE));
    }
}
