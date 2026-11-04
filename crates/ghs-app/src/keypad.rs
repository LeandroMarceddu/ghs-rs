//! In-app numeric keypad bottom sheet.
//!
//! The shell must be usable without an OS keyboard on a shop-floor touch panel.
//! This overlay owns no state of its own: it renders the buffer from
//! [`KeypadState`] and sends [`KeypadAction`] values back to the shell.

use ghs_i18n::t;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::{
    App, Entity, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    SharedString, Styled as _, div, px,
};

use crate::shell::Shell;
use crate::state::{KeypadAction, KeypadState, KeypadTarget};
use crate::theme::Palette;

const TOKENS: [&str; 8] = ["G", "M", "X", "Y", "Z", "S", "F", "SP"];

const KEYS: [[(&str, KeypadAction); 4]; 4] = [
    [
        ("7", KeypadAction::Digit('7')),
        ("8", KeypadAction::Digit('8')),
        ("9", KeypadAction::Digit('9')),
        ("DEL", KeypadAction::Backspace),
    ],
    [
        ("4", KeypadAction::Digit('4')),
        ("5", KeypadAction::Digit('5')),
        ("6", KeypadAction::Digit('6')),
        ("C", KeypadAction::Clear),
    ],
    [
        ("1", KeypadAction::Digit('1')),
        ("2", KeypadAction::Digit('2')),
        ("3", KeypadAction::Digit('3')),
        ("-", KeypadAction::Minus),
    ],
    [
        ("0", KeypadAction::Digit('0')),
        (".", KeypadAction::Dot),
        ("CANCEL", KeypadAction::Cancel),
        ("OK", KeypadAction::Submit),
    ],
];

/// Overlay the keypad over the workspace. The top bar stays visible.
pub fn overlay(
    keypad: &KeypadState,
    palette: Palette,
    view: Entity<Shell>,
    mono: SharedString,
    unit: &'static str,
    cx: &App,
) -> impl IntoElement {
    let target = keypad.target;
    let buffer = if keypad.buffer.is_empty() {
        "0".to_string()
    } else {
        keypad.buffer.clone()
    };

    let header = div()
        .flex()
        .items_end()
        .justify_between()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(palette.text_dim)
                        .child(format!("{} / {}", t("TOUCH KEYPAD"), target.title())),
                )
                .child(
                    div()
                        .font_family(mono)
                        .text_size(px(26.))
                        .text_color(palette.text)
                        .overflow_hidden()
                        .child(buffer),
                ),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(palette.text_faint)
                .child(match target {
                    KeypadTarget::Mdi => t("TOKENS + DIGITS").to_string(),
                    KeypadTarget::Rpm => "0 - 24000".to_string(),
                    KeypadTarget::Dro(_) => {
                        format!("{} {}", unit.to_uppercase(), t("UNITS"))
                    }
                    KeypadTarget::WorkOffset { .. } => keypad
                        .hint
                        .clone()
                        .unwrap_or_else(|| t("G10 L20 | DISPLAY UNITS").to_string()),
                    KeypadTarget::Setting(_) => keypad
                        .hint
                        .clone()
                        .unwrap_or_else(|| t("RANGE N/A").to_string()),
                    KeypadTarget::Tool(_) => keypad
                        .hint
                        .clone()
                        .unwrap_or_else(|| t("TOOL FIELD").to_string()),
                    KeypadTarget::ProbeField(_) => keypad
                        .hint
                        .clone()
                        .unwrap_or_else(|| t("PROBE FIELD (mm)").to_string()),
                    KeypadTarget::Utility(_) => keypad
                        .hint
                        .clone()
                        .unwrap_or_else(|| t("UTILITY FIELD").to_string()),
                }),
        );

    let mut sheet = div()
        .relative()
        .w(px(432.))
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded_lg()
        .border_1()
        .border_color(palette.border_strong)
        .bg(palette.raised)
        .child(header);

    if matches!(target, KeypadTarget::Mdi) {
        let mut token_row = div().flex().gap_1();
        for (index, token) in TOKENS.iter().enumerate() {
            let action = KeypadAction::Token(token);
            let view = view.clone();
            let variant = ButtonCustomVariant::new(cx)
                .color(palette.panel_alt)
                .foreground(palette.text_dim)
                .hover(palette.raised_hover)
                .active(palette.accent_muted);
            token_row = token_row.child(
                Button::new(format!("kp-token-{index}"))
                    .label(*token)
                    .flex_1()
                    .h(px(42.))
                    .rounded_lg()
                    .text_size(px(13.))
                    .custom(variant)
                    .border_1()
                    .border_color(palette.border)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |shell, cx| {
                            shell.keypad_action(action, cx);
                            cx.notify();
                        });
                    }),
            );
        }
        sheet = sheet.child(token_row);
    }

    let mut grid = div().flex().flex_col().gap_1();
    for (row_index, row) in KEYS.iter().enumerate() {
        let mut row_div = div().flex().gap_1();
        for (col_index, (label, action)) in row.iter().enumerate() {
            let action = *action;
            let view = view.clone();
            let is_commit = matches!(action, KeypadAction::Submit);
            let is_cancel = matches!(action, KeypadAction::Cancel);
            let (bg, fg) = if is_cancel {
                (palette.panel_alt, palette.text_dim)
            } else {
                (palette.panel_alt, palette.text)
            };
            let variant = ButtonCustomVariant::new(cx)
                .color(bg)
                .foreground(fg)
                .hover(palette.raised_hover)
                .active(palette.accent_muted);
            let digit =
                label.len() == 1 && label.chars().next().is_some_and(|c| c.is_ascii_digit());
            // The key grid is a table of protocol/digit tokens; only the two
            // named actions have catalog entries. Missing keys fall back.
            let label_text = match *label {
                "DEL" => t("DEL"),
                "CANCEL" => t("CANCEL"),
                other => other,
            };
            let button = Button::new(format!("kp-key-{row_index}-{col_index}"))
                .label(label_text)
                .flex_1()
                .h(px(58.))
                .rounded_lg()
                .text_size(px(if digit { 22. } else { 13. }));
            let button = if is_commit {
                button.primary()
            } else {
                button.custom(variant)
            };
            row_div = row_div.child(
                button
                    .border_1()
                    .border_color(if is_commit {
                        palette.accent
                    } else {
                        palette.border
                    })
                    .on_click(move |_, _, cx| {
                        view.update(cx, |shell, cx| {
                            shell.keypad_action(action, cx);
                            cx.notify();
                        });
                    }),
            );
        }
        grid = grid.child(row_div);
    }
    sheet = sheet.child(grid);

    // Consuming scrim: it owns the hitbox over the whole workspace below the
    // top bar, blocks clicks from reaching the controls underneath, and closes
    // the sheet when tapped. Without `.occlude()` a tap outside the sheet fell
    // through to the DRO/MDI control below.
    let scrim_view = view.clone();
    div()
        .absolute()
        .top(px(56.0))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .flex()
        .items_end()
        .justify_end()
        .p_4()
        .child(
            div()
                .id("keypad-scrim")
                .absolute()
                .inset_0()
                .bg(palette.scrim)
                .occlude()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    scrim_view.update(cx, |shell, cx| {
                        shell.keypad_action(KeypadAction::Cancel, cx);
                        cx.notify();
                    });
                }),
        )
        .child(sheet)
}
