# grblHAL protocol support

This is the protocol reference for ghs-rs. It is based on the grblHAL core
headers and wiki as of late 2026, not only on the reference board. The board is
the first validation profile; the protocol layer must handle any grblHAL
controller and ignore what it does not understand.

Sources:

- `grblHAL/core`: `grbl.h`, `report.c`, `system.c`, `errors.h`, `alarms.h`,
  `settings.h`, `protocol.c`, `tool_change.c`.
- grblHAL wiki: Report extensions, For sender developers, Additional settings,
  Manual tool change, MPG and DRO interfaces, Compatibility levels,
  Expressions and flow control.

## 1. Connection and startup

grblHAL controllers may not reset on DTR, may not emit a banner, and may have
an MPG/pendant in control. The startup sequence must be defensive:

1. Open the serial port at the configured baud rate, 8N1, no flow control by
   default. Apply DTR/RTS as configured.
2. Listen for 400–500 ms. If a status report arrives with `|MPG:1`, a secondary
   sender is in control: enter passive mode, keep listening, and wait for
   `|MPG:0` before claiming control.
3. Request a full status report (`0x80` if `RT+`, otherwise `?`) and wait up to
   ~250 ms. If it arrives, assume an extended grblHAL protocol is available.
4. If an alarm is active, read its substate. Critical alarms (1, 2, 10, 17, 20
   in current headers) lock the controller: only real-time commands work until
   the condition clears and the user unlocks/homes/resets.
5. Request `$I+`. From it learn axes, options, signals, driver/board, plugins,
   buffer sizes, tool-table entries, and firmware.
6. If `ENUMS` is advertised, fetch `$EA` (alarms), `$EE` (errors), `$ES`
   (settings), and `$EG` (setting groups). If `SED` is advertised, `$SED=<n>`
   provides descriptions. Fall back to `$$` and bundled tables only when the
   controller cannot enumerate.
7. Disable controller auto-reporting if configured (`$481` or the `0x8C`
   toggle), then begin polling.

Do not require the banner. Do not assume a hard reset is possible. Do not write
configuration until the controller is idle and unlocked.

## 2. Framing and terminators

- A line is terminated by `\n` or `\r\n`. grblHAL also treats `\n\r` as one
  terminator. Use a single `\n` consistently from the sender.
- `ok` acknowledges one streamed line. `error:<n>` rejects one line. Status
  reports and messages may be interleaved.
- `ALARM:<n>` is asynchronous and not tied to a line.
- `[MSG:...]` is an informational/warning/error message.
- `[echo: ...]` echoes a received line.
- `>...:ok` reports a startup line result.
- Lines can be long; bound the parser and reject overlong lines safely.

## 3. Real-time commands

| Code | Legacy | Meaning | Notes |
|---|---|---|---|
| `0x18` | Ctrl-X | Soft reset | Warm reset; some states require it |
| `0x19` | Ctrl-Y | Stop | Like reset but preserves more internal state; grblHAL extension |
| `0x03` | Ctrl-C | Exit | Stop current operation |
| `0x14` | Ctrl-T | Reboot | Only after `0x1B` (ESC); if `REBOOT` capability |
| `?` / `0x80` | yes | Status report | Use `0x80` when `RT+` |
| `~` / `0x81` | yes | Cycle start / resume | Use `0x81` when `RT+` |
| `!` / `0x82` | yes | Feed hold | Use `0x82` when `RT+` |
| `%` | — | Program demarcation | Stream start/end marker |
| `0x83` | — | Parser state report | Shortcut for `$G` |
| `0x84` | — | Safety door | |
| `0x85` | — | Jog cancel | Send on every jog key/touch release |
| `0x87` | — | Full status report | Includes fields normally sent only on change |
| `0x88` | — | Optional stop toggle | When no physical switch |
| `0x89` | — | Single-block toggle | When no physical switch |
| `0x8A` | — | Fan 0 toggle | Fans plugin |
| `0x8B` | — | MPG mode toggle | MPG mode 2 |
| `0x8C` | — | Auto-report toggle | |
| `0x90` | — | Feed override reset | |
| `0x91/0x92` | — | Feed coarse +/- | |
| `0x93/0x94` | — | Feed fine +/- | |
| `0x95` | — | Rapid override reset | |
| `0x96/0x97/0x98` | — | Rapid medium/low/extra-low | |
| `0x99` | — | Spindle override reset | |
| `0x9A/0x9B` | — | Spindle coarse +/- | |
| `0x9C/0x9D` | — | Spindle fine +/- | |
| `0x9E` | — | Spindle stop toggle | |
| `0x9F` | — | Soft E-stop | Soft reset + E-stop alarm |
| `0xA0/0xA1` | — | Flood/mist toggle | |
| `0xA2` | — | PID report | Spindle sync debug |
| `0xA3` | — | Tool change acknowledge | Manual tool change protocol |
| `0xA4` | — | Probe-connected toggle | Virtual switch, if supported |
| `0xB0–0xB7` | — | Plugin macros | Plugin-defined |

`RT+` or `RT-` in `$I+` `NEWOPT` indicates support for the extended real-time
codes. The extended status/cycle/hold codes are **fixed values 0x80, 0x81,
0x82**, not a bitwise OR of the printable ASCII characters.

Only use printable real-time commands when the controller is not reading a
`$`-command or message body; grblHAL ignores them there. Use the extended
codes for robustness.

## 4. Status report

Syntax:

```text
<State{:substate}|WPos:...|MPos:...|Bf:planner,rx|Ln:n|DTG:...|FS:f,rpm{,actual}|F:rate|SPn:...|Pn:...|WCO:...|WCS:G54|Ov:f,r,s|A:S,C,E,F,M,T|Sc:...|AR:...|MPG:0|H:1{,mask}|D:0|T:n|P:n,P|TLR:0|In:n|FW:grblHAL|$C:1>
```

States: `Idle`, `Run`, `Hold`, `Jog`, `Alarm`, `Door`, `Check`, `Home`,
`Sleep`, `Tool`.

Substates:

- `Run:1` feed hold pending, `Run:2` probing.
- `Hold:<n>` parking/hold state.
- `Door:<n>` parking state.
- `Alarm:<n>` alarm code (always with `0x87` if nonzero).

Fields:

| Field | Meaning |
|---|---|
| `WPos`/`MPos` | Work or machine position, variable axis count |
| `Bf` | Planner blocks free, serial RX bytes free |
| `Ln` | Current line number |
| `DTG` | Distance to go in the current block |
| `FS` / `F` | Feed rate and programmed/actual RPM, or feed only |
| `SP<n>` | Additional spindle data (multi-spindle builds) |
| `Pn` | Input signals |
| `WCO` | Work coordinate offset |
| `WCS` | Active coordinate system (`G54`–`G59.3`) |
| `Ov` | Feed, rapid, spindle override percentages |
| `A` | Accessories: `S` CW, `C` CCW, `E` encoder error, `F` flood, `M` mist, `T` tool change pending |
| `Sc` | Scaled axes |
| `AR` | Auto-report on/off and interval |
| `MPG` | `1` pendant controls input, `0` released |
| `H` | Homing complete, optional homed-axis mask |
| `D` | Lathe radius/diameter mode |
| `T` | Current tool id |
| `P` | Probe id, optional `P` for probe protection active |
| `TLR` | Tool length reference set |
| `In` | Last `M66` input result |
| `FW` | Firmware (`grblHAL`), only in full report |
| `$C` | Blocking event/check-mode flag |
| `SD` | SD mount status or streaming progress; see below |

`Pn` letters: `X Y Z A B C U V W` limits, `P` probe triggered, `O` probe
disconnected, `D` door, `R` reset, `H` feed hold, `S` cycle start, `E` E-stop,
`L` block delete, `T` optional stop, `M` motor warning, `F` motor fault,
`Q` single-block, plus `I/i` probe triggered, `J/j` tool setter, `K/k`
probe 2 for probe-state builds.

`SD` has two forms:

- Mount state: `|SD:0|1|2|3` (not mounted, mounted, auto-detect off, auto-detect
  on).
- Streaming progress: `|SD:<percent>,<filename>` or `|SD:Pending`. Streaming
  progress is only in regular reports, not in `0x87`.

Rules:

- Do not assume field order.
- Ignore unknown fields and unknown letters.
- Do not assume a field is present in every report; many are sent only on
  change. Use `0x87` for a complete snapshot.
- Axis count comes from `$I+` (`AXS`), not from a fixed 3.
- Parse positions as `f64`; keep millimetres canonical.

## 5. System commands

Purpose and response:

| Command | Purpose |
|---|---|
| `$I` | Build/version info; `$I+` for extended capabilities |
| `$I=<text>` | Set build info string, if enabled |
| `$G` | Parser state (`[GC:...]`) |
| `$#` | Offsets, G51 scaling, HOME, tool table, TLO, TLR, probe result |
| `$#=<n>` | One NGC parameter, if enabled |
| `$$` | All setting values |
| `$$=<n>` | Details for one setting |
| `$+` | All settings including extended ones |
| `$<n>` / `$<n>=<v>` | Read/write one setting |
| `$SED=<n>` | Setting description |
| `$HELP [topic]` | Help topics or a topic |
| `$B`, `$S`, `$O` | Toggle block delete, single block, optional stop |
| `$C` | Check mode; reset exits |
| `$X` | Unlock |
| `$H`, `$H<axis>` | Home all or one axis |
| `$HSS` | Homing switch state |
| `$J=<gcode>` | Jog |
| `$N`, `$N0=`, `$N1=` | Startup lines |
| `$SLP` | Sleep |
| `$TLR` | Set tool length reference |
| `$TPW` | Probe tool plate |
| `$RST=*`, `$RST=$`, `$RST=&`, `$RST=#` | Reset settings/offsets/tools |
| `$EA`, `$EE`, `$ES`, `$EG` | Enumerate alarms, errors, settings, groups |
| `$EAG`, `$EEG`, `$ESG`, `$ESH` | CSV/TSV variants |
| `$E*` | Enumerate all |
| `$PINS` | Pin mapping |
| `$PINSTATE` | Auxiliary pin states |
| `$PORTS` | UART ports |
| `$LIM` | Current limit switch state |
| `$LEV` | Last control/limit events |
| `$SPINDLES`, `$SPINDLESH` | Enumerate spindles |
| `$SD`, `$SR` | Spindle encoder data/reset |
| `$SDS` | Stepper driver status |
| `$REBOOT` | Reboot, if supported |
| `$RTC` | Read/set RTC, if supported |
| `$DWNGRD` | Toggle downgrade flags |

All `$` command responses end with `ok` after the last line. Some may be
blocked in alarm/locked states. `$HELP commands` lists what the controller
actually supports; do not assume every command above exists.

`$J` examples:

```text
$J=G91 X10 F1000
$J=G91 X-5 Y5 F800
$J=G90 X0 Y0 F500
```

Jogs are not line-numbered and must be canceled with `0x85` on release.

## 6. Settings and code enumerations

When `ENUMS` is present, prefer the controller's own definitions:

```text
[SETTING:<id>|<group>|<name>|<unit>|<type>|<format>|<min>|<max>|<reboot>|<null>]
[SETTINGGROUP:<id>|<parent>|<name>]
[ALARMCODE:<code>|<message>|<description>]
[ERRORCODE:<code>|<message>|<description>]
```

Setting types:

| Type | UI |
|---|---|
| 0 | Boolean checkbox |
| 1 | Bitfield checkbox column |
| 2 | Exclusive bitfield; disable other bits when bit 0 is unset |
| 3 | Radio buttons |
| 4 | Axis mask checkbox column |
| 5 | Integer field |
| 6 | Decimal field |
| 7 | String field |
| 8 | Password field |
| 9 | IPv4 address field |

`N/A` labels are unavailable and must not be shown. `$EG` provides hierarchical
groups. `$SED` provides a longer description. `$HELP <topic>` provides help.

This means the settings UI is generated, not hardcoded. A bundled schema is
only a fallback for controllers without `ENUMS`.

## 7. Errors, alarms, and messages

- `error:<n>` is per line. grblHAL keeps the error active for subsequent G-code
  until a reset, an empty line, or a `$` command clears it. After an error, the
  streamer must stop and clear before continuing.
- `ALARM:<n>` is asynchronous. Critical alarms (current headers: 1 hard limit,
  2 soft limit, 10 E-stop, 17 motor fault, 20 expander exception) lock the
  controller; only real-time reports are accepted until the condition clears.
- Alarm 11 means homing is required and is not automatically cleared by reset.
- `[MSG:Info: ...]`, `[MSG:Warning: ...]`, `[MSG:Error: ...]` are user messages.
- Use `$EA`/`$EE` for descriptions when available; otherwise use the bundled
  enum tables as a fallback. Unknown codes display as `alarm N`/`error N`.
- Do not auto-resume after an alarm, E-stop, or reconnect.

## 8. Manual tool change protocol

On `M6`, grblHAL enters `Tool` state and suspends processing subsequent lines.
The sender must:

1. Show the tool change UI with the requested tool.
2. Send `0xA3` to acknowledge.
3. Allow jogging, `$` commands, and setting the linear-axis offset/TLO while
   the change is pending. Motion commands other than jog return error 40.
4. Issue cycle start (`0x81` or `~`) when the user confirms completion.

grblHAL saves the suspended input buffer and reinstates it after cycle start.
The capability is `TC` (manual) or `ATC=1/0` (automatic/offline). The reference
board is `TC` with zero controller tool-table entries.

## 9. MPG/pendant takeover

`|MPG:1` means a pendant has taken control of the input stream. The sender must
disable its UI and remain passive while still updating telemetry. `|MPG:0`
means control is released. At startup, a status report may arrive before the
banner; treat `MPG:1` accordingly. `0x8B` toggles MPG mode when supported.

## 10. Streaming and buffer accounting

- The controller reports the serial RX buffer size in `$I` `OPT` (second value
  after the option string). The reference board reports 64 bytes; do not assume
  128.
- Character counting is the hard invariant: bytes sent since the last ack
  (including the terminator) must not exceed the RX buffer.
- `Bf` reports planner blocks free and RX bytes free. Use it to top up, but
  never let it override the invariant.
- Real-time bytes are out-of-band and do not count against the buffer.
- One `ok`/`error` acknowledges one streamed line.
- On `error`, stop and clear (empty line or `$` command) before continuing.
- Pause with `!`/`0x82`; resume with `~`/`0x81`; cancel jogs with `0x85`.
- A full report (`0x87`) is useful after connect, after alarm clear, and
  periodically during a job.
- Poll status at 5–20 Hz depending on activity; use a watchdog to pause a job if
  reports stop.

## 11. Capability matrix

| Capability | Protocol layer | v1.0 UI | Notes |
|---|---|---|---|
| Serial connection, status, realtime commands | Full | Yes | Core |
| `$I+` discovery and capability gating | Full | Yes | Drives the UI |
| Settings via `$ES`/`$EG`/`$SED` | Full | Yes | Generated forms |
| Alarm/error via `$EA`/`$EE` | Full | Yes | Fallback tables |
| Offsets, tools, macros, startup lines | Full | Yes | Manual tool change |
| Probing and height map | Full | Yes | M6 |
| SD card mount/stream status | Parse | Post-v1.0 | Reference board has no SD |
| ATC | Parse capability | Post-v1.0 | Reference board has `TC` only |
| MPG takeover | Parse and pause | Post-v1.0 | Reference board has no MPG |
| Multi-spindle | Parse | Post-v1.0 | Reference board has one VFD |
| Extra axes | Parse, 9-axis domain | Display if reported | UI focus is XYZ |
| Lathe mode | Parse `D`, `LATHE` | No | Explicitly out of scope |
| Plasma, drag knife | — | No | Explicitly out of scope |
| Network transports | — | No | Serial only |

## 12. Reference-board deviations

The FlexiHAL build (`1.1f.20240604`) advertises `ENUMS`, `RT+`, `HOME`, `ES`,
`EXPR`, `TC`, `SED`, three axes, 64-byte RX, and one probe. It does not
advertise `SD`, `ATC`, `MPG`, `PC`, or `PROBES=n`. Therefore:

- Use `$ES`/`$EG` for settings; the board supports enumerations.
- Use `$EA`/`$EE` for codes; do not rely only on bundled tables.
- Tool changes are manual; no controller tool table.
- Probing has no probe-connected signal; use probe status and explicit user
  confirmation.
- No SD/ATC/MPG UI, but the protocol parser still accepts those fields.
- Verify whether X/Y/Z limit inputs exist with `$LIM`, `$PINS`, and settings;
  the `SIGNALS:HSDEP` string does not list them.
