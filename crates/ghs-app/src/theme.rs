//! Shop-floor visual language for the touch shell.
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
//! - High contrast is a dark-toned variant with near-black surfaces, bright
//!   borders, and normal text at WCAG AAA (7:1) or better.

use ghs_config::ThemeMode;
use gpui_kit::component::{Theme, ThemeMode as ComponentThemeMode};
use gpui_kit::{App, Hsla, px, rgb};

/// Resolved colour palette for one frame.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Whether this palette uses a dark-toned background.
    pub dark: bool,
    /// Whether this is the maximum-contrast variant.
    pub high_contrast: bool,
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
    /// Pick the dark or light palette.
    ///
    /// Kept for callers that already have a resolved dark/light boolean. Use
    /// [`Self::for_mode`] when a persisted [`ThemeMode`] is available.
    pub fn new(dark: bool) -> Self {
        if dark { Self::dark() } else { Self::light() }
    }

    /// Pick the palette for a persisted theme mode.
    ///
    /// [`ThemeMode::System`] resolves to dark, matching the shell's historical
    /// behaviour for configs written before the selector showed a concrete
    /// mode. High contrast is its own dark-toned palette.
    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::System | ThemeMode::Dark => Self::new(true),
            ThemeMode::Light => Self::new(false),
            ThemeMode::HighContrast => Self::high_contrast(),
        }
    }

    fn dark() -> Self {
        Self {
            dark: true,
            high_contrast: false,
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
    }

    fn light() -> Self {
        Self {
            dark: false,
            high_contrast: false,
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

    /// Maximum-contrast palette for low-vision and shop-floor use.
    ///
    /// Near-black surfaces, white primary text, and 8:1 or better contrast for
    /// every text tier. Borders are deliberately bright instead of relying on
    /// a surface-tone difference, and the semantic colours stay highly
    /// saturated while meeting WCAG AA on every surface.
    pub fn high_contrast() -> Self {
        Self {
            dark: true,
            high_contrast: true,
            bg: c(0x050607),
            panel: c(0x0d1114),
            panel_alt: c(0x141a1f),
            raised: c(0x1b2329),
            raised_hover: c(0x26313a),
            border: c(0x7f9aab),
            border_strong: c(0xcfe0ea),
            text: c(0xffffff),
            text_dim: c(0xe2edf3),
            text_faint: c(0xc3d3dd),
            accent: c(0x00ffd1),
            accent_hover: c(0x66ffe3),
            accent_muted: c(0x003b33),
            on_accent: c(0x00130f),
            danger: c(0xff7878),
            warning: c(0xffd400),
            success: c(0x3dff7a),
            info: c(0x7ccbff),
            path_cut: c(0x00ffd1),
            path_rapid: c(0xc3d3dd),
            path_plunge: c(0xffd400),
            grid_minor: c(0x22313a),
            grid_major: c(0x3c5563),
            scrim: translucent(0x000000, 0.78),
        }
    }

    /// Apply both the GPUI Kit mode and this app's semantic colours.
    ///
    /// The component library has dark and light themes only; high contrast
    /// maps to its dark theme, then every surface, text, and control colour is
    /// overwritten from this palette so the high-contrast values are what the
    /// window actually paints.
    pub fn apply(self, cx: &mut App) {
        Theme::change(
            if self.dark {
                ComponentThemeMode::Dark
            } else {
                ComponentThemeMode::Light
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
            theme.colors.button_danger_foreground = if self.high_contrast {
                rgb(0x050607).into()
            } else {
                rgb(0xffffff).into()
            };
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

/// Resolve the launch flags that override the persisted theme for one run.
///
/// `--high-contrast` wins over `--light` when both are present. A missing flag
/// returns `None` so the caller keeps the persisted mode. These flags are for
/// screenshots and review; the selector is what persists a choice.
#[must_use]
pub fn launch_override(args: impl IntoIterator<Item = String>) -> Option<ThemeMode> {
    let mut selected = None;
    for arg in args {
        match arg.as_str() {
            "--high-contrast" => selected = Some(ThemeMode::HighContrast),
            "--light" if selected != Some(ThemeMode::HighContrast) => {
                selected = Some(ThemeMode::Light);
            }
            _ => {}
        }
    }
    selected
}

/// Apply a persisted theme mode without keeping a `Palette` around.
pub fn apply(mode: ThemeMode, cx: &mut App) {
    Palette::for_mode(mode).apply(cx);
}

#[cfg(test)]
mod tests {
    use gpui_kit::Rgba;

    use super::*;

    fn relative_luminance(color: Hsla) -> f64 {
        fn channel(value: f32) -> f64 {
            let value = f64::from(value);
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }
        let rgba = Rgba::from(color);
        0.2126 * channel(rgba.r) + 0.7152 * channel(rgba.g) + 0.0722 * channel(rgba.b)
    }

    fn contrast_ratio(a: Hsla, b: Hsla) -> f64 {
        let first = relative_luminance(a);
        let second = relative_luminance(b);
        let (lighter, darker) = if first >= second {
            (first, second)
        } else {
            (second, first)
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn palette_selection_follows_each_mode() {
        let system = Palette::for_mode(ThemeMode::System);
        let dark = Palette::for_mode(ThemeMode::Dark);
        let light = Palette::for_mode(ThemeMode::Light);
        let high_contrast = Palette::for_mode(ThemeMode::HighContrast);

        assert!(system.dark);
        assert!(!system.high_contrast);
        assert_eq!(system.bg, dark.bg, "System stays on the dark palette");
        assert!(dark.dark);
        assert!(!dark.high_contrast);
        assert!(!light.dark);
        assert!(!light.high_contrast);
        assert_eq!(light.bg, Palette::new(false).bg);
        assert!(high_contrast.dark);
        assert!(high_contrast.high_contrast);
        assert_eq!(
            Palette::new(true).bg,
            dark.bg,
            "the legacy constructor keeps its dark value"
        );
    }

    #[test]
    fn high_contrast_text_meets_aaa_on_every_surface() {
        let palette = Palette::for_mode(ThemeMode::HighContrast);
        let surfaces = [
            ("bg", palette.bg),
            ("panel", palette.panel),
            ("panel_alt", palette.panel_alt),
            ("raised", palette.raised),
            ("raised_hover", palette.raised_hover),
        ];
        for (name, foreground) in [
            ("text", palette.text),
            ("text_dim", palette.text_dim),
            ("text_faint", palette.text_faint),
        ] {
            for (surface_name, surface) in surfaces {
                let ratio = contrast_ratio(foreground, surface);
                assert!(
                    ratio >= 7.0,
                    "{name} on {surface_name} is {ratio:.2}:1, below the 7:1 target"
                );
            }
        }
    }

    #[test]
    fn high_contrast_semantic_colours_meet_aa_on_every_surface() {
        let palette = Palette::for_mode(ThemeMode::HighContrast);
        let surfaces = [
            palette.bg,
            palette.panel,
            palette.panel_alt,
            palette.raised,
            palette.raised_hover,
        ];
        for (name, foreground) in [
            ("accent", palette.accent),
            ("danger", palette.danger),
            ("warning", palette.warning),
            ("success", palette.success),
            ("info", palette.info),
        ] {
            for surface in surfaces {
                let ratio = contrast_ratio(foreground, surface);
                assert!(
                    ratio >= 4.5,
                    "{name} is {ratio:.2}:1 on a high-contrast surface, below AA"
                );
            }
        }
        assert!(
            contrast_ratio(palette.accent, palette.accent_muted) >= 7.0,
            "accent text on the accent-muted chip background must stay AAA"
        );
        assert!(
            contrast_ratio(palette.text, palette.accent_muted) >= 7.0,
            "text on the accent-muted chip background must stay AAA"
        );
    }

    #[test]
    fn high_contrast_differs_from_dark_in_borders_and_accents() {
        let dark = Palette::for_mode(ThemeMode::Dark);
        let high_contrast = Palette::for_mode(ThemeMode::HighContrast);

        assert_ne!(high_contrast.bg, dark.bg);
        assert_ne!(high_contrast.panel, dark.panel);
        assert_ne!(high_contrast.border, dark.border);
        assert_ne!(high_contrast.border_strong, dark.border_strong);
        assert_ne!(high_contrast.accent, dark.accent);
        assert_ne!(high_contrast.danger, dark.danger);
        assert_ne!(high_contrast.text, dark.text);
        assert!(
            contrast_ratio(high_contrast.border, high_contrast.panel) >= 3.0,
            "the high-contrast border must be visible against its panel"
        );
        assert!(
            contrast_ratio(high_contrast.border, high_contrast.panel)
                > contrast_ratio(dark.border, dark.panel),
            "the high-contrast border must be stronger than the dark border"
        );
    }

    #[test]
    fn high_contrast_has_no_low_contrast_faint_tier() {
        let palette = Palette::for_mode(ThemeMode::HighContrast);
        assert!(
            contrast_ratio(palette.text_faint, palette.panel) >= 7.0,
            "text_faint is a normal text tier in this app and must stay legible"
        );
        assert_ne!(
            palette.text_faint, palette.text,
            "the faint tier must still be distinguishable from primary text"
        );
    }

    #[test]
    fn launch_flags_are_reproducible_and_high_contrast_wins() {
        assert_eq!(launch_override(args(&[])), None);
        assert_eq!(launch_override(args(&["--light"])), Some(ThemeMode::Light));
        assert_eq!(
            launch_override(args(&["--high-contrast"])),
            Some(ThemeMode::HighContrast)
        );
        assert_eq!(
            launch_override(args(&["--light", "--high-contrast"])),
            Some(ThemeMode::HighContrast)
        );
        assert_eq!(
            launch_override(args(&["--high-contrast", "--light"])),
            Some(ThemeMode::HighContrast)
        );
        assert_eq!(
            launch_override(args(&["--unrelated", "--light"])),
            Some(ThemeMode::Light)
        );
    }
}
