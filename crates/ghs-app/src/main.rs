//! ghs-rs grblHAL Sender.
//!
//! Run with `cargo run -p ghs-app`. The default startup is the M2 vertical
//! slice: an in-process FlexiHal simulator, one engine thread, and live DRO,
//! state, overrides, spindle, coolant, signals, console, MDI and jog. The UI
//! only sends typed engine commands; it never writes controller bytes.
//!
//! Demo flags: `--mock` keeps the original mock-state shell for design review,
//! `--light` starts in the light theme, `--keypad` opens the X keypad at
//! startup, and `--connection-panel` (or `--connection-panel=serial`) opens
//! the transport picker. `--demo-toolchange` preloads the two-tool demo that
//! stops at `M6 T2`; `--demo-m0` preloads the same pocket with an `M0` pause;
//! `--demo-start` presses the job Start action once after the demo program
//! loads (screenshot switch: it exercises the same path as the toolbar
//! button). `--demo-check-confirm` opens the check-run confirmation without
//! confirming it, and `--demo-check` presses CHECK then START CHECK through
//! the same model actions for a dry-run status capture. `--viewer-3d` starts the job viewer in the orbit view; 2D top is
//! the default. `--settings` opens the generated settings workspace directly;
//! `--settings-group=NAME` selects a reported group in that workspace;
//! `--demo-settings-write` changes one safe integer through the normal
//! pending/Apply/read-back path for the applied screenshot. `--offsets`,
//! `--tools`, `--utility`, `--startup-lines`, and `--probe` open those
//! workspaces directly. `--pins` opens the read-only controller pin/limit
//! diagnostics sheet and runs the same explicit REFRESH the Utility button
//! uses. `--keymap` opens the Utility workspace at the machine
//! keymap section. `--demo-surfacing` opens the material surfacing wizard and
//! generates a preview without loading or starting it; `--demo-transform` opens
//! the line-number transform preview for the loaded demo program.
//! `--demo-calibration` fills the stepper-calibration fields and shows the
//! proposal without applying it; `--demo-calibration-confirm` opens the write
//! confirmation. `--demo-soft-limits` enables `$20` through the normal settings
//! write/read-back path, and `--demo-over-travel` then loads a program whose
//! bounds exceed the reported travel and presses START so the over-travel sheet
//! is shown without sending a start.
//! `--demo-tools` seeds the app-side tool table after the controller profile
//! loads. `--macros` opens the Utility workspace with the
//! user-macro section, while `--demo-macros` seeds two profile macros through
//! the normal save path; `--demo-macro-edit` then opens the macro editor and
//! `--demo-macro-confirm` opens the run confirmation without confirming it.
//! `--demo-startup-write` edits `$N0` and presses the
//! same APPLY / CONFIRM WRITE actions as a tap, then waits for the read-back so
//! the VERIFIED state can be captured; normal startup never writes a startup
//! line. `--demo-offsets-write` sends one real `G10 L20` through MDI and
//! refreshes `$#` for the applied-offsets screenshot. `--demo-offsets-goto`
//! opens the G28 confirmation sheet through the normal request action without
//! confirming it. `--demo-offsets-export` writes the fixed
//! `exports/offsets.nc` from live `$#` values through the same export action;
//! `--demo-offsets-restore` then validates it and opens the preview sheet
//! without loading it. `--probe-card=z|edge-x|
//! edge-y|corner|circle|rotation|tool|height` opens one probe cycle once live
//! telemetry and `$#` are available; `--demo-probe-check` stands in for the
//! operator's "probe connected" confirmation on a board without `PC`;
//! `--demo-probe-run` and `--demo-probe-apply` press the same explicit RUN and
//! Apply/Confirm actions for simulator evidence. `--demo-height-map-apply` and
//! `--demo-height-map-revert` press the same explicit height-map Apply/Confirm
//! and Revert actions after the 4x4 demo map completes.
//! `--demo-height-map-confirm` opens the confirmation step without confirming;
//! `--demo-height-map-job` switches to the Job workspace marker.
//! `--demo-height-map-save` saves the completed demo map through the normal
//! profile path, and `--demo-height-map-open-saved` then opens that record in
//! the read-only saved-map browser. Normal startup never applies or starts
//! anything.
//! `--file-browser=PATH` opens the in-app program file browser at PATH (a
//! folder, or a program file whose folder opens with the file selected);
//! `--demo-load-file=PATH` loads that file through the same bounded reader and
//! guarded `LoadProgram` path once the simulator is ready, without starting it.
//! `--demo-breakpoints` enables three demo breakpoints through the normal
//! toggle action and shows the Program tab; `--demo-run-to` does the same and
//! opens the run-to confirmation without confirming it.
//! `--console-filter=all|cmd|info|ok|warn|error` preselects a Console view
//! filter after the session starts (view only; the ring buffer keeps every
//! line), and `--demo-console-save` writes one `exports/console.log` snapshot
//! through the same SAVE LOG action for the export screenshot. `--console-tab`
//! reveals the Console after the demo program loads (a load normally shows the
//! Program tab); `--console-filter` implies it. Normal startup filters nothing,
//! writes no log file, and keeps the load behaviour unchanged.
//! `--size WxH` (or `--size=WxH`)
//! opens at an explicit logical size for layout review, e.g. `--size 1920x1080`
//! or `--size 1080x1920`; `--fullscreen` pins the shell to the current output.
//!
//! Window policy:
//! - 1280x800 default (UX-01), 1920x1080 works by widening the centre column.
//! - 1024x600 minimum; below 1120 px the right rail collapses.
//! - Resizable, so a developer can drag it to portrait and inspect the reflow
//!   plan documented at the top of `shell.rs`.

mod config;
mod connection;
mod console_log;
mod file_browser;
mod height_compensation;
mod height_map_library;
mod i18n;
mod keymap;
mod keypad;
mod live;
mod macros;
mod offsets;
mod offsets_export;
mod pins;
mod probe;
mod settings;
mod shell;
mod startup;
mod state;
mod theme;
mod tools;
mod utilities;
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

        // Screenshot/layout-review override; defaults to the UX-01 size.
        let (width, height) = window_size_override().unwrap_or((1280.0, 800.0));
        // `--fullscreen` pins the shell to the output for kiosk-style review
        // captures; it also makes headless (nested compositor) sizes exact.
        let fullscreen = std::env::args().any(|arg| arg == "--fullscreen");
        let bounds = Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(width), px(height)),
        };
        let options = WindowOptions {
            window_bounds: Some(if fullscreen {
                WindowBounds::Fullscreen(bounds)
            } else {
                WindowBounds::Windowed(Bounds {
                    origin: point(px(80.0), px(60.0)),
                    size: size(px(width), px(height)),
                })
            }),
            window_min_size: Some(size(px(1024.0), px(600.0))),
            titlebar: Some(TitlebarOptions {
                title: Some("ghs-rs - grblHAL Sender".into()),
                ..Default::default()
            }),
            app_id: Some("ghs-rs".into()),
            is_resizable: true,
            ..Default::default()
        };

        if let Err(err) = open_window(options, cx, |window, cx| {
            cx.new(|cx| shell::Shell::new(window, cx))
        }) {
            eprintln!("failed to open ghs-rs window: {err}");
            cx.quit();
        }
    });
}

/// Read `--size WxH` or `--size=WxH` from the process arguments.
///
/// Returns `None` for a missing or malformed value so the caller keeps the
/// default 1280x800 window. Values below 320 px are ignored: the shell cannot
/// render anything useful below that.
fn window_size_override() -> Option<(f32, f32)> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let raw = if arg == "--size" {
            args.next()?
        } else if let Some(value) = arg.strip_prefix("--size=") {
            value.to_string()
        } else {
            continue;
        };
        let (width, height) = raw.split_once(['x', 'X'])?;
        let width = width.parse::<f32>().ok()?;
        let height = height.parse::<f32>().ok()?;
        if width >= 320.0 && height >= 320.0 {
            return Some((width, height));
        }
        return None;
    }
    None
}
