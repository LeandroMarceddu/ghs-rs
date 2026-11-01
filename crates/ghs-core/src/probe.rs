//! Probe results reported by `[PRB:...]` and the `PRB:` status field.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::position::Position;
use crate::wcs::Wcs;

/// Result of the most recent probe cycle.
///
/// grblHAL reports the probed position in machine coordinates and a success
/// flag. The optional [`ProbeResult::wcs`] is present when a controller also
/// reports the coordinate system the position refers to.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ProbeResult {
    /// Whether the probe cycle succeeded.
    pub success: bool,
    /// Probed position in millimetres.
    pub position: Position,
    /// Coordinate system the position is expressed in, when reported.
    pub wcs: Option<Wcs>,
}

impl ProbeResult {
    /// Build a probe result.
    #[must_use]
    pub fn new(success: bool, position: Position) -> Self {
        ProbeResult {
            success,
            position,
            wcs: None,
        }
    }

    /// Build a probe result expressed in a work coordinate system.
    #[must_use]
    pub fn in_wcs(success: bool, position: Position, wcs: Wcs) -> Self {
        ProbeResult {
            success,
            position,
            wcs: Some(wcs),
        }
    }
}

impl fmt::Display for ProbeResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PRB:{}:{}", self.position, u8::from(self.success))?;
        if let Some(wcs) = self.wcs {
            write!(f, ":{wcs}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_result_display() {
        let result = ProbeResult::new(true, Position::xyz(1.0, -2.0, 3.0));
        assert!(result.success);
        assert_eq!(result.to_string(), "PRB:X1.000 Y-2.000 Z3.000:1");
        let failed = ProbeResult::new(false, Position::zero());
        assert!(!failed.success);
        assert_eq!(failed.to_string(), "PRB:X0.000 Y0.000 Z0.000:0");
        let work = ProbeResult::in_wcs(true, Position::zero(), Wcs::G55);
        assert_eq!(work.wcs, Some(Wcs::G55));
    }
}
