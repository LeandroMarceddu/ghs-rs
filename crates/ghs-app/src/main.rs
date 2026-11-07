//! ghs-rs grblHAL Sender.
//!
//! Run with `cargo run -p ghs-app`. The default startup is the M2 vertical
//! slice: an in-process FlexiHal simulator, one engine thread, and live DRO,
//! state, overrides, spindle, coolant, signals, console, MDI and jog. The UI
//! only sends typed engine commands; it never writes controller bytes.
//!
//! Demo flags: `--mock` keeps the original mock-state shell for design review,
//! `--light` starts in the light theme, `--high-contrast` starts in the
//! high-contrast theme, `--keypad` opens the X keypad at
//! startup, and `--connection-panel` (or `--connection-panel=serial`) opens
//! the transport picker. `--demo-toolchange` preloads the two-tool demo that
//! stops at `M6 T2`; `--demo-m0` preloads the same pocket with an `M0` pause;
//! `--demo-start` presses the job Start action once after the demo program
//! loads (screenshot switch: it exercises the same path as the toolbar
//! button). `--demo-check-confirm` opens the check-run confirmation without
//! confirming it, and `--demo-check` presses CHECK then START CHECK through
//! the same model actions for a dry-run status capture. `--demo-program-check`
//! loads a deterministic local sample through the normal demo load path and
//! opens Utility at the read-only `PROGRAM CHECK` section. The sample has feed,
//! tool, probe, and machine-coordinate warnings; the flag never starts the job
//! and sends nothing from the validation model. `--viewer-3d` starts the job
//! viewer in the orbit view; 2D top is the default.
//! `--viewer-stock-off` hides the inferred stock box for a capture; it changes
//! no controller state. `--settings` opens the generated settings workspace directly;
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
//! `--demo-rotation` seeds the Utility rotate fields with a reproducible local
//! sample and opens the rotation preview; the sample contains one `R` arc so
//! the radius-to-center warning is visible. `--demo-rotation-card` seeds the
//! same preview but leaves the sheet closed for the card capture, and
//! `--demo-rotation-confirm` additionally checks the load confirmation. All
//! three are local arithmetic: they load, start, and send nothing.
//! `--demo-scale` seeds the Utility scale fields with a reproducible local
//! sample and opens the scale preview; the sample contains one `R` arc so the
//! radius-to-center warning is visible. `--demo-scale-card` seeds the same
//! preview but leaves the sheet closed for the card capture, and
//! `--demo-scale-confirm` additionally checks the load confirmation. All three
//! are local arithmetic: they load, start, and send nothing.
//! `--demo-calibration` fills the stepper-calibration fields and shows the
//! proposal without applying it; `--demo-calibration-confirm` opens the write
//! confirmation. `--demo-gantry` loads a reproducible 500 mm / 0.250 mm sample
//! into the gantry-squaring calculator and opens its sheet; `--demo-gantry-card`
//! loads the same sample with the sheet closed so the Utility card can be
//! captured. Both are local arithmetic only: neither moves nor adjusts the
//! gantry, writes a setting, or starts a job.
//! `--demo-soft-limits` enables `$20` through the normal settings
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
//! Apply/Confirm actions for simulator evidence. `--demo-probe-tlo` runs the
//! tool-length card, opens the inline `APPLY TLO` confirmation with the
//! computed current/delta/proposed values, and presses the same confirm action
//! once; the applied state comes only from a real `$#` read-back.
//! `--demo-probe-tlo-confirm` stops at that confirmation without confirming it,
//! for the confirmation screenshot. `--demo-probe-defaults` opens Probe, waits
//! for the controller profile, and presses the same `SAVE DEFAULTS` action
//! once; it writes only local profile JSON and sends no probe command.
//! `--demo-probe-macros` fills the PRE/POST macro editor with valid sample
//! text and opens the sheet without saving; `--demo-probe-macros-saved` saves
//! the same sample through the normal SAVE MACROS path and leaves the sheet
//! closed so the summary bar can be captured. Both write only local profile
//! JSON on the saved path and send no probe command. `--demo-probe-macros-error`
//! opens the same sheet with a forbidden `G0 X0` line and its validation error,
//! using the normal model refusal; it writes nothing.
//! `--demo-height-map-apply` and
//! `--demo-height-map-revert` press the same explicit height-map Apply/Confirm
//! and Revert actions after the 4x4 demo map completes.
//! `--demo-height-map-confirm` opens the confirmation step without confirming;
//! `--demo-height-map-job` switches to the Job workspace marker.
//! `--demo-height-map-save` saves the completed demo map through the normal
//! profile path, and `--demo-height-map-open-saved` then opens that record in
//! the read-only saved-map browser. Normal startup never applies or starts
//! anything.
//! `--demo-reset` opens Utility at the `CONTROLLER RESET` card with the local
//! backup prompt and all four targets, acknowledging nothing and sending
//! nothing. `--demo-reset-confirm` exports a local backup through the same
//! action the card uses, then opens the `$RST=#` confirmation without
//! confirming it. Neither flag sends a reset, reconnects, homes, or starts a
//! job.
//! `--demo-setup` opens the first-run safety/setup wizard at SAFETY with
//! nothing checked; `--demo-setup-units` acknowledges the checklist and opens
//! UNITS, and `--demo-setup-review` does the same and opens REVIEW. The wizard
//! never connects, homes, jogs, probes, or writes controller settings, and no
//! demo setup flag completes it or changes `setup_complete`.
//! `--file-browser=PATH` opens the in-app program file browser at PATH (a
//! folder, or a program file whose folder opens with the file selected);
//! `--demo-load-file=PATH` loads that file through the same bounded reader and
//! guarded `LoadProgram` path once the simulator is ready, without starting it.
//! `--demo-recent-files` seeds three fixed recent-file records through the
//! normal app-config save path and opens the `LOAD PROGRAM` sheet with the
//! `RECENT FILES` list; it loads and starts nothing. Dropping one program file
//! on the Job workspace uses the same bounded reader and `LoadProgram` guard
//! as the browser and never starts the job. `--demo-drop-hint` renders the
//! drop hint over the Job workspace without an OS drag, for screenshots.
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
//! `--demo-job-history` seeds three fixed records through the normal local
//! `app.json` save path and opens Utility at the `JOB HISTORY` card;
//! `--demo-job-history-clear` seeds the same records and opens the inline
//! `CLEAR ALL HISTORY` / `CANCEL` confirmation without clearing;
//! `--demo-job-history-sheet` opens the all-records history sheet. All three
//! flags write only the app config and never send a controller command.
//! `--size WxH` (or `--size=WxH`)
//! opens at an explicit logical size for layout review, e.g. `--size 1920x1080`
//! or `--size 1080x1920`; `--portrait` is the 1080x1920 shorthand for portrait
//! screenshots, and `--size` wins when both are present. `--fullscreen` pins
//! the shell to the current output.
//!
//! Window policy:
//! - 1280x800 default (UX-01), 1920x1080 works by widening the centre column.
//! - 1024x600 minimum; below 1120 px the landscape right rail collapses.
//! - A viewport at least 1.15x taller than it is wide reflows to one vertical
//!   scrolling column with a two-row top bar (portrait, UX-03). The layout is
//!   computed from width and height, not from a device name.
//! - Resizable, so a developer can drag it between the three classes.

mod config;
mod connection;
mod console_log;
mod controller_reset;
mod file_browser;
mod height_compensation;
mod height_map_library;
mod i18n;
mod keymap;
mod keypad;
mod layout;
mod live;
mod macros;
mod offsets;
mod offsets_export;
mod pins;
mod probe;
mod settings;
mod setup;
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
        // Start with the screenshot/review flag or the dark-toned default.
        // `Shell::new` applies the persisted mode from `app.json` before the
        // first frame; this only prevents a flash with the wrong palette.
        let launch = theme::launch_override(std::env::args().skip(1))
            .unwrap_or(ghs_config::ThemeMode::System);
        theme::apply(launch, cx);

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
/// render anything useful below that. `--portrait` is the 1080x1920 shorthand
/// used by portrait screenshots; an explicit `--size` takes precedence.
fn window_size_override() -> Option<(f32, f32)> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Some(size) = size_argument(&args) {
        return Some(size);
    }
    if args.iter().any(|arg| arg == "--portrait") {
        return Some((1080.0, 1920.0));
    }
    None
}

/// Parse the first well-formed `--size` argument from `args`.
fn size_argument(args: &[String]) -> Option<(f32, f32)> {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let raw = if arg == "--size" {
            args.next()?
        } else if let Some(value) = arg.strip_prefix("--size=") {
            value
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
