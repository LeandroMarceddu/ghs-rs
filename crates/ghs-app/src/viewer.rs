//! Real toolpath canvas for the Job workspace.
//!
//! The canvas draws the parsed program toolpath through `ghs-viewer`: a
//! [`Camera2D`] fitted to the program bounds, a cached [`RenderPlan`], a
//! world-space grid, and a live tool marker projected from the DRO work
//! position. A drag pans, the wheel or a pinch zooms at the cursor, and a
//! double click or the FIT chip refits.
//!
//! Only `--mock`, or a live session with no program loaded, falls back to the
//! synthetic sketch. The plan is cached by program identity, camera transform,
//! and viewport size, so a status tick that only moves the marker never
//! rebuilds the geometry.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ghs_gcode::Program;
use ghs_viewer::{
    Bounds2, Camera2D, Point2, RenderOptions, RenderPlan, SegmentKind, Toolpath, Viewport,
};
use gpui_kit::{
    Bounds, Entity, Hsla, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    PathBuilder, Pixels, Point, ScrollDelta, Size, Styled as _, Window, canvas, div, fill, point,
    px, size,
};

use crate::state::Units;
use crate::theme::Palette;

/// Screen-space padding around the fitted toolpath, in logical pixels.
///
/// It clears the 22 px status chips at the top and the legend at the bottom,
/// so the program is never hidden under the viewer overlays.
const VIEW_PADDING_PX: f32 = 40.0;

/// Segment count of the synthetic preview used when no program is loaded.
const SYNTHETIC_SEGMENTS: usize = 6_000;

/// Guard against a pathological camera producing an unbounded grid loop.
const MAX_GRID_LINES: i64 = 512;

/// Shared camera and cached plan behind the job canvas.
///
/// The handle is cheap to clone (one `Rc`) and is what the paint closure and the
/// pointer handlers both hold, so a repaint never needs to rebuild the plan.
#[derive(Clone)]
pub struct ViewerHandle {
    inner: Rc<RefCell<Viewer>>,
}

impl Default for ViewerHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewerHandle {
    /// Creates an unfitted viewer. The first paint fits the current program.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(Viewer::default())),
        }
    }

    /// Records a drag start at a window-space pointer position.
    ///
    /// Returns `false`: the gesture alone changes no pixels, so the caller does
    /// not need to redraw.
    pub fn begin_drag(&self, at: Point<Pixels>) -> bool {
        self.inner.borrow_mut().drag = Some(at);
        false
    }

    /// Pans by the pointer delta since the last position.
    ///
    /// Returns `true` when the camera moved and the canvas needs a redraw.
    pub fn drag_to(&self, at: Point<Pixels>) -> bool {
        let mut viewer = self.inner.borrow_mut();
        let Some(last) = viewer.drag else {
            return false;
        };
        viewer.drag = Some(at);
        let delta = Point2::new(f32::from(at.x - last.x), f32::from(at.y - last.y));
        if !delta.is_finite() || delta.length_squared() <= f32::EPSILON {
            return false;
        }
        viewer.camera.pan(delta);
        true
    }

    /// Ends the current drag gesture.
    pub fn end_drag(&self) {
        self.inner.borrow_mut().drag = None;
    }

    /// Zooms around a window-space pointer position.
    ///
    /// Returns `true` when the camera scale changed.
    pub fn zoom_at(&self, at: Point<Pixels>, factor: f32) -> bool {
        let mut viewer = self.inner.borrow_mut();
        let Some(rect) = viewer.rect else {
            return false;
        };
        let local = local_point(rect, at);
        let before = viewer.camera.scale();
        viewer.camera.zoom_at(local, factor);
        let after = viewer.camera.scale();
        (after - before).abs() > f32::EPSILON * before.max(1.0)
    }

    /// Refits the program bounds on the next paint.
    ///
    /// Returns `true` so the caller redraws.
    pub fn fit(&self) -> bool {
        self.inner.borrow_mut().pending_fit = true;
        true
    }

    /// Current zoom relative to the fitted scale, as a whole percentage.
    #[must_use]
    pub fn zoom_percent(&self) -> u32 {
        let viewer = self.inner.borrow();
        let fit = viewer.fit_scale;
        if fit > 0.0 && fit.is_finite() {
            (viewer.camera.scale() / fit * 100.0)
                .round()
                .clamp(1.0, 100_000.0) as u32
        } else {
            100
        }
    }
}

/// A view that owns a [`ViewerHandle`], so canvas gestures can redraw it.
pub trait HasViewer {
    /// The job canvas state shared with the paint closure.
    fn viewer_handle(&self) -> &ViewerHandle;
}

/// Draws the interactive job canvas.
///
/// `program` is the parsed program from `JobLoaded`; `None` draws the synthetic
/// preview. `marker` is the tool position in world millimetres.
pub fn view<T: HasViewer + 'static>(
    handle: &ViewerHandle,
    palette: Palette,
    program: Option<Arc<Program>>,
    marker: Option<Point2>,
    view: Entity<T>,
) -> impl IntoElement {
    let canvas_element = {
        let measure = handle.clone();
        let paint_handle = handle.clone();
        canvas(
            move |bounds: Bounds<Pixels>, _, _| {
                measure.inner.borrow_mut().rect = Some(bounds);
            },
            move |bounds: Bounds<Pixels>, (), window: &mut Window, _| {
                paint(bounds, &paint_handle, program, marker, palette, window);
            },
        )
        .size_full()
    };

    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, {
            let view = view.clone();
            move |event, _, cx| {
                let at = event.position;
                let double_click = event.click_count > 1;
                view.update(cx, |this, cx| {
                    let viewer = this.viewer_handle();
                    let changed = if double_click {
                        viewer.fit()
                    } else {
                        viewer.begin_drag(at)
                    };
                    if changed {
                        cx.notify();
                    }
                });
            }
        })
        .on_mouse_move({
            let view = view.clone();
            move |event, _, cx| {
                if !event.dragging() {
                    return;
                }
                let at = event.position;
                view.update(cx, |this, cx| {
                    if this.viewer_handle().drag_to(at) {
                        cx.notify();
                    }
                });
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let view = view.clone();
            move |_, _, cx| {
                view.update(cx, |this, _| this.viewer_handle().end_drag());
            }
        })
        .on_scroll({
            let view = view.clone();
            move |event, _, cx| {
                let factor = scroll_zoom_factor(event.delta);
                let at = event.position;
                if (factor - 1.0).abs() <= 0.001 {
                    return;
                }
                view.update(cx, |this, cx| {
                    if this.viewer_handle().zoom_at(at, factor) {
                        cx.notify();
                    }
                });
            }
        })
        .on_pinch({
            let view = view.clone();
            move |event, _, cx| {
                let factor = (1.0_f32 + event.delta).clamp(0.05, 20.0);
                let at = event.position;
                view.update(cx, |this, cx| {
                    if this.viewer_handle().zoom_at(at, factor) {
                        cx.notify();
                    }
                });
            }
        })
        .child(canvas_element)
}

/// Projects the DRO work position into viewer millimetres.
///
/// The engine reports work coordinates (`wpos`, with `mpos - wco` as the
/// fallback) and the app stores them in the controller's display unit. The
/// toolpath is always millimetres, so an inch session is converted here.
#[must_use]
pub fn marker_mm(units: Units, dro: &[f64]) -> Option<Point2> {
    let x = units.to_mm(*dro.first()?);
    let y = units.to_mm(*dro.get(1)?);
    (x.is_finite() && y.is_finite()).then(|| Point2::new(x as f32, y as f32))
}

/// Chooses a readable world-space grid step for a pixels-per-millimetre scale.
///
/// The result is a 1/2/5 decade step that keeps minor lines between roughly 24
/// and 160 logical pixels apart.
#[must_use]
pub fn grid_step_mm(scale_px_per_mm: f32) -> f32 {
    const TARGET_PX: f32 = 40.0;
    const MIN_STEP_MM: f32 = 0.001;
    if !scale_px_per_mm.is_finite() || scale_px_per_mm <= 0.0 {
        return 1.0;
    }
    let raw = TARGET_PX / scale_px_per_mm;
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let decade = 10.0_f32.powf(raw.log10().floor());
    let normalized = raw / decade;
    let multiple = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    (decade * multiple).max(MIN_STEP_MM)
}

/// Converts a window-space pointer position into canvas-local coordinates.
fn local_point(rect: Bounds<Pixels>, at: Point<Pixels>) -> Point2 {
    Point2::new(
        f32::from(at.x) - f32::from(rect.origin.x),
        f32::from(at.y) - f32::from(rect.origin.y),
    )
}

/// Maps one wheel or trackpad scroll delta to a zoom factor.
fn scroll_zoom_factor(delta: ScrollDelta) -> f32 {
    match delta {
        ScrollDelta::Lines(delta) => 1.2_f32.powf(delta.y.clamp(-8.0, 8.0)),
        ScrollDelta::Pixels(delta) => {
            let dy = f32::from(delta.y).clamp(-240.0, 240.0);
            (1.0 + dy * 0.0025).clamp(0.2, 5.0)
        }
    }
}

/// Cached render plan and the state it was built for.
#[derive(Default)]
struct PlanCache {
    key: Option<PlanKey>,
    plan: RenderPlan,
}

/// Identity of one render plan: program, geometry, and camera transform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlanKey {
    program: usize,
    segments: usize,
    scale: u32,
    offset_x: u32,
    offset_y: u32,
    width: u32,
    height: u32,
}

impl PlanKey {
    fn new(program: usize, segments: usize, camera: &Camera2D) -> Self {
        let offset = camera.offset();
        let viewport = camera.viewport();
        Self {
            program,
            segments,
            scale: camera.scale().to_bits(),
            offset_x: offset.x.to_bits(),
            offset_y: offset.y.to_bits(),
            width: viewport.width().to_bits(),
            height: viewport.height().to_bits(),
        }
    }
}

/// Mutable viewer state shared through [`ViewerHandle`].
#[derive(Default)]
struct Viewer {
    camera: Camera2D,
    /// World bounds of the current toolpath, for the work-envelope outline.
    path_bounds: Bounds2,
    /// Identity of the program the camera is fitted to; `0` is synthetic.
    identity: usize,
    /// Camera scale produced by the last fit, for the zoom readout.
    fit_scale: f32,
    /// A resize, program change, or explicit FIT asks for a refit.
    pending_fit: bool,
    /// Canvas rectangle in window coordinates, refreshed on every prepaint.
    rect: Option<Bounds<Pixels>>,
    /// Last pointer position of an active drag, in window coordinates.
    drag: Option<Point<Pixels>>,
    /// Synthetic preview, built once and reused.
    synthetic: Option<Arc<Toolpath>>,
    cache: PlanCache,
    /// Number of plans built; a test seam for the caching contract.
    #[cfg(test)]
    plan_builds: u32,
}

impl Viewer {
    /// Updates the camera for the current viewport and program, then ensures
    /// the render plan matches. Returns the camera used for this frame.
    fn prepare(&mut self, size: Size<Pixels>, program: Option<&Arc<Program>>) -> Camera2D {
        let viewport = Viewport::new(f32::from(size.width), f32::from(size.height));
        let identity = program.map_or(0, |program| Arc::as_ptr(program) as usize);
        let synthetic: Arc<Toolpath>;
        let path: &Toolpath = match program {
            Some(program) => &program.toolpath,
            None => {
                synthetic = Arc::clone(self.synthetic.get_or_insert_with(|| {
                    Arc::new(ghs_viewer::synthetic_toolpath(SYNTHETIC_SEGMENTS))
                }));
                synthetic.as_ref()
            }
        };

        if viewport != self.camera.viewport() {
            self.camera.set_viewport(viewport);
            self.pending_fit = true;
        }
        if identity != self.identity {
            self.identity = identity;
            self.pending_fit = true;
        }
        self.path_bounds = path.bounds().xy_bounds();
        if self.pending_fit && !self.path_bounds.is_empty() && !viewport.is_empty() {
            self.camera.fit(self.path_bounds, VIEW_PADDING_PX);
            self.fit_scale = self.camera.scale();
            self.pending_fit = false;
        }

        let key = PlanKey::new(identity, path.segment_count(), &self.camera);
        if self.cache.key != Some(key) {
            self.cache.plan = RenderPlan::from_2d(path, &self.camera, RenderOptions::default());
            self.cache.key = Some(key);
            #[cfg(test)]
            {
                self.plan_builds += 1;
            }
        }
        self.camera
    }
}

fn paint(
    bounds: Bounds<Pixels>,
    handle: &ViewerHandle,
    program: Option<Arc<Program>>,
    marker: Option<Point2>,
    palette: Palette,
    window: &mut Window,
) {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    if width < 24.0 || height < 24.0 {
        return;
    }

    let mut viewer = handle.inner.borrow_mut();
    let camera = viewer.prepare(bounds.size, program.as_ref());

    paint_grid(&camera, bounds, palette, window);
    if !viewer.path_bounds.is_empty() {
        paint_envelope(&camera, viewer.path_bounds, bounds, palette, window);
    }
    paint_plan(bounds, &viewer.cache.plan, palette, window);
    if let Some(marker) = marker {
        paint_marker(&camera, bounds, marker, palette, window);
    }
}

/// Paints a world-aligned grid with major lines every fifth step.
fn paint_grid(camera: &Camera2D, bounds: Bounds<Pixels>, palette: Palette, window: &mut Window) {
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let right = left + f32::from(bounds.size.width);
    let bottom = top + f32::from(bounds.size.height);
    let visible = camera.visible_world_bounds();
    if visible.is_empty() || !visible.width().is_finite() || !visible.height().is_finite() {
        return;
    }

    let step = grid_step_mm(camera.scale());
    let mut minor = PathBuilder::stroke(px(1.0));
    let mut major = PathBuilder::stroke(px(1.0));

    let first_x = grid_index(visible.min().x, step);
    for index in first_x..=first_x + MAX_GRID_LINES {
        let x = index as f32 * step;
        if x > visible.max().x {
            break;
        }
        let screen = camera.world_to_screen(Point2::new(x, 0.0));
        let line_x = left + screen.x;
        let target = if index.rem_euclid(5) == 0 {
            &mut major
        } else {
            &mut minor
        };
        target.move_to(point(px(line_x), px(top)));
        target.line_to(point(px(line_x), px(bottom)));
    }

    let first_y = grid_index(visible.min().y, step);
    for index in first_y..=first_y + MAX_GRID_LINES {
        let y = index as f32 * step;
        if y > visible.max().y {
            break;
        }
        let screen = camera.world_to_screen(Point2::new(0.0, y));
        let line_y = top + screen.y;
        let target = if index.rem_euclid(5) == 0 {
            &mut major
        } else {
            &mut minor
        };
        target.move_to(point(px(left), px(line_y)));
        target.line_to(point(px(right), px(line_y)));
    }

    paint_path(minor, palette.grid_minor, window);
    paint_path(major, palette.grid_major, window);
    paint_axes(camera, bounds, palette, window);
}

/// Index of the first grid line at or below a world coordinate.
///
/// The quotient is clamped before the integer cast: `as` saturates, but a
/// saturated `i64` would overflow when the loop adds [`MAX_GRID_LINES`].
fn grid_index(world: f32, step: f32) -> i64 {
    (world / step)
        .floor()
        .clamp(-1_000_000_000.0, 1_000_000_000.0) as i64
}

/// Paints the world X=0 and Y=0 axes when they are on screen.
fn paint_axes(camera: &Camera2D, bounds: Bounds<Pixels>, palette: Palette, window: &mut Window) {
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let origin = camera.world_to_screen(Point2::ZERO);
    let mut axes = PathBuilder::stroke(px(1.0));

    if (0.0..=width).contains(&origin.x) {
        axes.move_to(point(px(left + origin.x), px(top)));
        axes.line_to(point(px(left + origin.x), px(top + height)));
    }
    if (0.0..=height).contains(&origin.y) {
        axes.move_to(point(px(left), px(top + origin.y)));
        axes.line_to(point(px(left + width), px(top + origin.y)));
    }
    paint_path(axes, palette.border_strong, window);
}

/// Paints the program's XY work envelope as a dashed outline.
fn paint_envelope(
    camera: &Camera2D,
    world: Bounds2,
    bounds: Bounds<Pixels>,
    palette: Palette,
    window: &mut Window,
) {
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let min = camera.world_to_screen(world.min());
    let max = camera.world_to_screen(world.max());
    // Screen Y is flipped, so the world minimum is the bottom-left corner.
    let x0 = left + min.x;
    let y0 = top + max.y;
    let x1 = left + max.x;
    let y1 = top + min.y;

    let mut outline = PathBuilder::stroke(px(1.0)).dash_array(&[px(7.0), px(5.0)]);
    outline.move_to(point(px(x0), px(y0)));
    outline.line_to(point(px(x1), px(y0)));
    outline.line_to(point(px(x1), px(y1)));
    outline.line_to(point(px(x0), px(y1)));
    outline.close();
    paint_path(outline, palette.border_strong, window);
}

/// Paints the render plan, one GPUI path per motion style.
fn paint_plan(bounds: Bounds<Pixels>, plan: &RenderPlan, palette: Palette, window: &mut Window) {
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);

    let mut rapid = PathBuilder::stroke(px(1.1)).dash_array(&[px(5.0), px(4.0)]);
    let mut cut = PathBuilder::stroke(px(1.6));
    let mut plunge = PathBuilder::stroke(px(1.9));
    let mut probe = PathBuilder::stroke(px(1.2)).dash_array(&[px(2.0), px(3.0)]);
    let mut dots: Vec<(f32, f32, Hsla)> = Vec::new();

    for polyline in plan.polylines() {
        let (target, color) = match polyline.kind {
            SegmentKind::Rapid => (&mut rapid, palette.path_rapid),
            SegmentKind::Feed | SegmentKind::Arc => (&mut cut, palette.path_cut),
            SegmentKind::Plunge => (&mut plunge, palette.path_plunge),
            SegmentKind::Probe => (&mut probe, palette.info),
        };
        if polyline.points.len() == 1 {
            let single = polyline.points[0];
            dots.push((left + single.x, top + single.y, color));
            continue;
        }
        for (index, screen) in polyline.points.iter().enumerate() {
            let world = point(px(left + screen.x), px(top + screen.y));
            if index == 0 {
                target.move_to(world);
            } else {
                target.line_to(world);
            }
        }
    }

    paint_path(rapid, palette.path_rapid, window);
    paint_path(cut, palette.path_cut, window);
    paint_path(plunge, palette.path_plunge, window);
    paint_path(probe, palette.info, window);
    for (x, y, color) in dots {
        paint_dot(window, color, x, y, 1.8);
    }
}

/// Screen position of the tool marker inside `bounds`, or `None` when it is
/// outside the canvas plus a small margin for the crosshair.
///
/// This is the single projection used by [`paint_marker`], so the painted
/// position always includes the canvas origin exactly once.
fn marker_screen(camera: &Camera2D, bounds: Bounds<Pixels>, world: Point2) -> Option<(f32, f32)> {
    if !world.is_finite() {
        return None;
    }
    let screen = camera.world_to_screen(world);
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let right = left + f32::from(bounds.size.width);
    let bottom = top + f32::from(bounds.size.height);
    let x = left + screen.x;
    let y = top + screen.y;
    if x < left - 12.0 || x > right + 12.0 || y < top - 12.0 || y > bottom + 12.0 {
        return None;
    }
    Some((x, y))
}

/// Paints the live tool marker: halo, ring, centre dot, and crosshair.
fn paint_marker(
    camera: &Camera2D,
    bounds: Bounds<Pixels>,
    world: Point2,
    palette: Palette,
    window: &mut Window,
) {
    let Some((x, y)) = marker_screen(camera, bounds, world) else {
        return;
    };

    paint_dot(window, palette.accent.alpha(0.18), x, y, 12.0);
    paint_dot(window, palette.accent_hover, x, y, 6.5);
    paint_dot(window, palette.bg, x, y, 4.8);
    paint_dot(window, palette.accent_hover, x, y, 2.8);

    let mut cross = PathBuilder::stroke(px(1.0));
    cross.move_to(point(px(x - 13.0), px(y)));
    cross.line_to(point(px(x + 13.0), px(y)));
    cross.move_to(point(px(x), px(y - 13.0)));
    cross.line_to(point(px(x), px(y + 13.0)));
    paint_path(cross, palette.text, window);
}

/// Paints one filled circle.
fn paint_dot(window: &mut Window, color: Hsla, x: f32, y: f32, radius: f32) {
    let bounds = Bounds::new(
        point(px(x - radius), px(y - radius)),
        size(px(radius * 2.0), px(radius * 2.0)),
    );
    window.paint_quad(fill(bounds, color).corner_radii(px(radius)));
}

fn paint_path(builder: PathBuilder, color: Hsla, window: &mut Window) {
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(left: f32, right: f32, epsilon: f32) -> bool {
        (left - right).abs() <= epsilon
    }

    fn test_bounds(width: f32, height: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(0.0), px(0.0)), size(px(width), px(height)))
    }

    fn program(source: &str) -> Arc<Program> {
        Arc::new(ghs_gcode::parse_program(source).expect("test program parses"))
    }

    #[test]
    fn fit_pads_and_centers_the_toolpath() {
        let handle = ViewerHandle::new();
        let program = program("G21 G90\nG0 X0 Y0\nG1 X40 F800\nG1 Y20\n");
        let bounds = test_bounds(800.0, 600.0);
        let camera = {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(bounds.size, Some(&program))
        };

        let world = program.toolpath.bounds().xy_bounds();
        assert_eq!(world.min(), Point2::new(0.0, 0.0));
        assert_eq!(world.max(), Point2::new(40.0, 20.0));
        assert!(approx(
            camera.scale(),
            (800.0 - 2.0 * VIEW_PADDING_PX) / 40.0,
            1.0e-3
        ));

        let min = camera.world_to_screen(world.min());
        let max = camera.world_to_screen(world.max());
        // X fills the padded width exactly; Y stays centered and inside.
        assert!(approx(min.x, VIEW_PADDING_PX, 0.01));
        assert!(approx(max.x, 800.0 - VIEW_PADDING_PX, 0.01));
        assert!(max.y >= VIEW_PADDING_PX - 0.01);
        assert!(min.y <= 600.0 - VIEW_PADDING_PX + 0.01);
        assert!(approx((min.y + max.y) * 0.5, 300.0, 0.01));
    }

    #[test]
    fn a_new_program_refits_the_camera() {
        let handle = ViewerHandle::new();
        let small = program("G0 X0 Y0\nG1 X10 F800\nG1 Y10\n");
        let large = program("G0 X100 Y100\nG1 X200 F800\nG1 Y200\n");
        let bounds = test_bounds(800.0, 600.0);
        let first = {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(bounds.size, Some(&small))
        };
        let second = {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(bounds.size, Some(&large))
        };

        assert!(first.scale() > second.scale());
        // The rapid from the origin to the far corner is part of the path, so
        // the large program spans 0..200 on both axes.
        let center = second.world_to_screen(Point2::new(100.0, 100.0));
        assert!(approx(center.x, 400.0, 0.01));
        assert!(approx(center.y, 300.0, 0.01));
    }

    #[test]
    fn the_plan_is_cached_until_the_camera_moves() {
        let handle = ViewerHandle::new();
        let program = program("G0 X0 Y0\nG1 X40 F800\nG1 Y20\n");
        let bounds = test_bounds(800.0, 600.0);
        {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(bounds.size, Some(&program));
            viewer.prepare(bounds.size, Some(&program));
            assert_eq!(viewer.plan_builds, 1, "a static frame rebuilt the plan");
            assert!(!viewer.cache.plan.is_empty());

            // Same program and camera: a marker-only status tick must not
            // rebuild the geometry.
            viewer.prepare(bounds.size, Some(&program));
            assert_eq!(viewer.plan_builds, 1);

            viewer.camera.pan(Point2::new(12.0, -4.0));
            viewer.prepare(bounds.size, Some(&program));
            assert_eq!(viewer.plan_builds, 2, "a pan did not rebuild the plan");
        }
    }

    #[test]
    fn zoom_keeps_the_pointer_pivot_fixed() {
        let handle = ViewerHandle::new();
        let program = program("G0 X0 Y0\nG1 X40 F800\nG1 Y20\n");
        let rect = Bounds::new(point(px(100.0), px(50.0)), size(px(800.0), px(600.0)));
        handle.inner.borrow_mut().rect = Some(rect);
        {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(rect.size, Some(&program));
        }

        let local = Point2::new(300.0, 200.0);
        let pivot = point(px(100.0 + local.x), px(50.0 + local.y));
        let world_before = handle.inner.borrow().camera.screen_to_world(local);
        assert!(handle.zoom_at(pivot, 2.5));
        let after = handle.inner.borrow().camera.world_to_screen(world_before);
        assert!(approx(after.x, local.x, 0.01));
        assert!(approx(after.y, local.y, 0.01));
    }

    #[test]
    fn drag_pans_in_screen_space() {
        let handle = ViewerHandle::new();
        let program = program("G0 X0 Y0\nG1 X40 F800\nG1 Y20\n");
        let bounds = test_bounds(800.0, 600.0);
        let world = {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(bounds.size, Some(&program));
            Point2::new(10.0, 5.0)
        };
        let before = handle.inner.borrow().camera.world_to_screen(world);
        assert!(!handle.begin_drag(point(px(200.0), px(200.0))));
        assert!(handle.drag_to(point(px(240.0), px(180.0))));
        let after = handle.inner.borrow().camera.world_to_screen(world);
        assert!(approx(after.x - before.x, 40.0, 0.01));
        assert!(approx(after.y - before.y, -20.0, 0.01));
        handle.end_drag();
    }

    #[test]
    fn grid_spacing_stays_readable() {
        for scale in [0.02, 0.1, 0.5, 1.0, 4.0, 12.5, 80.0] {
            let spacing = grid_step_mm(scale) * scale;
            assert!(
                (24.0..=160.0).contains(&spacing),
                "scale {scale} produced {spacing} px grid spacing"
            );
        }
        assert_eq!(grid_step_mm(0.0), 1.0);
        assert_eq!(grid_step_mm(f32::NAN), 1.0);
    }

    #[test]
    fn grid_index_saturates_instead_of_overflowing() {
        assert_eq!(grid_index(25.0, 5.0), 5);
        assert_eq!(grid_index(-1.0, 5.0), -1);
        assert!(grid_index(f32::MAX, 1.0) <= 1_000_000_000);
        assert!(grid_index(f32::MIN, 1.0) >= -1_000_000_000);
    }

    #[test]
    fn wheel_zoom_has_a_direction() {
        assert!(scroll_zoom_factor(ScrollDelta::Lines(Point::new(0.0, 1.0))) > 1.0);
        assert!(scroll_zoom_factor(ScrollDelta::Lines(Point::new(0.0, -1.0))) < 1.0);
        assert!(scroll_zoom_factor(ScrollDelta::Pixels(Point::new(px(0.0), px(-20.0)))) < 1.0);
    }

    #[test]
    fn local_pointer_mapping_removes_the_element_origin() {
        let rect = Bounds::new(point(px(100.0), px(50.0)), size(px(800.0), px(600.0)));
        assert_eq!(local_point(rect, point(px(100.0), px(50.0))), Point2::ZERO);
        assert_eq!(
            local_point(rect, point(px(300.0), px(250.0))),
            Point2::new(200.0, 200.0)
        );
    }

    #[test]
    fn marker_uses_work_coordinates_in_millimetres() {
        let mm = marker_mm(Units::Millimetre, &[10.0, 20.0, 3.0]).expect("marker");
        assert_eq!(mm, Point2::new(10.0, 20.0));

        let inch = marker_mm(Units::Inch, &[1.0, 2.0, 0.0]).expect("marker");
        assert!(approx(inch.x, 25.4, 1.0e-4));
        assert!(approx(inch.y, 50.8, 1.0e-4));

        assert!(marker_mm(Units::Millimetre, &[1.0]).is_none());
        assert!(marker_mm(Units::Millimetre, &[f64::NAN, 0.0]).is_none());
    }

    #[test]
    fn marker_reads_x_then_y_from_the_dro() {
        // `AppState::apply_snapshot` fills the DRO in axis order X, Y, Z and
        // the DRO card reads it back by `Axis::index`. The marker must use the
        // same slots, or a diagonal move would look horizontal.
        let dro = [12.0, -4.0, 7.0];
        assert_eq!(
            marker_mm(Units::Millimetre, &dro),
            Some(Point2::new(12.0, -4.0))
        );
    }

    #[test]
    fn marker_projects_up_and_right_on_a_canvas_with_a_nonzero_origin() {
        let origin = (120.0, 60.0);
        let bounds = Bounds::new(
            point(px(origin.0), px(origin.1)),
            size(px(800.0), px(600.0)),
        );
        let program = program("G0 X0 Y0\nG1 X40 F800\nG1 Y40\n");
        let camera = Camera2D::fitted(
            Viewport::new(f32::from(bounds.size.width), f32::from(bounds.size.height)),
            program.toolpath.bounds().xy_bounds(),
            VIEW_PADDING_PX,
        );

        let (x0, y0) = marker_screen(&camera, bounds, Point2::ZERO).expect("origin visible");
        let (x1, y1) =
            marker_screen(&camera, bounds, Point2::new(10.0, 10.0)).expect("marker visible");
        let scale = camera.scale();

        // Ten millimetres right is ten scales right; ten millimetres up (world
        // +Y) is ten scales *up the screen*, i.e. a smaller screen Y.
        assert!(approx(x1 - x0, 10.0 * scale, 0.01));
        assert!(approx(y0 - y1, 10.0 * scale, 0.01));

        // The painted point is the camera projection plus the canvas origin,
        // included once and only once.
        let projected = camera.world_to_screen(Point2::new(10.0, 10.0));
        assert!(approx(x1, origin.0 + projected.x, 0.01));
        assert!(approx(y1, origin.1 + projected.y, 0.01));

        // Off-screen markers are skipped, not clamped to an edge.
        assert!(marker_screen(&camera, bounds, Point2::new(10_000.0, 0.0)).is_none());
    }

    #[test]
    fn the_synthetic_preview_fits_without_a_program() {
        let handle = ViewerHandle::new();
        let bounds = test_bounds(800.0, 600.0);
        let camera = {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(bounds.size, None)
        };
        let viewer = handle.inner.borrow();
        assert!(viewer.fit_scale > 0.0);
        assert!(!viewer.cache.plan.is_empty());
        assert!(camera.scale() > 0.0);
    }
}
