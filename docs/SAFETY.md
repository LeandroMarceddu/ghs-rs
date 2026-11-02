# Safety

This software commands a machine that can move at high speed, spin a cutter,
and crush or cut anything in its path. Treat every new build as untrusted until
it has been verified on the machine.

## Before the first connection

- Know where the physical E-stop is and test it.
- Verify the spindle can be stopped independently of the sender.
- Confirm the machine's travel limits and soft-limit settings.
- Start with the spindle disabled or the tool clear of the workpiece.
- Use the simulator first; then connect with the spindle off and run only
  status, DRO, and small jogs.
- Keep hands clear of the spindle and axes during any commanded motion.

## Rules enforced by the application (design requirements)

1. The UI never writes bytes directly; all commands pass through the safety
   gateway.
2. Motion is refused in `Alarm` and `Door` states without an explicit,
   confirmed unlock.
3. Probing requires a safe Z, a bounded search distance, and either a
   probe-connected signal or explicit confirmation.
4. Reset during a job, run-from-line, goto, and probing moves require
   confirmation.
5. A lost status stream pauses a running job; it never silently continues.
6. The application never auto-resumes after an alarm, E-stop, or reconnect.
7. Settings writes are idle-only and read back for verification.
8. User macros run only after an explicit action; a macro marked confirm
   requires a second confirmation, and macros are refused in mock, Alarm/Door,
   a running job, or a non-Idle state. They never auto-run on load or import.
9. The stop/reset control is always visible and never disabled.

## Operator checklist for every release

- [ ] Simulator scenarios pass.
- [ ] Connect/disconnect/reconnect with the spindle off.
- [ ] Home, jog each axis, verify DRO directions and units.
- [ ] Verify feed, rapid, and spindle overrides.
- [ ] Verify E-stop, door, feed hold, and cycle start signals.
- [ ] Run a small air-cut program with pause/resume/stop.
- [ ] Run a tool change with the tool clear.
- [ ] Verify probing cycles with a test plate and safe Z.
- [ ] Verify limits/alarms and the unlock path.
- [ ] Confirm the job never resumes after an alarm or reconnect.

If any check fails, stop and fix it before using the build on a workpiece.
