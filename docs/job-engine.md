# M3 job engine

The M2 engine streams one line at a time and can send MDI/system commands. M3
adds a job runner around the same streamer; it does not create a second
protocol path.

## State

```text
Disconnected
  └─ connect ─▶ Ready
Ready ─ load(program) ─▶ Loaded
Loaded ─ start ─▶ Running
Running ⇄ Paused
Running ─ M6/T ─▶ ToolChange ─ ack/cycle start ─▶ Running
Running ─ M0/M1/breakpoint ─▶ Paused (operator)
Running ─ end ─▶ Completed
Running/Paused/Loaded ─ stop ─▶ Ready (program retained)
Running/Paused ─ error/alarm/disconnect ─▶ Failed/Interrupted
```

The job state is separate from the controller state. A controller `Alarm` or a
lost link always stops the job; nothing auto-resumes.

## API

Additive to `ghs-engine`:

```rust
Command::LoadProgram { name: String, source: String }
Command::StartProgram { from_line: Option<u32> }
Command::PauseProgram
Command::ResumeProgram
Command::StopProgram { reset: bool }
Command::AcknowledgeToolChange
Command::SetOptionalStop(bool)
Command::SetSingleBlock(bool)
Command::SetBlockDelete(bool)
Command::SetBreakpoint { line: u32, enabled: bool }
Command::StartCheckRun
```

Events:

```rust
EngineEvent::JobLoaded { total_lines, toolpath: Arc<ghs_viewer::Toolpath> }
EngineEvent::JobStarted { from_line, total_lines }
EngineEvent::JobProgress { current_line, completed, total, elapsed, eta }
EngineEvent::JobPaused { line, reason }
EngineEvent::JobResumed
EngineEvent::JobCompleted { elapsed }
EngineEvent::JobAborted { line, reason }
EngineEvent::ToolChangeRequired { tool, line }
EngineEvent::JobError { line, code, message }
```

The engine owns parsing and validation via `ghs-gcode`. The app supplies source
text and displays the returned line list/toolpath; it does not stream lines.

## Streaming

- Use `ghs_proto::Streamer` with the controller's reported RX buffer.
- Feed one parsed `ProgramLine` at a time. Preserve source text exactly,
  minus line terminator; do not rewrite coordinates.
- Skip blank/comment-only lines by default; they need no controller buffer.
- Track `line_number` and byte offsets. The controller's `Ln` is a cross-check,
  not the source of truth.
- On `ok`, advance. On `error:<n>`, stop, emit `JobError`, and clear the sticky
  error with a blank line or `$` command before returning to `Paused`.
- Pause sends feed hold (`0x82`/`!`) and freezes output. Resume sends cycle
  start (`0x81`/`~`). Stop is a feed hold followed by an explicit reset when
  requested; never silently continue.
- `Bf` can reduce the send budget but cannot exceed the character count.
- Real-time bytes remain out of band and are never counted as job bytes.

## Stop conditions

| Trigger | Behavior |
|---|---|
| `M0` | Pause, show "program stop"; resume on operator action |
| `M1` / optional stop | Pause when the virtual/physical optional-stop flag is set |
| Single block | Pause after each line |
| Breakpoint | Pause before the marked line |
| `M6` / `T` tool change | Enter `ToolChange`, send `0xA3`, allow jog/$ commands, resume with cycle start |
| `M2`/`M30` | Complete, reset job state |
| Controller `error` | Stop, emit, clear |
| `ALARM`/E-stop | Stop, emit, no resume |
| Lost status/link | Interrupt, emit, no resume |

## Check mode

`Command::StartCheckRun` runs the loaded program through the controller's `$C`
dry-run mode. The engine:

1. Requires a ready link, a loaded program, and `Idle` (or a controller already
   in `Check`).
2. Sends `$C` and waits for a fresh status reporting `Check` before streaming.
3. Streams the program with `pump_job` allowed in `Check`; the controller
   acknowledges lines without motion.
4. On completion or stop, exits `Check` with `$C` when the engine entered it;
   it never leaves a controller it found already in `Check` in that mode.
5. Refuses a second transition, caps exit retries, and never auto-resumes.

`EngineEvent::CheckStarted`/`CheckCompleted` and
`EngineHandle::check_run()` distinguish a dry run from a real job. The normal
`StartProgram` path is unchanged.

## Start-from-line

A plain line skip is not enough: the modal parser state (units, plane, WCS,
tool length, feed) must match. Use `ghs-gcode`'s parser state at the requested
line, then send the minimal preamble (`G21/G20`, `G90/G91`, `G17/G18/G19`,
WCS, `G43.1`, `F`, `S`, `T`) before the first real block. Mark the run as
"from line" in the UI. If the parser cannot provide a safe state, refuse and
explain.

## Progress

- `completed` is lines acknowledged by `ok`, not lines sent.
- `current_line` is the parsed program line number.
- `elapsed` starts at the first sent line and stops on pause/stop.
- `eta` uses the remaining line count and a rolling lines/second estimate;
  label it as an estimate, not a controller prediction.
- SD streaming (post-v1.0) has its own progress source and must not pretend to
  be local streaming.

## App integration

- Program panel: `ProgramLine` list, current line, breakpoints, search.
- Viewer: `JobLoaded.toolpath` feeds the existing `ghs-viewer` canvas; the
  live marker uses snapshot `wpos`.
- Job controls: Start/Pause/Resume/Stop, start-from-line, optional stop,
  single block, block delete.
- Tool change sheet: requested tool, jog controls, set offset, ack/cycle start.
- Console: `JobError`, `JobPaused`, `ToolChangeRequired` are events, not fake
  console strings.

## Acceptance

- 100k-line program streams with correct character counting and no buffer
  overrun on the simulator.
- Pause/resume/stop at random points; no line is duplicated or skipped.
- Error stops the job and requires explicit recovery.
- Tool change pauses, acknowledges with `0xA3`, allows jog, resumes on cycle
  start.
- Start-from-line produces the same modal state as a full run from that point.
- Disconnect/alarm never resumes.
- Simulator-backed integration test plus a hardware checklist run.
