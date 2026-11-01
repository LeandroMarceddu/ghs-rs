# App viewer and program list (M4)

The current center canvas is a synthetic sketch. M4 replaces it with the real
`ghs-gcode` toolpath and the real program list. The layout and theme stay
unchanged.

## Data flow

```text
File / M3 JobLoaded event
        │  Arc<ghs_gcode::Program>
        ▼
AppState.program ── toolpath: Arc<ghs_viewer::Toolpath>
        │                         │
        │                         └─ Camera2D fit(bounds)
        ▼
Program list (lines/current line)     GPUI canvas paint plan
        │
M3 JobProgress → current_line / progress
M2 snapshot.wpos → live tool marker
```

- Before M3 lands, parse a bundled demo program with
  `ghs_gcode::parse_program` so the canvas is real, not synthetic.
- When M3 lands, prefer `EngineEvent::JobLoaded { program, .. }`; the app never
  re-parses a streaming job.
- Keep the mock program only behind `--mock`.

## Viewer rendering

Use `ghs_viewer` directly:

1. `Camera2D::fitted(viewport, toolpath.bounds(), padding_px)` on load/resize.
2. Build a `RenderPlan`: group segments by `SegmentKind` and `layer`/`tool` so
   the canvas uses a small number of `PathBuilder`s.
3. Paint order: grid, stock/work-envelope, rapids (thin/dashed), feeds/arcs
   (solid/medium), plunges (accent), tool marker.
4. Rebuild the plan on camera or viewport change; cache it while static.
5. The tool marker uses `snapshot.wpos` (or `mpos` when no WPO) projected with
   the same camera; update at status rate, not by rebuilding geometry.
6. Draw a dot for one-point polylines (a plunge seen from above).

Performance target: 200k visible segments at 60 fps; use the viewer benchmark
as the reference and decimate by screen-space tolerance.

## Controls

- Drag: pan.
- Wheel/pinch: zoom at cursor (`Camera2D::zoom_at`).
- Fit button / double-click: fit bounds.
- Optional: 2D/3D toggle once the 3D path is ready; do not block M4 on 3D.

## Program list

- Virtualize the list; a million-line program must not allocate a widget per
  line.
- Columns: line number, source text, optional segment/kind marker.
- Current line is highlighted from `JobProgress.current_line`.
- Search/filter by line text or number.
- Context actions for M3: start from line, run to line, breakpoint; keep them
  disabled until the engine commands exist.
- Block-delete, comments, and empty lines keep their source text; do not hide
  them by default.

## Tests and proof

- Pure tests for world-to-screen mapping and bounds fit.
- Parse a sample with arcs and canned cycles; assert non-empty toolpath.
- Screenshots at 1280×800 and 1920×1080 using `docs/screenshots.md`.
- Verify a live tool marker moves after MDI `G0 X10 Y10`.
- Verify the list highlights the streaming line once M3 job events are wired.

## Dependencies

`ghs-app` adds `ghs-gcode` and `ghs-viewer`. It must not depend on
`ghs-engine` internals; the job event carries the parsed program.
