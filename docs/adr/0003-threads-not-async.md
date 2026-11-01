# ADR 0003: Threads and channels instead of an async runtime

**Status:** accepted
**Date:** 2026-11-01

## Context

GPUI has its own executor. A CNC control loop needs a small number of fixed
timers (status polling, watchdogs, debounce) and a serial reader. Pulling in
Tokio would add a second runtime, more dependencies, and a larger binary, while
providing little benefit for a 10–20 Hz protocol.

## Decision

- The serial port is owned by a dedicated reader/writer thread using the
  blocking `serialport` crate.
- The engine runs on its own thread and uses one bounded input channel plus
  `recv_timeout` for timers.
- The UI receives an `Arc<MachineSnapshot>` through a latest-value channel and
  events through a bounded queue.
- CPU-heavy parsing and geometry run on worker threads and return `Arc` data.

## Consequences

- Simple, predictable scheduling and shutdown.
- No runtime conflict with GPUI.
- Careful channel design is required to avoid backpressure stalls; the engine
  never blocks on the UI, and the UI only reads the latest snapshot.
- Hard real-time guarantees are not provided or needed; the controller handles
  motion timing.
