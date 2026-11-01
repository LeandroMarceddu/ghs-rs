# ghs-rs

A cross-platform G-code sender for **grblHAL**, written in Rust with
[GPUI](https://github.com/zed-industries/zed) and
[GPUI Kit](https://github.com/longbridge/gpui-kit).

The goal is a touch-first CNC sender with the ergonomics of
[ioSender Touch](https://github.com/Jay-Tech/ioSenderTouch) and the milling
feature set of [ioSender](https://github.com/terjeio/ioSender) — on Windows,
macOS, and Linux, with a single native binary.

> **Status: pre-alpha, M2.** The shell drives a live in-process grblHAL
> simulator and has a serial connection panel; job streaming, probing, and
> hardware validation are still in progress. See [docs/PLAN.md](docs/PLAN.md).

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
