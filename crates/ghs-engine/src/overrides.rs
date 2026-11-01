//! Absolute override targets translated into grblHAL real-time increments.
//!
//! grblHAL has no "set override to 37 %" command. The gateway composes the
//! documented coarse (10 %), fine (1 %) and reset commands from the last
//! reported override.

use ghs_proto::RealtimeCommand;

/// Build the feed-override command sequence that moves from `current` to
/// `target`.
///
/// Both values are percentages and callers validate the `10..=200` range.
#[must_use]
pub(crate) fn feed_override_commands(current: u16, target: u16) -> Vec<RealtimeCommand> {
    override_commands(
        current,
        target,
        RealtimeCommand::FeedOverrideReset,
        RealtimeCommand::FeedOverrideCoarsePlus,
        RealtimeCommand::FeedOverrideCoarseMinus,
        RealtimeCommand::FeedOverrideFinePlus,
        RealtimeCommand::FeedOverrideFineMinus,
    )
}

/// Build the spindle-override command sequence that moves from `current` to
/// `target`.
#[must_use]
pub(crate) fn spindle_override_commands(current: u16, target: u16) -> Vec<RealtimeCommand> {
    override_commands(
        current,
        target,
        RealtimeCommand::SpindleOverrideReset,
        RealtimeCommand::SpindleOverrideCoarsePlus,
        RealtimeCommand::SpindleOverrideCoarseMinus,
        RealtimeCommand::SpindleOverrideFinePlus,
        RealtimeCommand::SpindleOverrideFineMinus,
    )
}

/// Map an absolute rapid override to its single real-time command.
///
/// Only the four documented levels are accepted.
#[must_use]
pub(crate) fn rapid_override_command(target: u16) -> Option<RealtimeCommand> {
    match target {
        100 => Some(RealtimeCommand::RapidOverrideReset),
        50 => Some(RealtimeCommand::RapidOverrideMedium),
        25 => Some(RealtimeCommand::RapidOverrideLow),
        5 => Some(RealtimeCommand::RapidOverrideExtraLow),
        _ => None,
    }
}

fn override_commands(
    current: u16,
    target: u16,
    reset: RealtimeCommand,
    coarse_plus: RealtimeCommand,
    coarse_minus: RealtimeCommand,
    fine_plus: RealtimeCommand,
    fine_minus: RealtimeCommand,
) -> Vec<RealtimeCommand> {
    if current == target {
        return Vec::new();
    }
    if target == 100 {
        return vec![reset];
    }

    let difference = i32::from(target) - i32::from(current);
    let coarse_steps = difference / 10;
    let fine_steps = difference % 10;
    let mut commands = Vec::with_capacity(coarse_steps.unsigned_abs() as usize + 9);

    if coarse_steps > 0 {
        commands.extend(std::iter::repeat_n(coarse_plus, coarse_steps as usize));
    } else {
        commands.extend(std::iter::repeat_n(
            coarse_minus,
            coarse_steps.unsigned_abs() as usize,
        ));
    }

    if fine_steps > 0 {
        commands.extend(std::iter::repeat_n(fine_plus, fine_steps as usize));
    } else if fine_steps < 0 {
        commands.extend(std::iter::repeat_n(
            fine_minus,
            fine_steps.unsigned_abs() as usize,
        ));
    }

    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_override_uses_coarse_and_fine_steps() {
        assert!(feed_override_commands(100, 100).is_empty());
        assert_eq!(
            feed_override_commands(100, 120),
            vec![
                RealtimeCommand::FeedOverrideCoarsePlus,
                RealtimeCommand::FeedOverrideCoarsePlus
            ]
        );
        assert_eq!(
            feed_override_commands(120, 119),
            vec![RealtimeCommand::FeedOverrideFineMinus]
        );
        assert_eq!(
            feed_override_commands(100, 95),
            vec![RealtimeCommand::FeedOverrideFineMinus; 5]
        );
        assert_eq!(
            feed_override_commands(100, 100),
            Vec::<RealtimeCommand>::new()
        );
        assert_eq!(
            feed_override_commands(150, 100),
            vec![RealtimeCommand::FeedOverrideReset]
        );
        assert_eq!(
            feed_override_commands(100, 47),
            vec![
                RealtimeCommand::FeedOverrideCoarseMinus,
                RealtimeCommand::FeedOverrideCoarseMinus,
                RealtimeCommand::FeedOverrideCoarseMinus,
                RealtimeCommand::FeedOverrideCoarseMinus,
                RealtimeCommand::FeedOverrideCoarseMinus,
                RealtimeCommand::FeedOverrideFineMinus,
                RealtimeCommand::FeedOverrideFineMinus,
                RealtimeCommand::FeedOverrideFineMinus,
            ]
        );
    }

    #[test]
    fn spindle_override_uses_its_own_bytes() {
        assert_eq!(
            spindle_override_commands(100, 90),
            vec![RealtimeCommand::SpindleOverrideCoarseMinus]
        );
        assert_eq!(
            spindle_override_commands(100, 101),
            vec![RealtimeCommand::SpindleOverrideFinePlus]
        );
        assert_eq!(
            spindle_override_commands(80, 100),
            vec![RealtimeCommand::SpindleOverrideReset]
        );
    }

    #[test]
    fn rapid_override_only_accepts_documented_levels() {
        assert_eq!(
            rapid_override_command(100),
            Some(RealtimeCommand::RapidOverrideReset)
        );
        assert_eq!(
            rapid_override_command(50),
            Some(RealtimeCommand::RapidOverrideMedium)
        );
        assert_eq!(
            rapid_override_command(25),
            Some(RealtimeCommand::RapidOverrideLow)
        );
        assert_eq!(
            rapid_override_command(5),
            Some(RealtimeCommand::RapidOverrideExtraLow)
        );
        for target in [0, 10, 30, 75, 99, 101] {
            assert_eq!(rapid_override_command(target), None, "{target}");
        }
    }
}
