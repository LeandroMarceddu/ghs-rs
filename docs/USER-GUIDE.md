# ghs-rs operator guide

Practical guide for the current build of ghs-rs, a serial G-code sender for
grblHAL. It covers the workflows that exist in the app today.

> **Status: pre-alpha, simulator-only evidence.** The reference FlexiHAL /
> STM32F446 running grblHAL `1.1f.20240604` has not been available for
> hardware validation. Every procedure below has been exercised against the
> in-process simulator and the test suite, not on the reference machine.
> Treat a new build as untrusted until you have verified it yourself. Read
> [SAFETY.md](SAFETY.md) before connecting a machine.

This guide describes the app's controls, not a guarantee of machine behavior.
The controller, its firmware, and its safety wiring remain authoritative.

## 1. Before you start

1. **Use the simulator first.** The default launch opens an in-process
   FlexiHAL simulator. Learn the layout, jog, MDI, job controls, and the probe
   cards there before opening a serial port.
2. **Know the E-stop.** Locate the physical E-stop and test it before the
   first commanded move. The app cannot replace it.
3. **Verify the spindle can be stopped independently** of the sender. Start
   with the spindle off and the tool clear of the workpiece.
4. **Check travel limits.** Confirm the machine's physical travel and the
   controller's soft-limit settings. The viewer's envelope is a visual aid,
   not a limit check.
5. **Back up the controller profile.** Use Utility → BACKUP BUNDLE → EXPORT
   before changing settings, offsets, or startup lines. See section 13.
6. **Get help.** Start with [SAFETY.md](SAFETY.md), [probing.md](probing.md),
   [settings.md](settings.md), and [RELEASE.md](RELEASE.md). The project
   repository is <https://github.com/LeandroMarceddu/ghs-rs>; use its issue
   tracker for build-specific problems.

The non-negotiable motion rules are in [SAFETY.md](SAFETY.md). In particular:
motion is refused in `Alarm` and `Door`, probing requires preflight and a safe
Z, and the app never auto-resumes after an alarm, E-stop, or reconnect.

### First-run setup

On first launch, `MACHINE SETUP` opens over the workspace. It has four pages:

1. `SAFETY` — five required acknowledgements: E-stop located and tested;
   independent spindle stop; travel and soft limits verified; workholding and
   tool clearance checked; simulator or spindle-off first connection. `NEXT`
   stays disabled until every item is checked.
2. `UNITS` — the default display units for the next session (mm or inch).
3. `CONNECTION` — guidance only. The wizard never connects, homes, jogs,
   probes, or writes controller settings.
4. `REVIEW` — acknowledgement count and units, with `FINISH SETUP`.

`FINISH SETUP` saves the unit preference and `setup_complete` in the app
config, then closes. A failed save keeps the wizard open and shows the error.
`REMIND ME LATER` closes without saving; the wizard is offered again next
launch. Reopen it from Utility → `MACHINE SETUP`; the card shows `COMPLETE` or
`NOT COMPLETED`. Setup records that you reviewed the checklist and chose a
preference; it is not a machine certification.

## 2. Install and run

### From source

The pinned toolchain is Rust 1.98.1; `rust-toolchain.toml` selects it. You
also need a C toolchain and, on Linux, the GPUI system dependencies.

```sh
cargo build --workspace
cargo test --workspace
cargo run -p ghs-app
```

On Debian/Ubuntu the app needs:

```sh
sudo apt install build-essential pkg-config libssl-dev libfontconfig1-dev \
  libfreetype6-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libx11-xcb-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libvulkan1
```

Linux serial ports need group access:

```sh
sudo usermod -aG dialout "$USER"
```

Log out and back in after changing group membership.

### Start modes

| Command | What starts |
|---|---|
| `cargo run -p ghs-app` | In-process FlexiHAL simulator, auto-connected |
| `cargo run -p ghs-app -- --mock` | Static mock shell; no engine, no motion |
| `cargo run -p ghs-app -- --connection-panel=serial` | Simulator; Serial preselected |

The default simulator is the same engine, handshake, streamer, and safety
gateway used for a real controller; only the transport differs. `--mock` is a
design-review shell and never sends commands.

Useful layout and review flags: `--light`, `--high-contrast`, `--keypad`,
`--size WxH` (for example `--size 1920x1080`), `--fullscreen`, `--viewer-3d`,
`--settings`, `--offsets`, `--tools`, `--utility`, `--startup-lines`,
`--macros`, `--probe`, `--demo-gantry`, and `--demo-gantry-card`. Demo job
switches `--demo-m0` and `--demo-toolchange` are described in section 6.

### Packaged builds

Release archives are published for Linux x86_64 (tar.gz, deb, AppImage),
Windows x86_64, and macOS. They are **unsigned and not notarized**, and they
are pre-alpha previews. macOS Gatekeeper and Windows SmartScreen will warn on
first launch; use only a per-file override, never a system-wide disable. See
[RELEASE.md](RELEASE.md) for the artifact names, checksums, and signing
caveats.

## 3. First connection

### Simulator

The default launch connects to the in-process FlexiHAL simulator automatically.
The status pill in the top bar shows `READY | 3 axes | Idle` when the handshake
finishes. The demo program (`demo-pocket.nc`) loads once the controller is
ready; it is not started automatically.

### Real serial controller

1. Click the status pill, or the `CONNECT` button in the top bar. The
   `CONNECTION` sheet opens below the top bar, so `RESET` and `STOP` stay
   reachable.
2. Select **Serial port**.
3. Click `REFRESH` and select the port. Nothing opens until you press
   `CONNECT`.
4. Select the baud rate. The default is `115200`; line settings are fixed at
   8 data bits, no parity, one stop bit. DTR reset pulses are not implemented
   in this build.
5. Press `CONNECT`. The pill shows `CONNECTING` during the handshake, then
   `READY`.
6. To change transport, press `DISCONNECT` first. The sheet tells you to do
   this while connected.

### What the handshake does

The app opens the port, listens for a controller report, requests a full
status report, and then queries `$I+` for axes, options, signals, buffers, and
firmware identity. It fetches `$EA`/`$EE`/`$ES`/`$EG` when the controller
advertises `ENUMS`, and `$SED` descriptions when available. Settings and
offsets appear as those responses arrive.

The `$I+` response also selects the stored controller profile. Open
**Utility** → `CONTROLLER PROFILE` to see the identity and profile file. A
different controller must not silently inherit another machine's offsets,
tools, macros, or height maps.

### Check before the first motion

- Status pill is `READY`, and the machine state is `Idle`.
- No active alarm or door state.
- DRO values and directions respond correctly to a small jog.
- Units and the active WCS are what you expect.
- Spindle is off, tool is clear, and the E-stop is within reach.
- Soft limits and physical travel are known and correct.
- `Settings` shows the generated controller settings, and `Offset` shows
  live `$#` values.
- A profile backup exists (Utility → `EXPORT`).

If the controller identity is reported as `UNVERIFIED`, the app will not
silently adopt local profile data. Review the message and use
`USE LOCAL DATA` only if you are sure the data belongs to this machine.

## 4. Screen map

The shell has a fixed three-column layout at 1280×800 and wider. Below
1120 px the right rail collapses; a full portrait reflow is not implemented.

### Top bar

- `GHS` identity and the mode bar: **Job**, **Probe**, **Offset**,
  **Utility**, **Tools**, **Settings**.
- Status pill: connection label plus axis count and machine state, for
  example `READY | 3 axes | Idle`. Click it to open the connection sheet.
- Theme control: `DARK`, `LIGHT`, and `HIGH CONTRAST` on a wide window; a
  single 48 px cycle button on a narrow one.
- `CONNECT` / `DISCONNECT`.
- `RESET` (soft reset) and `STOP` (controller stop). Both stay visible and are
  never disabled.

### Themes and display

The theme choice is saved in `app.json` and applied immediately. `HIGH
CONTRAST` uses near-black surfaces, white text, stronger borders, and saturated
status colors; normal text tiers meet WCAG AAA contrast. `--light` and
`--high-contrast` override the saved mode for one session. GPUI logical-pixel
scaling follows the operating system's display scale; there is no separate
per-monitor DPI setting.

### Left column

- **Console** / **Program** tabs over one card.
- `Console` is the live protocol log (commands, `ok`, messages, errors,
  alarms). It keeps the last 200 lines; `ALL/CMD/INFO/OK/WARN/ERROR` filter the
  view, and `SAVE LOG` writes a snapshot to the config exports directory.
- `Program` is the parsed program list, with search and the highlighted
  current line.
- `MDI`: one-line command field, `SEND`, and a 10-entry `HIST` recall.
- `FEED OVERRIDE`: percentage, slider, `-10`/`-1`/`+1`/`+10`, and
  `RESET 100%`.

### Center column

- The active workspace (viewer, probe cards, offsets, utilities, tools, or
  settings).
- `DRO`: X/Y/Z tiles, per-axis `0` button, WCS menu.
- `JOG`: axis, step, `-`/`+`, and the `CONT` continuous switch.

### Right column

- `COOLANT`: Mist/Flood/Fan buttons for the channels the controller reports.
- `MACROS`: eight quick-action buttons. These are separate from user macros.
- `SIGNALS`: limit/probe/E-stop/door rows plus the reported `Pn` capabilities.
- `CONTROLLER DIAGNOSTICS` (Utility) opens a read-only snapshot of the `$PINS`
  pin map, `$PINSTATE` input/output bits, `$LIM` per-axis limits, and `$LEV`
  last events. `REFRESH` sends only those four read-only reports; there is no
  automatic refresh.
- `WORK`: current origin values, feed, WCS, and `G20`/`G21`, `G90`, `G17`
  chips.
- `SPINDLE`: on/off switch, RPM field, `CW`/`CCW`, and the override slider.

### Modes

- **Job** — viewer, job toolbar, progress, program list, tool-change sheet.
- **Probe** — probe cycle cards, preflight, runs, results, height map.
- **Offset** — live work offsets, zeroing, G28/G30/G92 actions, backup.
- **Utility** — user macros, `$N0`/`$N1` startup lines, controller profile,
  backup bundle.
- **Tools** — editable app tool table plus the read-only controller table.
- **Settings** — generated `$ES`/`$EG` editor.

## 5. Manual control

### DRO and WCS

The DRO shows the active work position in display units. The WCS menu selects
`G54` through `G59.3`. Tapping a DRO value opens the keypad and sends a
`G10 L20` line so the current position reads the entered value; the `0` button
next to an axis sets the current position to zero through the same guarded
path. Both require a live controller and `Idle`.

### Jog

- Select the axis, then a step: `0.01`, `0.1`, `1`, `10`, or `100` mm.
- `-` and `+` send one relative jog when tapped.
- Turn on `CONT` for press-and-hold. The app re-issues the step every 120 ms
  while held; releasing after at least 250 ms sends jog cancel. A quick tap
  keeps its single step.
- Jog is allowed in `Idle`, `Hold`, `Jog`, and `Tool`. It is refused in
  `Alarm` and `Door`.
- Continuous jog is a repeated step, not a true velocity jog. Keep steps small
  near fixtures and the probe.

### MDI

Type a line in the `MDI` field and press `SEND`. The engine logs the command
and the controller's `ok` or `error` in the Console. MDI is enabled only when
the link is ready and the machine is `Idle`.

The card keeps the last 10 accepted commands for the session. `UP`/`DN` recall
an entry into the field; recall never sends. Empty and consecutive duplicate
entries are ignored, and the history is not persisted.

### Overrides

- **Feed override:** 10–200%, slider plus `-10`/`+10`, fine `-1`/`+1`, and a
  primary `RESET 100%` action.
- **Spindle override:** 50–200%, slider in the Spindle card.
- **Rapid override:** the `RAPID 100%` chip in the viewer header cycles
  `100% -> 50% -> 25% -> 100%`. It shows the value reported by the controller;
  a tap never changes the label until the next snapshot confirms it.

### Spindle and coolant

The Spindle card has an `ON`/`OFF` switch, an RPM field (keypad, 0–24000), and
`CW`/`CCW`. The coolant buttons cover only the channels the controller
reports. The right-rail `M8/M9` quick action toggles flood.

### Reset and stop

- Top-bar `RESET` sends a soft reset.
- Top-bar `STOP` sends the controller stop command. It is not the same as the
  job toolbar's `STOP`, which aborts the job and sends a reset to discard
  buffered motion.
- The job toolbar also has `START`, `PAUSE`, `RESUME`, and `STOP`.

### Controller sleep

**Utility** → `CONTROLLER SLEEP` opens a confirmation sheet. Confirming sends
`$SLP` only when the link is ready, the machine is `Idle`, and no job is active.
Sleep stops status polling and freezes the DRO, signals, and job state; the app
does not wake the controller automatically. Use the top-bar `RESET` (or a
status wake that the controller supports) to recover. CANCEL sends nothing.

## 6. Jobs

### Program source

`LOAD PROGRAM` in the Job toolbar opens an in-app file browser. Browse one
directory at a time, type a path or use `UP`, and select a `.nc`, `.gcode`,
`.tap`, or `.ngc` file. Hidden names and symlinks are not shown. Binary,
non-UTF-8, empty, and files larger than 20 MiB are rejected with a clear
message. Loading only parses and installs the program: it never starts the job.

The sheet also lists up to ten `RECENT FILES`, newest first. Click a row to load
it through the same bounded reader. A missing path is marked `MISSING` and stays
selectable only for `REMOVE`; `CLEAR` removes the list. Recent entries are local
`app.json` data and are never sent to the controller.

You can drop one local program file onto the Job workspace. Directories,
multiple files, and empty drops are refused. The drop uses the same bounded
reader and guarded load path; it never starts the job.

With the simulator, the bundled demo program still loads after connect:

| Program | How to select it |
|---|---|
| `demo-pocket.nc` | Default demo |
| `demo-program-stop.nc` | Start with `--demo-m0` (M0 pause) |
| `demo-toolchange.nc` | Start with `--demo-toolchange` (stops at `M6 T2`) |

`--demo-start` presses `START` once after the demo loads. It is a review
switch, not a normal startup behavior.

Probe runs load their own generated programs. Height-map Apply loads a
rewritten copy of the current program, and **Offset** → `LOAD OFFSETS FILE`
loads a validated `offsets.nc` as a restore tool. Neither is a general job
loader.

### Program list and progress

Open the left **Program** tab to see the parsed line list. The current line is
highlighted as the job runs. The footer shows total lines and
`line N | P%`. The viewer header shows the same percentage, line count, and an
`ETA MM:SS` estimate while a job is running or paused with usable progress.
Search supports `FILTER` and `HIGHLIGHT`, with `PREV`, `NEXT`, and `JUMP`.

### Run options

The `RUN OPTIONS` chip in the viewer header opens three controller-reported
toggles:

- `OPTIONAL STOP` — honor `M1` stops.
- `SINGLE BLOCK` — hold after each block.
- `BLOCK DELETE` — skip lines marked `/`.

Each tap sends one command and the row updates only when the next status poll
reports the new value. Block delete is disabled while a job is running or
paused; optional stop and single block can be changed when the engine accepts
them. Mock mode shows the sheet but disables every toggle.

### Dry run / Check

`RUN OPTIONS` → `CHECK PROGRAM (DRY RUN)` loads the program through the
controller's `$C` check mode. Motion is suppressed; the controller still parses
and acknowledges every block, and spindle/coolant commands are not real output
in check mode. The job shows `CHECKING` / `CHECK COMPLETE`, then returns to
`Idle` with the program still loaded. STOP during a check uses a non-resetting
stop. A normal START still runs the program for real.

### Breakpoints and run-to

- Tap a program row's line-number gutter to toggle a breakpoint. A red dot and
  red line number mark an enabled breakpoint; the program strip shows the count
  (`3 BP`).
- Tap a row to select it, then use `BP` to toggle or `RUN TO` to open the
  confirmation.
- `RUN TO` sets a breakpoint at the chosen line and starts the program from
  line 1. Every earlier line still runs; the job pauses before the target.
  Like `START`, it holds the start behind the over-travel confirmation when
  the program exceeds reported travel; cancelling leaves the breakpoint set.
- Earlier breakpoints still pause first. The sheet lists them; use `RESUME`
  through each one to reach the target.
- `START-FROM-LINE` is not supported: the engine cannot synthesize the modal
  state for a mid-program start. The UI never sends it.
- Breakpoints are cleared for a new program. The job toolbar shows
  `BREAKPOINT AT LINE N` when the job pauses at one.

### Start, pause, resume, stop

- `START` is enabled for a loaded program when the link is ready, the machine
  is `Idle`, and no alarm/door is active.
- When the loaded program's estimated machine bounds exceed the travel reported
  by `$130`–`$132` with soft limits on, `START` (and `RUN TO`) opens an
  over-travel confirmation showing the work bounds, estimated machine bounds,
  and per-axis overrun. `START ANYWAY` sends one normal start; `CANCEL` sends
  nothing. This check is informational: it uses the active WCS plus `G92` and
  does not cover tool-length offset, in-program `G10`/`G92` changes, or the
  controller's own soft-limit enforcement. `NOT CHECKED` keeps the direct
  start.
- `PAUSE` sends feed hold and freezes streaming.
- `RESUME` sends cycle start. It is refused for a pending tool change and for
  a status-watchdog pause.
- `STOP` aborts the job and sends a reset. The loaded program stays available
  for a deliberate restart.
- The job status chip shows `RUNNING`, `PAUSED`, `COMPLETED`, `ABORTED`, or
  `INTERRUPTED`; the pause reason is shown next to it, for example
  `program stop (M0)`.

### Tool change

At `M6` the controller enters `Tool` state and the job waits. The
`TOOL CHANGE REQUIRED` sheet shows the tool number, the source line, and the
app tool-table record (name, diameter, length) if one exists.

1. Install the tool and set its offset. Jogging remains allowed while the job
   waits.
2. Press `1. ACKNOWLEDGE` (sends `0xA3`). This does not move any axis.
3. Press `2. RESUME` to continue.
4. `STOP JOB` is available if you do not want to continue.

The job does not send another line until you resume.

### Job history

Utility → `JOB HISTORY` records one entry when a job completes, stops, aborts,
fails, or is interrupted by a lost link. Each entry stores the local start
time, program name, elapsed time, line count, result, and whether it was a
`$C` dry run. The card shows the newest ten; `OPEN HISTORY` shows all stored
entries, capped at 100. History is stored in `app.json` on this computer and is
never sent to the controller. `CLEAR HISTORY` asks for confirmation and removes
only those local records.

### Alarms and disconnects

An `ALARM`, E-stop, door event, or lost link stops the job. Nothing
auto-resumes. Read the Console, clear the cause, reset or unlock as
appropriate, verify the position, then start again deliberately.

## 7. Viewer

The center workspace shows the parsed toolpath in the **Job** mode.

- `2D TOP` is the default and resets at every restart.
- `3D` is an orbit view. It is perspective only; there is no orthographic
  toggle.
- The `STOCK ON` / `STOCK OFF` chip toggles an inferred stock box in 3D. It is
  derived from the program's XY bounds with a small margin, a top at work Z=0
  (or above the path when the whole path is at/above zero), and a bottom two
  millimetres below the lowest path Z. It is a visual reference, not a
  measured stock model.
- `FIT` refits the current view. Double-click also refits.
- The overlay chips show WCS, units, and zoom. The legend shows `Cut`,
  `Rapid`, and `Plunge`.

### Gestures

- **2D:** drag pans; wheel or pinch zooms at the cursor.
- **3D:** left drag orbits; Shift+left drag or middle drag pans; wheel or
  pinch zooms at the cursor.
- The 3D strip reads `DRAG ORBIT | SHIFT/MIDDLE PAN | WHEEL ZOOM`.

The viewer draws a ground grid and a dashed work-envelope box in 3D, plus the
inferred stock box when `STOCK ON`, and a grid/envelope in 2D. The tool marker
follows the live work position (or machine position when no work offset is
available). A height-map cell overlay appears in the height-map result and
saved-map views.

Geometry is cached by program identity, camera, and viewport. A status tick
moves only the marker; it does not rebuild the toolpath. The performance
target is 200k visible segments at 60 fps, with screen-space decimation.

## 8. Settings

**Settings** is generated from the controller's `$$` values and `$ES`/`$EG`
metadata. There is no hardcoded settings list.

- The group rail lists `All settings`, `General` when ungrouped settings
  exist, and the reported `$EG` groups, including child groups. A dot marks a
  group with pending edits.
- `SEARCH` matches a setting number, name, or group name. Picking a group
  clears the search.
- Controls are chosen by type: boolean switch, bitfield/exclusive/axis-mask
  checkboxes, radio, integer/decimal field with the in-app keypad, text,
  password, and IPv4.
- Each row shows the setting number, name, type, unit, range, format, and
  `REBOOT` when the controller says a reboot is required.

### Drafts and Apply

Editing changes a local draft only. A row marked `PENDING` is not written yet.
Press `APPLY` to queue the changed rows; `REVERT` discards all drafts.

Writes are one at a time and read back from the controller. A row moves
through `QUEUED`, `WRITING`, and `APPLIED`; the app only marks it `APPLIED`
after the read-back matches. An out-of-range value is marked `INVALID`, and
`APPLY` is refused until it is fixed; no queued row is sent while an invalid
row exists. A rejected or mismatched write is marked `FAILED`; the previous
value stays active.

Settings writes are **idle-only**: the link must be ready, the machine must be
`Idle`, and no job may be active. The footer shows `IDLE: WRITES ENABLED` or
`WRITES LOCKED: <reason>`.

### Backup warning

This build does **not** prompt for or create a backup automatically before a
settings write. Export a backup first (Utility → `EXPORT`). The reference
board uses EEPROM; avoid unnecessary writes.

Utility → `CONTROLLER RESET` covers the destructive `$RST` commands. It shows
the backup path, can export a backup, and requires either a successful export
or the `I HAVE A BACKUP OR ACCEPT DATA LOSS` acknowledgement. Each target
(`$RST=#`, `$RST=$`, `$RST=&`, `$RST=*`) has its own confirmation with the
exact command and consequences. The reset is idle-only and is sent once;
local profiles, backups, and saved maps are not changed. `$RST=*` is the most
destructive and usually forces a controller re-handshake.

## 9. Offsets and tools

### Live offsets

**Offset** shows the live `$#` values for `G54`–`G59.3`, plus read-only
`G28`/`G30`, `G92`, `HOME`, `TLO`, `TLR`, `G51`, and `PRB` rows when reported.
`REFRESH $#` re-reads them.

- The active WCS panel has 48 px `ZERO X`, `ZERO Y`, `ZERO Z`, and `ZERO ALL`
  buttons.
- Tapping a WCS value opens the keypad. A value edit sends `G10 L20` for that
  WCS and axis.
- `GOTO G28`, `GOTO G30`, and `CLEAR G92` open a confirmation sheet. `G28`/
  `G30` move to the controller's stored target; `CLEAR G92` sends `G92.1`.
  Cancel sends nothing. The sheet also warns if the DRO changed since it
  opened.
- WCS changes and offset writes require a live, `Idle` controller.

### offsets.nc export and restore

`EXPORT OFFSETS` writes a local WCS-only file at
`<config>/exports/offsets.nc`. It starts with `G21` and contains one
`G10 L2 P<n>` line per reported `G54`–`G59.3`. It deliberately omits `G92`,
`G28`/`G30`, `HOME`, `TLO`/`TLR`, `G51`, and `PRB`.

`LOAD OFFSETS FILE` validates the file and opens a preview showing the
`G10 L2` lines and the `G21` units. `LOAD AS PROGRAM` loads it into the job;
it does **not** start it. Running it overwrites the stored WCS offsets and
needs the usual ready/`Idle` gate. `LOAD AS PROGRAM` is locked while a job is
active.

### Tool tables

- **Tools** is the app-side table: number, name, diameter, length, and pocket.
  `NEW TOOL`, `EDIT`, `DEL`, and `SAVE` persist it in the controller profile.
- The M6 tool-change sheet uses this table for the tool name and summary.
- A controller-reported tool table, when present, is shown separately and
  read-only. The reference board reports zero entries.
- Setting the tool length with `$TLR` or a controlled MDI move remains a
  manual step. The tool-length probe cycle offers a confirmed `G43.1` write
  and a `G49` clear after the measured delta; the reported `$#` TLO verifies
  the result.

## 10. Probing

Open **Probe**. The page starts with the always-visible `PROBE OFFSET X/Y`
bar, then the cycle cards:

- Z touch plate
- Edge X
- Edge Y
- Corner
- Center circle
- Center rectangle
- Rotation
- Tool length
- XY touch plate
- Height map

Select a card (or press its `CONFIGURE`) to open the detail sheet.

### Probe offset and coordinates

The probe mount offset is `probe tip - controlled point`, in millimetres. It is
stored per controller profile and never sent to the controller. Entered
positions, centres, and the height-map grid origin are **feature/probe-tip**
coordinates. Generated plan `FROM`/`TO` values and `PRB` results are
**controlled-point** coordinates: `controlled = feature - probe offset`.

### Probe defaults

The `PROBE DEFAULTS` bar owns the seven shared values: safe Z, search, feed,
retract, probe diameter, edge offset, and plate thickness. They are stored per
controller profile and prefill a newly loaded profile. Edit the fields on any
cycle, then press `SAVE DEFAULTS`; the app writes only the local profile JSON
and sends no controller command. Per-cycle anchors and expected values
(expected Z/X/Y, centre, radius, touches, separation, reference Z, grid
origin/step, rectangle size, plate X/Y) stay per run and are not saved.
`SAVE DEFAULTS` is refused while a probe run is active or a shared field is
invalid.

### Probe macros

The `PROBE MACROS` bar edits two optional snippets inserted into every generated
probe program: a pre-macro after the safety preamble and safe-Z move, and a
post-macro after the final safe-Z retract. Both are profile-wide, stored in the
local controller profile, and saved with `SAVE MACROS`; nothing is sent to the
controller.

Allowed lines are deliberately narrow: blank/comment lines, `G4 P0`–`G4 P60`,
`M0`, `M1`, `M5`, and `M9`. Each macro is limited to 512 bytes and 8 lines.
Motion, probing, spindle-on, coolant-on, tool changes, coordinate changes, and
system commands are rejected with the line number. The inserted lines still
pass the normal preflight and are shown in the generated program.

### Preflight

`RUN` is refused unless every gate passes:

- the link is ready;
- the machine is `Idle` and no job is active;
- spindle and coolant are off;
- safe Z is explicit and reachable, and the search distance/feed are bounded
  and positive;
- the probe input is reported and not already triggered;
- the probe is verified connected (a `PC` signal, or the explicit
  `Probe connected` confirmation when the controller has no `PC` signal);
- `$#` offsets are loaded;
- the entered fields are valid.

The preflight panel shows `READY` or `REFUSED`, each gate as a row, the
controlled-point search travel, and the generated program. The footer says
`RUN is the only action that starts motion; there is no auto-start`.

### Running

Press `RUN` (or `RUN AGAIN` after a completed run). The run panel shows
`PROBE RUN`, bound/expected results, and `ABORT` while running. A probe run
loads its generated program through the normal job runner, so it uses the same
streamer and safety gateway.

### Results

On completion, review the measured value and the old/new offset. `APPLY
OFFSET` opens `CONFIRM OFFSET WRITE` with the exact `G10 L2` line;
`CONFIRM WRITE` sends it, and `$#` is refreshed after the write. `KEEP` leaves
the offsets unchanged. Nothing is applied automatically.

`TOOL LENGTH` reports reference Z, contact Z, and the signed delta, then offers
`APPLY TLO` (`G43.1 Z<current + delta>`) and `CLEAR TLO` (`G49`). Both require
an inline confirmation, are allowed only while the controller is `Idle`, and
send no motion. The result shows `APPLIED` only after `$#` reports the target
TLO; a missing or mismatched read-back, alarm, controller error, disconnect, or
timeout shows `FAILED` and keeps the old value visible.

`ROTATION` reports the measured angle and deviation. Applying work rotation is
not supported in this build.

### PRB audit

The run panel shows `RAW PRB (machine mm)` results in plan order, and the
Console carries every generated line and `PRB` line. Probe failure alarms
(4/5/13) stop the job and require operator recovery. There is no automatic
retry or resume.

## 11. Height map

Choose the **Height map** card. The fields cover:

- `MAP Z OFFSET`, the constant datum shift;
- `ORIGIN X WORK`/`ORIGIN Y WORK`, `STEP X`/`STEP Y`, `COLUMNS`, `ROWS`;
- `SURFACE Z MACH`, `SAFE Z`, `SEARCH`, `FEED`, `RETRACT`, and `PROBE DIA`.

The `GRID SUMMARY` shows `columns x rows = points`, the probing distance, and
an estimated duration. The limit is 4096 points. The summary also prints the
work/machine origin, safe Z, surface Z, and travel.

### Safe Z and search

Safe Z must clear the highest point on the stock; every grid probe descends
from it. Search must cover `SAFE Z - SURFACE Z`. The preflight shows the
generated travel and at most eight probe lines for the map.

### Map Z offset

`MAP Z OFFSET` is added to every measured height for the summary, overlay,
CSV, and Apply. It does not change the grid, the plan, or the surface-Z search
target. Raw samples and the offset are kept.

### Run and partial maps

`RUN` starts the row-major grid with a retract between points. `ABORT` is
available while running. If the run stops early, the partial map is shown but
missing cells stay invalid. There is no auto-retry, no interpolation across a
failed corner, and no auto-resume.

### Overlay and CSV

The result view shows `HEIGHT MAP RESULT`, `COMPLETE` or `PARTIAL`, min/max
heights, and the compensation range. The measured cells appear in the viewer
overlay. `EXPORT CSV` writes `<config>/exports/height-map.csv`.

### Save and load per profile

`SAVE MAP` opens a name sheet and stores the map in the current controller
profile. A profile keeps at most 16 maps. The saved-map browser shows the
date, cell count, WCS, tool, complete/partial state, and Z offset. Opening a
saved map is read-only; `CLOSE MAP` returns to the editor.

### Frame check, Apply, Revert

A saved map records its WCS, G92, and TLO frame. If the live frame differs,
the sheet shows `FRAME MISMATCH` and lists the differences; Apply is refused.
Re-probe in the live frame or restore the recorded offsets.

`APPLY HEIGHT MAP` rewrites the loaded part program with bilinear
compensation, keeps the original for Revert, and labels the job
`HEIGHT MAP / COMPENSATED`. The confirmation sheet uses `CONFIRM APPLY` and
shows line/motion/arc/warning statistics. Apply requires a complete map, a
ready and `Idle` controller, no active job, reported offsets, a matching frame,
and the original program. Programs that leave the mapped area are refused
rather than extrapolated.

Apply only loads the rewritten program; it never starts it. `REVERT` restores
the original source. Opening a saved map never applies compensation or starts
a job.

## 12. Utilities, macros, and startup lines

### Utilities

**Utility** includes these setup and source-preparation tools. Generated or
transformed programs are previewed and only loaded after confirmation; nothing
starts automatically.

- `MACHINE SETUP` reopens the first-run safety/setup wizard. The card shows
  `COMPLETE` or `NOT COMPLETED`; opening it never clears the saved flag and
  finishing again saves the current choices.
- `MATERIAL SURFACING` generates a bounded back-and-forth raster in the active
  WCS. Fields are millimetres: stock origin/width/depth, cutter diameter,
  stepover, total depth, depth per pass, feed, plunge feed, safe Z, and spindle
  RPM. It reports depth passes, rows, lines, cut length, and estimated cutting
  time. The cutter centerline is inset by its radius; there is no lead-in/ramp.
  The generated program is **not checked against machine travel, stock, or
  tool**.
- `PROGRAM LIMITS` is read-only: work-coordinate X/Y/Z bounds, lines, segments,
  arcs, and tool changes. If `$20` soft limits and `$130`–`$132` travel are
  available it reports overruns; otherwise it says `NOT CHECKED` rather than
  implying safety. The card itself never blocks START; the separate pre-run
  confirmation can hold START when the loaded program exceeds reported travel.
- `SOURCE TRANSFORMS` adds line numbers (`N10` step 10 by default) or strips
  `(...)`/`;` comments. Preview first, then `LOAD AS PROGRAM`. Loading replaces
  the current job source and clears any height-map Apply/Revert state for the
  previous source.
- `ROTATE PROGRAM` rotates the loaded source about a work-coordinate pivot by a
  signed angle (counter-clockwise positive). Preview shows the line/move/arc
  counts, any `R` arc converted to `I`/`J`, and the first lines. `LOAD AS
  PROGRAM` requires confirmation and sends one load; it never starts. The
  transform refuses incremental motion, `G53`/`G28`/`G30`, probing, canned
  cycles, `G10`/`G92`, `G18`/`G19` arcs, helical arcs, `G93`, `G51`, checksums,
  and mid-program WCS switches. A completed rotation probe can copy its
  `-deviation` into the angle field; the pivot stays manual.
- `SCALE PROGRAM` applies a uniform positive factor about a work-coordinate
  pivot. Preview shows the stats, any `R` arc converted to `I`/`J`, and the
  first lines. Feeds, speeds, Z depths, and dwells are unchanged; only XY
  geometry is scaled. `LOAD AS PROGRAM` requires confirmation and never starts.
  It refuses the same unsupported constructs as rotation, plus a non-positive
  or out-of-range factor.
- `STEPPER CALIBRATION` calculates a new `$100`/`$101`/`$102` steps/mm value
  from the current value, commanded distance, and measured distance
  (`new = old × commanded ÷ measured`). It does **not** move the machine: jog or
  run the test move separately. `REVIEW & APPLY` requires confirmation, is
  idle-only, and writes through the verified setting path. The proposal is
  bounded to 1–10,000 steps/mm and shown with its delta.
- `GANTRY SQUARING` is a calculator only. Enter the span between the left and
  right measurement points and the Y reading at each side;
  `delta = right - left`, `angle = atan2(delta, span)`, and the pivot-centre
  per-side correction is `delta / 2`. Positive means the right side is further
  along +Y (forward). There is no APPLY, no motion, and no controller write:
  follow the machine's squaring procedure, adjust by hand, and measure again.
- `CONTROLLER RESET` sends one confirmed `$RST` target after a backup prompt:
  offsets/tool data (`#`), core settings (`$`), driver/plugin defaults (`&`),
  or everything (`*`). It is idle-only, never changes local profiles or
  backups, and never auto-reconnects or resumes.
- `JOB HISTORY` is local only. A completed, stopped, aborted, failed, or
  interrupted run records its program name, start time, elapsed time, line
  count, result, and whether it was a dry run. The card shows the newest ten;
  `OPEN HISTORY` shows all stored entries (maximum 100). `CLEAR HISTORY`
  requires confirmation and writes only `app.json`; nothing is sent to the
  controller.

### User macros

User macros live in **Utility** → `MACROS` and are stored per controller
profile.

- `NEW MACRO` opens the editor. A macro has a name and one MDI command line,
  up to 256 bytes. A name is unique per profile.
- System (`$`) commands, newlines, and control characters are rejected. This
  is not a multi-line or parameterized macro system.
- `Ask before running` is on by default. A macro with it on shows a
  confirmation with the exact command and `RUN MACRO`.
- `RUN`, `EDIT`, and `DEL` act on the selected macro. `RUN` sends through the
  guarded MDI path only when the live link is ready, the machine is `Idle`, no
  job is active, and the state is not `Alarm`/`Door`. A macro never runs on
  load, connect, or profile import.

The seven buttons in the right-rail `MACROS` panel are separate quick actions:
Zero XY, Zero Z, Safe Z, Home, Unlock, Probe Z, and M8/M9. `Probe Z` opens
the Probe workspace's Z touch-plate card; it does **not** start a probe.

### Startup lines

**Utility** → `STARTUP LINES` reads `$N0` and `$N1`.

- `RELOAD` requests them from the controller.
- `EDIT` opens the text sheet. `APPLY` opens `EXACT TEXT TO STORE`; nothing is
  written until `CONFIRM WRITE`.
- A write is idle-only, one at a time, and read back. The row shows
  `PENDING`, `QUEUED`, `WRITING`, `VERIFYING`, `VERIFIED`, `MISMATCH`, or
  `FAILED`.
- A mismatch or failure says `WRITE NOT APPLIED - THE PREVIOUS VALUE IS STILL
  ACTIVE`.
- Startup lines execute automatically on every controller reset and can
  command motion, spindle, or coolant. Edit them with the machine clear.

### Keymap

**Utility** → `KEYMAP` lists ten machine actions: jog X−/X+, Y−/Y+, Z−/Z+,
`START`, `PAUSE`, `RESUME`, and `STOP`.

- Defaults are modifier-heavy so typing is not stolen:
  `Ctrl+Shift+arrows` for X/Y, `Ctrl+Shift+PgUp/PgDn` for Z, and `F5`–`F8` for
  start/pause/resume/stop.
- `EDIT` opens a capture sheet; `RESET DEFAULTS` restores the defaults.
  Changes stay local until `SAVE`, which writes the per-profile keymap.
- Rebinding is applied at runtime after profile load or SAVE; no restart is
  required.
- A focused text field or the key-capture editor suppresses machine actions.
  `STOP` remains available while non-text sheets are open.
- Keyboard jog is one step per press; hold-to-jog remains pointer-only.
- Refusals follow the same gates as the buttons: mock, not ready, Alarm/Door,
  active job, no program, probe program, no paused/running job, pending tool
  change, or watchdog pause.

## 13. Profiles and backups

### Controller profile

The controller profile is keyed by the `$I+` identity: firmware, board,
driver, and serial number when available. It stores controller settings,
offsets, the app tool table, user macros, `$N0`/`$N1`, the local keymap
record, saved height maps, and the probe mount offset.

The profile file is under `<config>/profiles/`; the app preferences are
`<config>/app.json`. Writes are atomic, and the store keeps rolling backups
(last five versions by default).

### Save

**Utility** → `CONTROLLER PROFILE` shows the identity and file path.

- `SAVE PROFILE` writes the controller profile.
- `SAVE APP` writes the app preferences.
- `USE LOCAL DATA` appears when identity is unverified and local data is
  waiting; use it only when the data belongs to this machine.

### Backup bundle

**Utility** → `BACKUP BUNDLE` uses a fixed local file at
`<config>/exports/ghs-backup.json`.

- `EXPORT` writes the bundle.
- `VALIDATE` checks the schema, identity, and data without writing.
- `IMPORT` validates first. If the bundle needs confirmation (identity
  mismatch or unverified identity), the button changes to `CONFIRM IMPORT`;
  press it again to accept.

The bundle contains app preferences, controller identity, controller
settings, offsets, the app tool table, macros, startup lines, the keymap
record, saved height maps, and the probe offset. Import validates before
writing and never pushes anything to the controller. If the bundle belongs to
another controller, it is stored for that identity but the current session is
unchanged.

A reconnect with a different controller does not silently adopt the previous
machine's data. Check `CONTROLLER PROFILE` before trusting offsets or tools.

## 14. Troubleshooting

### Connection fails

- **No ports listed:** check the cable and adapter, press `REFRESH`, and
  confirm the OS sees the device.
- **Permission denied (Linux):** add your user to `dialout`, then log out and
  back in.
- **Port busy:** close other senders, serial monitors, or daemons holding the
  port.
- **Wrong baud:** select the controller's rate; the default is `115200`, 8N1.
- **No response but the port opens:** this build does not pulse DTR. Confirm
  the controller is powered and not held in reset. If a pendant has control
  (`|MPG:1`), release it and wait for `|MPG:0`.
- **Connect error:** the `CONNECTION` sheet shows the engine error. Correct it
  and press `CONNECT` again.

### Controls are disabled or the command is refused

The app gates commands by connection and machine state. Most manual controls
need a live link and `Idle`; jogging also allows `Hold`, `Jog`, and `Tool`.
Settings, offset edits, user macros, and startup-line writes are idle-only.
If a control is disabled, read the status pill and the Console. `NOT READY`,
`ALARM OR DOOR`, `JOB RUNNING`, and `CONTROLLER NOT IDLE` are the usual
reasons.

### Motion refused in Alarm or Door

The safety gateway refuses motion in `Alarm` and `Door`. Read the alarm text,
clear the physical cause, then use `RESET` or the `Unlock` quick action as
appropriate. Verify the position and homing state before moving. The job will
not resume by itself.

### Probe refused

Check the preflight `REFUSED` rows:

- link not ready or machine not `Idle`;
- a job is active;
- spindle or coolant is on;
- the probe input is missing, already triggered, or not verified;
- `$#` offsets are missing;
- safe Z, search, feed, or another field is invalid;
- the probe offset is invalid.

For a controller without a `PC` signal, tick `Probe connected` only after
physically checking the probe. The reference FlexiHAL has no `PC` signal.

### Probe failed or alarmed

Probe failure alarms (4/5/13) stop the job. There is no automatic retry or
resume. Clear the alarm, inspect the probe and workholding, verify safe Z and
the search bound, then run a fresh preflight. The raw `PRB` results remain in
the run panel and Console for audit.

### Height map problems

- **FRAME MISMATCH:** the live WCS, G92, or TLO differs from the recorded map.
  Re-probe in the live frame or restore the recorded offsets.
- **Incomplete map:** missing cells are invalid. Re-probe them; Apply is
  refused and the app will not interpolate across a failed corner.
- **Outside coverage:** the program leaves the mapped area. Apply is refused
  rather than running uncompensated moves. Re-measure a larger grid or adjust
  the program.
- **No original program:** load the part program before measuring; Apply needs
  the original source for Revert.
- **Save blocked:** the profile is absent, corrupt, unverified, or pending
  confirmation, or the map has no valid cell.

### Macro refused

A user macro is refused in mock mode, in `Alarm`/`Door`, during an active job,
or when the controller is not `Idle`. A command containing `$`, a newline, or
control characters is rejected at edit time. Macro saves are also locked when
the profile identity is unverified or pending.

### Startup-line write mismatch

The row shows `MISMATCH` or `FAILED` and states that the previous value is
still active. Press `RELOAD` to read the controller's value, correct the
draft, and apply again. Do not assume the new text is stored until the row
shows `VERIFIED`.

### Settings write rejected

- An `INVALID` row has a local range/type error; correct it before `APPLY`.
- A `FAILED` row was rejected by the controller or its read-back did not
  match. The previous value remains active.
- If `APPLY` is disabled, check the footer reason: mock mode, not connected,
  alarm/door, job running, or not idle.
- Some settings require a controller reboot; the row shows `REBOOT`.

### Job stopped or disconnected

A disconnect or alarm marks the job `INTERRUPTED` or `ABORTED`. The app keeps
the program and line information but never auto-resumes. Reconnect, confirm
the machine state and position, home if needed, and start the job again
deliberately. Do not assume the old position is still valid after a lost
link.

### Where logs live

- The in-app **Console** tab is the live log; it keeps the last 200 lines.
  `SAVE LOG` writes the current buffer (or the active filter) to
  `<config>/exports/console.log`, overwriting the previous snapshot. The file
  has a timestamped header, is capped at 64 KiB, and is not a continuous
  session recording.
- Launch the app from a terminal to see stderr startup and window errors.
- Configuration, profiles, exports, and rolling backups live under the
  platform config directory:
  - Linux: `~/.config/ghs-rs`
  - macOS: `~/Library/Application Support/ghs-rs`
  - Windows: `%APPDATA%\ghs-rs`

Exports are user-initiated. There is no telemetry and no network transport.

## 15. Known limits

- **Hardware validation is open.** The reference FlexiHAL / STM32F446 board
  has not been available. All evidence in this guide is from the in-process
  simulator and the test suite. Do not treat simulator success as machine
  sign-off.
- **Builds are unsigned and pre-alpha.** Linux archives have no signature,
  macOS builds are not signed/notarized, and Windows builds have no
  Authenticode signature. See [RELEASE.md](RELEASE.md).
- **Machine keymap only.** The editor covers jog/start/pause/resume/stop; full
  keyboard navigation, screen-reader labels, and gamepad mapping are not
  implemented.
- **No start-from-line.** The job toolbar can set breakpoints, run to a line
  from the beginning, and dry-run the whole program, but it cannot start
  mid-program; the engine rejects `from_line > 1` because it cannot synthesize
  a modal preamble.
- **File loading is one file at a time.** Drag-drop accepts exactly one regular
  program file, and the in-app browser lists one directory at a time. There is
  no multi-file queue.
- **Advanced macros are absent.** User macros are single-line MDI commands;
  no parameters, special words, multi-line bodies, or `$` system commands.
- **Probe Apply is partial.** Rotation reports its measured angle, but work
  rotation is deferred. Tool-length probing can apply or clear TLO with a
  confirmed `G43.1`/`G49` write verified through `$#`.
- **3D is perspective only.** There is no orthographic toggle. The stock box
  is inferred from program bounds, not measured from the fixture, and the
  height overlay is a cell projection rather than a shaded surface.
- **No SD, ATC, or MPG UI.** Their protocol fields are parsed for future use;
  the v1.0 shell does not expose them.
- **No Park quick action.** A machine-specific park position is not configured;
  use the confirmed `GOTO G28` / `GOTO G30` actions in **Offsets**.
- **No continuous session recording or replay.** The Console keeps a bounded
  in-memory log and `SAVE LOG` exports a 64 KiB snapshot; there is no
  start/stop recorder, sequence-numbered trace, or replay mode. There is no
  telemetry or network transport.
