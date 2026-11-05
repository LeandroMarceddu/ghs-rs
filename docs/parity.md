# Parity checklist

Target: **P2 milling parity** with ioSender for the reference FlexiHAL machine,
using the ioSender Touch interaction model.

The protocol layer targets grblHAL generally and parses the full command,
status, and enumeration surface. The v1.0 UI is the reference machine; rows
marked `post-v1.0` are parsed and modeled in M1 but their UI is deferred.

Status values: `todo`, `partial`, `done`.

A row is `done` here when the feature works in code and is exercised against
the simulator or in unit tests. **Hardware validation, mouse-free use, and full
localization are tracked globally**, not repeated on every row: no row meets the
full release definition until the FlexiHAL gate passes. See
[PARITY-AUDIT.md](PARITY-AUDIT.md) for the evidence-based audit snapshot.

## Layer status (2026-11-05)

| Layer | State | Evidence |
|---|---|---|
| Protocol/domain | Implemented | full grblHAL status/realtime/system/enumeration surface; fixture tests |
| Serial | Implemented | PTY/port tests; hardware modem-control/unplug validation pending |
| Simulator | Implemented | FlexiHAL fixture and generic-full profile; probe/check scenarios |
| G-code | Implemented (M4 core) | parser/arcs/cycles/height-map rewrite; expressions/flow control deferred |
| Viewer geometry | Implemented | 2D/3D camera, render plans, 200k-segment budget |
| Engine | Implemented | session, job runner, check mode, tools/startup/probe events |
| App shell | Implemented | approved touch layout, live demo/serial, localized primary UI |
| Program UI | Implemented | parsed list, follow, search, breakpoints, run-to, check |
| Viewer UI | Implemented | 2D/3D toolpath, marker, pan/zoom, height-map overlay |
| Settings UI | Implemented | generated `$ES`/`$EG`, verified writes |
| Offsets/tools | Implemented | live `$#`, G10, goto/clear, offsets.nc, app/controller tool tables |
| Probing | Implemented | Z/edge/corner/circle/rectangle/XY plate; TLO apply; height map Apply/Revert |
| Config/backup | Implemented | profiles, atomic saves, backup bundles, locale/keymap |
| Localization | Primary UI | en/it/de/nl; secondary help/warning text remains English |
| Hardware | Pending | board offline in the shed; simulator-first validation |

Rows below are the feature-level checklist. A protocol/simulator row can be
implemented before its UI or hardware validation row is complete.

## Communication and protocol

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| COM-01 | Serial port enumeration and selection on Linux, macOS, Windows | M1 | done | `serialport` crate; dialout note |
| COM-02 | Connect/disconnect with baud rate, 8N1 | M1 | done | Default 115200 |
| COM-03 | Optional DTR reset for 8-bit boards | M2 | partial | Default off for STM32F446 |
| COM-04 | Banner handshake and soft reset | M1 | done | Timeout and retry policy |
| COM-05 | `$I` and `$I+` capability parsing | M1 | done | Axes, all NEWOPT values, signals, plugins, buffers, spindles |
| COM-06 | `$ES`/`$EG` settings enumeration and `$SED` descriptions | M1 | done | Generated schema; `$$` fallback |
| COM-07 | `$#` offsets and `$G` parser state | M1 | done | Parse all reported fields |
| COM-08 | Adaptive status polling and full report `0x87` | M1 | done | 10 Hz active, 2 Hz idle |
| COM-09 | All grblHAL status fields | M1 | done | MPos, WPos, Bf, Ln, FS, Pn, WCO, WCS, Ov, A, H, D, Sc, TLR, In, SD, MPG, FW |
| COM-10 | All realtime commands | M1 | done | Reset, hold, resume, jog cancel, door, overrides, spindle stop, coolant, optional stop, single block |
| COM-11 | `RT+` top-bit realtime negotiation | M1 | done | Reference build advertises RT+ |
| COM-12 | Messages, errors, alarms with descriptions | M1 | done | `$EA`/`$EE` plus `[MSG:...]`; bundled fallback |
| COM-13 | Probe result (`PRB`) parsing | M1 | done | Used by the probing suite |
| COM-14 | Disable controller auto-reporting on connect | M1 | todo | Setting `$481` when present |
| COM-15 | Hotplug, reconnect, and status watchdog | M2 | done | Never auto-resume a job |
| COM-16 | Session recording and replay | M1 | todo | Real captures become fixtures |
| COM-17 | Check mode (`$C`) | M3 | done | Dry run with engine state machine |
| COM-18 | In-process loopback simulator for demo and tests | M1 | done | Not a user-facing network transport |
| COM-19 | Runtime alarm/error code sets (`$EA`, `$EE`) | M1 | done | Fallback tables only when unavailable |
| COM-20 | Runtime settings and groups (`$ES`, `$EG`) | M1 | done | Data types, formats, min/max, reboot flags |
| COM-21 | Parser state report (`$G`, `0x83`) | M1 | done | All grblHAL codes |
| COM-22 | Pins, limit state, last events (`$PINS`, `$PINSTATE`, `$LIM`, `$LEV`) | M1 | done | Reference pipe forms; upstream comma forms open |
| COM-23 | Spindle and port enumeration (`$SPINDLES`, `$SPINDLESH`, `$PORTS`) | M1 | partial | Multi-spindle capable |
| COM-24 | Manual tool change protocol (`Tool` state, `0xA3`) | M1 | done | Jog and offsets while pending |
| COM-25 | MPG takeover (`MPG:1/0`) | M1 | done | Telemetry/pause; MPG UI post-v1.0 |
| COM-26 | SD mount and streaming status (`SD:`) | M1 | done | Parse only; UI post-v1.0 |
| COM-27 | Startup sequence without banner or DTR reset | M1 | done | 400–500 ms listen, `0x87`, MPG wait |
| COM-28 | Error persistence handling | M1 | done | Clear with blank line or `$` before resume |
| COM-29 | Multi-spindle status fields (`SP<n>`) | M1 | done | Parse; UI post-v1.0 |
| COM-30 | Extra-axis status and capabilities | M1 | done | 9-axis domain; UI expands if reported |

## Machine control

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| CTL-01 | DRO for all axes and work coordinate systems | M2 | done | X/Y/Z first, 9-axis capable |
| CTL-02 | Zero axes and set position | M2 | done | Per axis and all |
| CTL-03 | Goto and park | M2 | partial | Confirmation for Goto |
| CTL-04 | Step and continuous jog, configurable distances/feeds, keyboard jog | M2 | partial | Keyboard one-step; hold pointer-only; jog feed global |
| CTL-05 | Jog cancel and pendant-safe behavior | M2 | done | `0x85` |
| CTL-06 | Feed override, fine and coarse, reset | M2 | done | Slider, ±1/±10, reset |
| CTL-07 | Rapid override | M2 | done | 100/50/25 compact chip |
| CTL-08 | Spindle control and override | M2 | done | On/off, CW/CCW, RPM, stop |
| CTL-09 | Coolant mist, flood, fan | M2 | done | Capability-gated |
| CTL-10 | MDI with history | M2 | done | 10-entry session history |
| CTL-11 | Home, unlock, reset, sleep | M2 | done | Confirmed sleep |
| CTL-12 | Signals, pins, and AUX IO display | M2 | done | Pn plus diagnostics panel |
| CTL-13 | Macros with parameters and special words | M5 | partial | Single-line MDI editor/run; params/system/multi-line deferred |
| CTL-14 | Startup lines | M5 | done | Read, edit, read-back verify |
| CTL-15 | Keymap editor | M5 | done | Ten machine actions, runtime rebind |
| CTL-16 | Work offsets editor, G54–G59.3, G10 | M5 | done | Edit, zero, goto, G92 clear |
| CTL-17 | App-side tool table and manual M6 workflow | M5 | done | Controller table read-only |
| CTL-18 | Metric and imperial units | M2 | partial | mm canonical |
| CTL-19 | Themes and DPI scaling | M5 | partial | Dark, light, high contrast |

## Jobs and program management

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| JOB-01 | Open, drag-and-drop, recent files | M3 | partial | In-app browser/load; drag-drop/recent open |
| JOB-02 | Validate program before streaming | M3 | partial | Syntax, tools, feeds |
| JOB-03 | Character-counting streaming with `Bf` top-up | M3 | done | Uses the reported RX buffer |
| JOB-04 | Pause, resume, stop | M3 | done | Hold, resume, reset semantics |
| JOB-05 | Manual tool change prompt and resume | M3 | done | `TC`, persistent tool plugin |
| JOB-06 | Optional stop, single block, block delete | M3 | done | RUN OPTIONS sheet |
| JOB-07 | Start-from-line, run-to-line, breakpoints | M3 | partial | Run-to + breakpoints; start-from-line open |
| JOB-08 | Progress, elapsed time, ETA | M3 | done | Progress/time and ETA |
| JOB-09 | Console with filtering and file log | M3 | done | Bounded console, filters, 64 KiB snapshot |
| JOB-10 | Dry run and check mode | M3 | done | `$C`; never moves |
| JOB-11 | Defined reconnect and interruption policy | M3 | done | No silent resume |
| JOB-12 | Program list with follow, search, context menu | M4 | partial | Follow/search/jump done; context menu open |
| JOB-13 | Add line numbers, strip comments | M4 | done | Non-destructive source transforms |
| JOB-14 | Job statistics and run history | M4 | todo | Local only |

## Viewer

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| VIEW-01 | G-code parser for lines, arcs, canned cycles, planes, units, incremental mode | M4 | done | grblHAL dialect |
| VIEW-02 | Expressions, parameters, and flow control | M4 | todo | `EXPR` supported |
| VIEW-03 | Parser-state emulation with offsets, scaling, rotation, tool length | M4 | done | Used for toolpath |
| VIEW-04 | 2D pan, zoom, fit, grid, bounds | M4 | done | 60 fps |
| VIEW-05 | Live tool marker | M4 | done | Status-driven |
| VIEW-06 | Color by motion type, layer, or tool | M4 | done | Configurable, color-blind safe |
| VIEW-07 | 3D orbit, pan, zoom, stock, and tool | M7 | partial | 3D camera/grid/envelope/marker; stock mesh open |
| VIEW-08 | Height map overlay | M6 | done | 2D cells and 3D projection |
| VIEW-09 | 1M-line and 200k-segment performance | M4 | partial | Benchmarked; CI threshold open |
| VIEW-10 | Program limits and outline display | M4 | done | Bounds card; over-travel confirmation |

## Setup and persistence

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| SETUP-01 | Settings editor generated from `$ES`/`$EG`/`$SED` | M5 | done | Fallback for older builds |
| SETUP-02 | Grouped, searchable, typed, validated settings | M5 | done | Axis arrays |
| SETUP-03 | Setting write, read-back verification, backup and restore | M5 | done | EEPROM caution |
| SETUP-04 | Stepper and axis calibration | M5 | done | Steps/mm wizard; backlash open |
| SETUP-05 | App settings and controller profiles | M5 | done | Keyed by `$I` identity |
| SETUP-06 | Import and export settings, offsets, tools | M5 | partial | Backup bundle; offsets `.nc` open |
| SETUP-07 | Controller reset (`$RST`) with backup prompt | M5 | done | Four targets; backup + two-step confirm |
| SETUP-08 | Keymap persistence | M5 | done | Machine actions; gamepad later |
| SETUP-09 | Startup line persistence | M5 | done | `$N` read/edit/verify |
| SETUP-10 | Tool table persistence | M5 | done | App-side |
| SETUP-11 | Offsets backup and restore | M5 | done | WCS-only `offsets.nc`; load-as-program |

## Probing

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| PROBE-01 | Z touch plate | M6 | done | Safe Z, retries |
| PROBE-02 | Tool length probing | M6 | done | Reports delta; confirmed `G43.1`/`G49`, `$#` verify |
| PROBE-03 | Edge finder X/Y, inside and outside | M6 | done | Edge offset |
| PROBE-04 | Corner finder | M6 | done | XY touch plate |
| PROBE-05 | Center finder, circle and rectangle | M6 | done | Circle and rectangle UI |
| PROBE-06 | Rotation and straightness | M6 | partial | Reports angle; work rotation apply deferred |
| PROBE-07 | XY touch plate offsets | M6 | done | Outside-edge plate workflow |
| PROBE-08 | Probe verify and connected flow | M6 | done | No `PC` signal on this build |
| PROBE-09 | Pre and post probe macros | M6 | todo | Bounded |
| PROBE-10 | Height map probing | M6 | done | Grid, feeds, safe Z, overlay, CSV |
| PROBE-11 | Height map compensation and export | M6 | done | Explicit Apply rewrite, original preserved |
| PROBE-12 | Probe-safe interlocks | M6 | done | Mandatory preflight |
| PROBE-13 | Probe feed, step, and retry settings | M6 | partial | Shared defaults persisted per profile; retry strategy open |
| PROBE-14 | Probe results display and log | M6 | done | PRB history |
| PROBE-15 | Probe mount XY offset | M6 | done | Persisted per profile; feature vs controlled point |
| PROBE-16 | Height-map Z offset | M6 | done | Effective map; raw samples retained |

## Utilities and wizards

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| UTIL-01 | Material surfacing wizard | M7 | done | Bounded raster; load-only |
| UTIL-02 | Gantry squaring and alignment | M7 | done | Guided calculator; manual adjustment; no motion |
| UTIL-03 | Stepper and XY calibration | M7 | done | `$100`–`$102` wizard |
| UTIL-04 | Program outline and limits check | M7 | done | Limits card; outline in viewer |
| UTIL-05 | Add line numbers, strip comments, transform tools | M7 | partial | Numbering/comments; rotate/scale/translate open |
| UTIL-06 | Utility macros and quick actions | M7 | done | Toolbar |
| UTIL-07 | Machine setup wizard | M7 | done | First-run safety/setup; app-config flag |

## Touch UX and platform

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| UX-01 | Touch layout at 1280×800 | M2 | done | Default |
| UX-02 | Touch layout at 1920×1080 | M2 | done | Scaled |
| UX-03 | Portrait layout | M7 | todo | 1080×1920 |
| UX-04 | In-app numeric and text keypad | M2 | done | No OS keyboard dependency |
| UX-05 | 48 px targets, no hover-only actions | M2 | done | Touch audit |
| UX-06 | Dockable Workbench layout | M7 | todo | Optional mode |
| UX-07 | Dark, light, high-contrast themes | M5 | partial | Shop-floor default |
| UX-08 | Keyboard navigation and accessibility | M7 | partial | Machine keymap; full navigation/AccessKit open |
| UX-09 | Localization, starting with en, it, de, nl | M7 | partial | Primary UI localized; long-form/secondary text English |
| UX-10 | First-run safety onboarding | M7 | done | Safety checklist + setup flag |
| UX-11 | Error dialogs and notifications | M2 | partial | Actionable |
| UX-12 | Crash-safe configuration and local logs | M5 | partial | Atomic writes |
| UX-13 | Gamepad support | post-v1.0 | todo | Optional |

## Release and quality

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| REL-01 | CI on Linux, macOS, Windows | M0 | done | fmt, clippy, tests, 3-OS build |
| REL-02 | Unit, golden, and property tests | M1 | done | Per crate |
| REL-03 | Simulator scenario suite | M1 | partial | Alarm/tool/probe/reconnect coverage |
| REL-04 | Hardware-in-the-loop checklist | M2 | partial | FlexiHAL |
| REL-05 | Windows, macOS, Linux packaging | M7 | partial | v0.6.0 published; signing/notarization open |
| REL-06 | Code signing and notarization | M7 | todo | Best effort |
| REL-07 | User manual and troubleshooting guide | M7 | done | Docs site or mdBook |
| REL-08 | Versioning and changelog | M7 | done | Semver |
| REL-09 | License and attribution review | M0 | done | MIT/Apache plus NOTICE |
| REL-10 | Performance benchmarks in CI | M4 | todo | Thresholds |

## Explicitly not applicable

| Capability | Reason |
|---|---|
| Legacy Grbl compatibility | grblHAL only |
| Lathe mode and lathe wizards | Not a lathe |
| Plasma THC | Not a plasma table |
| Drag knife | Not needed |
| Network transports (Telnet, WebSocket, TCP) | Serial only |
| FTP and WebDAV | No network transport |
| Camera, Vectric direct output, HPGL/Excellon converters | Not required |

## Post-v1.0, capability-gated

Parsed and modeled in M1; UI deferred until the milling feature bar is solid.

| Capability | Trigger |
|---|---|
| SD card browsing and YModem upload | `SD`, `YM` |
| ATC tool-change flows | `ATC=1` |
| MPG/pendant UI | `MPG` |
| Multi-spindle panels | `$SPINDLESH` with more than one |
| Extra-axis DRO/jog rows | `$I+` axes greater than 3 |
| Gamepad | Optional |
