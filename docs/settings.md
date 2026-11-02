# M5 settings, offsets, tools, and profiles

M5 turns the engine's already-discovered metadata into editable UI and persists
safe local data. The engine remains the only writer to the controller.

## Settings editor

The engine exposes:

- `EngineHandle::settings() -> Arc<Vec<Setting>>` from `$$`, enriched by
  `$ES`/`$EG` when `ENUMS` is available.
- `EngineEvent::Settings(...)` after discovery.
- `Command::SetSetting { number, value }` for writes; the engine rejects writes
  outside Idle, writes the line, reads it back, and emits
  `SettingWritten { verified }`.
- `SettingDescription`/`SettingGroup` metadata (type, format, min/max, reboot,
  null-allowed, unit).

Generate the form; do not hardcode setting numbers. Map types:

| Type | UI |
|---|---|
| Boolean | switch/checkbox |
| Bitfield | checkbox column |
| Exclusive bitfield | checkbox column; other bits disabled when bit 0 off |
| Radio | segmented/radio column |
| Axis mask | one checkbox per reported axis |
| Integer / Decimal | numeric field + in-app keypad; show unit |
| String / Password / IPv4 | text field; password masked |

- Group by `$EG` hierarchy; search by number/name; show unit, range and
  "reboot required".
- Write is explicit: edit locally, then Apply; show pending, verified, or error
  state. Do not mark a setting applied until `SettingWritten.verified`.
- Batch changes with a backup prompt; never auto-restore.
- `$RST` is destructive; require a typed confirmation and a backup.
- EEPROM on the reference board means avoid unnecessary writes.

## Offsets

- Read `$#` through `EngineHandle::offsets()`; show G54–G59.3, G28/G30, G92,
  G51 scaling, HOME, TLO, TLR when reported.
- Editing uses `G10 L20 P<n> <axis><value>` for work offsets; G92 edits are
  non-persistent and clearly labeled.
- "Set current position as zero" and "Goto origin" go through MDI with
  confirmation; do not move silently.
- Backup offsets as JSON/NC (`offsets.nc` equivalent); restore requires an
  explicit run and read-back.

## Tool table and manual M6

- The reference board has zero controller tool-table entries; the app owns the
  table (number, name, diameter, length) and persists it in the profile.
- Tool change: engine `ToolChangeRequired` → sheet with tool number, jog and
  set-offset actions, acknowledge (`0xA3`), then resume. The sheet already
  exists; M5 adds tool length probing and offset editing.
- If the controller reports `[T:...]` entries, display them read-only plus the
  app table; do not silently overwrite controller data.
- Tool length can be set with `$TLR`/probe (M6) or a controlled MDI move.

## Macros, startup lines, keymap

- Macros are named MDI/system sequences; persist in the profile; confirm risky
  motion macros.
- Startup lines `$N0`/`$N1` are read/written through the engine; never edit
  while a job is active.
- Keymap and app settings are local; JSON/TOML with atomic writes and rolling
  backups.

## Height maps

- A completed map is saved in the controller profile
  (`ControllerData.height_maps`) with its grid, row-major samples,
  WCS/G92/TLO frame, tool number, timestamp, and optional constant Z offset. A
  profile keeps at most 16 maps; loading a profile starts with its own maps only.
- The probe mount offset (`ControllerData.probe_offset`, `probe tip - controlled
  point`) is also per profile and is used for every probe plan/result; it is not
  sent to the controller.
- A saved map opens read-only. Loading never applies compensation or starts a
  job; Apply is a separate, gated action.
- A saved map whose WCS/G92/TLO frame differs from the live machine is flagged
  and refused for Apply. Incomplete maps and programs that leave the mapped
  area are refused rather than extrapolated or run uncompensated.

## Profiles and backup

- Profile keyed by `$I+` identity: firmware, board, driver, serial number when
  available. A reconnect with a different controller must not silently apply
  another machine's offsets/tools.
- Backup bundle: app settings, controller settings snapshot, offsets, tool
  table, macros, keymap, saved height maps, and the probe offset. Versioned
  schema; import validates before writing.
- Atomic writes: temp file → fsync → rename; keep the last N backups.
- No telemetry; exports are user-initiated.

## Acceptance

- The Settings tab is generated from the FlexiHAL simulator's `$ES`/`$EG` and
  writes a representative boolean/numeric/enum setting with read-back
  verification.
- A wrong/out-of-range value is rejected in the UI and by the controller.
- Offsets can be viewed, zeroed, and restored from a backup.
- Tool table edits persist across restart and feed the M6 sheet.
- A profile backup/restore round-trips and refuses mismatched controller
  identity without confirmation.
- No setting write while a job is running.
- Screenshots at 1280×800 and 1920×1080; tests for form generation, validation,
  and backup round-trip.
