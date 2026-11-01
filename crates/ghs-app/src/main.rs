//! ghs-rs M0 touch-shell prototype.
//!
//! Run with `cargo run -p ghs-app`. There is no serial port and no engine here:
//! every control mutates mock state so the layout, touch targets, canvas, and
//! in-app keypad can be reviewed on Linux before M1 lands.
//!
//! Demo flags: `--light` starts in the light theme, `--keypad` opens the X
//! keypad at startup. Neither flag changes normal operation.
//!
//! Window policy:
//! - 1280x800 default (UX-01), 1920x1080 works by widening the centre column.
//! - 1024x600 minimum; below 1120 px the right rail collapses.
//! - Resizable, so a developer can drag it to portrait and inspect the reflow
//!   plan documented at the top of `shell.rs`.

mod keypad;
mod shell;
mod state;
mod theme;
mod viewer;

use gpui_kit::*;

fn main() {
    application().with_assets(assets::Assets).run(|cx| {
        // `init` registers the GPUI Kit component layers. `open_window` mounts
        // the Base `Root`, which owns overlays, focus, and dialogs.
        init(cx);
        // Demo switch for screenshots: `cargo run -p ghs-app -- --light`.
        let light = std::env::args().any(|arg| arg == "--light");
        theme::apply(!light, cx);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(80.0), px(60.0)),
                size: size(px(1280.0), px(800.0)),
            })),
            window_min_size: Some(size(px(1024.0), px(600.0))),
            titlebar: Some(TitlebarOptions {
                title: Some("ghs-rs - grblHAL touch shell (M0)".into()),
                ..Default::default()
            }),
            app_id: Some("ghs-rs".into()),
            is_resizable: true,
            ..Default::default()
        };

        if let Err(err) = open_window(options, cx, |_, cx| cx.new(shell::Shell::new)) {
            eprintln!("failed to open ghs-rs window: {err}");
            cx.quit();
        }
    });
}
