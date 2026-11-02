# M6 probing and height map

Probing moves the machine into contact with a surface. It is the most
safety-sensitive feature after reset/stop. The app generates a bounded program,
the engine streams it through the same job runner, and every probe requires an
explicit preflight.

## Reference-machine constraints

- FlexiHAL reports one probe input and no `PC` (probe-connected) signal.
- The `Probe Protection` plugin is present; do not assume it is a substitute
  for a safe Z or a bounded search.
- No tool setter input beyond the probe input; tool-length probing uses the
  same probe/plate.
- Spindle must be off; the workflow refuses to start with `M3`/`M4` active.
- Work offset and current WCS are known and displayed before the run.

## Coordinate frames

- Operator-entered positions, centres, and height-map grid origins are
  **feature/probe-tip** coordinates.
- The probe mount offset is `probe tip - controlled point`, in millimetres. It
  is stored per controller profile and is never sent to the controller.
- Generated plan `from`/`to` values are **controlled-point** coordinates:
  `controlled = feature - probe_offset`. `PRB` positions are controlled-point;
  result math adds the offset back before computing an edge, centre, or angle.
- The height-map Z offset is a constant datum shift added to every measured
  height. Raw samples and the offset are persisted; summaries, the overlay,
  CSV, and compensation use the effective height. It never changes the probing
  plan or the surface-Z search target.

## Safety contract

A probe run is refused unless:

- the link is Ready and the machine is Idle;
- the spindle is off and coolant is off;
- a safe Z is explicit and reachable;
- the search distance and feed are bounded and positive;
- the probe is verified connected (signal, virtual toggle, or an explicit
  "probe connected" confirmation when no signal exists);
- the operator confirms the cycle and sees the direction/travel.

The engine safety gateway remains the final authority. A probe run never
auto-resumes after an alarm, E-stop, door, disconnect, or unexpected result.
Probe failure alarms (4/5/13) stop the job and require operator recovery.

## Architecture

```text
Probe card parameters
      │
      ▼
ghs-probe: ProbePlan { gcode, expected, postprocess }
      │  LoadProgram + StartProgram
      ▼
ghs-engine job runner (same streamer/safety gateway)
      │  PRB result event + line context
      ▼
ghs-probe computation (edge/center/rotation/tool length/height map)
      │
      ▼
result UI + optional offset update + height-map overlay
```

- `ghs-probe` is pure: it builds G-code and computes results. No UI or I/O.
- The engine needs one additive event/metadata field: the probe result should
  carry the source line or probe id so the app can correlate it with the
  pending cycle. If that is not cheap, a probe program should contain a single
  `G38.x` probe move and the app binds the next `Probe` event to it.
- Results that update offsets use `G10 L20`/`G10 L2` through the engine after
  explicit confirmation; never silently.
- The app owns the preflight and result sheet; the engine owns motion.

## Cycles

| Cycle | Inputs | Output |
|---|---|---|
| Z touch plate | plate Z, safe Z, search, feed, XY position | work Z zero or measured height |
| Tool length | reference surface Z, safe Z, search, feed | TLO / tool length |
| Edge X/Y | edge, direction, probe diameter, search, feed, edge offset | work X/Y zero |
| Corner | two edges + offsets | work X/Y zero |
| Center circle | approximate center, radius bound, search, feed | center X/Y |
| Center rectangle | two edges or four touches | center X/Y |
| Rotation | two points on an edge, separation | work rotation (if supported) |
| XY touch plate | plate X/Y, offsets | work X/Y zero |
| Height map | grid origin/size/spacing, safe Z, search, feed | height grid + compensation |

Each plan is a small, complete G-code program with a safe preamble (`G21 G90`,
safe Z, spindle off) and a retract. Do not hand the app raw prelude text.

## Height map

- Define a grid: origin X/Y, size, spacing, safe Z, search distance, feed.
- Probe row by row with retracts; bound the number of points and estimated
  time; show it before starting.
- Store `z[x][y]` with validity flags. Interpolate bilinearly.
- Apply compensation with an explicit **Apply height map** action that rewrites
  the loaded program in the sender, matching ioSender's `ApplyHeightMap`:
  split `G0`/`G1` and `G2`/`G3` moves at the smaller grid step, add the
  bilinearly interpolated height to Z, re-emit the program, and keep the
  original for Revert. This is a static, operator-triggered transform, not a
  real-time stream correction.
- Refuse Apply when the map is incomplete or the program leaves the mapped
  area unless the operator explicitly accepts uncompensated moves; never
  extrapolate a missing cell.
- Preserve non-motion words, comments, `G90`/`G91`, `G20`/`G21`, and
  `G17`/`G18`/`G19`; refuse radius arcs outside XY and checksummed input
  rather than emitting a corrupt program.
- Do not compensate a non-moving `G0`/`G1` or an `F`-only block. ioSender had
  this bug (issue #451); it must be a regression test here.
- Probe XY offset and map Z offset are part of the parity contract; the map
  must be stored with the setup and not reused after a WCS or tool change.
- Show the map in the 2D viewer as a colored cell overlay now; 3D surface and
  contours are M7.
- Re-probe invalid points; do not interpolate across a failed corner blindly.
- A height map is per workpiece/setup; do not silently reuse it after a WCS or
  tool change. Save it in the controller profile with its grid, samples, and
  WCS/G92/TLO frame; loading is read-only and Apply remains an explicit,
  gated action.

## UI

- Probe mode with cards for each cycle; large touch controls.
- Preflight sheet: current WCS, DRO, safe Z, search, feed, probe state, spindle
  state, expected travel, and a clear Run button.
- Result sheet: measured value, old/new offset, Apply/Keep, raw `PRB`, retry.
- Height map card: grid editor, estimated points/time, progress, result overlay.
- Console shows the exact generated program and `PRB` lines for audit.

## Testing

- Golden G-code for every cycle, including safe preamble/retract.
- Math tests for edge/center/rotation/tool length using known offsets.
- Simulator runs: success, probe fail (alarm 4/5), disconnected probe, alarm
  mid-cycle, no auto-resume.
- Height-map interpolation and bounds; failed-point handling.
- Hardware checklist on the FlexiHAL with a test plate, spindle off, hand on
  the E-stop.
- No probe test is "done" from simulator-only evidence.

## Deferred

- Multiple probe inputs (`PROBES=n`) UI; the reference board has one.
- Tool setter as a separate input.
- Automatic tool-length probing in the M6 sheet (the manual M6 flow is M5).
- 3D height-map rendering; start with a 2D overlay.
