# ghs-rs — implementation plan

**Status:** M0 — bootstrap and technical spikes
**Updated:** 2026-11-01
**Reference hardware:** FlexiHAL / STM32F446, grblHAL `1.1f.20240604`

This is the canonical plan. It is deliberately narrower than a full ioSender
clone: the target is a first-class sender for a 3-axis grblHAL mill, with the
touch ergonomics of ioSender Touch. Features the reference machine cannot use
are out of scope until after v1.0.

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
| SD card | not reported | No SD/YModem work in v1.0 |
| ATC / MPG / network | not reported | Out of scope for v1.0 |
| Spindle | Huanyang VFD (`SPINDLE:Huanyang v1`) | RPM, CW/CCW, spindle override |
| AUX IO | 4 digital in, 4 digital out | Signals/AUX panel |
| Buffer | 64-byte RX, 1024 planner blocks | Character-counting streamer with headroom |

The simulator must reproduce this profile by default, and every UI panel must
be capability-gated so other grblHAL boards degrade gracefully.

> **Hardware access note.** The controller lives offline in a garden shed, so
> M1 is simulator-first. Hardware-in-the-loop validation is a gate at M2/M3
> once the board is reachable. The first hardware session will be captured with
> a standalone capture tool into one file, brought back on a USB stick, and
> imported as a fixture. No feature depends on internet access at the machine.

## 3. Scope

### In scope

- Serial connection, controller identification, capability discovery.
- Live status, DRO, work offsets, jogging, MDI, overrides, spindle, coolant.
- Program load, validation, streaming, pause/resume/stop, manual tool change.
- Program list, 2D toolpath, 3D toolpath, live tool marker.
- Settings editor generated from `$SED`/`$HELP`, with a bundled fallback.
- Offsets, tool table, macros, startup lines, keymap, app settings, profiles.
- Full milling probing suite and height-map compensation.
- Material surfacing, gantry squaring, stepper/XY calibration.
- Touch layout, on-screen keypad, portrait/landscape, themes, units.
- Localization, packaging, installer, documentation, safety onboarding.

### Out of scope (v1.0)

| Feature | Reason |
|---|---|
| Legacy Grbl | Reference machine and target are grblHAL |
| Lathe mode and lathe wizards | Not a lathe |
| Plasma THC | Not a plasma table |
| Drag knife | Not needed |
| Network transports (Telnet/WebSocket/TCP) | Serial only |
| FTP/WebDAV controller file transfer | No network transport |
| SD card / YModem | Reference controller has no SD card |
| ATC | Reference controller reports `TC`, not ATC |
| MPG/pendant | Reference controller has a keypad plugin, no MPG |
| Rotary / 4th–5th axis UI | 3-axis machine |
| Camera, Vectric direct output, HPGL/Excellon converters | Not required |

These can be revisited after v1.0, behind capability flags, without changing
the architecture.

## 4. Parity gates

**P1 — Core sender (v0.2).** A usable sender: connect, status, DRO, jog, MDI,
overrides, spindle/coolant, console, alarms, load and stream a program with
pause/resume/stop, manual tool change.

**P2 — Milling parity (v1.0).** Everything the reference machine needs:

- ioSender Touch layout and touch workflows.
- ioSender milling features: settings, offsets, tools, macros, 2D/3D viewer,
  full probing suite, height map, surfacing/calibration, program management,
  transforms, signals, backup/restore, localization.
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
   UI initially exposes X/Y/Z.
6. **mm is canonical.** G20/G21 changes display and protocol state, not stored
   coordinates. Domain uses `f64`; rendering uses `f32`.
7. **GPUI Kit 0.8.x, pinned exactly.** A thin `ghs-ui` layer contains the API
   churn.

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
| **M1 Protocol + simulator** | 2–3 wk | `ghs-core`, `ghs-proto`, `ghs-sim`, fixture replay | Connect to simulator; identity/capabilities/settings; live status at 10 Hz; streamer property tests |
| **M2 Manual control — v0.1** | 2–3 wk | Engine, serial transport, DRO/WCS, jog, overrides, spindle/coolant, MDI, home/unlock/reset, signals, touch shell | Operate simulator and the real board from the UI; guards and stop always visible |
| **M3 Job engine — v0.2** | 3 wk | Load/validate/stream, pause/resume/stop, tool change, single block/optional stop, start-from-line, timer/progress, console | 100k-line job with repeated pause/abort/reconnect; zero buffer overruns; hardware run |
| **M4 Program + 2D — v0.5** | 3 wk | G-code parser, geometry, 2D viewer, virtualized program list | 1M lines under 2 s; 60 fps pan/zoom; live marker; arcs/incremental/planes correct |
| **M5 Setup & data — v0.6** | 3 wk | Settings editor (`$SED`), offsets, tool table, macros, keymap, profiles, backup | Configure the FlexiHAL machine end-to-end; settings round-trip; restore verified |
| **M6 Probing + height map — v0.8** | 4 wk | Probe framework, Z/edge/center/rotation/tool-length/XY plate, height map, compensation | Golden tests per cycle; hardware sign-off; safe-Z and probe interlocks proven |
| **M7 3D, utilities, release — v1.0** | 4 wk | 3D viewer, surfacing/squaring/calibration, i18n, installers, first-run wizard, docs | P2 audit signed off; installers on 3 OSes; safety onboarding complete |

Total to v1.0: **~5.5–7 months** for one focused developer with AI assistance and
the machine available. The critical path is hardware validation of streaming,
probing, and spindle control, not code volume.

Post-v1.0 candidates, only if needed: gamepad, keypad/pendant integration,
camera, ATC, SD card, Workbench polish, 4th axis.

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
