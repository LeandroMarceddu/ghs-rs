//! Shop-floor visual language for the M0 touch shell.
//!
//! The tokens here are intentionally crate-local for now. At M2 they move into
//! `ghs-ui` together with the component wrappers, so that a GPUI Kit upgrade
//! touches one small crate instead of every panel.
//!
//! Design rules:
//! - Touch targets are at least 48 logical pixels; primary motion controls are
//!   larger.
//! - Type is larger than a desktop form: 11 px labels, 14-16 px body, 26-32 px
//!   numeric readouts.
//! - Colour is never the only signal: state also changes text and surface tone.
//! - Dark is the shop-floor default; light keeps the same contrast hierarchy.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, px, rgb};

/// Resolved colour palette for one frame.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub bg: Hsla,
    pub panel: Hsla,
    pub panel_alt: Hsla,
    pub raised: Hsla,
    pub raised_hover: Hsla,
    pub border: Hsla,
    pub border_strong: Hsla,
    pub text: Hsla,
    pub text_dim: Hsla,
    pub text_faint: Hsla,
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub accent_muted: Hsla,
    pub on_accent: Hsla,
    pub danger: Hsla,
    pub warning: Hsla,
    pub success: Hsla,
    pub info: Hsla,
    pub path_cut: Hsla,
    pub path_rapid: Hsla,
    pub path_plunge: Hsla,
    pub grid_minor: Hsla,
    pub grid_major: Hsla,
    pub scrim: Hsla,
}

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn translucent(hex: u32, alpha: f32) -> Hsla {
    c(hex).alpha(alpha)
}

impl Palette {
    /// Pick the palette matching the requested mode.
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                dark,
                bg: c(0x080b0e),
                panel: c(0x10161b),
                panel_alt: c(0x151d24),
                raised: c(0x1b252e),
                raised_hover: c(0x24313c),
                border: c(0x283640),
                border_strong: c(0x3a4c59),
                text: c(0xe9f1f6),
                text_dim: c(0x9baebb),
                text_faint: c(0x657a87),
                accent: c(0x3dd6c0),
                accent_hover: c(0x5ae5d1),
                accent_muted: c(0x123d39),
                on_accent: c(0x04211d),
                danger: c(0xe5484d),
                warning: c(0xf5a524),
                success: c(0x46d17d),
                info: c(0x5aa9ff),
                path_cut: c(0x3dd6c0),
                path_rapid: c(0x6c8291),
                path_plunge: c(0xf5a524),
                grid_minor: c(0x1b262e),
                grid_major: c(0x2a3943),
                scrim: translucent(0x000000, 0.62),
            }
        } else {
            Self {
                dark,
                bg: c(0xe9eef1),
                panel: c(0xffffff),
                panel_alt: c(0xf2f6f8),
                raised: c(0xf7fafb),
                raised_hover: c(0xe7eef2),
                border: c(0xd3dde3),
                border_strong: c(0xaebec8),
                text: c(0x0c171e),
                text_dim: c(0x4e626f),
                text_faint: c(0x84949d),
                accent: c(0x0c8f7f),
                accent_hover: c(0x0aa394),
                accent_muted: c(0xd5f2ed),
                on_accent: c(0xffffff),
                danger: c(0xc7353a),
                warning: c(0xb87505),
                success: c(0x1d8a4b),
                info: c(0x1868b8),
                path_cut: c(0x0c8f7f),
                path_rapid: c(0x6c7d88),
                path_plunge: c(0xb87505),
                grid_minor: c(0xdee7ec),
                grid_major: c(0xc5d2da),
                scrim: translucent(0x000000, 0.38),
            }
        }
    }

    /// Apply both the GPUI Kit mode and this app's semantic colours.
    pub fn apply(self, cx: &mut App) {
        Theme::change(
            if self.dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            None,
            cx,
        );
        Theme::update(cx, |theme| {
            theme.font_size = px(15.);
            theme.mono_font_size = px(13.);
            theme.radius = px(10.);
            theme.radius_lg = px(14.);
            theme.colors.background = self.bg;
            theme.colors.foreground = self.text;
            theme.colors.border = self.border;
            theme.colors.input = self.panel_alt;
            theme.colors.primary = self.accent;
            theme.colors.primary_hover = self.accent_hover;
            theme.colors.primary_active = self.accent;
            theme.colors.primary_foreground = self.on_accent;
            theme.colors.button = self.raised;
            theme.colors.button_hover = self.raised_hover;
            theme.colors.button_active = self.raised;
            theme.colors.button_foreground = self.text;
            theme.colors.button_primary = self.accent;
            theme.colors.button_primary_hover = self.accent_hover;
            theme.colors.button_primary_active = self.accent_hover;
            theme.colors.button_primary_foreground = self.on_accent;
            theme.colors.button_secondary = self.raised;
            theme.colors.button_secondary_hover = self.raised_hover;
            theme.colors.button_secondary_active = self.raised;
            theme.colors.button_secondary_foreground = self.text;
            theme.colors.button_danger = self.danger;
            theme.colors.button_danger_hover = self.danger;
            theme.colors.button_danger_active = self.danger;
            theme.colors.button_danger_foreground = rgb(0xffffff).into();
            theme.colors.button_warning = self.warning;
            theme.colors.button_warning_hover = self.warning;
            theme.colors.button_warning_active = self.warning;
            theme.colors.button_warning_foreground = rgb(0x111111).into();
            theme.colors.accent = self.accent_muted;
            theme.colors.accent_foreground = self.text;
            theme.colors.secondary = self.raised;
            theme.colors.secondary_hover = self.raised_hover;
            theme.colors.secondary_foreground = self.text;
            theme.colors.muted = self.panel_alt;
            theme.colors.muted_foreground = self.text_dim;
            theme.colors.popover = self.raised;
            theme.colors.popover_foreground = self.text;
            theme.colors.title_bar = self.panel;
            theme.colors.title_bar_border = self.border;
            theme.colors.sidebar = self.panel;
            theme.colors.sidebar_foreground = self.text;
            theme.colors.sidebar_border = self.border;
            theme.colors.danger = self.danger;
            theme.colors.warning = self.warning;
            theme.colors.success = self.success;
            theme.colors.info = self.info;
        });
    }
}

/// Apply a mode without keeping a `Palette` around.
pub fn apply(dark: bool, cx: &mut App) {
    Palette::new(dark).apply(cx);
}
