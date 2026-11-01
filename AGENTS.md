# Repository instructions for agents

ghs-rs is a cross-platform grblHAL G-code sender in Rust using GPUI and GPUI
Kit. Read these first, in order:

1. `docs/PLAN.md` — scope, milestones, execution model.
2. `docs/architecture.md` — crates, threading, protocol, safety.
3. `docs/hardware/flexihal-1.1f.20240604.md` — the reference controller.
4. `docs/parity.md` — the feature checklist and current status.
5. `docs/SAFETY.md` — non-negotiable motion rules.

## Scope guardrails

- **Serial only.** No Telnet, WebSocket, TCP, HTTP, FTP, or WebDAV transports.
  The in-process loopback is for tests and demo mode only.
- **grblHAL only.** No legacy Grbl compatibility layer.
- **3-axis milling.** No lathe, plasma THC, drag knife, rotary, or 5-axis UI.
- **No SD card, ATC, MPG, or camera work** before v1.0.
- Do not copy ioSender source. Behavioral compatibility and protocol facts only.
- Do not add telemetry or network calls to the app.

## Ownership and workflow

- The orchestrator owns the architecture, public APIs, root `Cargo.toml`, docs,
  CI, integration, and commits.
- A subagent may edit **only** the paths named in its brief. Do not touch other
  crates, root files, or docs unless the brief says so.
- Do not create git commits. Report changed files and test results; the
  orchestrator reviews and commits.
- If a task needs an interface change in another crate, stop and report the
  needed change instead of editing that crate.
- Use `CARGO_TARGET_DIR=/tmp/opencode/ghs-target-<role>` when building or testing
  in parallel to avoid Cargo lock contention.
- Prefer small, reviewable changes over broad refactors.

## Code standards

- Edition 2024, Rust 1.98.1. Format with `cargo fmt`; keep lines ≤ 100 columns.
- Library crates use typed errors with `thiserror`; no `anyhow` in libraries.
- No `unwrap`/`expect`/`panic!` on input data in library code. Tests may panic.
- Validate and bound all controller input; never index by untrusted lengths.
- Use `tracing` for diagnostics in libraries; no `println!` except in binaries
  and examples.
- Public items get doc comments. Units are explicit in names or docs.
- Keep protocol and domain code independent of GPUI, serial ports, and clocks.
- Derive `Debug`, `Clone`, `PartialEq` where useful; use `serde` only at
  persistence and fixture boundaries.
- ASCII-only source and default UI strings until the i18n milestone.

## Safety rules

- The UI and parsers must never send motion directly. Encode commands; let the
  engine's safety gateway decide.
- Motion is refused in `Alarm` and `Door` states without explicit confirmation.
- Probing requires safe Z, bounded search, and probe verification.
- Never auto-resume after an alarm, E-stop, or reconnect.
- A status watchdog must pause a running job if telemetry stops.
- Settings writes are idle-only and read back for verification.

## Testing

- Every parser needs golden tests for valid input and non-panicking tests for
  malformed input.
- Protocol accounting gets property tests: the streamer must never exceed the
  controller's receive buffer.
- Prefer table-driven tests and fixtures from `fixtures/`.
- Simulator scenarios must cover alarm, tool change, probing, pause/resume, and
  disconnect.
- Required commands before reporting:

```sh
cargo fmt --all
cargo clippy -p <crate> --all-targets -- -D warnings
cargo test -p <crate>
```

The orchestrator runs the full workspace gates before committing.

## Reporting format

Return a concise report with:

1. **What changed** — files and public API.
2. **Tests** — commands run and results.
3. **Open questions / risks** — anything the orchestrator must decide.
4. **Follow-ups** — intentionally deferred work.

Do not claim completion without test output. Do not hide failing tests.
