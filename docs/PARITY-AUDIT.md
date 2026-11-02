# v1.0 milling parity audit

**Audit date:** 2026-11-02  
**Code basis:** `b82c9a4`, clean worktree  
**Scope:** all 130 numbered rows in [`docs/parity.md`](parity.md), with the
P1/P2 release gates from [`docs/PLAN.md`](PLAN.md) as the bar.

> **This is a code and test audit, not hardware validation.** The FlexiHAL /
> STM32F446 board has not been available. Per the definition already in
> `parity.md`, every row still lacks the hardware sign-off, mouse-free use and
> localization checks. In this document **done** means "implemented and
> exercised by code/tests against the simulator", not "released as done".

## Method and status rules

- `done` — the row's working path exists in product code and has automated
  coverage or a simulator scenario. No material part of the row is missing.
- `partial` — a working path exists, but a named part of the row is missing,
  deferred, or unverified.
- `missing` — no implementation found.
- The status column in `parity.md` was **not** used as evidence. It is stale in
  both directions: for example COM-01/02/05/07/09 are implemented but marked
  `todo`; VIEW-06 is marked `done` but only colours by motion type; PROBE-01 is
  marked `done` with "retries" in the note, but no retry exists.
- Test evidence was collected with `cargo test --workspace`: **987 tests and
  13 doctests passed, 0 failed** in 33 test binaries.

## Summary

| Area | Rows | Done | Partial | Missing | V1.0 blocker rows |
|---|---:|---:|---:|---:|---:|
| Communication and protocol | 30 | 24 | 4 | 2 | 2 |
| Machine control | 19 | 9 | 9 | 1 | 2 |
| Jobs and program management | 14 | 6 | 5 | 3 | 5 |
| Viewer | 10 | 5 | 4 | 1 | 2 |
| Settings, offsets, tools | 7 | 6 | 0 | 1 | 1 |
| Profiles and backups | 4 | 1 | 2 | 1 | 1 |
| Probing and height map | 16 | 10 | 5 | 1 | 6 |
| Utilities and wizards | 7 | 1 | 1 | 5 | 6 |
| Touch UX and platform | 13 | 4 | 3 | 6 | 5 |
| Release and quality | 10 | 6 | 2 | 2 | 3 |
| **Total** | **130** | **72** | **35** | **23** | **33** |

The blocker count is larger than "missing" because several partially
implemented rows are still release gates. The 33 blocker rows are grouped in
[V1.0 blockers](#v10-blockers).

## Communication and protocol

Key evidence: [`ghs-proto/src/status.rs`](../crates/ghs-proto/src/status.rs),
[`ghs-proto/src/command.rs`](../crates/ghs-proto/src/command.rs),
[`ghs-proto/src/response.rs`](../crates/ghs-proto/src/response.rs),
[`ghs-engine/src/runtime.rs`](../crates/ghs-engine/src/runtime.rs), and
[`ghs-proto/tests/flexihal_fixture.rs`](../crates/ghs-proto/tests/flexihal_fixture.rs).

| ID | Capability | Status | Evidence |
|---|---|---|---|
| COM-01 | Serial port enumeration and selection on Linux, macOS, Windows | done | `ghs-serial/src/ports.rs` `list_ports` and `PortWatcher`; `ghs-app/src/connection.rs`; watcher tests in `ports.rs`. No real-port CI test. |
| COM-02 | Connect/disconnect with baud rate, 8N1 | done | `ghs-serial/src/config.rs` 8N1 defaults and validation; `serial.rs` open path; `engine.rs::disconnect_then_reconnect_works_on_the_same_engine`. |
| COM-03 | Optional DTR reset for 8-bit boards | partial | `serial.rs::set_dtr` and `pulse_dtr` exist and are bounded, but the app never exposes or pulses DTR; `docs/USER-GUIDE.md` says DTR reset is not implemented. Reference board does not need it. |
| COM-04 | Banner handshake and soft reset | done | `runtime.rs` listen, `0x87` probe, printable `?` fallback; `soft_reset`; tests `handshake_discovers_capabilities_without_a_banner`, `banner_is_reported_but_not_required`, `overrides_home_unlock_sleep_and_reset`. |
| COM-05 | `$I` and `$I+` capability parsing | done | `ghs-core/src/info.rs::parse_controller_info`; `flexihal_fixture.rs::parses_the_recorded_reference_capabilities`. |
| COM-06 | `$ES`/`$EG` settings enumeration and `$SED` descriptions | done | `runtime.rs` `DiscoveryStep`; `ghs-proto/src/params.rs`; `ghs-app/src/settings.rs`; tests in `params.rs` and `settings.rs`. |
| COM-07 | `$#` offsets and `$G` parser state | done | `ghs-proto/src/offsets.rs`, `parser_state.rs`; automatic discovery in `runtime.rs`; round-trip fixture test. |
| COM-08 | Adaptive status polling and full report `0x87` | done | `runtime.rs::poll_interval` (100 ms active, 500 ms idle) and `next_full_at`; tests `motion_switches_polling_to_the_active_rate`, `full_status_reports_are_requested_periodically`. |
| COM-09 | All grblHAL status fields | done | `status.rs` parses MPos, WPos, WCO, DTG, Bf, Ln, FS, Pn, WCS, Ov, A, H, D, Sc, TLR, In, SD, MPG, FW, `$C`, SP; test `full_status_report_contains_every_documented_field`. |
| COM-10 | All realtime commands | done | `command.rs::RealtimeCommand` covers reset, hold, resume, jog cancel, door, overrides, spindle stop, coolant, optional stop, single block, tool change, probe toggle, macros; safety filter in `runtime.rs`. |
| COM-11 | `RT+` top-bit realtime negotiation | done | `runtime.rs` sets `rt_plus` from capabilities; tests `rt_plus_uses_the_extended_status_code`, `rt_minus_uses_the_printable_status_code`. |
| COM-12 | Messages, errors, alarms with descriptions | done | `response.rs`; `ghs-core/src/codes.rs` fallback descriptions; `ghs-proto/src/params.rs::CodeTable`; tests for `$EA`/`$EE` and `[MSG:...]`. |
| COM-13 | Probe result (`PRB`) parsing | done | `response.rs` and `offsets.rs::parse_probe_payload`; `engine.rs::probe_results_update_the_snapshot_and_offsets`. |
| COM-14 | Disable controller auto-reporting on connect (`$481`) | missing | No `$481` write or auto-report setup in `ghs-engine` or `ghs-app`; the engine polls with `?`/`0x87`. Low risk on the reference build, but the row is unimplemented. |
| COM-15 | Hotplug, reconnect, and status watchdog | done | `PortWatcher` detects port changes; manual disconnect/reconnect test; `watchdog_pauses_motion_then_disconnects_without_resuming`. Reconnect is manual, not automatic. |
| COM-16 | Session recording and replay | missing | No recorder, replay, or session file writer. The console is bounded in-memory only (`ghs-app/src/state.rs`). |
| COM-17 | Check mode (`$C`) | partial | `SystemCommand::CheckMode` encodes `$C`; simulator test `check_mode_reports_without_moving`; `Command::StartCheckRun` returns `EngineError::Unsupported`. No end-to-end job path. |
| COM-18 | In-process loopback simulator for demo and tests | done | `ghs-demo/src/lib.rs`, `ghs-serial/src/loopback.rs`; demo and loopback tests. |
| COM-19 | Runtime alarm/error code sets (`$EA`, `$EE`) | done | Discovery steps and `CodeTable`; tests in `ghs-proto/src/params.rs`. |
| COM-20 | Runtime settings and groups (`$ES`, `$EG`) | done | Parsed into `SettingDescription`/`SettingGroup`; generated settings UI; tests. |
| COM-21 | Parser state report (`$G`, `0x83`) | done | `parser_state.rs`; discovery step; realtime `ParserStateReport`. |
| COM-22 | Pins, limit state, last events (`$PINS`, `$PINSTATE`, `$LIM`, `$LEV`) | partial | `ExtendedCommand` encodes all four, but `Response` has no typed pin-state, limit or event variants; `[PIN:...]` becomes `Feedback`. Only `Pn` and `$I+` signals reach the UI. |
| COM-23 | Spindle and port enumeration (`$SPINDLES`, `$SPINDLESH`, `$PORTS`) | partial | Commands are encoded in `command.rs`; responses are not typed, and the simulator's `[SPINDLESH:...]`/`[PORTS:...]` remain raw. Multi-spindle UI is post-v1.0. |
| COM-24 | Manual tool change protocol (`Tool` state, `0xA3`) | done | `MachineState::Tool`, `RealtimeCommand::ToolChangeAck`, job runner handshake; tests `completes_a_tool_change_on_the_simulator`, `tool_change_waits_for_ack_and_cycle_start`. |
| COM-25 | MPG takeover (`MPG:1` / `MPG:0`) | done | `status.rs` parses `MPG`; `runtime.rs` `mpg_active` and `poll_pendant`; test `mpg_takeover_waits_for_release_before_discovery`. |
| COM-26 | SD mount and streaming status (`SD:`) | done | `status.rs::SdStatus` parses mount, progress, pending and raw forms; simulator test covers `SD`. UI is post-v1.0. |
| COM-27 | Startup sequence without banner or DTR reset | done | 450 ms listen, `0x87`, fallback `?`, MPG wait; tests `handshake_discovers_capabilities_without_a_banner`, `mpg_takeover_waits_for_release_before_discovery`. |
| COM-28 | Error persistence handling | done | `ghs-proto/src/error_state.rs`; `runtime.rs` `error_latched`, `ClearError`, blank-line retry; tests `sticky_errors_require_a_blank_line_or_system_command`, `mdi_error_stops_and_clears_before_the_next_line`. |
| COM-29 | Multi-spindle status fields (`SP<n>`) | done | `status.rs` keeps up to four `SP<n>` values in `spindle_data`; simulator `generic_full` test; no panel (post-v1.0). |
| COM-30 | Extra-axis status and capabilities | done | `Position` supports 9 axes (`ghs-core/src/position.rs`, `axis.rs`); `$I+` axes parsed; DRO and jog lists iterate the reported axes. |

## Machine control

Key evidence: [`ghs-app/src/shell.rs`](../crates/ghs-app/src/shell.rs),
[`ghs-app/src/state.rs`](../crates/ghs-app/src/state.rs),
[`ghs-app/src/offsets.rs`](../crates/ghs-app/src/offsets.rs), and
[`ghs-engine/src/command.rs`](../crates/ghs-engine/src/command.rs).

| ID | Capability | Status | Evidence |
|---|---|---|---|
| CTL-01 | DRO for all axes and work coordinate systems | done | `shell.rs::dro_card` iterates `state.axes()`; `state.rs` holds mpos/dro/wco/dtg; WCS menu G54-G59.3. Capability-driven, not hardcoded to XYZ. |
| CTL-02 | Zero axes and set position | done | Per-axis zero button and keypad send `G10 L20`; `offsets.rs` model and tests `offsets_export.rs`/`state.rs`. |
| CTL-03 | Goto and park | partial | Confirmed `GOTO G28`/`GOTO G30` and `CLEAR G92` exist in `offsets.rs` and `shell.rs`; no configurable park position or Park action. `docs/USER-GUIDE.md` documents the gap. |
| CTL-04 | Step and continuous jog, configurable distances/feeds, keyboard jog | partial | `shell.rs::jog_card` has 0.01/0.1/1/10/100 mm, hold-to-repeat at 120 ms, and engine jog commands. No keyboard jog handler exists. Jog feed follows the global feed value; no dedicated jog-feed control. |
| CTL-05 | Jog cancel and pendant-safe behavior | done | `shell.rs::end_jog` cancels after a real hold; engine sends `0x85`; tests `jog_cancel_and_motion_gating_in_alarm`, simulator `jog_moves_over_ticks_and_cancels`; `ensure_not_pendant` gates commands. |
| CTL-06 | Feed override, fine and coarse, reset | partial | Slider 10-200% and ±10 buttons in `feed_card`; no reset-to-100 or ±1 fine controls, although the protocol supports both (`RealtimeCommand::FeedOverrideReset`, `FinePlus/Minus`). |
| CTL-07 | Rapid override | done | `rapid_override_chip` cycles 100/50/25; `state.rs::next_rapid_override`; engine validates 100/50/25/5; tests `rapid_override_step_is_the_exact_command`, `rapid_override_snapshot_is_the_only_value_source`, `rapid_override_is_refused_when_mock_or_not_ready`. |
| CTL-08 | Spindle control and override | done | `spindle_card` on/off, RPM keypad, CW/CCW, override slider; engine `SetSpindleOverride`; idle-gated. |
| CTL-09 | Coolant mist, flood, fan | done | `state.rs::coolant_channels` and `shell.rs` buttons are capability-gated; engine realtime flood/mist/fan commands. |
| CTL-10 | MDI with history | partial | One-line `mdi_card` and guarded `Command::Mdi`; `AppState` stores only the current string, no history list or recall. |
| CTL-11 | Home, unlock, reset, sleep | partial | Home and Unlock quick actions, top-bar reset, engine `Command::Sleep`; no sleep control in the UI. |
| CTL-12 | Signals, pins, and AUX IO display | partial | `SIGNALS` card renders `Pn` state and `$I+` signal capabilities. No `$PINS`/`$PINSTATE`/`$LIM`/`$LEV` rows and no AUX IO panel, although `AuxIoCounts` is parsed. |
| CTL-13 | Macros with parameters and special words | partial | `ghs-app/src/macros.rs` provides single-line MDI macros, name/command validation, explicit RUN and confirmation; no parameters, special words, multi-line bodies or `$` commands. |
| CTL-14 | Startup lines | done | `ghs-app/src/startup.rs` read/edit/confirm/read-back-verify `$N0`/`$N1`; engine tests `startup_line_write_is_idle_gated_and_not_optimistic` and `reads_writes_and_verifies_startup_lines_on_the_simulator`. |
| CTL-15 | Keymap editor | missing | No keymap editor, no key binding UI, no runtime key handling. `ghs-config` only defines a `Keymap` record (see SETUP-08). |
| CTL-16 | Work offsets editor, G54-G59.3, G10 | done | `ghs-app/src/offsets.rs` live `$#`, per-value keypad edits, `G10 L20`, zero buttons, `G28`/`G30` goto, `G92.1`; tests in `offsets.rs`, `offsets_export.rs`. |
| CTL-17 | App-side tool table and manual M6 workflow | done | `ghs-app/src/tools.rs` persisted table; M6 sheet uses it; controller table is separate and read-only; config tests. |
| CTL-18 | Metric and imperial units | partial | Display switches from the controller's parsed `G20`/`G21` state (`live.rs` calls `set_units`); G20/G21 chip is display-only. No operator unit switch, and jog steps, probe fields and tool fields remain labelled mm. |
| CTL-19 | Themes and DPI scaling | partial | `theme.rs` has dark and light palettes and relies on OS DPI; no high-contrast theme. |

## Jobs and program management

Key evidence: [`ghs-engine/src/job.rs`](../crates/ghs-engine/src/job.rs),
[`ghs-engine/src/runtime.rs`](../crates/ghs-engine/src/runtime.rs),
[`ghs-app/src/file_browser.rs`](../crates/ghs-app/src/file_browser.rs), and
[`crates/ghs-engine/tests/job.rs`](../crates/ghs-engine/tests/job.rs).

| ID | Capability | Status | Evidence |
|---|---|---|---|
| JOB-01 | Open, drag-and-drop, recent files | partial | `file_browser.rs` provides an in-app one-directory browser, extension filter, 20 MiB / UTF-8 / binary checks and guarded load. No drag-and-drop and no recent-files list. |
| JOB-02 | Validate program before streaming | partial | `JobRunner::load` parses the whole source with `ghs_gcode::parse_program` and rejects syntax/unsupported codes before installing it; test `load_rejects_invalid_programs_and_accepts_valid_ones`. No tool, feed, or bounds validation report. |
| JOB-03 | Character-counting streaming with `Bf` top-up | done | `ghs-proto/src/streamer.rs`; property test `job_streaming_never_exceeds_the_reported_rx_buffer`; `Bf` budget test `bf_rx_free_space_can_only_reduce_the_send_budget`. |
| JOB-04 | Pause, resume, stop | done | Job runner hold/cycle-start/reset paths; tests `pause_and_resume_never_duplicate_or_skip_a_line`, `stop_retains_the_program_and_never_resumes`, `stop_with_reset_sends_a_soft_reset`. |
| JOB-05 | Manual tool change prompt and resume | done | `job.rs` tool-change phase, `0xA3` ack and cycle start; app sheet and tests. |
| JOB-06 | Optional stop, single block, block delete | done | `SetOptionalStop`, `SetSingleBlock`, `SetBlockDelete`; tests `optional_stop_pauses_only_when_enabled`, `single_block_pauses_after_each_acknowledged_line`, `block_delete_skips_marked_lines`; RUN OPTIONS UI. |
| JOB-07 | Start-from-line, run-to-line, breakpoints | partial | Breakpoints exist in the engine (`SetBreakpoint`, `breakpoint_hit`, test `breakpoint_pauses_before_the_marked_line`) but there is no UI. `StartProgram { from_line: Some(n) }` with `n > 1` returns `Unsupported`; no run-to-line implementation. |
| JOB-08 | Progress, elapsed time, ETA | done | `JobView` progress/elapsed/ETA; `estimate_remaining` clamped; state tests; header and program list. |
| JOB-09 | Console with filtering and file log | partial | Bounded 200-line console with discovery-noise filtering (`live.rs::discovery_noise`). No user filter and no persistent file log. |
| JOB-10 | Dry run and check mode | missing | `Command::StartCheckRun` explicitly returns `Unsupported`; no UI dry run. `$C` exists only as a protocol/simulator command (COM-17). |
| JOB-11 | Defined reconnect and interruption policy | done | Alarm, door, disconnect and watchdog tests all mark the job interrupted/aborted and never resume; `EngineEvent::JobAborted` reasons. |
| JOB-12 | Program list with follow, search, context menu | partial | Search/filter/highlight, previous/next and jump are implemented in `state.rs` and the Program tab. No context menu. |
| JOB-13 | Add line numbers, strip comments | missing | No user-facing transform. The only line-number stripping is internal to height-map compensation. |
| JOB-14 | Job statistics and run history | missing | No run-history store or statistics beyond the current job's progress/elapsed data. |

## Viewer

Key evidence: [`ghs-gcode`](../crates/ghs-gcode/src/lib.rs),
[`ghs-viewer`](../crates/ghs-viewer/src/lib.rs), and
[`ghs-app/src/viewer.rs`](../crates/ghs-app/src/viewer.rs).

| ID | Capability | Status | Evidence |
|---|---|---|---|
| VIEW-01 | G-code parser for lines, arcs, canned cycles, planes, units, incremental mode | done | `ghs-gcode` parser/planner; tests `arcs.rs`, `cycles.rs`, `motion.rs`, `program.rs`. Unsupported codes are rejected with structured errors. |
| VIEW-02 | Expressions, parameters, and flow control | missing | Explicitly deferred in `ghs-gcode/src/lib.rs`; `#` parameters, `[...]`, `IF`/`WHILE`, `O` words and `M98`/`M99` return `UnsupportedExpression`/`UnsupportedCode`. Scope decision needed if the reference post-processor emits expressions. |
| VIEW-03 | Parser-state emulation with offsets, scaling, rotation, tool length | done | `ghs-gcode/src/state.rs` and `planner.rs` apply G51, rotation, WCS, G92 and G43.1; tests in `motion.rs` and `compensation.rs`. |
| VIEW-04 | 2D pan, zoom, fit, grid, bounds | done | `camera2d.rs`, `render.rs`, `viewport.rs`; app `viewer.rs` drag/wheel/pinch tests. |
| VIEW-05 | Live tool marker | done | `ghs-app/src/viewer.rs` marker projection in 2D/3D; tests marker mm conversion and placement. |
| VIEW-06 | Colour by motion type, layer, or tool | partial | `path_color` maps only `SegmentKind` to fixed palette colours. Layer/tool colouring and user configuration are absent; the "configurable, colour-blind safe" note is not implemented. |
| VIEW-07 | 3D orbit, pan, zoom, stock, and tool | partial | `camera3d.rs` orbit/pan/zoom, 3D grid, envelope box and tool marker are implemented. No stock mesh; perspective only. |
| VIEW-08 | Height map overlay | done | `ghs-app/src/viewer.rs` projects map cells in 2D and 3D; tests cover valid, invalid and missing cells. |
| VIEW-09 | 1M-line and 200k-segment performance | partial | `ghs-viewer/examples/bench.rs` benchmarks 200k and 1M segments. No CI threshold, so regressions are not gated (REL-10). |
| VIEW-10 | Program limits and outline display | partial | The viewer draws the parsed toolpath bounds/envelope in 2D and 3D. No pre-run limits comparison or operator confirmation before running. |

## Settings, offsets, tools

Key evidence: [`ghs-app/src/settings.rs`](../crates/ghs-app/src/settings.rs),
[`ghs-app/src/offsets.rs`](../crates/ghs-app/src/offsets.rs),
[`ghs-app/src/tools.rs`](../crates/ghs-app/src/tools.rs), and
[`ghs-app/src/startup.rs`](../crates/ghs-app/src/startup.rs).

| ID | Capability | Status | Evidence |
|---|---|---|---|
| SETUP-01 | Settings editor generated from `$ES`/`$EG`/`$SED` | done | `settings.rs` builds rows from reported values and metadata; type inference and a General group are the fallback when metadata is absent. Tests in `settings.rs`. |
| SETUP-02 | Grouped, searchable, typed, validated settings | done | Group rail, search, boolean/bitfield/axis-mask/radio/numeric/text/password controls, range validation; tests `type_mapping_covers_metadata_and_value_fallbacks`, `begin_apply`/`written` tests. |
| SETUP-03 | Setting write, read-back verification, backup and restore | done | Engine `SetSetting` and `SettingWritten` verification; app `QUEUED`/`WRITING`/`APPLIED`/`FAILED` states; no automatic backup before a write (manual backup is documented). |
| SETUP-04 | Stepper and axis calibration | missing | No calibration wizard, measurement capture, or `$100`-style guided calculation. |
| SETUP-09 | Startup line persistence | done | `startup.rs` `$N0`/`$N1` read/edit/confirm/read-back verify; engine idle gate. |
| SETUP-10 | Tool table persistence | done | `ghs-config` `ToolTable`/`ToolRecord`; `ghs-app/src/tools.rs`; atomic profile save and backup. |
| SETUP-11 | Offsets backup and restore | done | `ghs-app/src/offsets_export.rs` writes WCS-only `offsets.nc` with `G21`; validated load-as-program; tests. |

## Profiles and backups

Key evidence: [`ghs-config`](../crates/ghs-config/src/lib.rs) and
[`ghs-app/src/config.rs`](../crates/ghs-app/src/config.rs).

| ID | Capability | Status | Evidence |
|---|---|---|---|
| SETUP-05 | App settings and controller profiles | done | `ProfileKey` from `$I+` identity; `ConfigStore`; app config and profile files; tests `profile.rs`, `store.rs`, `config.rs`. |
| SETUP-06 | Import and export settings, offsets, tools | partial | `BackupBundle` exports/imports app config, identity, settings, offsets and tools in one JSON bundle; offsets also have `offsets.nc`. No standalone settings or tool-table file formats. |
| SETUP-07 | Controller reset (`$RST`) with backup prompt | missing | `SystemCommand::Reset` and `ResetTarget` exist in the protocol/engine, but the app exposes no `$RST` action and no backup prompt before a destructive reset. |
| SETUP-08 | Keymap persistence | partial | `ghs-config` persists a validated `Keymap`/`KeyBinding` record in the profile and backup bundle. No editor, no runtime key bindings, so the persisted record is effectively empty. |

## Probing and height map

Key evidence: [`ghs-probe`](../crates/ghs-probe/src/lib.rs),
[`ghs-app/src/probe.rs`](../crates/ghs-app/src/probe.rs),
[`ghs-app/src/height_compensation.rs`](../crates/ghs-app/src/height_compensation.rs), and
[`ghs-app/src/height_map_library.rs`](../crates/ghs-app/src/height_map_library.rs).

| ID | Capability | Status | Evidence |
|---|---|---|---|
| PROBE-01 | Z touch plate | done | `ProbeCycle::ZTouchPlate`; safe Z/retract/search bounds; app card; tests `z_touch_plate_golden`, `every_cycle_plan_has_the_same_safety_preamble_and_retract`, probe model tests. Note: no automatic retry despite the parity note. |
| PROBE-02 | Tool length probing | partial | `ProbeCycle::ToolLength` reports the signed delta and the UI shows it. Applying TLO (`G43.1`) is explicitly deferred in `USER-GUIDE.md` and the UI. |
| PROBE-03 | Edge finder X/Y, inside and outside | done | `EdgeSpec` with `EdgeMode::Inside/Outside`, approach sign, probe radius and edge offset; app Edge X/Y cards; tests `edge_x_golden`, `edge_y_inside_golden`, `edge_correction_uses_the_approach_direction`, `edge_outcome_computes_edge_and_zero`. |
| PROBE-04 | Corner finder | done | `CornerProbe` X-then-Y plan; app Corner card; tests `corner_golden`, `corner_zero_combines_both_edges_and_offsets`, `corner_outcome_combines_two_edges`. |
| PROBE-05 | Center finder, circle and rectangle | partial | Center-circle plan and UI card are implemented. `ProbeCycle::CenterRectangle` and `rectangle_center_from_*` exist in `ghs-probe`, with `rectangle_inside_golden` / `rectangle_outside_plan_moves_inward_from_outside`, but no rectangle card or result flow exists. |
| PROBE-06 | Rotation and straightness | partial | `ProbeCycle::Rotation` reports a rotation angle (`rotation_golden`); app card marks apply as deferred. No work-rotation apply and no separate straightness measurement. |
| PROBE-07 | XY touch plate offsets | partial | `ProbeCycle::XyTouchPlate` and `xy_plate_zero` exist in `ghs-probe` (`xy_touch_plate_golden`); there is no XY-plate card or app workflow. |
| PROBE-08 | Probe verify and connected flow | done | Preflight requires `PC` or an explicit operator confirmation for a board without `PC`; app preflight rows and tests `preflight_refuses_spindle_coolant_and_unverified_probe`, `preflight_uses_the_pc_signal_when_reported`. |
| PROBE-09 | Pre and post probe macros | missing | No macro hook or configuration around probe plans. |
| PROBE-10 | Height map probing | done | `HeightMapGrid` bounded row-major plan; app editor, estimate, partial-map handling; tests `heightmap.rs`. |
| PROBE-11 | Height map compensation and export | done | `ghs-gcode::apply_height_compensation`; app Apply/Revert; tests `compensation.rs` and app `height_compensation.rs` incl. outside-map refusal and revert. |
| PROBE-12 | Probe-safe interlocks | done | `ProbePreflight` gates link, Idle, spindle/coolant off, safe Z, bounded search/feed, probe state, offsets and fields; app preflight tests. |
| PROBE-13 | Probe feed, step, and retry settings | partial | Feed, search, retract and step are per-run fields and validated; probe mount offset and map Z offset are persisted. No retry count/strategy, and cycle feed/step are not persisted per profile. |
| PROBE-14 | Probe results display and log | done | Run panel binds `PRB` results in plan order and keeps raw machine-coordinate results; tests `completing_a_z_probe_binds_results_in_order`, `a_missed_or_alarmed_run_fails_and_never_resumes`. |
| PROBE-15 | Probe mount XY offset | done | Persisted per profile; plan subtracts the offset and result math adds it back; tests `nonzero_probe_offset_shifts_the_plan_and_binds_feature_coordinates`. |
| PROBE-16 | Height-map Z offset | done | Effective `z`, lookup, summary, overlay, CSV and Apply use the offset while raw samples stay unchanged; tests `map_z_offset_is_effective_in_summary_overlay_and_csv`. |

## Utilities and wizards

| ID | Capability | Status | Evidence |
|---|---|---|---|
| UTIL-01 | Material surfacing wizard | missing | No surfacing generator or wizard. |
| UTIL-02 | Gantry squaring and alignment | missing | No squaring wizard or measurement plan. |
| UTIL-03 | Stepper and XY calibration | missing | No calibration wizard (overlaps SETUP-04). |
| UTIL-04 | Program outline and limits check | partial | The viewer draws the parsed envelope, but there is no limits comparison or pre-run warning; same gap as VIEW-10. |
| UTIL-05 | Add line numbers, strip comments, transform tools | missing | No non-destructive transform UI or save-as path. |
| UTIL-06 | Utility macros and quick actions | done | Right-rail quick actions (Zero XY, Zero Z, Safe Z, Home, Unlock, Probe Z, M8/M9) and user macros in Utility; guarded and tested. |
| UTIL-07 | Machine setup wizard | missing | No first-run machine setup wizard. |

## Touch UX and platform

| ID | Capability | Status | Evidence |
|---|---|---|---|
| UX-01 | Touch layout at 1280x800 | done | Default window is 1280x800 (`ghs-app/src/main.rs`); approved three-column shell in `shell.rs`. |
| UX-02 | Touch layout at 1920x1080 | done | Resizable shell and `--size 1920x1080`; the centre column widens. No automated layout snapshot test. |
| UX-03 | Portrait layout | missing | Window can be resized, but there is no portrait reflow; `USER-GUIDE.md` states a full portrait reflow is not implemented. |
| UX-04 | In-app numeric and text keypad | done | `ghs-app/src/keypad.rs`; used by DRO, RPM, offsets, tools, settings and file path fields. |
| UX-05 | 48 px targets, no hover-only actions | done | Design rule in `theme.rs` and repeated 48 px/64 px controls in `shell.rs`; actions also have click handlers. No automated hit-target audit. |
| UX-06 | Dockable Workbench layout | missing | No dockable/resizable panel mode; only the fixed touch shell. |
| UX-07 | Dark, light, high-contrast themes | partial | `theme.rs` provides dark and light only. No high-contrast palette. |
| UX-08 | Keyboard navigation and accessibility | missing | No AccessKit integration, focus order, keyboard jog, or keyboard navigation beyond text fields. |
| UX-09 | Localization, starting with en, pt-BR, de | missing | No `ghs-i18n` crate and no locale catalogs; UI strings are hardcoded English/ASCII. |
| UX-10 | First-run safety onboarding | missing | No onboarding checklist, first-run gate, or safety walkthrough. |
| UX-11 | Error dialogs and notifications | partial | Connection sheet shows engine errors; job/probe/macro errors surface inline and in the console. No general notification/toast system outside those sheets. |
| UX-12 | Crash-safe configuration and local logs | partial | `ghs-config` uses atomic writes and rolling backups. No persistent local log file; the console is memory-only. |
| UX-13 | Gamepad support (post-v1.0) | missing | No gamepad input handling. Explicitly post-v1.0; not a blocker. |

## Release and quality

Key evidence: [`.github/workflows/ci.yml`](../.github/workflows/ci.yml),
[`.github/workflows/release.yml`](../.github/workflows/release.yml),
[`docs/RELEASE.md`](RELEASE.md), and [`CHANGELOG.md`](../CHANGELOG.md).

| ID | Capability | Status | Evidence |
|---|---|---|---|
| REL-01 | CI on Linux, macOS, Windows | done | CI runs fmt, clippy and the workspace tests on Linux, and a debug app build on all three OSes. Tests are not run on macOS/Windows. |
| REL-02 | Unit, golden, and property tests | done | 987 tests and 13 doctests pass; protocol robustness proptests and streamer property tests are present. |
| REL-03 | Simulator scenario suite | done | `ghs-sim/tests/protocol_behavior.rs` plus engine job tests cover alarm, tool change, probing, pause/resume, disconnect and watchdog. |
| REL-04 | Hardware-in-the-loop checklist | partial | The checklist exists in `docs/SAFETY.md` and `docs/RELEASE.md` section 11, but it has not been run; the board is unavailable. |
| REL-05 | Windows, macOS, Linux packaging | partial | The release workflow builds unsigned `.tar.gz`/`.zip` archives. There are no installers, no macOS `.app` bundle, and no Windows installer. |
| REL-06 | Code signing and notarization | missing | No signing/notarization in the workflow; `RELEASE.md` documents the gap. Needs maintainer certificates and CI secrets. |
| REL-07 | User manual and troubleshooting guide | done | `docs/USER-GUIDE.md` is a full operator guide with troubleshooting; `docs/SAFETY.md` and `docs/RELEASE.md` support it. In-repo Markdown, not a docs site. |
| REL-08 | Versioning and changelog | done | `CHANGELOG.md` exists with Unreleased notes; workspace version is 0.1.0. No release tag yet. `RELEASE.md` still says no changelog exists, which is stale. |
| REL-09 | License and attribution review | done | `LICENSE-APACHE`, `LICENSE-MIT`, `NOTICE`, `deny.toml`; ioSender credited as behavioural reference. |
| REL-10 | Performance benchmarks in CI | missing | `ghs-viewer/examples/bench.rs` is a manual benchmark only; no CI threshold or regression job. |

## Post-v1.0, capability-gated

These are outside the v1.0 blocker list by `AGENTS.md` and `PLAN.md`. Protocol
parsing exists for most; UI is deferred.

| Capability | Code evidence | Audit status |
|---|---|---|
| SD card browsing and YModem upload | `SdStatus`, `ControllerInfo::has_ymodem`, `$SD` parsing | missing UI; post-v1.0 |
| ATC tool-change flows | `ControllerInfo::has_atc` | missing UI; post-v1.0 |
| MPG/pendant UI | `MPG` status parsing and engine pause | missing UI; post-v1.0 (engine pause exists) |
| Multi-spindle panels | `SP<n>` fields in `spindle_data` | missing UI; post-v1.0 |
| Extra-axis DRO/jog rows | capability-driven DRO/jog iteration | partially present, not validated; post-v1.0 |
| Gamepad | none | missing; post-v1.0 |

## V1.0 blockers

This list is the 33 blocker rows only. It excludes documented post-v1.0
surfaces and items that can ship as a stated limitation.

### 1. Hardware sign-off (1 row)

- **REL-04** — the FlexiHAL checklist is written but unrun. This is the global
  release gate: no row can be called `done` under the parity definition until
  it passes. **External/hardware-only.**

### 2. Job correctness and program management (5 rows)

- **JOB-02** — validation before streaming covers syntax only; no tool/feed/
  bounds report.
- **JOB-07** — start-from-line is explicitly rejected for line > 1, run-to-line
  does not exist, and breakpoints have no UI.
- **JOB-09** — no user console filter or persistent file log.
- **JOB-10** — `StartCheckRun` is `Unsupported`; no dry run.
- **JOB-13** — no add-line-numbers/strip-comments transform.

### 3. Localization (1 row)

- **UX-09** — no i18n infrastructure, no `ghs-i18n` crate, no en/pt-BR/de
  catalogs. The P2 gate explicitly includes localization.

### 4. Keymap and accessibility (3 rows)

- **CTL-15** — no keymap editor.
- **SETUP-08** — the keymap record is persisted but unused and has no editor.
- **UX-08** — no keyboard navigation or accessibility layer. This is the
  "usable without a mouse" gate and also covers keyboard jog.

### 5. Packaging and signing (2 rows)

- **REL-05** — archives exist, but no installers, no macOS `.app` bundle.
- **REL-06** — no signing or notarization. Needs certificates and managed CI
  secrets.

### 6. Full probing suite (6 rows)

- **PROBE-02** — tool-length delta reported, TLO apply deferred.
- **PROBE-05** — circle UI exists; rectangle center has no UI.
- **PROBE-06** — rotation angle reported; work-rotation apply/straightness
  missing.
- **PROBE-07** — XY touch-plate cycle exists in `ghs-probe`, no UI workflow.
- **PROBE-09** — no pre/post probe macros.
- **PROBE-13** — no retry strategy or persisted cycle settings.

### 7. Utilities and calibration (7 rows)

- **SETUP-04** — no stepper/axis calibration.
- **UTIL-01** — no surfacing wizard.
- **UTIL-02** — no gantry squaring.
- **UTIL-03** — no calibration wizard.
- **UTIL-04** — outline only; no limits check.
- **UTIL-05** — no transforms.
- **UTIL-07** — no first-run machine setup wizard.

### 8. Signals and AUX (2 rows)

- **COM-22** — `$PINS`/`$PINSTATE`/`$LIM`/`$LEV` commands are encoded but their
  responses are untyped; only `Pn` reaches the UI.
- **CTL-12** — no pins/limit-event/AUX IO panel.

### 9. Session logging (1 row)

- **COM-16** — no session recording/replay. JOB-09 (console file log) is
  counted once in the job group above.

### 10. Viewer (2 rows)

- **VIEW-07** — no stock mesh.
- **VIEW-10** — no pre-run limits check/confirmation.

### 11. Touch layout (3 rows)

- **UX-03** — no portrait reflow.
- **UX-07** — no high-contrast theme.
- **UX-10** — no first-run safety onboarding.

### Explicitly not v1.0 blockers

- **Post-v1.0 by scope:** SD/YModem UI, ATC flows, MPG/pendant UI,
  multi-spindle panels, extra-axis UI expansion, gamepad (UX-13).
- **Can ship as documented limitation:** COM-03 DTR reset (reference board does
  not need it), COM-14 `$481` (polling works), COM-23 spindle/port enumeration
  (single-spindle reference), CTL-03 park (G28/G30 cover the need), CTL-06 feed
  reset/fine, CTL-10 MDI history, CTL-11 sleep UI, CTL-18 unit switch,
  JOB-01 drag-and-drop/recent files (file open works), JOB-12 context menu,
  JOB-14 run history, SETUP-06 standalone imports (bundle covers it), SETUP-07
  `$RST` prompt, VIEW-02 G-code expressions, VIEW-06 layer/tool colouring,
  VIEW-09 and REL-10 CI performance thresholds, UX-06 Workbench, UX-11 toast
  notifications, UX-12 local log file (covered by COM-16).
- **Scope decision needed:** VIEW-02 expressions and REL-10 performance gates
  are the two "not a blocker today" items most likely to become blockers if the
  reference post-processor uses expressions or release policy requires a
  performance gate.

## Top next actions for v1.0

1. **Run the FlexiHAL hardware validation** (REL-04). Why: it is the release
   gate and no amount of simulator work replaces it. Size: S once the board is
   reachable; hardware-only, external dependency.
2. **Finish job preflight: start-from-line, run-to-line, modal preamble,
   program validation, and `$C` check mode** (JOB-02, JOB-07, JOB-10, COM-17).
   Why: M3 ships start-from-line, and check mode/validation are the safety
   boundary before motion. Size: L.
3. **Add i18n infrastructure and en/pt-BR/de catalogs** (UX-09). Why: the P2
   gate and the "done" definition require localization; there is no crate yet.
   Size: L.
4. **Implement the keymap editor, runtime bindings, keyboard jog, and
   accessibility** (CTL-15, SETUP-08, UX-08, CTL-04). Why: M5 keymap is still
   open and P2 requires mouse-free operation. Size: M/L.
5. **Produce signed installers and a macOS app bundle** (REL-05, REL-06). Why:
   P2 requires installers on three OSes; current artifacts are unsigned
   archives. Size: M; needs certificates and CI secrets.
6. **Complete the probe suite** (PROBE-02/05/06/07/09/13): rectangle center,
   XY plate UI, TLO/rotation apply, pre/post macros, retry settings. Size: L.
7. **Build the utility/calibration set** (SETUP-04, UTIL-01/02/03/04/05/07):
   surfacing, squaring, calibration, transforms, limits check, first-run setup.
   Size: L.
8. **Parse and display pins, limits, events and AUX IO** (COM-22, CTL-12). Why:
   P2 signals panel; currently only `Pn` and `$I+` capabilities are shown.
   Size: M.
9. **Add session recording and a console file log/filter** (COM-16, JOB-09).
   Why: P2 session logging; currently memory-only. Size: S/M.
10. **Add the 3D stock mesh and pre-run limits confirmation** (VIEW-07,
    VIEW-10). Why: 3D viewer parity and a useful pre-run safety check. Size: M.

## Counts and caveats

- **Rows audited:** 130 numbered rows. The six unnumbered post-v1.0
  capabilities are listed separately and excluded from the counts.
- **Done:** 72 (code/test only).
- **Partial:** 35.
- **Missing:** 23.
- **V1.0 blocker rows:** 33.
- **Hardware-validated rows:** 0. The FlexiHAL board has not been available, so
  no row meets the full `parity.md` definition of done.
- **Tests:** `cargo test --workspace` passed 987 tests and 13 doctests, 0
  failed. This is simulator/unit evidence, not machine evidence.
- **Documentation drift found:** `parity.md`'s status column and layer table are
  stale (implemented rows marked `todo`); `docs/RELEASE.md` section 2 says there
  is no `CHANGELOG.md`, but one exists; README and PLAN describe M5/M6 as
  closeout while keymap, i18n, probe-apply and utility work remain open.
