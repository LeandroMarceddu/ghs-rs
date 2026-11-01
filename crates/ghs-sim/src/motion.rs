//! Deterministic linear motion used by the simulator.
//!
//! The engine models one block at a time. A block interpolates linearly from
//! its start to its target; arcs are reduced to their endpoint because the
//! simulator reports positions, not step pulses. Feed rate and duration are in
//! millimetres and millimetres per minute.

use std::time::Duration;

use ghs_core::{Axes, Axis, Position};

/// Longest simulated block, as a guard against near-zero feed rates.
const MAX_MOTION_SECONDS: f64 = 86_400.0;

/// What started a motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MotionKind {
    /// `G0` rapid.
    Rapid,
    /// `G1`/`G2`/`G3` feed move.
    Feed,
    /// `$J=` jog.
    Jog,
    /// `G38.x` probing move.
    Probe,
}

/// A single in-flight linear block.
#[derive(Debug, Clone)]
pub(crate) struct Motion {
    start: Position,
    target: Position,
    axes: Axes,
    kind: MotionKind,
    feed_mm_min: f64,
    elapsed: Duration,
    duration: Duration,
}

impl Motion {
    /// Create a motion block.
    ///
    /// A zero duration means the block is already complete.
    #[must_use]
    pub(crate) fn new(
        start: Position,
        target: Position,
        axes: Axes,
        kind: MotionKind,
        feed_mm_min: f64,
        duration: Duration,
    ) -> Self {
        Motion {
            start,
            target,
            axes,
            kind,
            feed_mm_min,
            elapsed: Duration::ZERO,
            duration,
        }
    }

    /// Advance the block and return `true` when it finished.
    pub(crate) fn advance(&mut self, elapsed: Duration) -> bool {
        self.elapsed = self.elapsed.saturating_add(elapsed).min(self.duration);
        self.is_finished()
    }

    /// Returns `true` when the block reached its target.
    #[must_use]
    pub(crate) fn is_finished(&self) -> bool {
        self.elapsed >= self.duration
    }

    /// Interpolated position for the elapsed time.
    #[must_use]
    pub(crate) fn position(&self) -> Position {
        let fraction = if self.duration.is_zero() {
            1.0
        } else {
            (self.elapsed.as_secs_f64() / self.duration.as_secs_f64()).clamp(0.0, 1.0)
        };
        let mut position = self.start;
        for axis in self.axes.iter() {
            let delta = self.target.get(axis) - self.start.get(axis);
            position.set(axis, self.start.get(axis) + delta * fraction);
        }
        position
    }

    /// Block target in machine coordinates.
    #[must_use]
    pub(crate) fn target(&self) -> Position {
        self.target
    }

    /// What started this block.
    #[must_use]
    pub(crate) fn kind(&self) -> MotionKind {
        self.kind
    }

    /// Programmed feed rate in millimetres per minute.
    #[must_use]
    pub(crate) fn feed_mm_min(&self) -> f64 {
        self.feed_mm_min
    }
}

/// Duration of a feed move at a constant feed rate.
#[must_use]
pub(crate) fn duration_for_feed(start: Position, target: Position, feed_mm_min: f64) -> Duration {
    if feed_mm_min <= 0.0 {
        return Duration::ZERO;
    }
    let distance = start.distance_to(target);
    let seconds = (distance / (feed_mm_min / 60.0)).clamp(0.0, MAX_MOTION_SECONDS);
    Duration::from_secs_f64(seconds)
}

/// Duration of a rapid move, limited by the slowest moving axis.
///
/// `max_rate_mm_min` returns each axis' configured maximum rate.
#[must_use]
pub(crate) fn duration_for_rapid<F>(
    start: Position,
    target: Position,
    axes: Axes,
    max_rate_mm_min: F,
) -> Duration
where
    F: Fn(Axis) -> f64,
{
    let mut seconds = 0.0_f64;
    for axis in axes.iter() {
        let delta = (target.get(axis) - start.get(axis)).abs();
        let rate = max_rate_mm_min(axis).max(1.0);
        seconds = seconds.max(delta / (rate / 60.0));
    }
    Duration::from_secs_f64(seconds.clamp(0.0, MAX_MOTION_SECONDS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_motion_interpolates_across_ticks() {
        let start = Position::xyz(0.0, 0.0, 0.0);
        let target = Position::xyz(10.0, 0.0, 0.0);
        let mut motion = Motion::new(
            start,
            target,
            Axes::xyz(),
            MotionKind::Feed,
            600.0,
            Duration::from_secs(1),
        );
        assert!(!motion.advance(Duration::from_millis(250)));
        assert!((motion.position().get(Axis::X) - 2.5).abs() < 1e-9);
        assert!(motion.advance(Duration::from_secs(1)));
        assert_eq!(motion.position(), target);
        assert!(motion.is_finished());
    }

    #[test]
    fn zero_duration_is_complete() {
        let motion = Motion::new(
            Position::zero(),
            Position::zero(),
            Axes::xyz(),
            MotionKind::Rapid,
            0.0,
            Duration::ZERO,
        );
        assert!(motion.is_finished());
        assert_eq!(motion.position(), Position::zero());
    }

    #[test]
    fn rapid_duration_uses_the_slowest_axis() {
        let start = Position::zero();
        let target = Position::xyz(10.0, 20.0, 0.0);
        let duration = duration_for_rapid(start, target, Axes::xyz(), |axis| match axis {
            Axis::X => 6000.0,
            Axis::Y => 3000.0,
            _ => 6000.0,
        });
        assert!((duration.as_secs_f64() - 0.4).abs() < 1e-9);
    }

    #[test]
    fn feed_duration_is_distance_over_feed() {
        let duration = duration_for_feed(Position::zero(), Position::xyz(0.0, 3.0, 4.0), 300.0);
        assert!((duration.as_secs_f64() - 1.0).abs() < 1e-9);
        assert_eq!(
            duration_for_feed(Position::zero(), Position::xyz(1.0, 0.0, 0.0), 0.0),
            Duration::ZERO
        );
    }
}
