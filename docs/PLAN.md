# ghs-rs — implementation plan

**Status:** M4 — real toolpath viewer; M3 job UI complete
**Updated:** 2026-11-01
**Reference hardware:** FlexiHAL / STM32F446, grblHAL `1.1f.20240604`

Protocol, serial, viewer geometry, the capability-configured simulator, the
G-code parser, the demo loopback bridge, the headless engine, the M3 job runner,
and the job UI are implemented. The approved touch shell runs live against the
in-process simulator with serial connect, hold-to-jog, a real program list,
job controls, progress, and the tool-change sheet. The M4 real viewer is in
progress; start-from-line, check mode, probing, settings, and hardware
validation remain.

This is the canonical plan. The **protocol layer targets grblHAL generally**:
it parses every status field, real-time command, system command, and runtime
enumeration the controller can report, and ignores unknown extensions
gracefully. The **v1.0 UI is focused on the reference 3-axis mill**, with every
panel capability-gated so other grblHAL controllers expose the features they
actually support. Lathe, plasma, drag knife, and network transports remain
explicitly out of scope.

## 1. Product

A native, cross-platform G-code sender for grblHAL:

- Windows, macOS, and Linux from one Rust codebase.
- Serial port only (USB/UART).
- Touch-first UI that also works well with a mouse and keyboard.
- Fast enough for a cheap mini-PC driving a 1280×800 or 1920×1080 touch screen.
- Safe by construction: every motion command passes through one guarded engine
  path; the UI never writes bytes to the controller.

## 2. Reference hardware profile

Recorded from the machine's `$I+` response (see
[hardware/flexihal-1.1f.20240604.md](hardware/flexihal-1.1f.20240604.md)):

| Property | Value | Consequence |
|---|---|---|
| Firmware | grblHAL `1.1f.20240604` | Target this build; no legacy Grbl compatibility layer |
| Axes | 3 (XYZ) | 3-axis UI; domain types remain 9-axis capable |
| Options | `ENUMS`, `RT+`, `HOME`, `ES`, `EXPR`, `TC`, `SED` | Top-bit realtime, expressions, settings descriptions, manual tool change |
| Signals | `HSDEP` | Feed hold, cycle start, door, E-stop, probe |
| Tool table | 0 controller entries | Tool table lives in the app; `TC` = manual M6 |
| SD card | not reported | Protocol parses `SD`; UI is post-v1.0 |
| ATC / MPG / network | not reported | Protocol parses ATC/MPG; UI is post-v1.0; network is out of scope |
| Spindle | Huanyang VFD (`SPINDLE:Huanyang v1`) | RPM, CW/CCW, spindle override |
| AUX IO | 4 digital in, 4 digital out | Signals/AUX panel |
| Buffer | 1024-byte RX, 64 planner blocks | Character-counting streamer uses the reported RX size |

The simulator is **one engine parameterized by a controller profile**, not a
separate implementation per board. `flexihal` is a strict subset of
`generic-full`; the same engine and UI code must work for both. We keep both
profiles as test fixtures because the *absence* paths matter: no `SD`, no `PC`,
no extra axes, an older build, and a different reported RX buffer are exactly
where fallback bugs hide. `generic-full` is the superset used for broad scenario coverage;
`flexihal` is the regression fixture for the real machine. There is no
board-specific product mode: every panel is driven by reported capabilities.

> **Hardware access note.** The controller lives offline in a garden shed, so
> M1 is simulator-first. Hardware-in-the-loop validation is a gate at M2/M3
> once the board is reachable. The first hardware session will be captured with
> a standalone capture tool into one file, brought back on a USB stick, and
> imported as a fixture. No feature depends on internet access at the machine.

## 3. Scope

### In scope

- Serial connection, controller identification, capability discovery.
- Full grblHAL protocol surface: status fields, realtime commands, system
  commands, parser state, probe results, tool change, MPG takeover, SD status.
- Runtime code sets and settings from `$EA`/`$EE`/`$ES`/`$EG`/`$SED`, with
  bundled fallbacks for older builds.
- Live status, DRO, work offsets, jogging, MDI, overrides, spindle, coolant.
- Program load, validation, streaming, pause/resume/stop, tool change.
- Program list, 2D toolpath, 3D toolpath, live tool marker.
- Generated settings editor, offsets, tool table, macros, startup lines,
  keymap, app settings, profiles.
- Full milling probing suite and height-map compensation.
- Material surfacing, gantry squaring, stepper/XY calibration.
- Touch layout, on-screen keypad, portrait/landscape, themes, units.
- Localization, packaging, installer, documentation, safety onboarding.

### Out of scope (v1.0)

| Feature | Reason |
|---|---|
| Legacy Grbl | Target is grblHAL |
| Lathe mode and lathe wizards | Not a lathe |
| Plasma THC | Not a plasma table |
| Drag knife | Not needed |
| Network transports (Telnet/WebSocket/TCP) | Serial only |
| FTP/WebDAV controller file transfer | No network transport |
| Camera, Vectric direct output, HPGL/Excellon converters | Not required |

### Post-v1.0, capability-gated

The protocol and domain layers already carry these; the UI is added after the
milling feature bar is solid:

- SD card browsing and YModem upload when the controller reports `SD`/`YM`.
- ATC flows when the controller reports `ATC=1`; manual `TC` is in v1.0.
- MPG/pendant UI when `MPG` is reported; the sender already pauses on `|MPG:1`.
- Multi-spindle panels when `$SPINDLESH` reports more than one.
- Extra-axis DRO/jog rows when `$I+` reports more than three axes.
- Gamepad support.

## 4. Parity gates

**P1 — Core sender (v0.2).** A usable sender: connect, status, DRO, jog, MDI,
overrides, spindle/coolant, console, alarms, load and stream a program with
pause/resume/stop, manual tool change.

**P2 — Milling parity (v1.0).** Everything the reference machine needs:

- ioSender Touch layout and touch workflows.
- ioSender milling features: settings, offsets, tools, macros, 2D/3D viewer,
  full probing suite, height map, surfacing/calibration, program management,
  transforms, signals, backup/restore, localization.
- Generic grblHAL protocol support, with runtime enumerations and
  capability-gated panels rather than a board-specific UI.
- Serial hotplug/reconnect, session logging, crash-safe configuration.
- Windows, macOS, Linux builds and installers.

A feature is **done** only when it: works against the simulator, has automated
tests, is verified on the FlexiHAL board, is usable without a mouse, is
documented, and is localized. The live checklist is [parity.md](parity.md).

## 5. Architecture (summary)

Details: [architecture.md](architecture.md).

```text
ghs-app        GPUI shell, touch layout, panels, keypad, gamepad
ghs-ui         design tokens and thin gpui-kit wrappers (introduced at M2)
ghs-engine     session, streamer, job runner, safety gateway (own thread)
ghs-proto      sans-IO grblHAL framing, status, commands, ACK tracking
ghs-core       axes, positions, units, WCS, settings, tools, capabilities
ghs-serial     serial reader/writer threads, hotplug, loopback
ghs-sim        deterministic grblHAL simulator for tests and demo mode
ghs-gcode      parser, expressions, parser-state emulation, toolpath (M4)
ghs-viewer     geometry cache, 2D/3D projection (M4)
ghs-probe      probe cycles and height map (M6)
ghs-config     profiles, backups, import/export (M5)
ghs-i18n       locales (M7)
```

Decisions already made:

1. **Sans-IO protocol core.** Deterministic, testable, no sockets or threads in
   the parser/streamer.
2. **Threads and channels, no async runtime.** A dedicated engine thread owns
   the session; a serial reader thread owns the port. This avoids an async
   runtime fighting GPUI's executor and keeps the binary small.
3. **Serial only.** An in-process byte-level loopback exists for the simulator
   and demo mode; it is not a user-facing network transport.
4. **Capability-driven UI.** `$I+` decides which panels exist.
5. **9-axis domain, 3-axis UI.** Positions and settings use fixed arrays; the
   UI initially exposes X/Y/Z and expands if the controller reports more.
6. **mm is canonical.** G20/G21 changes display and protocol state, not stored
   coordinates. Domain uses `f64`; rendering uses `f32`.
7. **GPUI Kit 0.8.x, pinned exactly.** A thin `ghs-ui` layer contains the API
   churn.
8. **Protocol breadth, UI depth.** Parse the full grblHAL surface and ignore
   unknown extensions; expose only what the controller supports.
9. **Runtime enumeration over hardcoded tables.** Prefer `$EA`/`$EE`/`$ES`/
   `$EG`/`$SED`; bundled tables are fallbacks, not the source of truth.

## 6. UI direction

Touch layout (default):

- Top mode bar: Job, Probe, Offset, Utility, Tools, Settings.
- Left rail: console / program list (switchable).
- Bottom control dock: MDI, DRO, work parameters, feed, jog, spindle.
- Right rail: coolant, macros, accessory panels.
- Center: active workspace (viewer, probing, settings, utilities).
- Minimum 48 px hit targets; no hover-only actions; in-app numeric keypad;
  long-press for advanced actions; portrait and landscape layouts.

Workbench layout (optional): the same panels in a dockable, resizable layout
for large monitors.

Themes: shop-floor dark (default), light, high contrast. Path colors are
configurable and color-blind safe.

## 7. Milestones

| Stage | Estimate | Ships | Exit gate |
|---|---|---|---|
| **M0 Bootstrap & spikes** | 1–2 wk | Workspace, CI (Linux/macOS/Windows), licenses, docs, hardware profile; GPUI shell, touch, 2D canvas, 3D projection, serial loopback spikes | Demo app at 60 fps with 200k segments, working keypad and layout; serial loopback proven |
| **M1 Protocol + simulator** | 2–3 wk | `ghs-core`, `ghs-proto`, one `ghs-sim` engine with `flexihal` (subset) and `generic-full` (superset) profiles, fixture replay | Connect to both profiles with identical code; identity/capabilities; `$EA`/`$EE`/`$ES`/`$EG`; live status at 10 Hz; streamer property tests |
| **M2 Manual control — v0.1** | 2–3 wk | Engine, serial transport, DRO/WCS, jog, overrides, spindle/coolant, MDI, home/unlock/reset, signals, touch shell | Operate simulator and the real board from the UI; guards and stop always visible |
| **M3 Job engine — v0.2** | 3 wk | Load/validate/stream, pause/resume/stop, tool change, single block/optional stop, start-from-line, timer/progress, console | 100k-line job with repeated pause/abort/reconnect; zero buffer overruns; hardware run |
| **M4 Program + 2D — v0.5** | 3 wk | G-code parser, geometry, 2D viewer, virtualized program list | 1M lines under 2 s; 60 fps pan/zoom; live marker; arcs/incremental/planes correct |
| **M5 Setup & data — v0.6** | 3 wk | Generated settings editor (`$ES`/`$EG`/`$SED`), offsets, tool table, macros, keymap, profiles, backup | Configure the FlexiHAL machine end-to-end; settings round-trip; restore verified |
| **M6 Probing + height map — v0.8** | 4 wk | Probe framework, Z/edge/center/rotation/tool-length/XY plate, height map, compensation | Golden tests per cycle; hardware sign-off; safe-Z and probe interlocks proven |
| **M7 3D, utilities, release — v1.0** | 4 wk | 3D viewer, surfacing/squaring/calibration, i18n, installers, first-run wizard, docs | P2 audit signed off; installers on 3 OSes; safety onboarding complete |

Total to v1.0: **~5.5–7 months** for one focused developer with AI assistance and
the machine available. The critical path is hardware validation of streaming,
probing, and spindle control, not code volume.

Post-v1.0, capability-gated: SD card/YModem, ATC, MPG/pendant, multi-spindle,
extra-axis UI, gamepad, Workbench polish. Protocol parsing for these lands in
M1; only the UI is deferred.

## 8. Execution model with subagents

I act as the orchestrator and integration owner. Subagents implement isolated,
well-specified chunks in parallel.

**Orchestrator owns**

- Architecture, public APIs, crate boundaries, root `Cargo.toml`.
- Integration, final review, formatting/lint/test gates, commits and releases.
- Scope decisions, parity checklist, hardware validation.

**Subagent roles**

- `protocol`: `ghs-core` + `ghs-proto`.
- `simulator`: `ghs-sim` and fixture replay.
- `serial`: `ghs-serial`, hotplug/reconnect, loopback.
- `ui-shell`: `ghs-app` and `ghs-ui` shell, theme, touch layout.
- `viewer`: `ghs-viewer` geometry and projection.
- `settings`: settings engine and forms.
- `probing`: `ghs-probe` and wizards.
- `qa`: test/CI audits, no production code unless assigned.

**Waves**

1. Protocol core, UI shell, viewer geometry (independent crates).
2. Simulator, serial transport, settings UI.
3. Job engine, program list, 2D viewer integration.
4. Probing, height map, utilities.
5. 3D, localization, release.

**Rules for every subagent**

- Read `docs/PLAN.md`, `docs/architecture.md`, `docs/hardware/...`, and
  `AGENTS.md` first.
- Edit only the paths assigned in the brief. Never edit root `Cargo.toml`,
  `docs/`, CI, or another crate.
- No commits; report files changed, test results, open questions, and risks.
- Run `cargo fmt` and `cargo clippy -p <crate> --all-targets -- -D warnings`;
  use `CARGO_TARGET_DIR=/tmp/opencode/ghs-target-<role>` to avoid lock contention.
- No network transports, no legacy Grbl, no lathe/plasma/drag-knife code.
- No copied ioSender source. Behavior and protocol compatibility only.

**Review gate** (orchestrator, before any commit)

1. Diff review for scope, API consistency, and safety.
2. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`,
   `cargo test --workspace`.
3. Contract tests against the hardware profile and simulator.
4. Update `docs/parity.md`; only then commit.

## 9. Testing and safety

- **Unit/golden:** protocol parsing, command encoding, G-code parser, geometry.
- **Property tests:** streamer never exceeds the receive buffer; malformed input
  never panics.
- **Simulator:** deterministic grblHAL profile with alarms, tool change, probing,
  overrides, and disconnect scenarios.
- **Replay:** recorded real sessions become fixtures.
- **Hardware-in-the-loop:** the FlexiHAL board, with a documented checklist for
  every release.
- **UI:** GPUI headless tests and layout snapshots at 1280×800, 1920×1080, and
  portrait.
- **Fuzzing:** protocol and G-code input.

Safety rules are non-negotiable: single command gateway, motion gated on state,
always-visible stop/reset, confirmations for probing/goto/run-from-line, link
watchdog, no auto-resume after alarm or reconnect, probe-connected and safe-Z
interlocks, and check mode promoted in the UI. See [SAFETY.md](SAFETY.md).

## 10. Performance budgets

| Metric | Budget |
|---|---|
| Cold start | < 1.5 s |
| Idle CPU at 10 Hz status | < 2% of one core |
| Program parse (1M lines) | < 2 s, < 300 MB |
| Viewer | 60 fps with 200k visible segments |
| UI interaction latency | < 50 ms |
| Streaming | Limited by controller; no UI stalls |

## 11. Risks

| Risk | Mitigation |
|---|---|
| GPUI pre-1.0 API churn | Pin exact versions; thin `ghs-ui`; upgrade sprints |
| Linux GPUI/X11/Wayland issues | Spike in M0 before deeper work |
| 3D performance in a 2D painter | Software projection first; decimation/caching; wgpu fallback |
| Streaming correctness | Sans-IO core, property tests, simulator, hardware early |
| Real-machine safety | Interlocks, dry runs, HIL checklist, conservative defaults |
| Intermittent hardware access | Simulator-first M1; recorded fixtures; HIL gate when the board is reachable |
| Scope creep | Explicit exclusions; parity checklist; v1.0 gate |
| Long GPUI compile times | Shared target cache for orchestrator; per-agent target dirs |
| Serial quirks (DTR/RTS, permissions) | Dedicated transport tests; document dialout setup |

## 12. Decision log

| Date | Decision |
|---|---|
| 2026-11-01 | v1.0 = P2 milling parity; no full-ioSender P3 gate |
| 2026-11-01 | Serial only; no network transports |
| 2026-11-01 | grblHAL only; no legacy Grbl compatibility mode |
| 2026-11-01 | No lathe, plasma THC, or drag knife |
| 2026-11-01 | Target hardware: FlexiHAL / STM32F446, grblHAL 1.1f.20240604 |
| 2026-11-01 | CI on Windows, macOS, Linux from day one |
| 2026-11-01 | Touch-first layout plus optional dockable Workbench |
| 2026-11-01 | MIT OR Apache-2.0; ioSender credited in NOTICE |
| 2026-11-01 | Subagent execution with orchestrator review gates |
