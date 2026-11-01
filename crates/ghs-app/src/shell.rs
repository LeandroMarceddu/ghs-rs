//! Touch shell layout for the M0 spike.
//!
//! Fixed panels, 48 px targets, and a single always-visible Reset/Stop pair in
//! the top bar. The layout targets 1280x800 (UX-01) and scales to 1920x1080
//! (UX-02) because the centre column and jog card absorb the extra width.
//!
//! Portrait / narrow behaviour plan (UX-03, deferred to M7):
//! - At < 1120 px the right rail collapses behind a vertical "AUX" tab and the
//!   left rail narrows to 300 px.
//! - At < 760 px the window should reflow to a single column: mode bar, active
//!   workspace, then a paged bottom dock (DRO / Jog / MDI / Spindle). The
//!   keypad becomes a full-width bottom sheet.
//! - Every panel keeps its 48 px targets in portrait; the dock scrolls rather
//!   than shrinking controls.
//!
//! For M0 only the right-rail collapse is implemented, because a 1024x600
//! landscape minimum is the only supported touch size.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, App, AppContext as _, Context, Div, Entity, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, px,
};

use crate::keypad;
use crate::state::{
    AppState, Axis, ConsoleLevel, KeypadTarget, LeftTab, Mode, ProgramKind, SignalState,
};
use crate::theme::Palette;
use crate::viewer;

const STEP_SIZES: [f64; 5] = [0.01, 0.1, 1.0, 10.0, 100.0];

pub struct Shell {
    pub state: AppState,
    feed_slider: Entity<SliderState>,
    spindle_slider: Entity<SliderState>,
    mono: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let feed_slider = cx.new(|_| {
            SliderState::new()
                .min(10.0)
                .max(200.0)
                .step(1.0)
                .default_value(100.0)
        });
        let spindle_slider = cx.new(|_| {
            SliderState::new()
                .min(50.0)
                .max(200.0)
                .step(1.0)
                .default_value(100.0)
        });

        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe(&feed_slider, |this, _, event, cx| {
            this.state.feed_override = slider_value(event).round().clamp(10.0, 200.0) as u16;
            cx.notify();
        }));
        subscriptions.push(cx.subscribe(&spindle_slider, |this, _, event, cx| {
            this.state.spindle_override = slider_value(event).round().clamp(50.0, 200.0) as u16;
            cx.notify();
        }));

        let mono = cx.theme().mono_font_family.clone();
        let mut state = AppState::new();
        // Demo-only switches used for screenshots and manual reviews. They do
        // not affect normal startup and never touch the controller.
        state.dark = !std::env::args().any(|arg| arg == "--light");
        if std::env::args().any(|arg| arg == "--keypad") {
            state.open_keypad(KeypadTarget::Dro(Axis::X));
        }
        Self {
            state,
            feed_slider,
            spindle_slider,
            mono,
            _subscriptions: subscriptions,
        }
    }

    fn top_bar(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();

        let modes = Mode::ALL.iter().enumerate().map(|(index, mode)| {
            let mode = *mode;
            let active = self.state.mode == mode;
            let view = view.clone();
            let variant = mode_variant(cx, p, active);
            Button::new(format!("mode-{index}"))
                .label(mode.label())
                .h(px(48.0))
                .px_3()
                .rounded_lg()
                .text_size(px(14.0))
                .custom(variant)
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.state.mode = mode;
                        cx.notify();
                    });
                })
        });

        let theme_view = view.clone();
        let reset_view = view.clone();
        let stop_view = view.clone();

        div()
            .h(px(64.0))
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .bg(p.panel)
            .border_b_1()
            .border_color(p.border)
            .child(
                v_flex()
                    .gap_0()
                    .w(px(116.0))
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child("GHS"),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child("grblHAL touch"),
                    ),
            )
            .child(div().flex().items_center().gap_1().children(modes))
            .child(div().flex_1())
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .h(px(40.0))
                    .px_3()
                    .rounded(px(999.0))
                    .bg(p.panel_alt)
                    .border_1()
                    .border_color(p.border)
                    .child(div().w(px(8.0)).h(px(8.0)).rounded(px(999.0)).bg(
                        if self.state.machine_state == "Alarm" {
                            p.danger
                        } else {
                            p.success
                        },
                    ))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_dim)
                            .child(format!(
                                "MOCK {}A | {}",
                                self.state.profile.capabilities.axes.len(),
                                self.state.machine_state
                            )),
                    ),
            )
            .child(
                Switch::new("theme-toggle")
                    .checked(!self.state.dark)
                    .label("Light")
                    .text_size(px(12.0))
                    .on_change(move |checked, _, cx| {
                        let dark = !*checked;
                        theme_view.update(cx, |this, cx| {
                            this.state.dark = dark;
                            Palette::new(dark).apply(cx);
                            cx.notify();
                        });
                    }),
            )
            .child(
                Button::new("reset")
                    .label("RESET")
                    .danger()
                    .h(px(48.0))
                    .w(px(84.0))
                    .rounded_lg()
                    .text_size(px(13.0))
                    .on_click(move |_, _, cx| {
                        reset_view.update(cx, |this, cx| {
                            this.state.machine_state = "Idle";
                            this.state.push_console(
                                ConsoleLevel::Warn,
                                "SOFT RESET requested (visual only)",
                            );
                            cx.notify();
                        });
                    }),
            )
            .child(
                Button::new("stop")
                    .label("STOP")
                    .warning()
                    .h(px(48.0))
                    .w(px(84.0))
                    .rounded_lg()
                    .text_size(px(13.0))
                    .on_click(move |_, _, cx| {
                        stop_view.update(cx, |this, cx| {
                            this.state.machine_state = "Hold";
                            this.state
                                .push_console(ConsoleLevel::Warn, "Feed hold (visual only)");
                            cx.notify();
                        });
                    }),
            )
    }

    fn left_rail(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let console_tab = tab_button(
            cx,
            p,
            LeftTab::Console,
            self.state.left_tab == LeftTab::Console,
            view.clone(),
        );
        let program_tab = tab_button(
            cx,
            p,
            LeftTab::Program,
            self.state.left_tab == LeftTab::Program,
            view,
        );

        let body = match self.state.left_tab {
            LeftTab::Console => self.console(p).into_any_element(),
            LeftTab::Program => self.program(p).into_any_element(),
        };

        div()
            .w(px(356.0))
            .h_full()
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(p.border)
            .bg(p.panel)
            .overflow_hidden()
            .child(
                h_flex()
                    .gap_1()
                    .p_2()
                    .border_b_1()
                    .border_color(p.border)
                    .child(console_tab)
                    .child(program_tab),
            )
            .child(div().flex_1().overflow_hidden().child(body))
            .child(
                div()
                    .h(px(26.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .border_t_1()
                    .border_color(p.border)
                    .text_size(px(10.0))
                    .text_color(p.text_faint)
                    .child(format!("{} lines", self.state.program.len()))
                    .child(format!(
                        "line {} | {:.0}%",
                        self.state.active_line,
                        self.state.progress * 100.0
                    )),
            )
    }

    fn console(&self, p: Palette) -> impl IntoElement {
        v_flex()
            .gap_1()
            .p_2()
            .h_full()
            .id("console-scroll")
            .overflow_y_scroll()
            .font_family(self.mono.clone())
            .children(self.state.console.iter().map(|line| {
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(34.0))
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child(line.time),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(10.5))
                            .text_color(console_color(line.level, p))
                            .child(line.text.clone()),
                    )
            }))
    }

    fn program(&self, p: Palette) -> impl IntoElement {
        v_flex()
            .py_1()
            .h_full()
            .id("program-scroll")
            .overflow_y_scroll()
            .font_family(self.mono.clone())
            .children(self.state.program.iter().map(|line| {
                let active = line.number == self.state.active_line;
                let text_color = match line.kind {
                    ProgramKind::Comment => p.text_faint,
                    ProgramKind::Setup => p.info,
                    ProgramKind::Rapid => p.text_dim,
                    ProgramKind::Cut => p.text,
                    ProgramKind::Plunge => p.warning,
                };
                h_flex()
                    .items_center()
                    .gap_2()
                    .h(px(23.0))
                    .px_2()
                    .bg(if active { p.accent_muted } else { p.panel })
                    .when(active, |this| this.border_l(px(3.0)).border_color(p.accent))
                    .child(
                        div()
                            .w(px(26.0))
                            .flex_shrink_0()
                            .text_size(px(9.0))
                            .text_color(if active { p.accent } else { p.text_faint })
                            .child(format!("{}", line.number)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(10.5))
                            .text_color(if active { p.accent } else { text_color })
                            .child(line.text),
                    )
            }))
    }

    fn center(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let header = div()
            .h(px(48.0))
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .border_b_1()
            .border_color(p.border)
            .child(
                v_flex()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_dim)
                            .child(self.state.mode.title()),
                    )
                    .child(div().text_size(px(14.0)).text_color(p.text).child(
                        if self.state.mode == Mode::Job {
                            self.state.job_name
                        } else {
                            "M0 preview"
                        },
                    )),
            )
            .child(div().flex_1());

        let header = if self.state.mode == Mode::Job {
            header
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(110.0))
                                .h(px(5.0))
                                .rounded(px(999.0))
                                .bg(p.panel_alt)
                                .child(
                                    div()
                                        .h_full()
                                        .w(px(110.0 * self.state.progress))
                                        .rounded(px(999.0))
                                        .bg(p.accent),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(p.text_dim)
                                .child(format!("{:.0}%", self.state.progress * 100.0)),
                        ),
                )
                .child(chip(self.state.wcs(), p))
                .child(chip("XY", p))
        } else {
            header
        };

        let body = if self.state.mode == Mode::Job {
            self.job_view(p).into_any_element()
        } else {
            self.mode_view(p, cx).into_any_element()
        };

        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(p.border)
            .bg(p.panel)
            .overflow_hidden()
            .child(header)
            .child(div().flex_1().min_h(px(0.0)).child(body))
    }

    fn job_view(&self, p: Palette) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .bg(p.bg)
            .child(viewer::toolpath(p))
            .child(
                div()
                    .absolute()
                    .top(px(8.0))
                    .left(px(8.0))
                    .flex()
                    .gap_1()
                    .child(chip("2D TOP", p))
                    .child(chip("SYNTHETIC", p)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(8.0))
                    .right(px(8.0))
                    .flex()
                    .gap_1()
                    .child(chip("G54", p))
                    .child(chip(self.state.unit(), p))
                    .child(chip("1:1", p)),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(8.0))
                    .left(px(8.0))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_2()
                    .py_1()
                    .rounded_lg()
                    .bg(p.panel.opacity(0.9))
                    .border_1()
                    .border_color(p.border)
                    .child(legend(p.path_cut, "Cut", p))
                    .child(legend(p.path_rapid, "Rapid", p))
                    .child(legend(p.path_plunge, "Plunge", p)),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(8.0))
                    .right(px(8.0))
                    .px_2()
                    .py_1()
                    .rounded_lg()
                    .bg(p.panel.opacity(0.9))
                    .border_1()
                    .border_color(p.border)
                    .text_size(px(10.0))
                    .text_color(p.text_dim)
                    .child("TOOL 3 | 6 mm FLAT | S12000"),
            )
    }

    fn mode_view(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let caps = &self.state.profile.capabilities;
        let body = match self.state.mode {
            Mode::Job => unreachable!(),
            Mode::Probe => {
                let items: &[(&str, &str)] = if caps.has_probe {
                    &[
                        ("Z touch plate", "M6"),
                        ("XY corner finder", "M6"),
                        ("Circle centre", "M6"),
                        ("Height map", "M6"),
                    ]
                } else {
                    &[
                        ("No probe input reported", "CAP"),
                        ("Probe cycles hidden", "CAP"),
                    ]
                };
                v_flex().gap_2().child(self.list_rows(p, items))
            }
            Mode::Offset => self.offset_rows(p),
            Mode::Utility => self.utility_rows(p),
            Mode::Tools => self.tool_rows(p),
            Mode::Settings => self.settings_placeholder(p),
        };
        let view = cx.entity();
        v_flex()
            .gap_2()
            .p_3()
            .size_full()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child(self.state.mode.title()),
                    )
                    .child(chip("M0 PREVIEW", p)),
            )
            .child(body)
            .child(div().flex_1())
            .child(
                Button::new("mode-back")
                    .label("Back to Job")
                    .h(px(44.0))
                    .w(px(140.0))
                    .rounded_lg()
                    .text_size(px(12.0))
                    .secondary()
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.state.mode = Mode::Job;
                            cx.notify();
                        });
                    }),
            )
    }

    fn list_rows(&self, p: Palette, items: &[(&str, &str)]) -> Div {
        v_flex().gap_2().children(
            items
                .iter()
                .map(|(name, milestone)| info_row(name, milestone, p)),
        )
    }

    fn offset_rows(&self, p: Palette) -> Div {
        const ROWS: [(&str, &str, &str, &str); 6] = [
            ("G54", "-123.450", "-87.200", "-2.000"),
            ("G55", "0.000", "0.000", "0.000"),
            ("G56", "-500.000", "-320.000", "0.000"),
            ("G57", "0.000", "0.000", "0.000"),
            ("G58", "0.000", "0.000", "0.000"),
            ("G59", "0.000", "0.000", "0.000"),
        ];
        v_flex()
            .gap_1()
            .p_1()
            .children(ROWS.iter().map(|(wcs, x, y, z)| {
                let active = *wcs == self.state.wcs();
                h_flex()
                    .items_center()
                    .h(px(34.0))
                    .gap_2()
                    .px_2()
                    .rounded(px(6.0))
                    .bg(if active { p.accent_muted } else { p.panel_alt })
                    .child(
                        div()
                            .w(px(48.0))
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(if active { p.accent } else { p.text_dim })
                            .child(*wcs),
                    )
                    .child(num_cell(x, p))
                    .child(num_cell(y, p))
                    .child(num_cell(z, p))
            }))
    }

    fn utility_rows(&self, p: Palette) -> Div {
        const ITEMS: [(&str, &str); 4] = [
            ("Surface stock", "Generate a facing program"),
            ("Square gantry", "Two-edge alignment routine"),
            ("Calibrate steps", "Move, measure, correct"),
            ("Outline check", "Bounds before a run"),
        ];
        v_flex()
            .gap_2()
            .children(ITEMS.iter().map(|(name, detail)| {
                h_flex()
                    .items_center()
                    .justify_between()
                    .p_2()
                    .rounded_lg()
                    .bg(p.panel_alt)
                    .child(
                        v_flex()
                            .child(div().text_size(px(13.0)).text_color(p.text).child(*name))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(p.text_faint)
                                    .child(*detail),
                            ),
                    )
                    .child(chip("M7", p))
            }))
    }

    fn tool_rows(&self, p: Palette) -> Div {
        let caps = &self.state.profile.capabilities;
        let tools: &[(&str, &str, &str)] = if caps.has_tool_table {
            &[
                ("T1", "6 mm flat", "0.000"),
                ("T2", "3 mm flat", "0.000"),
                ("T3", "6 mm ball", "0.000"),
                ("T4", "90 deg V", "0.000"),
            ]
        } else {
            &[]
        };
        v_flex()
            .gap_1()
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.text_faint)
                    .child(format!(
                        "APP-SIDE TABLE | controller reports {} entries{}",
                        caps.tool_table_entries,
                        if caps.has_tool_table {
                            ""
                        } else {
                            " | capability not reported"
                        }
                    )),
            )
            .children(tools.iter().map(|(tool, kind, offset)| {
                h_flex()
                    .items_center()
                    .gap_3()
                    .h(px(36.0))
                    .px_2()
                    .rounded(px(6.0))
                    .bg(p.panel_alt)
                    .child(
                        div()
                            .w(px(34.0))
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.accent)
                            .child(*tool),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.0))
                            .text_color(p.text)
                            .child(*kind),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(p.text_dim)
                            .child(format!("TLO {offset}")),
                    )
            }))
    }

    fn settings_placeholder(&self, p: Palette) -> Div {
        let caps = &self.state.profile.capabilities;
        let facts = [
            format!("{} axes", caps.axes.len()),
            format!("probe: {}", if caps.has_probe { "yes" } else { "no" }),
            format!("tool table: {} entries", caps.tool_table_entries),
            format!("units: {}", caps.units.label()),
            format!(
                "spindle: {}",
                if caps.multi_spindle {
                    "multi"
                } else {
                    "single"
                }
            ),
        ];
        v_flex()
            .gap_2()
            .child(
                v_flex()
                    .gap_1()
                    .p_3()
                    .rounded_lg()
                    .bg(p.panel_alt)
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child(format!(
                                "PROFILE: {} | METADATA-DRIVEN SETTINGS",
                                self.state.profile.name
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(p.text_dim)
                            .child("Forms are generated from $ES / $EG / $SED at M5. This shell hardcodes no setting list; the facts below come from the active profile."),
                    ),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .children(facts.iter().map(|fact| chip(fact, p))),
            )
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(p.text_faint)
                    .child("ACCESSORY HOOKS (post-v1.0 panels, not built)"),
            )
            .child(h_flex().flex_wrap().gap_1().children(
                caps.accessory_panels().iter().map(|(name, enabled)| {
                    chip(
                        &format!(
                            "{}: {}",
                            name,
                            if *enabled { "reported" } else { "not reported" }
                        ),
                        p,
                    )
                }),
            ))
    }

    fn right_rail(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let coolant = self
            .state
            .profile
            .capabilities
            .coolant_channels()
            .into_iter()
            .enumerate()
            .map(|(slot, (name, channel))| {
                let active = self.state.coolant[channel];
                let view = view.clone();
                let variant = if active {
                    ButtonCustomVariant::new(cx)
                        .color(p.accent_muted)
                        .foreground(p.accent)
                        .hover(p.raised_hover)
                        .active(p.accent_muted)
                } else {
                    ButtonCustomVariant::new(cx)
                        .color(p.panel_alt)
                        .foreground(p.text_dim)
                        .hover(p.raised_hover)
                        .active(p.raised)
                };
                Button::new(format!("coolant-{slot}"))
                    .label(name)
                    .flex_1()
                    .h(px(48.0))
                    .rounded(px(6.0))
                    .text_size(px(10.0))
                    .custom(variant)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.state.toggle_coolant(channel);
                            cx.notify();
                        });
                    })
            });

        let macro_view = view.clone();
        let macro_rows = (0..4).map(|row| {
            div().flex().gap_1().children((0..2).map(|col| {
                let index = row * 2 + col;
                let macro_view = macro_view.clone();
                let Some(mac) = self.state.macros.get(index) else {
                    return div().into_any_element();
                };
                Button::new(format!("macro-{index}"))
                    .label(mac.label)
                    .flex_1()
                    .h(px(48.0))
                    .rounded(px(6.0))
                    .text_size(px(10.0))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(p.panel_alt)
                            .foreground(p.text_dim)
                            .hover(p.raised_hover)
                            .active(p.accent_muted),
                    )
                    .on_click(move |_, _, cx| {
                        macro_view.update(cx, |this, cx| {
                            this.state.run_macro(index);
                            cx.notify();
                        });
                    })
                    .into_any_element()
            }))
        });

        let signal_rows = (0..3).map(|row| {
            div().flex().gap_1().children((0..2).map(|col| {
                let index = row * 2 + col;
                let Some(signal) = self.state.signals.get(index) else {
                    return div().into_any_element();
                };
                let (color, text) = match signal.state {
                    SignalState::Off => (p.text_faint, p.text_faint),
                    SignalState::Ok => (p.success, p.text_dim),
                    SignalState::Warn => (p.warning, p.warning),
                    SignalState::Alert => (p.danger, p.danger),
                };
                h_flex()
                    .items_center()
                    .gap_1()
                    .w(px(96.0))
                    .h(px(20.0))
                    .child(div().w(px(6.0)).h(px(6.0)).rounded(px(999.0)).bg(color))
                    .child(
                        div()
                            .text_size(px(8.5))
                            .font_weight(FontWeight::BOLD)
                            .text_color(text)
                            .child(signal.label.clone()),
                    )
                    .into_any_element()
            }))
        });

        div()
            .w(px(220.0))
            .h_full()
            .flex()
            .flex_col()
            .gap_1()
            .id("right-rail-scroll")
            .overflow_y_scroll()
            .child(
                card(&p)
                    .gap_1()
                    .child(section_label("COOLANT", p))
                    .child(div().flex().gap_1().children(coolant)),
            )
            .child(
                card(&p)
                    .gap_1()
                    .child(section_label("MACROS", p))
                    .child(div().flex().flex_col().gap_1().children(macro_rows)),
            )
            .child(
                card(&p)
                    .gap_1()
                    .child(section_label("SIGNALS", p))
                    .child(div().flex().flex_col().gap_1().children(signal_rows))
                    .child(
                        div()
                            .text_size(px(8.0))
                            .text_color(p.text_faint)
                            .child("HSDEP | no probe-connected pin"),
                    ),
            )
    }

    fn bottom_dock(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(326.0))
            .flex()
            .flex_col()
            .gap_2()
            .px_2()
            .pb_2()
            .child(
                h_flex()
                    .gap_2()
                    .h(px(138.0))
                    .child(self.mdi_card(p, cx))
                    .child(self.dro_card(p, cx))
                    .child(self.params_card(p)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .flex_1()
                    .child(self.feed_card(p))
                    .child(self.jog_card(p, cx))
                    .child(self.spindle_card(p, cx)),
            )
    }

    fn mdi_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let empty = self.state.mdi.trim().is_empty();
        let field_view = view.clone();
        let send_view = view.clone();

        card(&p)
            .flex_1()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(section_label("MDI", p))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child("tap field for keypad"),
                    ),
            )
            .child(
                div()
                    .id("mdi-field")
                    .flex()
                    .items_center()
                    .h(px(48.0))
                    .w_full()
                    .px_3()
                    .rounded(px(7.0))
                    .bg(p.panel_alt)
                    .border_1()
                    .border_color(p.border)
                    .font_family(self.mono.clone())
                    .text_size(px(16.0))
                    .text_color(if empty { p.text_faint } else { p.text })
                    .cursor_pointer()
                    .child(if empty {
                        "G-code or numeric input".to_string()
                    } else {
                        self.state.mdi.clone()
                    })
                    .on_click(move |_, _, cx| {
                        field_view.update(cx, |this, cx| {
                            this.state.open_keypad(KeypadTarget::Mdi);
                            cx.notify();
                        });
                    }),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child("Mock send appends to console"),
                    )
                    .child(
                        Button::new("mdi-send")
                            .label("SEND")
                            .primary()
                            .h(px(48.0))
                            .w(px(88.0))
                            .rounded_lg()
                            .text_size(px(12.0))
                            .on_click(move |_, _, cx| {
                                send_view.update(cx, |this, cx| {
                                    this.state.send_mdi();
                                    cx.notify();
                                });
                            }),
                    ),
            )
    }

    fn dro_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let wcs_view = view.clone();
        let wcs_menu = Button::new("wcs-menu")
            .label(self.state.wcs())
            .h(px(30.0))
            .w(px(74.0))
            .rounded(px(6.0))
            .text_size(px(11.0))
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(p.panel_alt)
                    .foreground(p.accent)
                    .hover(p.raised_hover)
                    .active(p.accent_muted),
            )
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                let mut menu = menu.min_w(px(150.0));
                for index in 0..AppState::wcs_count() {
                    let wcs_view = wcs_view.clone();
                    menu = menu.item(PopupMenuItem::new(AppState::wcs_at(index)).on_click(
                        move |_, _, cx| {
                            wcs_view.update(cx, |this, cx| {
                                this.state.wcs_index = index;
                                cx.notify();
                            });
                        },
                    ));
                }
                menu
            });

        let tiles = self.state.axes().iter().map(|axis| {
            let axis = *axis;
            let value_view = view.clone();
            let zero_view = view.clone();
            let value = format!("{:>8.3}", self.state.value(axis));
            div()
                .flex_1()
                .min_w(px(112.0))
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(10.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(p.text_dim)
                                .child(axis.label()),
                        )
                        .child(
                            div()
                                .text_size(px(8.0))
                                .text_color(p.text_faint)
                                .child(self.state.unit()),
                        ),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .h(px(48.0))
                        .child(
                            div()
                                .id(format!("dro-{}", axis.label()))
                                .flex_1()
                                .flex()
                                .items_center()
                                .justify_end()
                                .px_2()
                                .rounded(px(7.0))
                                .bg(p.panel_alt)
                                .border_1()
                                .border_color(p.border)
                                .cursor_pointer()
                                .font_family(self.mono.clone())
                                .text_size(px(19.0))
                                .text_color(p.text)
                                .child(value)
                                .on_click(move |_, _, cx| {
                                    value_view.update(cx, |this, cx| {
                                        this.state.open_keypad(KeypadTarget::Dro(axis));
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(
                            Button::new(format!("zero-{}", axis.label()))
                                .label("0")
                                .h(px(48.0))
                                .w(px(48.0))
                                .rounded(px(7.0))
                                .text_size(px(15.0))
                                .custom(
                                    ButtonCustomVariant::new(cx)
                                        .color(p.panel_alt)
                                        .foreground(p.warning)
                                        .hover(p.raised_hover)
                                        .active(p.raised),
                                )
                                .on_click(move |_, _, cx| {
                                    zero_view.update(cx, |this, cx| {
                                        this.state.zero_axis(axis);
                                        cx.notify();
                                    });
                                }),
                        ),
                )
        });

        card(&p)
            .flex_1()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(section_label("DRO", p))
                    .child(wcs_menu),
            )
            .child(div().flex().flex_wrap().gap_2().flex_1().children(tiles))
    }

    fn params_card(&self, p: Palette) -> impl IntoElement {
        let rows = [
            ("Origin X", "-123.450".to_string()),
            ("Origin Y", "-87.200".to_string()),
            ("Origin Z", "-2.000".to_string()),
            (
                "Feed",
                format!("{} {}/min", self.state.feed_rate, self.state.unit()),
            ),
        ];
        card(&p)
            .w(px(240.0))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(section_label("WORK", p))
                    .child(chip(self.state.wcs(), p)),
            )
            .child(v_flex().gap_1().children(rows.iter().map(|(name, value)| {
                h_flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(10.0))
                    .child(div().text_color(p.text_faint).child(*name))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(9.0))
                            .font_family(self.mono.clone())
                            .text_color(p.text_dim)
                            .child(value.clone()),
                    )
            })))
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(chip("G21", p))
                    .child(chip("G90", p))
                    .child(chip("G17", p)),
            )
    }

    fn feed_card(&self, p: Palette) -> impl IntoElement {
        card(&p)
            .w(px(220.0))
            .child(
                h_flex()
                    .items_end()
                    .justify_between()
                    .child(section_label("FEED OVERRIDE", p))
                    .child(
                        div()
                            .font_family(self.mono.clone())
                            .text_size(px(22.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.accent)
                            .child(format!("{}%", self.state.feed_override)),
                    ),
            )
            .child(Slider::new(&self.feed_slider).h(px(36.0)))
            .child(
                div()
                    .text_size(px(8.0))
                    .text_color(p.text_faint)
                    .child("DRAG THE TRACK"),
            )
    }

    fn jog_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let axes = self.state.axes().iter().enumerate().map(|(index, axis)| {
            let axis = *axis;
            let active = self.state.jog_axis == axis;
            let axis_view = view.clone();
            Button::new(format!("jog-axis-{index}"))
                .label(axis.label())
                .h(px(38.0))
                .w(px(44.0))
                .rounded(px(6.0))
                .text_size(px(12.0))
                .custom(mode_variant(cx, p, active))
                .border_1()
                .border_color(if active { p.accent } else { p.border })
                .on_click(move |_, _, cx| {
                    axis_view.update(cx, |this, cx| {
                        this.state.jog_axis = axis;
                        cx.notify();
                    });
                })
        });

        let continuous_view = view.clone();
        let step_labels =
            ["0.01", "0.1", "1", "10", "100"]
                .iter()
                .enumerate()
                .map(|(index, label)| {
                    let step = STEP_SIZES[index];
                    let active = (self.state.jog_step - step).abs() < f64::EPSILON;
                    let step_view = view.clone();
                    Button::new(format!("jog-step-{index}"))
                        .label(*label)
                        .flex_1()
                        .h(px(44.0))
                        .rounded(px(6.0))
                        .text_size(px(11.0))
                        .custom(mode_variant(cx, p, active))
                        .border_1()
                        .border_color(if active { p.accent } else { p.border })
                        .on_click(move |_, _, cx| {
                            step_view.update(cx, |this, cx| {
                                this.state.jog_step = step;
                                cx.notify();
                            });
                        })
                });

        let minus_view = view.clone();
        let plus_view = view.clone();

        card(&p)
            .flex_1()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(section_label("JOG", p))
                    .child(div().flex().flex_wrap().gap_1().children(axes))
                    .child(
                        Switch::new("continuous")
                            .checked(self.state.continuous)
                            .label("CONT")
                            .text_size(px(9.0))
                            .on_change(move |checked, _, cx| {
                                continuous_view.update(cx, |this, cx| {
                                    this.state.continuous = *checked;
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .h(px(64.0))
                    .child(
                        Button::new("jog-minus")
                            .label("-")
                            .h(px(64.0))
                            .w(px(76.0))
                            .rounded_lg()
                            .text_size(px(30.0))
                            .custom(
                                ButtonCustomVariant::new(cx)
                                    .color(p.raised)
                                    .foreground(p.text)
                                    .hover(p.raised_hover)
                                    .active(p.accent_muted),
                            )
                            .border_1()
                            .border_color(p.border)
                            .on_click(move |_, _, cx| {
                                minus_view.update(cx, |this, cx| {
                                    this.state.jog(-1.0);
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .font_family(self.mono.clone())
                                    .text_size(px(18.0))
                                    .text_color(p.text)
                                    .child(format!(
                                        "{} {:.2} mm",
                                        self.state.jog_axis.label(),
                                        self.state.jog_step
                                    )),
                            )
                            .child(div().text_size(px(8.0)).text_color(p.text_faint).child(
                                if self.state.continuous {
                                    "CONTINUOUS"
                                } else {
                                    "STEP"
                                },
                            )),
                    )
                    .child(
                        Button::new("jog-plus")
                            .label("+")
                            .h(px(64.0))
                            .w(px(76.0))
                            .rounded_lg()
                            .text_size(px(30.0))
                            .custom(
                                ButtonCustomVariant::new(cx)
                                    .color(p.raised)
                                    .foreground(p.text)
                                    .hover(p.raised_hover)
                                    .active(p.accent_muted),
                            )
                            .border_1()
                            .border_color(p.border)
                            .on_click(move |_, _, cx| {
                                plus_view.update(cx, |this, cx| {
                                    this.state.jog(1.0);
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .child(div().flex().gap_1().children(step_labels))
    }

    fn spindle_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let power_view = view.clone();
        let cw_view = view.clone();
        let ccw_view = view.clone();
        let rpm_view = view;

        card(&p)
            .w(px(360.0))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(section_label("SPINDLE", p))
                    .child(
                        Switch::new("spindle-power")
                            .checked(self.state.spindle_on)
                            .label(if self.state.spindle_on { "ON" } else { "OFF" })
                            .text_size(px(10.0))
                            .on_change(move |checked, _, cx| {
                                power_view.update(cx, |this, cx| {
                                    this.state.spindle_on = *checked;
                                    this.state.push_console(
                                        ConsoleLevel::Command,
                                        format!("M{} (mock)", if *checked { 3 } else { 5 }),
                                    );
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .h(px(48.0))
                    .child(
                        div()
                            .id("rpm-field")
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_end()
                            .px_2()
                            .rounded(px(7.0))
                            .bg(p.panel_alt)
                            .border_1()
                            .border_color(p.border)
                            .cursor_pointer()
                            .font_family(self.mono.clone())
                            .text_size(px(22.0))
                            .text_color(if self.state.spindle_on {
                                p.text
                            } else {
                                p.text_faint
                            })
                            .child(format!("{}", self.state.rpm))
                            .on_click(move |_, _, cx| {
                                rpm_view.update(cx, |this, cx| {
                                    this.state.open_keypad(KeypadTarget::Rpm);
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        Button::new("spindle-cw")
                            .label("CW")
                            .h(px(48.0))
                            .w(px(52.0))
                            .rounded(px(7.0))
                            .text_size(px(11.0))
                            .custom(mode_variant(cx, p, self.state.spindle_cw))
                            .on_click(move |_, _, cx| {
                                cw_view.update(cx, |this, cx| {
                                    this.state.spindle_cw = true;
                                    this.state.spindle_on = true;
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        Button::new("spindle-ccw")
                            .label("CCW")
                            .h(px(48.0))
                            .w(px(52.0))
                            .rounded(px(7.0))
                            .text_size(px(11.0))
                            .custom(mode_variant(cx, p, !self.state.spindle_cw))
                            .on_click(move |_, _, cx| {
                                ccw_view.update(cx, |this, cx| {
                                    this.state.spindle_cw = false;
                                    this.state.spindle_on = true;
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child("OVERRIDE")
                            .child(format!("{}%", self.state.spindle_override)),
                    )
                    .child(Slider::new(&self.spindle_slider).h(px(24.0))),
            )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::new(self.state.dark);
        let compact = window.viewport_size().width < px(1120.0);

        let mut main_row = h_flex()
            .gap_2()
            .flex_1()
            .min_h(px(0.0))
            .px_2()
            .child(self.left_rail(p, cx))
            .child(self.center(p, cx));
        if !compact {
            main_row = main_row.child(self.right_rail(p, cx));
        }

        let content = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .gap_2()
            .child(main_row)
            .child(self.bottom_dock(p, cx));

        let mut root = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(p.bg)
            .text_color(p.text)
            .font_family("sans-serif")
            .child(self.top_bar(p, cx))
            .child(content);

        let keypad_state = self.state.keypad.clone();
        if let Some(keypad) = keypad_state.as_ref() {
            root = root.child(keypad::overlay(
                keypad,
                p,
                cx.entity(),
                self.mono.clone(),
                self.state.unit(),
                cx,
            ));
        }
        root
    }
}

fn slider_value(event: &SliderEvent) -> f32 {
    match event {
        SliderEvent::Change(value) | SliderEvent::Release(value) => value.end(),
    }
}

fn mode_variant(cx: &App, p: Palette, active: bool) -> ButtonCustomVariant {
    if active {
        ButtonCustomVariant::new(cx)
            .color(p.accent_muted)
            .foreground(p.accent)
            .hover(p.raised_hover)
            .active(p.accent_muted)
    } else {
        ButtonCustomVariant::new(cx)
            .color(p.panel_alt)
            .foreground(p.text_dim)
            .hover(p.raised_hover)
            .active(p.raised)
    }
}

fn tab_button(cx: &App, p: Palette, label: LeftTab, active: bool, view: Entity<Shell>) -> Button {
    Button::new(format!("left-tab-{}", label.label()))
        .label(label.label())
        .flex_1()
        .h(px(44.0))
        .rounded_lg()
        .text_size(px(13.0))
        .custom(mode_variant(cx, p, active))
        .on_click(move |_, _, cx| {
            view.update(cx, |this, cx| {
                this.state.left_tab = label;
                cx.notify();
            });
        })
}

fn card(p: &Palette) -> Div {
    v_flex()
        .gap_2()
        .p_2()
        .rounded_lg()
        .border_1()
        .border_color(p.border)
        .bg(p.panel)
}

fn section_label(text: &'static str, p: Palette) -> Div {
    div()
        .text_size(px(9.5))
        .font_weight(FontWeight::BOLD)
        .text_color(p.text_dim)
        .child(text)
}

fn chip(text: &str, p: Palette) -> Div {
    div()
        .px_2()
        .h(px(22.0))
        .flex()
        .items_center()
        .rounded(px(5.0))
        .bg(p.panel_alt)
        .border_1()
        .border_color(p.border)
        .text_size(px(9.0))
        .font_weight(FontWeight::BOLD)
        .text_color(p.text_dim)
        .child(text.to_string())
}

fn legend(color: Hsla, text: &'static str, p: Palette) -> Div {
    h_flex()
        .items_center()
        .gap_1()
        .child(div().w(px(6.0)).h(px(6.0)).rounded(px(999.0)).bg(color))
        .child(div().text_size(px(9.0)).text_color(p.text_dim).child(text))
}

fn info_row(name: &str, milestone: &str, p: Palette) -> Div {
    h_flex()
        .items_center()
        .justify_between()
        .h(px(38.0))
        .px_2()
        .rounded(px(6.0))
        .bg(p.panel_alt)
        .child(
            div()
                .text_size(px(12.0))
                .text_color(p.text)
                .child(name.to_string()),
        )
        .child(chip(milestone, p))
}

fn num_cell(value: &str, p: Palette) -> Div {
    div()
        .flex_1()
        .text_size(px(11.0))
        .text_color(p.text_dim)
        .child(value.to_string())
}

fn console_color(level: ConsoleLevel, p: Palette) -> Hsla {
    match level {
        ConsoleLevel::Info => p.text_dim,
        ConsoleLevel::Command => p.info,
        ConsoleLevel::Ok => p.success,
        ConsoleLevel::Warn => p.warning,
    }
}
