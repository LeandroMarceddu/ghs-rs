# Parity checklist

Target: **P2 milling parity** with ioSender for the reference FlexiHAL machine,
using the ioSender Touch interaction model.

Status values: `todo`, `wip`, `done`, `n/a`.

A row is `done` only when it works against the simulator, has automated tests,
has been verified on the FlexiHAL board, is usable without a mouse, is
documented, and is localized.

## Communication and protocol

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| COM-01 | Serial port enumeration and selection on Linux, macOS, Windows | M1 | todo | `serialport` crate; dialout note |
| COM-02 | Connect/disconnect with baud rate, 8N1 | M1 | todo | Default 115200 |
| COM-03 | Optional DTR reset for 8-bit boards | M2 | todo | Default off for STM32F446 |
| COM-04 | Banner handshake and soft reset | M1 | todo | Timeout and retry policy |
| COM-05 | `$I` and `$I+` capability parsing | M1 | todo | Axes, options, signals, plugins, buffers |
| COM-06 | `$$` settings read and `$SED` descriptions | M1 | todo | Dynamic settings schema |
| COM-07 | `$#` offsets and `$G` parser state | M1 | todo | Parse all reported fields |
| COM-08 | Adaptive status polling and full report `0x87` | M1 | todo | 10 Hz active, 2 Hz idle |
| COM-09 | All grblHAL status fields | M1 | todo | MPos, WPos, Bf, Ln, FS, Pn, WCO, WCS, Ov, A, H, D, Sc, TLR, In, SD, MPG, FW |
| COM-10 | All realtime commands | M1 | todo | Reset, hold, resume, jog cancel, door, overrides, spindle stop, coolant, optional stop, single block |
| COM-11 | `RT+` top-bit realtime negotiation | M1 | todo | Reference build advertises RT+ |
| COM-12 | Messages, errors, alarms with descriptions | M1 | todo | grblHAL tables plus `[MSG:...]` |
| COM-13 | Probe result (`PRB`) parsing | M1 | todo | Used by the probing suite |
| COM-14 | Disable controller auto-reporting on connect | M1 | todo | Setting `$481` when present |
| COM-15 | Hotplug, reconnect, and status watchdog | M2 | todo | Never auto-resume a job |
| COM-16 | Session recording and replay | M1 | todo | Real captures become fixtures |
| COM-17 | Check mode (`$C`) | M3 | todo | Dry run |
| COM-18 | In-process loopback simulator for demo and tests | M1 | todo | Not a user-facing network transport |

## Machine control

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| CTL-01 | DRO for all axes and work coordinate systems | M2 | todo | X/Y/Z first, 9-axis capable |
| CTL-02 | Zero axes and set position | M2 | todo | Per axis and all |
| CTL-03 | Goto and park | M2 | todo | Confirmation for Goto |
| CTL-04 | Step and continuous jog, configurable distances/feeds, keyboard jog | M2 | todo | Jog cancel on release |
| CTL-05 | Jog cancel and pendant-safe behavior | M2 | todo | `0x85` |
| CTL-06 | Feed override, fine and coarse, reset | M2 | todo | Slider plus buttons |
| CTL-07 | Rapid override | M2 | todo | Low, medium, reset |
| CTL-08 | Spindle control and override | M2 | todo | On/off, CW/CCW, RPM, stop |
| CTL-09 | Coolant mist, flood, fan | M2 | todo | Capability-gated |
| CTL-10 | MDI with history | M2 | todo | Multi-line support |
| CTL-11 | Home, unlock, reset, sleep | M2 | todo | State-aware |
| CTL-12 | Signals, pins, and AUX IO display | M2 | todo | HSDEP plus 4/4 AUX |
| CTL-13 | Macros with parameters and special words | M5 | todo | Toolbar and editor |
| CTL-14 | Startup lines | M5 | todo | Read, edit, verify |
| CTL-15 | Keymap editor | M5 | todo | Keyboard and gamepad later |
| CTL-16 | Work offsets editor, G54–G59.3, G10 | M5 | todo | Edit, zero, save |
| CTL-17 | App-side tool table and manual M6 workflow | M5 | todo | 0 controller entries |
| CTL-18 | Metric and imperial units | M2 | todo | mm canonical |
| CTL-19 | Themes and DPI scaling | M5 | todo | Dark, light, high contrast |

## Jobs and program management

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| JOB-01 | Open, drag-and-drop, recent files | M3 | todo | `.nc`, `.gcode`, `.tap`, `.ngc` |
| JOB-02 | Validate program before streaming | M3 | todo | Syntax, tools, feeds |
| JOB-03 | Character-counting streaming with `Bf` top-up | M3 | todo | 64-byte buffer |
| JOB-04 | Pause, resume, stop | M3 | todo | Hold, resume, reset semantics |
| JOB-05 | Manual tool change prompt and resume | M3 | todo | `TC`, persistent tool plugin |
| JOB-06 | Optional stop, single block, block delete | M3 | todo | Realtime toggles |
| JOB-07 | Start-from-line, run-to-line, breakpoints | M3 | todo | Confirmations |
| JOB-08 | Progress, elapsed time, ETA | M3 | todo | Based on parsed geometry |
| JOB-09 | Console with filtering and file log | M3 | todo | Bounded ring buffer |
| JOB-10 | Dry run and check mode | M3 | todo | Never moves |
| JOB-11 | Defined reconnect and interruption policy | M3 | todo | No silent resume |
| JOB-12 | Program list with follow, search, context menu | M4 | todo | Virtualized |
| JOB-13 | Add line numbers, strip comments | M4 | todo | Non-destructive save-as |
| JOB-14 | Job statistics and run history | M4 | todo | Local only |

## Viewer

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| VIEW-01 | G-code parser for lines, arcs, canned cycles, planes, units, incremental mode | M4 | todo | grblHAL dialect |
| VIEW-02 | Expressions, parameters, and flow control | M4 | todo | `EXPR` supported |
| VIEW-03 | Parser-state emulation with offsets, scaling, rotation, tool length | M4 | todo | Used for toolpath |
| VIEW-04 | 2D pan, zoom, fit, grid, bounds | M4 | todo | 60 fps |
| VIEW-05 | Live tool marker | M4 | todo | Status-driven |
| VIEW-06 | Color by motion type, layer, or tool | M4 | todo | Configurable, color-blind safe |
| VIEW-07 | 3D orbit, pan, zoom, stock, and tool | M7 | todo | Software projection first |
| VIEW-08 | Height map overlay | M6 | todo | Surface visualization |
| VIEW-09 | 1M-line and 200k-segment performance | M4 | todo | Budgets in PLAN.md |
| VIEW-10 | Program limits and outline display | M4 | todo | Confirmation before running |

## Setup and persistence

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| SETUP-01 | Settings editor generated from `$SED` and `$HELP` | M5 | todo | Fallback for gaps |
| SETUP-02 | Grouped, searchable, typed, validated settings | M5 | todo | Axis arrays |
| SETUP-03 | Setting write, read-back verification, backup and restore | M5 | todo | EEPROM caution |
| SETUP-04 | Stepper and axis calibration | M5 | todo | Guided measurement |
| SETUP-05 | App settings and controller profiles | M5 | todo | Keyed by `$I` identity |
| SETUP-06 | Import and export settings, offsets, tools | M5 | todo | JSON/CSV |
| SETUP-07 | Controller reset (`$RST`) with backup prompt | M5 | todo | Destructive, confirmed |
| SETUP-08 | Keymap persistence | M5 | todo | Includes jog keys |
| SETUP-09 | Startup line persistence | M5 | todo | `$N` |
| SETUP-10 | Tool table persistence | M5 | todo | App-side |
| SETUP-11 | Offsets backup and restore | M5 | todo | `offsets.nc` equivalent |

## Probing

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| PROBE-01 | Z touch plate | M6 | todo | Safe Z, retries |
| PROBE-02 | Tool length probing | M6 | todo | Update TLO |
| PROBE-03 | Edge finder X/Y, inside and outside | M6 | todo | Edge offset |
| PROBE-04 | Corner finder | M6 | todo | XY touch plate |
| PROBE-05 | Center finder, circle and rectangle | M6 | todo | Multi-pass |
| PROBE-06 | Rotation and straightness | M6 | todo | Two-point and multi-point |
| PROBE-07 | XY touch plate offsets | M6 | todo | Single probe input |
| PROBE-08 | Probe verify and connected flow | M6 | todo | No `PC` signal on this build |
| PROBE-09 | Pre and post probe macros | M6 | todo | Bounded |
| PROBE-10 | Height map probing | M6 | todo | Grid, feeds, safe Z |
| PROBE-11 | Height map compensation and export | M6 | todo | Stream correction |
| PROBE-12 | Probe-safe interlocks | M6 | todo | Mandatory |
| PROBE-13 | Probe feed, step, and retry settings | M6 | todo | Persisted |
| PROBE-14 | Probe results display and log | M6 | todo | PRB history |

## Utilities and wizards

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| UTIL-01 | Material surfacing wizard | M7 | todo | Saves a runnable program |
| UTIL-02 | Gantry squaring and alignment | M7 | todo | Guided |
| UTIL-03 | Stepper and XY calibration | M7 | todo | Overlaps SETUP-04 |
| UTIL-04 | Program outline and limits check | M7 | todo | Uses viewer geometry |
| UTIL-05 | Add line numbers, strip comments, transform tools | M7 | todo | Non-destructive |
| UTIL-06 | Utility macros and quick actions | M7 | todo | Toolbar |
| UTIL-07 | Machine setup wizard | M7 | todo | First-run |

## Touch UX and platform

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| UX-01 | Touch layout at 1280×800 | M2 | todo | Default |
| UX-02 | Touch layout at 1920×1080 | M2 | todo | Scaled |
| UX-03 | Portrait layout | M7 | todo | 1080×1920 |
| UX-04 | In-app numeric and text keypad | M2 | todo | No OS keyboard dependency |
| UX-05 | 48 px targets, no hover-only actions | M2 | todo | Touch audit |
| UX-06 | Dockable Workbench layout | M7 | todo | Optional mode |
| UX-07 | Dark, light, high-contrast themes | M5 | todo | Shop-floor default |
| UX-08 | Keyboard navigation and accessibility | M7 | todo | AccessKit |
| UX-09 | Localization, starting with en, pt-BR, de | M7 | todo | Runtime switch |
| UX-10 | First-run safety onboarding | M7 | todo | Checklist |
| UX-11 | Error dialogs and notifications | M2 | todo | Actionable |
| UX-12 | Crash-safe configuration and local logs | M5 | todo | Atomic writes |
| UX-13 | Gamepad support | post-v1.0 | todo | Optional |

## Release and quality

| ID | Capability | Milestone | Status | Notes |
|---|---|---|---|---|
| REL-01 | CI on Linux, macOS, Windows | M0 | wip | Workflow being added |
| REL-02 | Unit, golden, and property tests | M1 | todo | Per crate |
| REL-03 | Simulator scenario suite | M1 | todo | Alarms, tool change, probing, reconnect |
| REL-04 | Hardware-in-the-loop checklist | M2 | todo | FlexiHAL |
| REL-05 | Windows, macOS, Linux packaging | M7 | todo | MSI, DMG, AppImage/deb |
| REL-06 | Code signing and notarization | M7 | todo | Best effort |
| REL-07 | User manual and troubleshooting guide | M7 | todo | Docs site or mdBook |
| REL-08 | Versioning and changelog | M7 | todo | Semver |
| REL-09 | License and attribution review | M0 | todo | MIT/Apache plus NOTICE |
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
| SD card and YModem | Controller has no SD card |
| ATC | Controller reports `TC`, not ATC |
| MPG and pendant input takeover | Controller has a keypad, no MPG |
| Rotary and 4th–5th axis UI | 3-axis machine |
| Camera, Vectric direct output, HPGL/Excellon converters | Not required |
