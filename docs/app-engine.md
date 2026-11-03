# App ↔ engine integration

`ghs-app` is a live view over one `ghs-engine` session. Normal startup connects
to the in-process FlexiHAL demo; the connection panel opens a real serial port;
`--mock` keeps the original static shell for design review only. The UI never
writes bytes and never owns protocol state.

## Data flow

```text
ghs-demo loopback / ghs-serial
        │
        ▼
ghs-engine thread: handshake, discovery, polling, streamer, job runner,
                  check mode, safety gateway
        │  Arc<MachineSnapshot> (latest) + EngineEvent queue
        ▼
GPUI task (50 ms): LiveLink::poll → apply_snapshot + drain events → AppState
        │                                                        → cx.notify()
        ▼
AppState action → ghs_engine::Command → EngineHandle
```

`LiveLink` owns the `EngineHandle` and, in demo mode, the `SimSession` that keeps
the loopback alive. `AppState` is a UI projection; it is not a second source of
truth. Run options, the check-run label, and connection state are copied from
engine accessors on every poll rather than kept optimistically.

## Connection modes

- **Demo**: `ghs_demo::start(SimProfile::FlexiHal)` returns a loopback endpoint
  and a `SimSession`. The session is kept alive and the endpoint is passed to
  `EngineHandle::connect(ConnectTarget::Transport(..))`. The demo program loads
  once after the handshake, never on every poll.
- **Serial**: the port picker is backed by `ghs_serial::list_ports`/
  `PortWatcher`; connect uses `ConnectTarget::Serial(SerialConfig)`.
- **Mock**: `--mock` only. No engine is started and no controller bytes are
  written. Job and motion actions update local preview state or are refused;
  this mode exists for layout review and screenshots.

The app's `Profile`/`Capabilities` are rebuilt from `ControllerInfo` after
connect: axes, units, probe, tool table, SD, ATC, MPG, multi-spindle, coolant,
limit/E-stop/door signals. No board-name branch.

## State mapping

| Engine / core | AppState |
|---|---|
| `MachineState`, connection phase | state label, status banner, connection chip |
| `Position` (mm) | `mpos`/`dro`/`wco`/`dtg` per reported axis, converted to display units |
| `Wcs` | active WCS index and chip |
| `Overrides` | feed, spindle, and rapid override controls |
| `FeedSpindle` | feed rate, RPM, spindle on/off and direction |
| `Signals`, `AuxPins` | signals card and pins/limits sheet |
| `BufferState` | buffer/`Bf` diagnostic |
| `EngineEvent::Log` | bounded console ring buffer (200 lines) |
| `EngineEvent::Alarm/Error` | console plus state banner |
| Job events | job name, progress, active line, program list, pause reason |
| `EngineEvent::CheckStarted/Completed` | check-run label and completion state |
| `Settings`, `Offsets`, `ToolEntry`, `StartupLine` | generated settings, offsets, tools, startup-line panels |
| `PinMapping`, `PinState`, `LimitState`, `LastEvents` | read-only pins/limits sheet |
| `AppConfig.locale` | active `ghs-i18n` catalog, applied before the first frame |

## Command routing

Every action becomes a typed `ghs_engine::Command`; the engine decides whether
the machine state allows it.

- MDI → `Command::Mdi`; jog → `Command::Jog`.
- Zero, goto, and clear → `G10 L20`, `G28`/`G30`, or `G92.1` through the guarded
  MDI/system path.
- Coolant, spindle, and overrides → realtime commands or the dedicated
  `SetFeedOverride`/`SetSpindleOverride`/`SetRapidOverride` commands.
- Macros → their single-line MDI command through the engine, with explicit RUN
  and confirmation.
- Job load/start/pause/resume/stop, breakpoints, and check mode → the dedicated
  job commands (`LoadProgram`, `StartProgram`, `PauseProgram`, `ResumeProgram`,
  `StopProgram`, `SetBreakpoint`, `StartCheckRun`).
- Settings → `Command::SetSetting` with read-back verification.
- Startup lines → guarded `$N0`/`$N1` writes with read-back verification.
- Probe and height-map plans → loaded as a program and streamed through the same
  job runner; accepted results are applied with a confirmed `G10 L2`.
- Pins/limits refresh → `$PINS`, `$PINSTATE`, `$LIM`, `$LEV`.
- Tool change → `AcknowledgeToolChange` after the operator confirms.
- Keymap actions → the same start/pause/resume/stop/jog handlers as the
  on-screen controls. Binding a key never dispatches an action by itself.

## Update loop

The shell starts one GPUI task after connect:

1. Read the latest `Arc<MachineSnapshot>` (cheap clone).
2. Drain `EngineEvent`s without blocking.
3. Apply both to `AppState`, plus engine accessors for run options and check
   mode.
4. `cx.notify()`.
5. Wait ~50 ms using the GPUI background executor, then repeat.

Do not spawn one task per event, and do not block the UI thread on the engine.

## Safety and reconnect

- The engine rejects motion in `Alarm`/`Door` and in other locked states; the UI
  surfaces the rejection.
- Reset/Stop remain UI-visible and route through `RealtimeCommand`.
- On `Disconnected`, mark the link down, stop the update loop, and keep the last
  snapshot visible but visibly stale.
- Never auto-resume after disconnect or alarm.
- Serial connection failures surface as events with actionable text.

## Dependencies

`ghs-app` uses:

- `ghs-core` for snapshot/profile types
- `ghs-engine` for the handle, commands, events, and protocol re-exports
- `ghs-demo` for the in-process simulator
- `ghs-serial` for the port picker and `SerialConfig`
- `ghs-gcode` for the parsed program and height-map compensation
- `ghs-viewer` for geometry and cameras
- `ghs-config` for profiles, settings, tools, macros, keymap, and backups
- `ghs-probe` for probe plans and height-map math
- `ghs-i18n` for locale catalogs
- `gpui-kit` for the UI

`ghs-proto` is not a direct app dependency; protocol types come through
`ghs-engine` re-exports.

## History

This document started as the M0→M2 integration plan, when the shell was a view
model over mock data. The original M2 acceptance list (demo connect/disconnect,
live DRO/state/overrides/spindle/coolant/signals, jog and MDI reaching the
simulator, alarm gating, unchanged touch layout, live screenshots, and loopback
integration tests) is met. Current release status and remaining gaps are in
[PLAN.md](PLAN.md) and [parity.md](parity.md).
