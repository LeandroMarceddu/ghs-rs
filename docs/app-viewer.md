# App viewer and program list

The center canvas renders the parsed `ghs-gcode` toolpath in either a 2D top
view or a 3D orbit view, with the real program list alongside it. The layout and
theme are unchanged by the mode switch.

## Data flow

```text
File / M3 JobLoaded event
        │  Arc<ghs_gcode::Program>
        ▼
AppState.program ── toolpath: Arc<ghs_viewer::Toolpath>
        │                         │
        │                         ├─ Camera2D fit(bounds)
        │                         └─ Camera3D fit(bounds)
        ▼
Program list (lines/current line)     GPUI canvas paint plan
        │
M3 JobProgress → current_line / progress
M2 snapshot.wpos → live tool marker
```

- `EngineEvent::JobLoaded` carries the parsed program; the app never re-parses a
  streaming job.
- The mock program is used only behind `--mock`.
- The tool marker uses `snapshot.wpos` (or `mpos` when no WPO) and is projected
  with the active camera. Status ticks only move the marker; they do not rebuild
  the geometry plan.

## 2D rendering

1. `Camera2D::fitted(viewport, toolpath.bounds().xy_bounds(), padding_px)` on
   load, program change, resize, or FIT.
2. `RenderPlan::from_2d` batches consecutive same-style segments.
3. Paint order: grid, work-envelope, rapids (thin/dashed), feeds/arcs
   (solid/medium), plunges (accent), height-map cells, tool marker.
4. The plan cache is keyed by program identity, segment count, camera scale and
   offset, and viewport. Pan/zoom rebuilds; a marker-only tick does not.
5. Draw a dot for one-point polylines (a plunge seen from above).

## 3D rendering

1. `Camera3D::fitted(viewport, toolpath.bounds(), padding_px)` with Z up and the
   crate's default yaw/pitch.
2. `RenderPlan::from_3d` projects and sorts polylines far to near; the painter
   uses that order. Motion styles and colors match the 2D view.
3. A subdued Z=0 ground grid and a dashed work-envelope box give depth cues.
4. The marker is projected with `Camera3D::project`, hidden behind the near
   plane, and drawn as a dropline plus a plane shadow and ring.
5. The height-map overlay projects measured cells at their effective Z and sorts
   them far to near.
6. The 3D cache key adds target X/Y/Z, yaw, pitch, distance, and viewport size.
   Orbit/pan/zoom rebuilds; a marker-only tick does not.

Performance target: 200k visible segments at 60 fps; use the viewer benchmark
as the reference and decimate by screen-space tolerance.

## Controls

- **2D:** drag pans; wheel/pinch zooms at the cursor; double-click or FIT
  refits.
- **3D:** left drag orbits (yaw/pitch); Shift+left drag or middle drag pans;
  wheel/pinch zooms at the cursor; double-click or FIT refits once.
- `2D TOP` / `3D` chips switch modes. The mode is session-local and resets to
  2D on restart.
- 3D is perspective only; there is no orthographic toggle. The painter uses
  painter's-algorithm order, so heavily self-overlapping paths can overlap
  visually. The height overlay is a cell projection, not a shaded surface mesh.

## Program list

- Virtualized list; a million-line program must not allocate a widget per line.
- Columns: line number, source text, optional segment/kind marker.
- The current line is highlighted from `JobProgress.current_line`.
- Search/filter by line text or number remains a follow-up.
- Block-delete, comments, and empty lines keep their source text; do not hide
  them by default.
- Start-from-line, run-to-line, and breakpoint editing stay disabled until the
  engine commands exist.

## Tests and proof

- Pure tests for world-to-screen mapping, bounds fit, 3D projection, orbit/pan/
  zoom, marker projection, cache invalidation, and far-to-near ordering.
- Parse the demo pocket with arcs; assert a non-empty toolpath and the expected
  segment count.
- Screenshots at 1280×800 and 1920×1080 using `docs/screenshots.md`.
- Verify a live tool marker moves after MDI `G0 X10 Y10` in both modes.
- Verify the program list highlights the streaming line.

## Dependencies

`ghs-app` depends on `ghs-gcode` and `ghs-viewer`. It must not depend on
`ghs-engine` internals; the job event carries the parsed program.
