# ADR 0001: Sans-IO protocol core

**Status:** accepted
**Date:** 2026-11-01

## Context

A CNC sender must parse a continuous stream of status reports, acknowledgements,
errors, messages, and probe results while also feeding a character-counted
stream of G-code lines to a small receive buffer. Bugs here can overrun the
controller, lose position, or stop a job at the wrong time. This logic is
difficult to test when it is entangled with a serial port, timers, and UI
threads.

## Decision

`ghs-proto` is a pure, sans-IO crate:

- It consumes byte slices and produces typed events.
- It exposes a streamer state machine that consumes acknowledgements and emits
  lines, but performs no I/O and owns no clock.
- It knows nothing about serial ports, GPUI, the simulator, or file systems.

`ghs-engine` owns the threads, timers, and transport, and drives the protocol
state machine.

## Consequences

- Deterministic unit, golden, and property tests with no hardware.
- The simulator can reuse the same types and encoders.
- The protocol can be fuzzed without a port.
- Slightly more plumbing in the engine, which is acceptable.
- All timing policy (poll cadence, watchdog) lives in `ghs-engine`, not in the
  parser.
