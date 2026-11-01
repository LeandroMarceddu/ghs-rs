# ADR 0002: Serial-only transport with an in-process loopback

**Status:** accepted
**Date:** 2026-11-01

## Context

The reference controller is a FlexiHAL/STM32F446 board connected over USB
serial. It has no network stack. Supporting Telnet, WebSocket, and TCP adds
surface area, configuration, security considerations, and tests for no user
value on this machine.

## Decision

- The only production transport is a serial port.
- `ghs-serial` exposes a byte-level transport abstraction with two
  implementations: `SerialTransport` and `LoopbackTransport`.
- `LoopbackTransport` connects the engine to `ghs-sim` in-process. It is used by
  tests and by the app's demo mode. It is not presented as a machine connection.
- No network crates are added.

## Consequences

- A smaller dependency tree and a smaller attack surface.
- The simulator is still fully testable and useful for onboarding.
- Testing against external network simulators is not possible; the real board
  and recorded fixtures are the compatibility oracle.
- If network support is ever needed, it can be added behind the same transport
  trait without touching the protocol core.
