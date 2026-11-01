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
   panels and controls exist. Never hardcode the reference board's capability
   set into the UI or protocol.
5. **Protocol breadth, UI depth.** Parse the full grblHAL status, realtime,
   system-command, and runtime-enumeration surface; ignore unknown extensions.
   See [protocol.md](protocol.md).
6. **Runtime enumerations over bundled tables.** Prefer `$EA`, `$EE`, `$ES`,
   `$EG`, and `$SED`; bundled tables are fallbacks for older builds.
7. **Deterministic tests first.** The simulator and recorded sessions are the
   primary test oracles; hardware is the final gate.
8. **No async runtime.** Threads plus channels are enough for a 10–20 Hz control
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

Handshake (do not assume a banner or a DTR reset):

1. Open the port at the configured baud rate (default 115200), 8N1; apply
   DTR/RTS/flow control.
2. Listen for 400–500 ms. If a status report with `|MPG:1` arrives, a
   pendant/secondary sender is in control: stay passive and wait for
   `|MPG:0`.
3. Request a full status report (`0x87`; `0x80` when `RT+`, otherwise `?`) and
   wait up to ~250 ms. This confirms an extended grblHAL controller without
   relying on the banner.
4. If the report is `Alarm:<n>`, treat critical alarms (1, 2, 10, 17, 20) as
   locked: no motion or configuration until the condition clears and the user
   unlocks/homes/resets.
5. Query `$I+` for axes, options, signals, buffers, driver, board, plugins, and
   tool-table entries.
6. With `ENUMS`, fetch `$EA` (alarms), `$EE` (errors), `$ES` (settings), and
   `$EG` (groups). With `SED`, fetch `$SED=<n>` descriptions. Fall back to
   `$$`/bundled tables only when needed.
7. Disable controller auto-reporting if enabled (`$481` or `0x8C`).
8. Start status polling.

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
- The build advertises `RT+`: use the fixed extended codes `0x80` (status),
  `0x81` (cycle start), and `0x82` (feed hold), plus the additional `0x83`–
  `0xA4` commands where applicable. These are not a bitwise OR of printable
  ASCII.
- Pause sends `0x82`; resume sends `0x81`; jog cancel sends `0x85`; reset sends
  `0x18`; stop sends `0x19`; soft E-stop sends `0x9F`. The streamer freezes
  while paused and tracks in-flight lines.
- On `error:<n>`, grblHAL keeps the error active until a reset, empty line, or
  `$` command clears it. The job runner stops, clears, and requires a decision
  before resuming.

Job runner responsibilities:

- Validate the program before streaming (syntax, unsupported codes, missing
  feed/spindle where required, tool changes).
- Track line numbers, byte offsets, progress, elapsed time, and estimated
  remaining time.
- Stop at `M0`, `M1`, optional stop, single-block, breakpoints, and manual `M6`.
- Capture `PRB` results for probing and `[MSG:...]` for the console.
- Support start-from-line and run-to-line with explicit confirmations.

### Tool change and MPG

- On `M6`, the controller enters `Tool` state. The engine pauses the streamer,
  emits a tool-change event, and waits for the UI. After the user acknowledges,
  the engine sends `0xA3`; jogging, `$` commands, and tool-length offsets are
  allowed while pending. Cycle start resumes the saved program.
- When a status report contains `|MPG:1`, a pendant has taken over the input
  stream. The engine disables motion commands from the app, keeps telemetry
  live, and waits for `|MPG:0` before restoring control.
- These are protocol-level features even when the reference board does not use
  them; the UI is capability-gated.

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
- Settings are a `BTreeMap<u16, Setting>` with a typed value and metadata from
  `$ES`/`$EG`/`$SED`; a bundled fallback covers controllers without runtime
  enumeration. The settings UI is generated from the metadata, never hardcoded.
- A `ControllerProfile` is pure data: firmware version/build, option set,
  signals, axes, RX/planner buffers, tool-table entries, plugins, and known
  quirks. `Capabilities` is derived from it and from runtime responses. The UI
  reads capabilities only; it never asks which profile is active. `flexihal`
  and `generic-full` are two instances of the same profile type.

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
- `ghs-sim` scenario tests for handshake, MPG takeover, alarms, tool change,
  probing, overrides, disconnect, and reconnect.
- The simulator is one engine parameterized by a `ControllerProfile`.
  `flexihal` is a strict subset of `generic-full`; both run the same scenario
  suite with capability-dependent expectations. The subset is essential for
  testing absence and fallback paths (no `SD`, no `PC`, no extra axes, older
  build, 64-byte buffer). Product code never branches on a profile name.
- Recorded real sessions replayed through the same parser.
- `ghs-gcode` golden toolpaths, differential tests against hand-computed
  geometry, and fuzzing.
- GPUI headless tests and layout snapshots at the three target sizes.
- Hardware-in-the-loop checklist for every release.

## Explicit non-goals

Legacy Grbl, network transports, lathe, plasma THC, drag knife, and camera.
SD/YModem, ATC, MPG, multi-spindle, and extra-axis *UI* are post-v1.0, but their
protocol fields and capabilities are parsed and modeled from M1. The domain
model leaves room for more axes and capabilities; no board-specific assumption
is allowed.
