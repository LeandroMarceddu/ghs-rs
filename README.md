# ghs-rs

A cross-platform G-code sender for **grblHAL**, written in Rust with
[GPUI](https://github.com/zed-industries/zed) and
[GPUI Kit](https://github.com/longbridge/gpui-kit).

The goal is a touch-first CNC sender with the ergonomics of
[ioSender Touch](https://github.com/Jay-Tech/ioSenderTouch) and the milling
feature set of [ioSender](https://github.com/terjeio/ioSender) — on Windows,
macOS, and Linux.

> **Status: pre-alpha, M6.** Protocol, simulator, serial, job streaming, the
> real 2D viewer, generated settings, live offsets, the tool table, profiles,
> backups, the probe workspace, and the height-map measurement and explicit
> Apply/Revert workflow are implemented. The reference FlexiHAL is not yet
> available for hardware validation, so all evidence so far is simulator and
> unit-test based. See [docs/PLAN.md](docs/PLAN.md).

## Scope

- grblHAL over a **serial port only** (USB/UART).
- 3-axis milling; the domain layer is built for up to 9 axes.
- Touch-first layout plus an optional dockable desktop layout.
- Primary reference hardware: FlexiHAL / STM32F446 running grblHAL
  `1.1f.20240604`, Huanyang VFD spindle, probe and E-stop.
- Explicitly out of scope: legacy Grbl, lathe, plasma THC, drag knife, network
  transports, FTP/WebDAV.

See [docs/parity.md](docs/parity.md) for the feature checklist and
[docs/architecture.md](docs/architecture.md) for the design.

## What works today

- **Connection and protocol** — serial port picker, in-process FlexiHAL
  simulator, capability-driven `$I+` handshake, live status, realtime commands,
  alarm/error tables, parser state, and runtime enumeration.
- **Manual control** — live DRO, hold-to-jog, MDI, feed/spindle overrides,
  spindle and coolant controls, reset/stop, and a console.
- **Jobs** — streaming, pause/resume/stop, tool-change sheet, progress and
  current line, a real parsed program list, and job recovery that never
  auto-resumes after an alarm or disconnect.
- **Viewer** — parsed toolpath, fit/pan/zoom/pinch, world grid and work
  envelope, motion-style paths, and a live tool marker.
- **Settings** — generated from `$ES`/`$EG` metadata, grouped and searchable,
  with boolean, bitfield, axis-mask, radio, numeric, text and password controls,
  range validation, and read-back-verified writes.
- **Offsets and tools** — live `$#` values for G54–G59.3, G28/G30, G92, HOME,
  TLO, TLR, G51 and PRB; `G10 L20` zero/edit with `$#` refresh; a persisted
  app-side tool table used by the M6 tool-change sheet.
- **Profiles and backups** — identity-keyed controller profiles, app config,
  atomic saves, rolling backups, export/validate/import, and identity-mismatch
  confirmation.
- **Probing** — Z touch plate, edge X/Y, corner, and center-circle cycles, plus
  rotation and tool-length reporting. Every run has a mandatory preflight
  (link, Idle, spindle/coolant off, probe signal or explicit confirmation, and
  loaded offsets), ordered `PRB` binding, and a confirmed `G10 L2` apply.
- **Height map** — bounded grid probing with a point/time estimate, preflight,
  partial-map invalidation, 2D overlay, and CSV export. Explicit Apply rewrites
  the loaded program with bilinear compensation, keeps the original for Revert,
  and labels the compensated job.
- **Touch shell** — approved C1/C2/C3 layout, 48 px primary controls,
  dark/light themes, and a resizable desktop layout from 1024×600 up.

## In progress

- Macros, startup lines, keymap editing, goto/restore, and the typed controller
  tool table.
- Start-from-line/check mode, 3D view, i18n, packaging, and the post-v1.0
  SD/ATC/MPG surfaces.
- Hardware validation on the reference FlexiHAL.

## Build

Prerequisites: Rust 1.98.1 (installed automatically by `rust-toolchain.toml`),
a C toolchain, and on Linux the GPUI system dependencies.

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

Linux users must be in the `dialout` group to open serial ports:

```sh
sudo usermod -aG dialout "$USER"
```

The default `cargo run -p ghs-app` starts against the in-process FlexiHAL
simulator. Use `cargo run -p ghs-app -- --mock` for the static UI preview and
open the connection panel to select a real serial port.

## Documentation

- [docs/PLAN.md](docs/PLAN.md) — milestones and release gates
- [docs/architecture.md](docs/architecture.md) — crates, threading, safety
- [docs/protocol.md](docs/protocol.md) — grblHAL protocol surface
- [docs/parity.md](docs/parity.md) — ioSender feature checklist
- [docs/settings.md](docs/settings.md) — settings, offsets, tools, backups
- [docs/probing.md](docs/probing.md) — probing and height-map contract
- [docs/app-engine.md](docs/app-engine.md) — app/engine boundary
- [docs/SAFETY.md](docs/SAFETY.md) — non-negotiable motion rules

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

ioSender is BSD-3-Clause and is used only as a behavioral reference; see
[NOTICE](NOTICE).

## Safety

This software controls machinery. A G-code sender can command rapid motion,
start a spindle, and override limits. Always verify a new build with the
simulator first, keep the physical E-stop within reach, and read
[docs/SAFETY.md](docs/SAFETY.md) before connecting a machine.
