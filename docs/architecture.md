# Architecture

## Principles

1. **One owner per resource.** The serial port is owned by one reader thread; a
   session is owned by one engine thread; UI state is owned by GPUI entities.
2. **Sans-IO protocol.** Parsing and command encoding are pure functions over
   bytes. No sockets, no clocks, no threads inside `ghs-proto`.
3. **Typed commands, one gateway.** The UI never writes bytes. Motion, spindle,
   coolant, probing, and job control all pass through `ghs-engine`, which
   enforces machine-state and safety rules.
4. **Capability-driven UI.** The controller's `$I+` response decides which
   panels and controls exist.
5. **Deterministic tests first.** The simulator and recorded sessions are the
   primary test oracles; hardware is the final gate.
6. **No async runtime.** Threads plus channels are enough for a 10–20 Hz control
   loop and avoid competing with GPUI's executor.

## Crates

| Crate | Responsibility | Depends on |
|---|---|---|
| `ghs-core` | Axes, positions, units, WCS, overrides, signals, capabilities, settings, tools, alarms/errors | — |
| `ghs-proto` | Framing, status/response parsing, command encoding, ACK/buffer tracking | `ghs-core` |
| `ghs-serial` | Port enumeration, serial reader/writer threads, hotplug, loopback | — |
| `ghs-sim` | Deterministic grblHAL simulator and scenario fixtures | `ghs-core`, `ghs-proto` |
| `ghs-engine` | Session state machine, streamer, job runner, safety gateway | all of the above |
| `ghs-gcode` | G-code lexer/parser, expressions, parser-state emulation, toolpath (M4) | `ghs-core` |
| `ghs-viewer` | Geometry cache, 2D/3D projection, render plan (M4) | — |
| `ghs-config` | Profiles, app settings, backups (M5) | `ghs-core` |
| `ghs-probe` | Probe cycles, height map, compensation (M6) | `ghs-core`, `ghs-gcode` |
| `ghs-ui` | Design tokens and thin GPUI Kit wrappers (M2) | `ghs-app` |
| `ghs-app` | GPUI shell, panels, touch layout, keypad, dialogs | all |

`ghs-gcode`, `ghs-probe`, `ghs-config`, and `ghs-i18n` are introduced when their
milestone starts; creating empty crates early is avoided.

## Runtime topology

```text
┌───────────────────────────── GPUI main thread ─────────────────────────────┐
│ ghs-app views ── Command ──▶ EngineHandle                                  │
│      ▲                        │                                            │
│      └── Arc<MachineSnapshot>, Vec<Event> ◀── telemetry channel            │
└───────────────────────────────┼────────────────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────── ghs-engine thread ──────────┐
│ Session state machine                                                      │
│   ├─ ProtocolSession (ghs-proto)                                           │
│   ├─ Streamer (ghs-proto)                                                  │
│   ├─ JobRunner                                                             │
│   └─ SafetyGateway                                                         │
└───────────────────────────────┬────────────────────────────────────────────┘
                                │ bytes
┌───────────────────────────────▼─────────────── ghs-serial thread ──────────┐
│ serialport reader/writer; hotplug poller; loopback in tests/demo           │
└────────────────────────────────────────────────────────────────────────────┘
```

- **Engine input:** one bounded `crossbeam-channel` of `EngineInput` values
  (`Command`, `TransportBytes`, `TransportError`, `Tick`).
- **Telemetry:** the engine publishes an `Arc<MachineSnapshot>` through a
  `watch`-style channel; the UI reads the latest value at frame time. Events
  (messages, alarms, job transitions) go through a bounded queue with an
  explicit drop-oldest policy for the console.
- **Timers:** the engine uses `recv_timeout` to schedule status polls and
  watchdog checks; no timer thread.
- **Parsing/geometry:** CPU-heavy work runs on a worker thread (or rayon pool at
  M4) and returns `Arc` results.

## Session state machine

```text
Disconnected
  └─ open(port) ─▶ Connecting
        └─ port opened ─▶ Handshaking
              ├─ banner received ─▶ Discovering
              │     ├─ $I+ / $$ / $# / $G / $SED ─▶ Ready
              │     └─ timeout/error ─▶ Failed
              └─ port error ─▶ Disconnected
Ready ⇄ Jogging
Ready ⇄ Running ⇄ Paused
Running ─▶ ToolChange ─▶ Running
Running ─▶ Alarm (terminal until unlock/reset)
```

Rules:

- Motion is only accepted in `Ready`, `Jogging`, `Running`, or `Paused` as
  appropriate; probing requires `Ready`.
- An `ALARM` transitions to `Alarm`, stops the streamer, and requires an explicit
  unlock or reset. Never auto-resume.
- A disconnect during a job marks the job `Interrupted`; the file and line
  position are retained, but resuming requires an explicit user action and a
  fresh homing/position check.
- A status watchdog pauses the stream if no status arrives for 2 s during a job
  and declares the link lost after 5 s.

## Protocol and streaming

Handshake:

1. Open the port with the configured baud rate (default 115200), 8N1.
2. Optionally toggle DTR to reset 8-bit boards (setting; default off for the
   STM32F446).
3. Drain the banner until `\r\n` or `Grbl ...`/`grblHAL ...` is seen, with a
   2 s timeout.
4. Send a soft reset (`0x18`) if configured, then wait for the banner.
5. Query `$I+`, `$$`, `$#`, `$G`, and `$SED`/`$HELP` as needed.
6. Disable controller auto-reporting if enabled (grblHAL setting `$481`).
7. Start status polling.

Status polling is adaptive: 10 Hz while running/jogging, 2 Hz when idle. A full
report (`0x87`) is requested after connect, after alarm/unlock, and every few
seconds during a job to refresh fields that are only sent on change.

Streamer:

- The controller reports a 64-byte serial RX buffer in this build's `OPT` field;
  the streamer uses the reported size, falling back to 128 bytes.
- Character counting: a line may be sent only if
  `line_len <= buffer_size - sent_unacked - queued_bytes`.
- `Bf:` (planner/serial free space) is used to top up the pipeline after the
  initial fill, but the character-counting invariant remains the hard limit.
- One `ok`/`error` acknowledges one line. Messages and status reports can be
  interleaved and must not disturb accounting.
- Real-time bytes are sent out-of-band and never count against the buffer.
- The build advertises `RT+`, so top-bit-set variants are available; the session
  negotiates this and uses the safest form the controller supports.
- Pause sends `!`; resume sends `~`; jog cancel sends `0x85`; reset sends
  `0x18`. The streamer freezes while paused and tracks in-flight lines.

Job runner responsibilities:

- Validate the program before streaming (syntax, unsupported codes, missing
  feed/spindle where required, tool changes).
- Track line numbers, byte offsets, progress, elapsed time, and estimated
  remaining time.
- Stop at `M0`, `M1`, optional stop, single-block, breakpoints, and manual `M6`.
- Capture `PRB` results for probing and `[MSG:...]` for the console.
- Support start-from-line and run-to-line with explicit confirmations.

## Domain model

- `Position` is a fixed `[f64; 9]` plus an axis mask; `Axes` uses X/Y/Z plus
  A/B/C/U/V/W.
- All stored coordinates are millimetres. `Units::Inch` is a display/protocol
  mode; conversion happens at the edges.
- `MachineSnapshot` is immutable and cheap to clone via `Arc`: state, positions,
  offsets, overrides, feed/spindle, signals, pins, buffer, job info, and
  timestamp.
- `Capabilities` is parsed from `$I+` and drives UI visibility: axes, options,
  signals, AUX IO, plugins, buffer sizes, tool-table entries.
- Settings are `BTreeMap<u16, Setting>` with a typed value and optional
  description/group from `$SED`/`$HELP`; a bundled fallback covers gaps.

## Safety gateway

Every command is classified:

| Class | Examples | Guard |
|---|---|---|
| Read-only | status, `$$`, `$#` | Always allowed |
| State | unlock, homing, sleep, reset | State checks + confirmation for reset during job |
| Motion | jog, goto, MDI, run, probe | Not in alarm; homing state; soft-limit awareness |
| Spindle/coolant | M3/M4/M5, M7/M8/M9 | Not in alarm; visible state |
| Configuration | `$` writes, offsets, tools | Idle only; backup before write |

Hard rules:

- No motion while `Alarm` or `Door` without an explicit, confirmed unlock.
- Probe cycles require a probe-connected signal when available, a safe Z, and a
  bounded search distance.
- A job never starts with an unknown position after connect unless the user
  confirms.
- The stop/reset control is always visible and never disabled.
- All writes to persistent controller settings are read-back verified.

## Persistence

- App config: TOML in the platform config directory
  (`~/.config/ghs-rs`, `%APPDATA%\ghs-rs`, `~/Library/Application Support/ghs-rs`).
- Controller profiles keyed by `$I` identity plus board and firmware.
- Tool table and offsets exported as JSON/CSV; backups keep the last N versions.
- Atomic writes: write to a temporary file, fsync, rename.
- No telemetry. Logs are local and opt-in for sharing.

## UI

- `ghs-app` uses GPUI Kit components: dock, tabs, buttons, switches, sliders,
  tables, virtual lists, modals, notifications, charts.
- `ghs-ui` owns tokens (spacing, type scale, colors, touch sizes) and wraps
  GPUI Kit so version upgrades are localized.
- Layout modes: **Touch** (fixed, large, responsive to 1280×800, 1920×1080, and
  portrait) and **Workbench** (dockable panels).
- The numeric keypad is an in-app bottom sheet; no dependency on an OS keyboard.
- All interactive elements have keyboard equivalents; gamepad mapping is added
  after v1.0 if needed.

## Viewer

- Parsing produces a compact segment stream: `Line`, `Arc` (tessellated on
  demand), `Rapid`, and `Plunge`, with layer/tool/feature metadata.
- Geometry is stored in flat arrays and shared via `Arc`; the viewer never
  copies a whole program per frame.
- 2D: orthographic top view, pan/zoom/fit, grid, bounds, work envelope, tool
  marker, color by motion type.
- 3D: software projection into painter-sorted polylines, orbit/pan/zoom, stock
  and tool primitives. If profiling fails the budget, render offscreen with wgpu
  into a texture and present it as an image; the geometry layer is independent
  of either path.
- Decimation is zoom-dependent; hit testing and selection use the same index.

## Testing

- `ghs-proto` unit and golden tests; property tests for buffer accounting and
  malformed input.
- `ghs-sim` scenario tests for handshake, alarms, tool change, probing,
  overrides, disconnect, and reconnect.
- Recorded real sessions replayed through the same parser.
- `ghs-gcode` golden toolpaths, differential tests against hand-computed
  geometry, and fuzzing.
- GPUI headless tests and layout snapshots at the three target sizes.
- Hardware-in-the-loop checklist for every release.

## Explicit non-goals

Legacy Grbl, network transports, lathe, plasma THC, drag knife, SD/YModem, ATC,
MPG, camera, and rotary/4th-axis UI. The domain model leaves room for more axes
and capabilities, but no UI or protocol work is done for them before v1.0.
