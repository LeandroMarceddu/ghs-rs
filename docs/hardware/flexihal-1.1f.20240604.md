# Reference controller: FlexiHAL / STM32F446

![Controller `$I` output](controller-info.jpg)

Recorded 2026-10-03 from ioSender Touch's console. Raw transcription of the
visible `$I+` output:

```text
[VER:1.1f.20240604:]
[OPT:VNMSL+2,64,1024,3,0]
[AXS:3:XYZ]
[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]
[FIRMWARE:grblHAL]
[SIGNALS:HSDEP]
[NVS STORAGE:*EEPROM]
[FREE MEMORY:80K]
[DRIVER:STM32F446]
[DRIVER VERSION:240428]
[BOARD:FlexiHAL_20240604_210624]
[AUX IO:4,4,0,0]
[PLUGIN:Bootloader Entry v0.02]
[PLUGIN:MODBUS v0.16]
[PLUGIN:HUANYANG VFD v0.11]
[PLUGIN:HUANYANG P2A VFD v0.10]
[PLUGIN:Durapulse VFD GS20 v0.05]
[PLUGIN:Yalang VFD YL620A v0.02]
[PLUGIN:MODVFD v0.03]
[PLUGIN:H-100 VFD v0.03]
[PLUGIN:Nowforever VFD v0.01]
[PLUGIN:KEYPAD v1.4 Jog2K]
[PLUGIN:Macro plugin v0.02]
[PLUGIN:Probe Protection v0.01]
[PLUGIN:Persistent tool v0.02]
[PLUGIN:RGB Indicator Lights v1.0]
[SPINDLE:Huanyang v1]
```

The preceding lines in the photo were `$#` offsets (G54–G59.3, G28, G30, G92,
HOME, TLO, PRB) and show, for example, `G59.3:-1031.000,-847.000,-2.000` and
`G28:-1043.330,-801.000,-2.000`.

## Interpretation

| Field | Value | Meaning for ghs-rs |
|---|---|---|
| Firmware | `1.1f.20240604` | The only firmware version that must work at v1.0 |
| Build options | `VNMSL+2` | Variable spindle, line numbers, mist, spindle, limits; `+2` variant |
| Serial RX buffer | `64` bytes (second `OPT` field) | Streamer must use 64, not the grbl default of 128 |
| Planner blocks | `1024` | Deep pipeline; character counting remains the hard limit |
| Axes | `3` (`XYZ`) | 3-axis UI; domain stays 9-axis capable |
| Tool table | `0` controller entries | Tool table is app-side; push offsets with `G10`/`$` as needed |
| `NEWOPT` | `ENUMS`, `RT+`, `HOME`, `ES`, `EXPR`, `TC`, `SED` | Enumerations, top-bit realtime, homing, E-stop, expressions, manual M6, settings descriptions |
| Signals | `HSDEP` | Feed hold, cycle start, door, E-stop, probe |
| NVS | `*EEPROM` | Avoid unnecessary setting writes; back up before changes |
| Free memory | `80K` | Responses are bounded; do not assume large controller-side storage |
| Driver/board | STM32F446 / FlexiHAL | USB serial; no network stack |
| AUX IO | 4 in, 4 out, 0 analog | Signals/AUX panel; capability-gated |
| Spindle | Huanyang VFD v1 | Modbus VFD; RPM, direction, spindle override |
| Keypad | KEYPAD v1.4 Jog2K | Hardware keypad/jog; not an MPG; post-v1.0 integration |
| Plugins | MODBUS, VFDs, Macro, Probe Protection, Persistent tool, RGB lights | Capability flags; probe protection matters for probing safety |

## Notable absences

The controller does **not** report:

- `SD`, `YM` — no SD card or YModem.
- `ATC` — manual tool change only (`TC`).
- `MPG` — no MPG input stream takeover.
- `LATHE`, `SS` — no lathe mode or spindle sync.
- `WIFI`, `ETH`, `BT` — serial only.
- `FTP`, `WebDAV` — no controller file transfer.
- `PC` — no probe-connected signal; the probe verify flow must use probe status
  or a user confirmation.
- `PROBES=n` — a single probe input.
- `ODO`, `PID` — no odometer or PID log.

## Open questions to verify on the machine

1. Are X/Y/Z limit switches present? `SIGNALS` lists only `HSDEP`; homing is
   enabled, so the settings page must be checked before assuming hard limits.
2. Does `$pins` (or the Signals panel) expose the limit/probe inputs?
3. Which VFD registers are readable (actual RPM vs commanded RPM)?
4. Does the `Persistent tool` plugin report the current tool after reset?
5. What is the exact `$$` setting list, and do all settings have `$SED`
   descriptions?

These are captured during the first hardware session and added as fixtures.
