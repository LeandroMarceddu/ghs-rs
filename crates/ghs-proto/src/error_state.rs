//! grblHAL's sticky error state.
//!
//! `error:<n>` rejects one line, but grblHAL keeps the error active for
//! subsequent G-code until a reset, an empty line or a `$` command clears it.
//! The engine must stop sending program lines while an error is latched and
//! clear it deliberately, otherwise every following line fails.

/// Latch for the controller's sticky error state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ErrorState {
    code: Option<u16>,
}

impl ErrorState {
    /// Create an empty latch.
    #[must_use]
    pub const fn new() -> Self {
        ErrorState { code: None }
    }

    /// Returns `true` when an error is latched.
    #[must_use]
    pub const fn is_set(&self) -> bool {
        self.code.is_some()
    }

    /// Latched error code, if any.
    #[must_use]
    pub const fn code(&self) -> Option<u16> {
        self.code
    }

    /// Latch an error code. Code 0 means `ok` and clears the latch.
    pub fn record(&mut self, code: u16) {
        self.code = if code == 0 { None } else { Some(code) };
    }

    /// Clear the latch.
    pub fn clear(&mut self) {
        self.code = None;
    }

    /// Returns `true` when sending `line` clears the controller's error state.
    ///
    /// grblHAL clears the sticky error on an empty line or on a `$` command.
    #[must_use]
    pub fn clears_error(line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.is_empty() || trimmed.starts_with('$')
    }

    /// Observe an outgoing line and clear the latch when the line clears the
    /// controller error.
    pub fn observe_outgoing(&mut self, line: &str) {
        if Self::clears_error(line) {
            self.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latches_and_clears() {
        let mut state = ErrorState::new();
        assert!(!state.is_set());
        state.record(20);
        assert!(state.is_set());
        assert_eq!(state.code(), Some(20));
        state.record(0);
        assert!(!state.is_set());
        state.record(3);
        state.clear();
        assert!(!state.is_set());
    }

    #[test]
    fn clearing_lines() {
        assert!(ErrorState::clears_error(""));
        assert!(ErrorState::clears_error("   "));
        assert!(ErrorState::clears_error("$X"));
        assert!(ErrorState::clears_error(" $H"));
        assert!(!ErrorState::clears_error("G0 X0"));
        assert!(!ErrorState::clears_error("M3"));

        let mut state = ErrorState::new();
        state.record(20);
        state.observe_outgoing("G0 X0");
        assert!(state.is_set());
        state.observe_outgoing("$X");
        assert!(!state.is_set());
    }
}
