//! First-run safety/setup wizard model.
//!
//! This module is plain data plus the step gate. It never builds, queues, or
//! sends a controller command: the wizard records operator acknowledgement and
//! an app-side unit preference only. Persisting the completion flag and the
//! units belongs to [`crate::config::ConfigModel`]; the shell wires the model
//! to that store through [`crate::state::AppState`].
//!
//! The checklist is deliberately short and absolute: every item must be
//! acknowledged before the wizard leaves [`SetupStep::Safety`]. The review
//! step states that completion is not a machine certification.

use ghs_core::Units;

/// Number of required safety acknowledgements.
pub const SAFETY_ITEM_COUNT: usize = SafetyItem::ALL.len();

/// Wizard page shown in the setup overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupStep {
    /// Required safety checklist.
    Safety,
    /// App display-unit preference.
    Units,
    /// Informational connection guidance.
    Connection,
    /// Summary and final confirmation.
    Review,
}

impl SetupStep {
    /// Steps in display order.
    pub const ALL: [Self; 4] = [Self::Safety, Self::Units, Self::Connection, Self::Review];

    /// One-based step number shown in the header.
    #[must_use]
    pub fn number(self) -> usize {
        match self {
            Self::Safety => 1,
            Self::Units => 2,
            Self::Connection => 3,
            Self::Review => 4,
        }
    }

    /// Human-readable step name.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Safety => "SAFETY",
            Self::Units => "UNITS",
            Self::Connection => "CONNECTION",
            Self::Review => "REVIEW",
        }
    }

    /// Next step, if any.
    #[must_use]
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Safety => Some(Self::Units),
            Self::Units => Some(Self::Connection),
            Self::Connection => Some(Self::Review),
            Self::Review => None,
        }
    }

    /// Previous step, if any.
    #[must_use]
    pub fn previous(self) -> Option<Self> {
        match self {
            Self::Safety => None,
            Self::Units => Some(Self::Safety),
            Self::Connection => Some(Self::Units),
            Self::Review => Some(Self::Connection),
        }
    }
}

/// One required safety acknowledgement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafetyItem {
    /// Physical E-stop found and proven.
    EStop,
    /// Spindle can be stopped without the sender.
    SpindleStop,
    /// Physical travel and soft limits verified.
    TravelLimits,
    /// Workholding and tool clearance checked.
    Workholding,
    /// Simulator or spindle-off first connection understood.
    SimulatorFirst,
}

impl SafetyItem {
    /// Items in display order. Every one is required.
    pub const ALL: [Self; 5] = [
        Self::EStop,
        Self::SpindleStop,
        Self::TravelLimits,
        Self::Workholding,
        Self::SimulatorFirst,
    ];

    /// Short checklist title.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::EStop => "PHYSICAL E-STOP LOCATED AND TESTED",
            Self::SpindleStop => "SPINDLE CAN BE STOPPED INDEPENDENTLY",
            Self::TravelLimits => "MACHINE TRAVEL AND SOFT LIMITS VERIFIED",
            Self::Workholding => "WORKHOLDING AND TOOL CLEARANCE CHECKED",
            Self::SimulatorFirst => "SIMULATOR OR SPINDLE-OFF FIRST CONNECTION",
        }
    }

    /// One-line explanation shown under the title.
    #[must_use]
    pub fn detail(self) -> &'static str {
        match self {
            Self::EStop => {
                "Know where it is, reach it, and prove it stops the machine before any commanded move."
            }
            Self::SpindleStop => {
                "Use the VFD or the power control, not this sender, to stop the cutter."
            }
            Self::TravelLimits => {
                "Confirm the physical travel and the controller's soft-limit settings."
            }
            Self::Workholding => {
                "Secure the stock, clear the tool, and keep hands clear of every axis."
            }
            Self::SimulatorFirst => {
                "Learn the sender in the simulator, then connect with the spindle off and run only status, DRO, and small jogs."
            }
        }
    }

    /// Stable index for the acknowledgement array.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::EStop => 0,
            Self::SpindleStop => 1,
            Self::TravelLimits => 2,
            Self::Workholding => 3,
            Self::SimulatorFirst => 4,
        }
    }
}

/// First-run wizard state. Nothing here can reach the controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupModel {
    open: bool,
    step: SetupStep,
    safety: [bool; SAFETY_ITEM_COUNT],
    units: Units,
    error: Option<String>,
}

impl SetupModel {
    /// Closed model at [`SetupStep::Safety`] with nothing acknowledged.
    #[must_use]
    pub fn new(units: Units) -> Self {
        Self {
            open: false,
            step: SetupStep::Safety,
            safety: [false; SAFETY_ITEM_COUNT],
            units,
            error: None,
        }
    }

    /// Whether the setup overlay is visible.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Current wizard step.
    #[must_use]
    pub fn step(&self) -> SetupStep {
        self.step
    }

    /// Unit preference selected in the wizard.
    #[must_use]
    pub fn units(&self) -> Units {
        self.units
    }

    /// Last local save error, shown while the wizard stays open.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Open at [`SetupStep::Safety`] with a fresh, unchecked checklist.
    ///
    /// Used by the first-run prompt, the Utility `OPEN SETUP` action, and the
    /// `--demo-setup` screenshot switch. The unit preference is prefilled from
    /// the current app config by the caller.
    pub fn open(&mut self, units: Units) {
        self.open = true;
        self.step = SetupStep::Safety;
        self.safety = [false; SAFETY_ITEM_COUNT];
        self.units = units;
        self.error = None;
    }

    /// Open the review page with every item acknowledged (demo/screenshot aid).
    pub fn open_review_for_demo(&mut self, units: Units) {
        self.open = true;
        self.step = SetupStep::Review;
        self.safety = [true; SAFETY_ITEM_COUNT];
        self.units = units;
        self.error = None;
    }

    /// Open the units page with every safety item acknowledged (demo aid).
    pub fn open_units_for_demo(&mut self, units: Units) {
        self.open = true;
        self.step = SetupStep::Units;
        self.safety = [true; SAFETY_ITEM_COUNT];
        self.units = units;
        self.error = None;
    }

    /// Close without completing setup. This never changes the saved flag.
    pub fn close(&mut self) {
        self.open = false;
        self.error = None;
    }

    /// Select the app display-unit preference.
    pub fn set_units(&mut self, units: Units) {
        self.units = units;
    }

    /// Whether one safety item is acknowledged.
    #[must_use]
    pub fn safety_checked(&self, item: SafetyItem) -> bool {
        self.safety.get(item.index()).copied().unwrap_or(false)
    }

    /// Set one safety acknowledgement.
    pub fn set_safety(&mut self, item: SafetyItem, checked: bool) {
        if let Some(slot) = self.safety.get_mut(item.index()) {
            *slot = checked;
        }
    }

    /// Toggle one safety acknowledgement.
    pub fn toggle_safety(&mut self, item: SafetyItem) {
        let checked = self.safety_checked(item);
        self.set_safety(item, !checked);
    }

    /// Number of acknowledged safety items.
    #[must_use]
    pub fn acknowledged_count(&self) -> usize {
        self.safety.iter().filter(|checked| **checked).count()
    }

    /// Whether every required safety item is acknowledged.
    #[must_use]
    pub fn all_safety_checked(&self) -> bool {
        self.acknowledged_count() == SAFETY_ITEM_COUNT
    }

    /// Whether NEXT may leave the current step.
    ///
    /// Only SAFETY has a gate; the later pages are informational or a local
    /// preference.
    #[must_use]
    pub fn can_advance(&self) -> bool {
        self.step != SetupStep::Safety || self.all_safety_checked()
    }

    /// Advance one page. Returns `false` at REVIEW or while SAFETY is blocked.
    pub fn next(&mut self) -> bool {
        if !self.can_advance() {
            return false;
        }
        match self.step.next() {
            Some(step) => {
                self.step = step;
                self.error = None;
                true
            }
            None => false,
        }
    }

    /// Go back one page, preserving acknowledgements and the unit choice.
    pub fn back(&mut self) -> bool {
        match self.step.previous() {
            Some(step) => {
                self.step = step;
                self.error = None;
                true
            }
            None => false,
        }
    }

    /// Record a local save error while keeping the wizard open.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_model_is_closed_at_safety_with_nothing_checked() {
        let model = SetupModel::new(Units::Mm);
        assert!(!model.is_open());
        assert_eq!(model.step(), SetupStep::Safety);
        assert_eq!(model.acknowledged_count(), 0);
        assert!(!model.all_safety_checked());
        assert!(!model.can_advance());
        assert_eq!(model.units(), Units::Mm);
        assert_eq!(model.error(), None);
    }

    #[test]
    fn safety_blocks_advancing_until_every_item_is_checked_and_unchecking_blocks_again() {
        let mut model = SetupModel::new(Units::Mm);
        model.open(Units::Mm);

        for item in SafetyItem::ALL.iter().copied() {
            assert!(!model.next(), "advanced with {item:?} unchecked");
            assert_eq!(model.step(), SetupStep::Safety);
            model.set_safety(item, true);
        }
        assert!(model.next());
        assert_eq!(model.step(), SetupStep::Units);

        assert!(model.back());
        model.set_safety(SafetyItem::Workholding, false);
        assert!(!model.can_advance());
        assert!(!model.next());
        assert_eq!(model.step(), SetupStep::Safety);

        model.set_safety(SafetyItem::Workholding, true);
        assert!(model.next());
        assert_eq!(model.step(), SetupStep::Units);
    }

    #[test]
    fn back_navigation_preserves_acknowledgements_and_units() {
        let mut model = SetupModel::new(Units::Mm);
        model.open(Units::Mm);
        for item in SafetyItem::ALL {
            model.set_safety(item, true);
        }
        assert!(model.next());
        model.set_units(Units::Inch);
        assert!(model.next());
        assert_eq!(model.step(), SetupStep::Connection);

        assert!(model.back());
        assert_eq!(model.step(), SetupStep::Units);
        assert_eq!(model.units(), Units::Inch);
        assert!(model.back());
        assert_eq!(model.step(), SetupStep::Safety);
        assert!(model.all_safety_checked());
        assert_eq!(model.acknowledged_count(), SAFETY_ITEM_COUNT);
        assert_eq!(model.units(), Units::Inch);
    }

    #[test]
    fn units_round_trip_through_the_model() {
        let mut model = SetupModel::new(Units::Mm);
        model.open(Units::Inch);
        assert_eq!(model.units(), Units::Inch);
        model.set_units(Units::Mm);
        assert_eq!(model.units(), Units::Mm);
        model.set_units(Units::Inch);
        assert_eq!(model.units(), Units::Inch);
    }

    #[test]
    fn reopening_resets_the_checklist_but_keeps_the_supplied_unit_preference() {
        let mut model = SetupModel::new(Units::Mm);
        model.open(Units::Inch);
        for item in SafetyItem::ALL {
            model.set_safety(item, true);
        }
        assert!(model.next());
        model.set_error("not saved");

        model.open(Units::Mm);
        assert!(model.is_open());
        assert_eq!(model.step(), SetupStep::Safety);
        assert_eq!(model.acknowledged_count(), 0);
        assert_eq!(model.units(), Units::Mm);
        assert_eq!(model.error(), None);
    }

    #[test]
    fn review_helpers_produce_reproducible_demo_states() {
        let mut review = SetupModel::new(Units::Mm);
        review.open_review_for_demo(Units::Inch);
        assert!(review.is_open());
        assert_eq!(review.step(), SetupStep::Review);
        assert_eq!(review.acknowledged_count(), SAFETY_ITEM_COUNT);
        assert_eq!(review.units(), Units::Inch);

        let mut units = SetupModel::new(Units::Mm);
        units.open_units_for_demo(Units::Inch);
        assert_eq!(units.step(), SetupStep::Units);
        assert!(units.all_safety_checked());

        units.open_review_for_demo(Units::Inch);
        assert_eq!(units.step(), SetupStep::Review);
        assert_eq!(units.acknowledged_count(), SAFETY_ITEM_COUNT);
    }
}
