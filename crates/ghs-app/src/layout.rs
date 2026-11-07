//! Pure viewport layout decisions for the touch shell.
//!
//! The thresholds are expressed in logical pixels and keyed by both width and
//! height, never by a device name:
//!
//! - **Wide landscape** (`>= 1120 px` wide, not portrait): the existing three
//!   columns (left Console/Program, centre workspace, right auxiliary rail).
//! - **Compact landscape** (`< 1120 px` wide, not portrait): the existing two
//!   columns with the right rail collapsed. This preserves 1024x600.
//! - **Portrait** (height at least `1.15 x` width): one vertically scrolling
//!   column. The workspace comes first, then DRO/Jog/MDI/Feed, the
//!   Console/Program card, and finally the auxiliary rail. The top bar becomes
//!   two rows so all six workspace buttons and RESET/STOP stay reachable
//!   without horizontal panning.
//!
//! The module is deliberately free of GPUI (and of every other dependency) so
//! the decisions can be unit-tested without a window.

/// Shared gap between cards and columns, in logical pixels.
pub const GAP: f32 = 12.0;

/// Width at which the landscape layout gains its third (right) column.
pub const WIDE_MIN_WIDTH: f32 = 1120.0;

/// Width at which the landscape left column widens for large desktops.
pub const WIDE_LEFT_MIN_WIDTH: f32 = 1600.0;

/// Height/width ratio at or above which the shell reflows to one column.
///
/// 1080x1920 (1.78) is portrait; 1024x600 (0.59) and 1280x800 (0.63) are not.
pub const PORTRAIT_ASPECT: f32 = 1.15;

/// Primary touch target, in logical pixels.
pub const TOUCH_TARGET: f32 = 48.0;

/// One 56 px row for landscape windows.
pub const LANDSCAPE_TOP_BAR_HEIGHT: f32 = 56.0;

/// Two 48 px action rows plus 4 px padding and one 4 px gap.
pub const PORTRAIT_TOP_BAR_HEIGHT: f32 = 2.0 * TOUCH_TARGET + 2.0 * 4.0 + 4.0;

/// Smallest width the portrait action row can use before content is clipped.
///
/// Identity (56) + theme (48) + connect (96) + RESET (84) + STOP (84) +
/// six 8 px gaps + 8 px horizontal padding = 432 px. The status pill flexes and
/// truncates, so 512 px keeps a useful status label as well.
pub const PORTRAIT_ACTION_ROW_MIN_WIDTH: f32 = 512.0;

/// A vertically scrolling overlay is used below this available height.
///
/// 1280x800 has 744 px below the top bar and keeps centred sheets; 1024x600
/// has 544 px, where tall 48 px-driven sheets need the scroller.
pub const OVERLAY_SCROLL_MIN_HEIGHT: f32 = 740.0;

/// Minimum centre workspace width assumed by the three- and two-column layouts.
pub const MIN_WORKSPACE_WIDTH: f32 = 320.0;

/// Height bounds for the stacked workspace on a portrait viewport.
pub const PORTRAIT_VIEWER_MIN_HEIGHT: f32 = 430.0;
pub const PORTRAIT_VIEWER_MAX_HEIGHT: f32 = 900.0;
/// Fraction of the viewport height reserved for the workspace in portrait.
pub const PORTRAIT_VIEWER_FRACTION: f32 = 0.45;

/// Height bounds for the Console/Program card in portrait.
pub const PORTRAIT_CONSOLE_MIN_HEIGHT: f32 = 360.0;
pub const PORTRAIT_CONSOLE_MAX_HEIGHT: f32 = 640.0;
pub const PORTRAIT_CONSOLE_FRACTION: f32 = 0.28;

/// Height bounds for the auxiliary rail (Coolant/Macros/Signals/Work) in portrait.
pub const PORTRAIT_AUX_MIN_HEIGHT: f32 = 520.0;
pub const PORTRAIT_AUX_MAX_HEIGHT: f32 = 720.0;
pub const PORTRAIT_AUX_FRACTION: f32 = 0.32;

/// Layout family selected for one viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutClass {
    /// Landscape at or above [`WIDE_MIN_WIDTH`]; three columns.
    Wide,
    /// Landscape below [`WIDE_MIN_WIDTH`]; two columns, right rail collapsed.
    Compact,
    /// Tall viewport; one scrolling column and a two-row top bar.
    Portrait,
}

/// Column arrangement for the workspace content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnLayout {
    /// Left rail, centre workspace, right rail.
    Three,
    /// Left rail and centre workspace; right rail is not shown.
    Two,
    /// One full-width scrolling stack.
    One,
}

/// Top-bar arrangement for the current viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopBarLayout {
    /// One 56 px row (landscape).
    SingleRow,
    /// Two rows: actions and status above, all six workspaces below.
    TwoRows,
}

/// Placement for modal sheets inside the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayPlacement {
    /// Sheet is centred in the area below the top bar.
    Centered,
    /// Sheet starts at the top of a vertically scrolling area.
    Scroll,
}

/// Resolved layout for one viewport.
///
/// Construct with [`ShellLayout::for_viewport`]; every field is derived from
/// the width and height passed in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellLayout {
    /// Viewport width in logical pixels (at least 1.0).
    pub width: f32,
    /// Viewport height in logical pixels (at least 1.0).
    pub height: f32,
    /// Selected layout family.
    pub class: LayoutClass,
    /// Column arrangement for the workspace.
    pub columns: ColumnLayout,
    /// Top-bar arrangement.
    pub top_bar: TopBarLayout,
    /// Left column width for [`ColumnLayout::Three`] and
    /// [`ColumnLayout::Two`]. Unused for one column.
    pub left_width: f32,
    /// Right column width for [`ColumnLayout::Three`]. Unused otherwise.
    pub right_width: f32,
    /// Fixed workspace height for [`ColumnLayout::One`].
    pub viewer_height: f32,
    /// Fixed Console/Program height for [`ColumnLayout::One`].
    pub console_height: f32,
    /// Fixed auxiliary rail height for [`ColumnLayout::One`].
    pub aux_height: f32,
    /// Whether the workspace content lives in a vertical scroller.
    pub content_scroll: bool,
    /// How modal sheets are placed below the top bar.
    pub overlay: OverlayPlacement,
    /// Top-bar height used as the overlay offset.
    pub top_bar_height: f32,
}

impl ShellLayout {
    /// Resolve the layout for a viewport measured in logical pixels.
    #[must_use]
    pub fn for_viewport(width: f32, height: f32) -> Self {
        let width = width.max(1.0);
        let height = height.max(1.0);
        let portrait = height >= width * PORTRAIT_ASPECT;
        let class = if portrait {
            LayoutClass::Portrait
        } else if width >= WIDE_MIN_WIDTH {
            LayoutClass::Wide
        } else {
            LayoutClass::Compact
        };
        let columns = match class {
            LayoutClass::Wide => ColumnLayout::Three,
            LayoutClass::Compact => ColumnLayout::Two,
            LayoutClass::Portrait => ColumnLayout::One,
        };
        let (left_width, right_width) = if width >= WIDE_LEFT_MIN_WIDTH {
            (400.0, 260.0)
        } else {
            (340.0, 220.0)
        };
        let top_bar = if portrait {
            TopBarLayout::TwoRows
        } else {
            TopBarLayout::SingleRow
        };
        let top_bar_height = if portrait {
            PORTRAIT_TOP_BAR_HEIGHT
        } else {
            LANDSCAPE_TOP_BAR_HEIGHT
        };
        let available_overlay_height = (height - top_bar_height).max(0.0);
        let overlay = if available_overlay_height < OVERLAY_SCROLL_MIN_HEIGHT {
            OverlayPlacement::Scroll
        } else {
            OverlayPlacement::Centered
        };

        Self {
            width,
            height,
            class,
            columns,
            top_bar,
            left_width,
            right_width,
            viewer_height: (height * PORTRAIT_VIEWER_FRACTION)
                .clamp(PORTRAIT_VIEWER_MIN_HEIGHT, PORTRAIT_VIEWER_MAX_HEIGHT),
            console_height: (height * PORTRAIT_CONSOLE_FRACTION)
                .clamp(PORTRAIT_CONSOLE_MIN_HEIGHT, PORTRAIT_CONSOLE_MAX_HEIGHT),
            aux_height: (height * PORTRAIT_AUX_FRACTION)
                .clamp(PORTRAIT_AUX_MIN_HEIGHT, PORTRAIT_AUX_MAX_HEIGHT),
            content_scroll: portrait,
            overlay,
            top_bar_height,
        }
    }

    /// Width available to workspace content after the outer page padding.
    #[must_use]
    pub fn content_width(&self) -> f32 {
        (self.width - 2.0 * GAP).max(0.0)
    }

    /// Top offset used by every modal sheet.
    #[must_use]
    pub fn overlay_top(&self) -> f32 {
        self.top_bar_height
    }

    /// Whether the overlay layer should scroll vertically.
    #[must_use]
    pub fn overlay_scroll(&self) -> bool {
        self.overlay == OverlayPlacement::Scroll
    }

    /// Width of one workspace button in the two-row mode strip.
    ///
    /// The strip is split into six equal cells with 4 px gaps and 8 px side
    /// padding. Values below [`TOUCH_TARGET`] mean at least one workspace
    /// button would be too small to tap.
    #[must_use]
    pub fn mode_slot_width(&self) -> f32 {
        (((self.width - 16.0) - 5.0 * 4.0) / 6.0).max(0.0)
    }

    /// Whether a portrait action row can show identity, status, theme,
    /// connect, RESET, and STOP without horizontal panning.
    #[must_use]
    pub fn action_row_fits(&self) -> bool {
        self.width >= PORTRAIT_ACTION_ROW_MIN_WIDTH
    }

    /// Minimum centre width the current column arrangement leaves after the
    /// fixed rails and gaps.
    #[must_use]
    pub fn centre_width(&self) -> f32 {
        let rails = match self.columns {
            ColumnLayout::Three => self.left_width + self.right_width + 2.0 * GAP,
            ColumnLayout::Two => self.left_width + GAP,
            ColumnLayout::One => 0.0,
        };
        (self.content_width() - rails).max(0.0)
    }

    /// True when the centre workspace keeps at least [`MIN_WORKSPACE_WIDTH`].
    #[must_use]
    pub fn fits_without_horizontal_overflow(&self) -> bool {
        self.centre_width() >= MIN_WORKSPACE_WIDTH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portrait_1080x1920_reflows_to_one_scrolling_column() {
        let layout = ShellLayout::for_viewport(1080.0, 1920.0);
        assert_eq!(layout.class, LayoutClass::Portrait);
        assert_eq!(layout.columns, ColumnLayout::One);
        assert_eq!(layout.top_bar, TopBarLayout::TwoRows);
        assert_eq!(layout.top_bar_height, PORTRAIT_TOP_BAR_HEIGHT);
        assert!(layout.content_scroll);
        assert_eq!(layout.viewer_height, 864.0);
        assert!(layout.height - layout.top_bar_height > 1700.0);
    }

    #[test]
    fn landscape_1280x800_keeps_three_columns_and_single_row() {
        let layout = ShellLayout::for_viewport(1280.0, 800.0);
        assert_eq!(layout.class, LayoutClass::Wide);
        assert_eq!(layout.columns, ColumnLayout::Three);
        assert_eq!(layout.top_bar, TopBarLayout::SingleRow);
        assert_eq!(layout.top_bar_height, LANDSCAPE_TOP_BAR_HEIGHT);
        assert_eq!(layout.left_width, 340.0);
        assert_eq!(layout.right_width, 220.0);
        assert!(!layout.content_scroll);
        assert_eq!(layout.overlay, OverlayPlacement::Centered);
    }

    #[test]
    fn landscape_1920x1080_keeps_three_columns_and_widens_left_rail() {
        let layout = ShellLayout::for_viewport(1920.0, 1080.0);
        assert_eq!(layout.class, LayoutClass::Wide);
        assert_eq!(layout.columns, ColumnLayout::Three);
        assert_eq!(layout.left_width, 400.0);
        assert_eq!(layout.right_width, 260.0);
        assert!(!layout.content_scroll);
        assert!(layout.fits_without_horizontal_overflow());
    }

    #[test]
    fn compact_1024x600_keeps_two_columns_without_right_rail() {
        let layout = ShellLayout::for_viewport(1024.0, 600.0);
        assert_eq!(layout.class, LayoutClass::Compact);
        assert_eq!(layout.columns, ColumnLayout::Two);
        assert_eq!(layout.top_bar, TopBarLayout::SingleRow);
        assert!(!layout.content_scroll);
        assert!(layout.centre_width() >= MIN_WORKSPACE_WIDTH);
        // The right rail is collapsed, matching the pre-portrait behaviour.
        assert_eq!(layout.right_width, 220.0);
    }

    #[test]
    fn portrait_threshold_is_aspect_based_not_a_device_name() {
        for (width, height, expected) in [
            (1080.0, 1920.0, LayoutClass::Portrait),
            (1280.0, 1920.0, LayoutClass::Portrait),
            (900.0, 1200.0, LayoutClass::Portrait),
            (1024.0, 1200.0, LayoutClass::Portrait),
            (1080.0, 1200.0, LayoutClass::Compact),
            (1200.0, 900.0, LayoutClass::Wide),
            (1920.0, 1080.0, LayoutClass::Wide),
            (1024.0, 600.0, LayoutClass::Compact),
        ] {
            assert_eq!(
                ShellLayout::for_viewport(width, height).class,
                expected,
                "{width}x{height}"
            );
        }
        // Width boundary: 1119 is compact, 1120 is wide.
        assert_eq!(
            ShellLayout::for_viewport(1119.0, 800.0).class,
            LayoutClass::Compact
        );
        assert_eq!(
            ShellLayout::for_viewport(1120.0, 800.0).class,
            LayoutClass::Wide
        );
    }

    #[test]
    fn portrait_top_bar_keeps_reset_stop_and_six_workspace_slots() {
        for width in [1024.0, 1080.0] {
            let layout = ShellLayout::for_viewport(width, 1920.0);
            assert_eq!(layout.top_bar, TopBarLayout::TwoRows);
            assert!(layout.action_row_fits(), "action row at {width}");
            assert!(
                layout.mode_slot_width() >= TOUCH_TARGET,
                "mode slot at {width}: {}",
                layout.mode_slot_width()
            );
        }
        assert!(!ShellLayout::for_viewport(480.0, 900.0).action_row_fits());
    }

    #[test]
    fn overlay_scrolls_only_on_short_and_narrow_viewports() {
        assert_eq!(
            ShellLayout::for_viewport(1024.0, 600.0).overlay,
            OverlayPlacement::Scroll
        );
        assert_eq!(
            ShellLayout::for_viewport(1280.0, 800.0).overlay,
            OverlayPlacement::Centered
        );
        assert_eq!(
            ShellLayout::for_viewport(1080.0, 1920.0).overlay,
            OverlayPlacement::Centered
        );
        assert_eq!(
            ShellLayout::for_viewport(1024.0, 600.0).overlay_top(),
            LANDSCAPE_TOP_BAR_HEIGHT
        );
        assert_eq!(
            ShellLayout::for_viewport(1080.0, 1920.0).overlay_top(),
            PORTRAIT_TOP_BAR_HEIGHT
        );
    }

    #[test]
    fn every_layout_class_leaves_a_usable_centre_without_overflow() {
        for (width, height) in [
            (1024.0, 600.0),
            (1080.0, 1920.0),
            (1280.0, 800.0),
            (1920.0, 1080.0),
        ] {
            let layout = ShellLayout::for_viewport(width, height);
            assert!(
                layout.fits_without_horizontal_overflow(),
                "{width}x{height}: centre {}",
                layout.centre_width()
            );
            assert!(layout.content_width() > 0.0);
            if layout.columns == ColumnLayout::One {
                assert!((layout.content_width() - (width - 2.0 * GAP)).abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn portrait_stack_heights_are_bounded_and_scrollable() {
        let layout = ShellLayout::for_viewport(1080.0, 1920.0);
        assert!(layout.viewer_height >= PORTRAIT_VIEWER_MIN_HEIGHT);
        assert!(layout.viewer_height <= PORTRAIT_VIEWER_MAX_HEIGHT);
        assert!(layout.console_height >= PORTRAIT_CONSOLE_MIN_HEIGHT);
        assert!(layout.console_height <= PORTRAIT_CONSOLE_MAX_HEIGHT);
        assert!(layout.aux_height >= PORTRAIT_AUX_MIN_HEIGHT);
        assert!(layout.aux_height <= PORTRAIT_AUX_MAX_HEIGHT);
        let stacked = layout.viewer_height
            + layout.console_height
            + layout.aux_height
            + 5.0 * 112.0
            + 7.0 * GAP;
        assert!(
            stacked > layout.height,
            "portrait stack {stacked} should exceed the viewport and scroll"
        );
    }
}
