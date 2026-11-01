# App ↔ engine integration

The M0 shell is a view model over mock data. This is the plan for replacing the
mock with a live `ghs-engine` session without rewriting the layout.

## Target data flow

```text
SerialTransport / ghs-demo loopback
        │
        ▼
ghs-engine thread: handshake, polling, streamer, safety gateway
        │  Arc<MachineSnapshot> + EngineEvent queue
        ▼
GPUI task (50 ms): apply_snapshot + drain events → AppState → cx.notify()
        │
        ▼
AppState methods: send_mdi / jog / zero / coolant / spindle / overrides
        │  ghs_engine::Command
        ▼
EngineHandle → engine thread
```

The UI never writes bytes and never owns protocol state. Mock mode remains as a
screenshot/design fallback, clearly labeled, but live mode is the default.

## Connection modes

- **Demo**: `ghs_demo::start(SimProfile::FlexiHal)` returns a loopback endpoint
  and a `SimSession`. Keep the session alive; pass the endpoint to
  `EngineHandle::connect(ConnectTarget::Transport(Box::new(endpoint)))`.
- **Serial**: port picker backed by `ghs_serial::list_ports`/`PortWatcher`;
  `ConnectTarget::Serial(SerialConfig)`.
- **Mock**: no engine; used only for design review and when explicitly chosen.

The app's `Profile`/`Capabilities` are rebuilt from `ControllerInfo` after
connect: axes, units, probe, tool table, SD, ATC, MPG, multi-spindle, coolant,
limit/E-stop/door signals. No board-name branch.

## State mapping

| Engine / core | AppState |
|---|---|
| `MachineState` | `machine_state` display string |
| `Position` (mm) | `dro` per axis, converted to display units |
| `Wcs` | `wcs_index` |
| `Overrides` | `feed_override`, `spindle_override`, rapid when exposed |
| `FeedSpindle` | `feed_rate`, `rpm`, `spindle_on`, `spindle_cw` |
| `AccessoryState` | `coolant[3]`, spindle state |
| `Signals` | `signals` rows (limits, probe, E-stop, door) |
| `EngineEvent::Log` | `ConsoleLine` (bounded 200) |
| `EngineEvent::Alarm/Error` | console + state banner |
| Job events (M3) | `job_name`, `progress`, `active_line`, program list |

## Command routing

`AppState` methods currently mutate mock data. They gain a live branch:

- `send_mdi()` → `Command::Mdi`
- `jog()` → `Command::Jog`
- `zero_axis()` → the appropriate `G10 L20`/zero command or a dedicated
  zero-axis command once the engine exposes one
- `toggle_coolant()` → `Command::Realtime(FloodToggle/MistToggle/Fan0Toggle)`
- spindle on/off/direction/RPM → engine commands
- override sliders → `Command::SetFeedOverride`/`SetSpindleOverride`/rapid
- macros → their G-code/`$` command equivalents through the engine

The mock branch remains only when `Link::Mock`.

## Update loop

The shell starts one GPUI task after connect:

1. Read the latest `Arc<MachineSnapshot>` (cheap clone).
2. Drain `EngineEvent`s without blocking.
3. Apply both to `AppState`.
4. `cx.notify()`.
5. Wait ~50 ms using the GPUI background executor, then repeat.

Do not spawn one task per event, and do not block the UI thread on the engine.

## Safety and reconnect

- The engine rejects motion in `Alarm`/`Door`; the UI must surface the rejection.
- Reset/Stop remain UI-visible and route through `RealtimeCommand`.
- On `Disconnected`, mark the link down, stop the update loop, and keep the
  last snapshot visible but visibly stale.
- Never auto-resume after disconnect or alarm.
- Serial connection failures surface as events with actionable text.

## Dependencies

`ghs-app` re-adds:

- `ghs-core` for snapshot/profile types
- `ghs-engine` for the handle, commands, and events
- `ghs-demo` for the in-process simulator
- `ghs-serial` for the port picker and `SerialConfig`

`ghs-proto` is optional in the app; prefer engine/core re-exports.

## Acceptance for M2

- Demo connect/disconnect from the UI.
- Live DRO, state, overrides, spindle, coolant, signals from the simulator.
- Jog and MDI actually reach the simulator; console shows commands and `ok`.
- Alarm gating: a simulated alarm blocks motion until unlock.
- Layout and touch behavior unchanged.
- Screenshots at 1280×800 and 1920×1080 with live data, not mock data.
- Engine integration tests run the same path over `ghs-demo` loopback.
