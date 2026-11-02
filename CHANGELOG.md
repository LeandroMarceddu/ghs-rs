# Changelog

All notable changes to ghs-rs are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
ghs-rs is pre-1.0: every version before 1.0.0 is pre-alpha, may change without
notice, and carries no compatibility guarantee. Version numbers will follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) from 1.0.0 onward.
Until a release is tagged, changes stay under `Unreleased`; a version section
is added only when that release is cut, with its release date. No released
version is recorded yet.

## [Unreleased]

This initial entry summarizes the whole pre-alpha tree; no earlier release
section exists to diff against.

### Added

- **Protocol and domain** — serial-only grblHAL support with a capability-driven
  `$I+` handshake; status, realtime, system-command, parser-state, alarm/error,
  and runtime-enumeration handling. Unknown fields and extensions are ignored,
  and panels are gated by reported capabilities rather than a board name.
- **Simulator** — one deterministic grblHAL engine parameterized by a
  `ControllerProfile`, with `flexihal` as a subset of `generic-full`, plus an
  in-process loopback for tests and demo mode. It is not a network transport.
- **Serial transport** — port enumeration and selection, a dedicated reader
  thread with synchronized writes, hotplug watching, and a connection panel.
  Line settings are 8N1 with a selectable baud rate.
- **Engine and manual control** — a dedicated engine session with live DRO,
  hold-to-jog, MDI, feed, spindle, and rapid overrides, spindle and coolant
  controls, reset/stop, and a console. All commands pass through one safety
  gateway.
- **Jobs** — in-app program file browser and loading with a 20 MiB UTF-8
  bound; character-counting streaming against the reported receive buffer;
  pause, resume, and stop; a tool-change sheet; optional-stop, single-block, and
  block-delete run options; progress, elapsed time, and an ETA; a parsed program
  list with text/line-number search, filter, highlight, previous/next, and jump.
  Job recovery never auto-resumes.
- **Viewer** — parsed toolpath in 2D top view or 3D orbit view, with fit, pan,
  zoom, pinch, ground grid, work envelope, motion-style paths, a live tool
  marker, and height-map cells projected in both views.
- **Generated settings** — forms generated from `$ES`/`$EG` metadata and
  `$SED` descriptions, grouped and searchable, with boolean, bitfield,
  axis-mask, radio, numeric, text, and password controls, range validation, and
  read-back-verified writes.
- **Offsets and tools** — live `$#` values for G54–G59.3, G28/G30, G92, HOME,
  TLO, TLR, G51, and PRB; `G10 L20` zero controls with `$#` refresh; confirmed
  `G28`/`G30` goto and explicit `G92.1` clear; a WCS-only `offsets.nc` export
  with validated load-as-program restore; an app-side persisted tool table
  used by the M6 sheet; and a read-only controller tool table.
- **Probing** — Z touch plate, edge X/Y, corner, and center-circle cycles, plus
  rotation-angle and tool-length delta reporting. Every run uses a mandatory
  preflight, ordered `PRB` binding, and a confirmed `G10 L2` apply.
- **Height map** — bounded grid probing with a point/time estimate, preflight,
  partial-map invalidation, 2D/3D cell overlay, and CSV export. A constant map
  Z offset shifts effective heights without re-probing. Explicit Apply rewrites
  the loaded program with bilinear compensation, keeps the original for Revert,
  and labels the compensated job. Maps are saved per controller profile and
  reloaded with a WCS/G92/TLO frame check.
- **Probe offsets** — the probe mount XY offset is persisted per profile, and
  operator-entered positions are feature/probe-tip coordinates that the plan
  shifts to the controlled point. The height-map Z offset is also persisted.
- **Macros** — per-profile single-line MDI macros with an explicit RUN action
  and optional second confirmation. Macros never run on load, connect, or
  profile import.
- **Startup lines** — `$N0`/`$N1` read, edit, and read-back verification.
- **Profiles and backups** — identity-keyed controller profiles from `$I+`, app
  configuration, atomic saves, rolling backups, export/validate/import
  bundles, and identity-mismatch confirmation.
- **Touch shell** — the approved touch layout with 48 px primary controls,
  dark/light themes, and a resizable desktop layout from 1024×600.
- **Release workflow and CI** — CI runs formatting and clippy on Linux, the
  workspace test suite on Linux, and a debug app build on Linux, macOS, and
  Windows. A tag-driven workflow packages unsigned Linux, Windows, and macOS
  archives, writes `SHA256SUMS.txt`, and opens a draft GitHub release. Linux
  packaging includes a desktop entry; `docs/RELEASE.md` records the release
  gates.

### Changed

- The app shell now drives a live engine session against the in-process
  simulator or a selected serial port, with live DRO, MDI, console, and
  connection controls; `--mock` remains a static preview.
- The viewer and program list use the real parsed program instead of a
  placeholder, and current-line follow is suspended while a program search is
  active.
- The settings editor is generated from controller metadata instead of a
  hardcoded form; writes are explicit, idle-only, and read back.
- Offset goto and restore are explicit actions: `G28`/`G30` goto is confirmed,
  `G92.1` clear is separate, and `offsets.nc` loads only after validation and
  confirmation.
- Probe plans are emitted verbatim. Entered feature positions are converted to
  controlled-point coordinates by the plan using the persisted mount offset.
- Height-map Apply only loads the rewritten program and never starts it; Revert
  restores the exact original source, and neither action runs on map load.
- Release packaging uses the single workspace version and produces a draft
  release rather than publishing or signing automatically.

### Fixed

- **Arc tessellation** — arc sampling used the previous sample's parameter for
  the end check, collapsing the second half of a semicircle into one chord. The
  full arc is now tessellated, including the second-half midpoint.
- **Probe modal-word parsing** — a modal-only block after a `G38.2` move, such
  as `G91`, was treated as another probe move and failed for a missing axis. A
  probe now requires a motion word; a bare `G38.2` without an axis still errors.
- **Probe signal latch** — the simulator now keeps live probe contact separate
  from the last `PRB` result. Contact asserts on a successful probe and opens on
  the next non-probe motion (the retract), while `$#`/`[PRB]` keeps the result
  until a reset or another probe. `Pn` reports the live contact signal.
- **Cached PRB on reset** — a soft reset and `$RST=#`/`$RST=*` now invalidate
  the engine's cached probe result, so the offsets and probe panels cannot show
  a `PRB` the controller has already discarded.
- **Height-map unit preamble** — height-map compensation respects the source
  program's `G20`/`G21` mode and emits in those units instead of assuming
  millimetres; Revert restores the exact original source, including its `G21`
  preamble. Separately, `offsets.nc` restore requires one `G21` preamble and
  rejects a missing, duplicate, or `G20` preamble.

### Safety

- The UI and parsers never write bytes to the controller; motion is encoded as
  commands and decided by the engine's safety gateway.
- Motion is refused in `Alarm` and `Door` without an explicit, confirmed
  unlock. A status watchdog pauses a running job when telemetry stops, and the
  app never auto-resumes after an alarm, E-stop, or reconnect.
- Probing requires a ready link, `Idle`, spindle and coolant off, an explicit
  safe Z, a bounded search distance and feed, a probe signal or explicit
  confirmation, and loaded offsets. Results are correlated by order, and
  offset updates use a confirmed `G10 L2`.
- Height-map Apply requires a complete map, a ready and `Idle` controller, no
  active job, reported offsets, a matching WCS/G92/TLO frame, and an original
  program. Programs that leave the mapped area are refused rather than
  extrapolated; Apply only loads the result.
- Settings writes are idle-only and read back for verification. Macros require
  an explicit action and are refused in mock, `Alarm`/`Door`, a running job, or
  a non-`Idle` state. The stop/reset control stays visible and enabled.

### Known Limitations

- **No hardware validation.** The reference FlexiHAL / STM32F446 running
  grblHAL `1.1f.20240604` has not been available. All evidence is simulator and
  unit-test based, so every build is a simulator-only pre-alpha preview.
- **Unsigned packages.** Linux archives have no signature or distro package;
  Windows builds have no Authenticode signature or installer; macOS builds are
  not code-signed or notarized and have no `.app` bundle. Only per-file
  Gatekeeper/SmartScreen overrides are appropriate, and downloads should be
  checked against `SHA256SUMS.txt`.
- **No keymap editor.** Keyboard jog, gamepad mapping, and keymap persistence
  are not implemented; the app ships no configurable keymap.
- **No start-from-line or check mode.** The UI has no start-from-line or
  dry-run/check workflow, and the engine only accepts a start at line 1 because
  it cannot synthesize a modal preamble for a later line.
- **No advanced macros.** Macros are single-line MDI commands; parameter
  substitution, multi-line macros, special words, and `$` system commands are
  deferred.
- **3D is perspective-only.** There is no orthographic toggle, stock mesh, or
  shaded surface; the height overlay is a cell projection. Heavily overlapping
  paths can sort incorrectly in the painter's-algorithm renderer.
- **Post-v1.0 surfaces.** SD-card browsing/YModem, ATC flows, and MPG/pendant
  UI are parsed and modeled but have no UI in this preview. Multi-spindle,
  extra-axis, and gamepad UI are also deferred.
- **Other deferred work.** Localization, high-contrast theme, first-run
  onboarding, and the material-surfacing, gantry-squaring, and calibration
  wizards are not implemented.
