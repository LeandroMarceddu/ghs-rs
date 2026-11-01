//! Tool table entries.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::position::Position;

/// One tool table entry.
///
/// The reference FlexiHAL build has no controller-side tool table, so the
/// application owns the table; the type is still modelled from the grblHAL
/// `$#` tool-report format so other controllers work unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolEntry {
    /// Tool number, the `T` word value.
    pub number: u32,
    /// Tool radius in millimetres, when reported.
    pub radius_mm: Option<f64>,
    /// Tool offset in millimetres (usually X, Y and Z).
    pub offset: Position,
    /// Optional tool name.
    pub name: Option<String>,
    /// Optional pocket number.
    pub pocket: Option<u32>,
}

impl Default for ToolEntry {
    fn default() -> Self {
        ToolEntry {
            number: 0,
            radius_mm: None,
            offset: Position::zero(),
            name: None,
            pocket: None,
        }
    }
}

impl ToolEntry {
    /// Build a tool entry from a tool number and offset.
    #[must_use]
    pub fn new(number: u32, offset: Position) -> Self {
        ToolEntry {
            number,
            offset,
            ..ToolEntry::default()
        }
    }

    /// Build a tool entry with a radius.
    #[must_use]
    pub fn with_radius(mut self, radius_mm: f64) -> Self {
        self.radius_mm = Some(radius_mm);
        self
    }

    /// Tool diameter in millimetres, when a radius is known.
    #[must_use]
    pub fn diameter_mm(&self) -> Option<f64> {
        self.radius_mm.map(|radius| radius * 2.0)
    }
}

impl fmt::Display for ToolEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "T{}", self.number)?;
        if let Some(radius) = self.radius_mm {
            write!(f, " D{radius:.3}")?;
        }
        for (axis, value) in self.offset.iter() {
            write!(f, " {axis}{value:.3}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_entry_display_and_diameter() {
        let entry = ToolEntry::new(3, Position::xyz(10.0, 20.0, -5.0)).with_radius(3.175);
        assert_eq!(entry.diameter_mm(), Some(6.35));
        assert_eq!(entry.to_string(), "T3 D3.175 X10.000 Y20.000 Z-5.000");
        let plain = ToolEntry::new(0, Position::zero());
        assert_eq!(plain.to_string(), "T0 X0.000 Y0.000 Z0.000");
        assert_eq!(ToolEntry::default(), plain);
    }
}
