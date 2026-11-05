# Screenshots and UI verification

Use this when a task needs a visual proof. A screenshot is part of acceptance:
report the path **and** verify its pixel size with `identify`.

## Rules

- Never leave `cargo run` or the app in the foreground. Build first, launch in
  the background, capture, kill, and verify `pgrep -a ghs-app` is empty.
- The development desktop runs a KWin script that remembers and restores window
  sizes. It overrides the app's `--size`; do not try to fix that in the app.
- Use an isolated environment for deterministic captures: empty
  `XDG_CONFIG_HOME`/`XDG_DATA_HOME`/`XDG_CACHE_HOME` and a private
  `XDG_RUNTIME_DIR` (mode 700) so the KWin script does not run inside the test
  compositor.
- The app flags are `--size WxH`, `--fullscreen`, `--mock`, `--light`,
  `--keypad`, `--connection-panel=serial`, `--demo-start`, `--demo-m0`,
  `--demo-toolchange`, `--demo-surfacing`, `--demo-transform`,
  `--demo-calibration`, `--demo-calibration-confirm`, `--demo-soft-limits`,
  `--demo-over-travel`, `--demo-setup`, `--demo-setup-units`,
  `--demo-setup-review`, `--demo-probe-tlo`, `--demo-probe-tlo-confirm`,
  `--demo-reset`, `--demo-reset-confirm`, `--demo-probe-defaults`,
  `--demo-gantry`, `--demo-gantry-card`, `--demo-job-history`,
  `--demo-job-history-clear`, `--demo-job-history-sheet`, `--viewer-3d`,
  `--viewer-stock-off`, `--demo-probe-macros`, `--demo-probe-macros-saved`,
  `--demo-probe-macros-error`, `--demo-recent-files`, `--demo-drop-hint`,
  `--high-contrast`, `--demo-rotation`, `--demo-rotation-card`, and
  `--demo-rotation-confirm`.
- Always verify the PNG with `identify`; a capture command can exit 0 and write
  nothing.
- Kill the app and the compositor in a `trap`/cleanup step. Check
  `pgrep -a ghs-app` and `pgrep -a kwin_wayland` afterward.
- Do not kill an app process from another target directory unless it is an
  orphan blocking a run; check its parent and, if in doubt, ask the
  orchestrator.

## Verified method: nested KWin + private D-Bus

This is the method used for the approved captures. The private
`dbus-run-session` is required: without it, `spectacle` can exit 0 and write
nothing, while the nested compositor has no screen-capture service.

```sh
cd /home/leandro/Desktop/Dev/Testing/ghs-rs
cargo build -p ghs-app

SIZE=1280x800
W=${SIZE%x*}; H=${SIZE#*x}
OUT=/tmp/opencode/ghs-live-$SIZE.png
ROOT=/tmp/opencode/ghs-shot-env
rm -rf "$ROOT"
mkdir -p "$ROOT/config" "$ROOT/data" "$ROOT/cache" "$ROOT/runtime"
chmod 700 "$ROOT/runtime"
export XDG_RUNTIME_DIR="$ROOT/runtime"
export XDG_CONFIG_HOME="$ROOT/config"
export XDG_DATA_HOME="$ROOT/data"
export XDG_CACHE_HOME="$ROOT/cache"
export W H OUT

dbus-run-session -- bash -c '
  export WAYLAND_DISPLAY=wayland-0
  export XDG_CURRENT_DESKTOP=KDE
  kwin_wayland --virtual --no-lockscreen --width "$W" --height "$H" --scale 1 \
    --socket wayland-0 >"$XDG_RUNTIME_DIR/kwin.log" 2>&1 &
  KWIN=$!
  sleep 2
  ./target/debug/ghs-app --fullscreen >"$XDG_RUNTIME_DIR/app.log" 2>&1 &
  APP=$!
  sleep 4
  spectacle --fullscreen --background --nonotify --output "$OUT" \
    >"$XDG_RUNTIME_DIR/spectacle.log" 2>&1 || true
  kill "$APP" 2>/dev/null || true
  kill "$KWIN" 2>/dev/null || true
'

identify "$OUT" || echo "capture failed; check $ROOT/spectacle.log"
pgrep -a ghs-app || true
```

A verified run produced `/tmp/opencode/ghs-shot-test3.png` at 1280×800.
`/tmp/opencode/ghs-shots/start-nested.sh` is the app agent's equivalent
long-lived helper if you need repeated captures. Prefer wrapping the whole
`dbus-run-session` in `timeout 60` and never leave a helper sleeping for an
hour; an orphan `dbus-run-session` leaves a private session bus behind.

If you suspect leftovers, check:

```sh
pgrep -af 'kwin_wayland --virtual|ghs-app|spectacle|dbus-daemon --nofork --print-address'
```

Kill only nested `kwin_wayland --virtual`, the test `ghs-app`, `spectacle`, and
private `dbus-daemon --nofork --print-address` processes with no children.
Never kill the host `kwin_wayland`, its Xwayland, or the session bus from your
real desktop. A private bus from `dbus-run-session` is normally reaped when its
wrapper exits; a long-lived `sleep` inside the wrapper prevents that.

### Portal and secret-service leftovers

Each isolated XDG environment can also start an `xdg-desktop-portal` and
`ksecretd` bound to that environment. They outlive the app and recreate
`data/kwalletd` after the directory is deleted, which looks like temp dirs
"popping back". Use the repo helper after a capture session:

```sh
python3 tools/cleanup-test-sessions.py          # show what would be killed
python3 tools/cleanup-test-sessions.py --apply  # kill test-only daemons + remove env dirs
```

The script acts only on processes whose `/proc/<pid>/environ` contains an
`XDG_*` path under `/tmp/opencode/ghs-*`; the real desktop's portal/secret daemons are
untouched. Run it when no capture is in progress, then verify:

```sh
pgrep -af 'xdg-desktop-por|ksecretd'
ls -d /tmp/opencode/ghs-m5-* /tmp/opencode/ghs-m6-* 2>/dev/null
```

The host's normal `xdg-desktop-portal`/`ksecretd` processes may remain; only the
ones with the temp XDG prefix are test leftovers.

The `--no-lockscreen` flag matters: if the host session is locked, the nested
virtual output shows the host lock screen instead of the app.

### Demo-job captures

The M3 job UI has review flags:

- `--demo-start` presses Start once after the demo program loads.
- `--demo-m0` loads the program with an `M0` pause, giving a stable mid-job
  frame with progress and the current line.
- `--demo-toolchange` loads the two-tool program and stops at `M6 T2`, giving
  a stable tool-change sheet.

A short program can be acknowledged faster than the 50 ms UI poll, so a
"RUNNING" frame is not reliably capturable. Do not fake it; capture the M0 or
tool-change pause and label it correctly. The integration tests cover the
running state and the exact Start/Acknowledge/Resume/Stop commands.

Notes:

- The nested compositor normally creates `wayland-0`; do not set a custom
  `--socket` unless you also set `WAYLAND_DISPLAY` to that exact name.
- The app must be `--fullscreen` to fill the virtual output at the requested
  size.
- `spectacle` can depend on the session bus/portal. If it exits without a file,
  fall back to the host X11 path below. Do not spend a long time fighting it.
- Capture 1280×800, 1920×1080, and 1080×1920 by repeating with a different
  `SIZE`.

## Fallback: host X11/Xwayland interaction

The host runs KWin Wayland with Xwayland. This path is useful for click-driven
proof (MDI, jog, dialogs) where `xdotool` is needed.

- Build the app.
- Launch it as an X11 client if GPUI selects X11: set `DISPLAY=:0` and clear
  `WAYLAND_DISPLAY` (or use the app's own X11 backend if it exposes one).
- KWin may restore the remembered 1920×1002 size; capture that as the large
  case and record the actual size.
- `xdotool` can click by screen coordinates; take a screenshot after the click
  and verify the console/DRO changed.
- Capture with `import -window root` (ImageMagick) or `spectacle --fullscreen
  --background --nonotify --output ...`.

The previous successful interaction proofs are:

- `/tmp/opencode/ghs-x11-mdi.png` — MDI `G0 X10 Y10`, console `> ...` + `ok`,
  DRO X/Y 10.000.
- `/tmp/opencode/ghs-x11-jog.png` — jog command and the simulator error before
  the `$J=G21` fix.
- `/tmp/opencode/ghs-x11-disconnected.png` and `-reconnected.png`.

## What to report

1. Absolute screenshot paths.
2. Actual dimensions from `identify`.
3. What the screenshot proves (live vs mock, command/console line, DRO values).
4. Any capture limitation (KWin size override, missing portal, hardware
   unavailable).
5. Confirmation that no `ghs-app` process remains.
