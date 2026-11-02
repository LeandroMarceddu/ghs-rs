//! Real toolpath canvas for the Job workspace.
//!
//! The canvas draws the parsed program toolpath through `ghs-viewer` in one of
//! two modes:
//!
//! - [`ViewerMode::Top2D`] (the default) uses a [`Camera2D`] fitted to the
//!   program bounds, a cached 2D [`RenderPlan`], and a world-space grid.
//! - [`ViewerMode::ThreeD`] uses an orbit [`Camera3D`] with Z up, a cached 3D
//!   [`RenderPlan`] painted far-to-near, and a ground grid plus work-envelope
//!   box on the Z=0 plane.
//!
//! Both modes paint the live tool marker from the DRO work position. In 3D the
//! marker is projected with [`Camera3D::project`], so it is hidden when it is
//! behind the near plane or outside the canvas.
//!
//! Gestures: in 2D a drag pans, the wheel or a pinch zooms at the cursor, and a
//! double click or the FIT chip refits. In 3D a left drag orbits, a shift drag
//! or middle drag pans, and the wheel or a pinch zooms at the cursor. FIT and a
//! double click refit the active camera once; a status tick that only moves the
//! marker never rebuilds either plan.
//!
//! Only `--mock`, or a live session with no program loaded, falls back to the
//! synthetic sketch. A plan is cached by program identity, segment count, the
//! active camera transform, and viewport size.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ghs_gcode::Program;
use ghs_viewer::{
    Bounds2, Bounds3, Camera2D, Camera3D, Point2, Point3, RenderOptions, RenderPlan, SegmentKind,
    Toolpath, Viewport,
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

/// Orbit sensitivity: radians of yaw or pitch per logical drag pixel.
///
/// A full 800 px drag is a little over three radians, so the operator can turn
/// the model around without a long stroke.
const ORBIT_RAD_PER_PX: f32 = 0.007;

/// Ground-grid margin around the toolpath footprint, as a fraction of its size.
///
/// The grid is a work-plane reference, not a machine table, so it stops close
/// to the job instead of filling the whole view.
const GRID_MARGIN_FRACTION: f32 = 0.35;

/// Segment count of the synthetic preview used when no program is loaded.
const SYNTHETIC_SEGMENTS: usize = 6_000;

/// Guard against a pathological camera producing an unbounded grid loop.
const MAX_GRID_LINES: i64 = 512;

/// Which projection the viewer shows.
///
/// [`ViewerMode::Top2D`] is the default. The mode is owned by the
/// [`ViewerHandle`], so it survives status ticks and marker updates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewerMode {
    /// Orthographic top view with the world Y axis up.
    #[default]
    Top2D,
    /// Orbiting perspective view with world Z up.
    ThreeD,
}

impl ViewerMode {
    /// Short label for the toolbar chip.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Top2D => "2D TOP",
            Self::ThreeD => "3D",
        }
    }

    /// Returns `true` for the orbit view.
    #[must_use]
    pub const fn is_three_d(self) -> bool {
        matches!(self, Self::ThreeD)
    }
}

/// What an in-flight pointer drag is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragKind {
    /// 2D camera pan (top view).
    Pan2D,
    /// 3D orbit around the target.
    Orbit3D,
    /// 3D camera pan in the screen plane.
    Pan3D,
}

/// One in-flight pointer drag.
#[derive(Clone, Copy, Debug)]
struct DragState {
    at: Point<Pixels>,
    kind: DragKind,
}

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

    /// Records a primary drag start at a window-space pointer position.
    ///
    /// In 2D this pans; in 3D it orbits. Returns `false`: the gesture alone
    /// changes no pixels, so the caller does not need to redraw.
    pub fn begin_drag(&self, at: Point<Pixels>) -> bool {
        let mut viewer = self.inner.borrow_mut();
        let kind = if viewer.mode.is_three_d() {
            DragKind::Orbit3D
        } else {
            DragKind::Pan2D
        };
        viewer.drag = Some(DragState { at, kind });
        false
    }

    /// Records a pan drag start (shift drag or middle drag).
    ///
    /// In 2D this is the same pan as [`ViewerHandle::begin_drag`]; in 3D it pans
    /// in the camera's screen plane instead of orbiting.
    pub fn begin_pan_drag(&self, at: Point<Pixels>) -> bool {
        let mut viewer = self.inner.borrow_mut();
        let kind = if viewer.mode.is_three_d() {
            DragKind::Pan3D
        } else {
            DragKind::Pan2D
        };
        viewer.drag = Some(DragState { at, kind });
        false
    }

    /// Applies the pointer delta since the last position for the active drag.
    ///
    /// Returns `true` when a camera moved and the canvas needs a redraw.
    pub fn drag_to(&self, at: Point<Pixels>) -> bool {
        let mut viewer = self.inner.borrow_mut();
        let Some(last) = viewer.drag else {
            return false;
        };
        viewer.drag = Some(DragState {
            at,
            kind: last.kind,
        });
        let delta = Point2::new(f32::from(at.x - last.at.x), f32::from(at.y - last.at.y));
        if !delta.is_finite() || delta.length_squared() <= f32::EPSILON {
            return false;
        }
        match last.kind {
            DragKind::Pan2D => viewer.camera.pan(delta),
            DragKind::Orbit3D => viewer
                .camera3d
                .orbit(-delta.x * ORBIT_RAD_PER_PX, -delta.y * ORBIT_RAD_PER_PX),
            DragKind::Pan3D => viewer.camera3d.pan(delta),
        }
        true
    }

    /// Ends the current drag gesture.
    pub fn end_drag(&self) {
        self.inner.borrow_mut().drag = None;
    }

    /// Zooms around a window-space pointer position.
    ///
    /// In 3D the target is shifted so the world point under the cursor stays
    /// approximately fixed, matching the 2D pivot behaviour. Returns `true`
    /// when the active camera changed.
    pub fn zoom_at(&self, at: Point<Pixels>, factor: f32) -> bool {
        let mut viewer = self.inner.borrow_mut();
        let Some(rect) = viewer.rect else {
            return false;
        };
        let local = local_point(rect, at);
        if viewer.mode.is_three_d() {
            let viewport = viewer.camera3d.viewport();
            if viewport.is_empty() || !local.is_finite() || !factor.is_finite() || factor <= 0.0 {
                return false;
            }
            let before_distance = viewer.camera3d.distance();
            let before_target = viewer.camera3d.target();
            let units_per_pixel = viewer.camera3d.units_per_pixel();
            if units_per_pixel.is_finite() && units_per_pixel > 0.0 {
                let center = viewport.center();
                let offset_x = local.x - center.x;
                let offset_y = center.y - local.y;
                // Panning by this fraction before the zoom keeps the point in
                // the target plane under the cursor at the same screen pixel.
                let k = 1.0 - 1.0 / factor;
                viewer
                    .camera3d
                    .pan(Point2::new(-offset_x * k, offset_y * k));
            }
            viewer.camera3d.zoom(factor);
            return viewer.camera3d.distance() != before_distance
                || viewer.camera3d.target() != before_target;
        }
        let before = viewer.camera.scale();
        viewer.camera.zoom_at(local, factor);
        let after = viewer.camera.scale();
        (after - before).abs() > f32::EPSILON * before.max(1.0)
    }

    /// Refits the active camera on the next paint.
    ///
    /// Returns `true` so the caller redraws. The 2D and 3D cameras keep
    /// independent fits: FIT in one mode never disturbs the other.
    pub fn fit(&self) -> bool {
        let mut viewer = self.inner.borrow_mut();
        if viewer.mode.is_three_d() {
            viewer.pending_fit_3d = true;
        } else {
            viewer.pending_fit = true;
        }
        true
    }

    /// Switches the projection mode and refits the destination camera once.
    ///
    /// The 2D camera and plan are left untouched when the operator enters 3D,
    /// and the 3D camera is left untouched when they return to 2D. Returns
    /// `true` when the mode changed.
    pub fn set_mode(&self, mode: ViewerMode) -> bool {
        let mut viewer = self.inner.borrow_mut();
        if viewer.mode == mode {
            return false;
        }
        viewer.mode = mode;
        if mode.is_three_d() {
            // Entering 3D fits once; later polls keep the orbit.
            viewer.pending_fit_3d = true;
        }
        true
    }

    /// Current projection mode.
    #[must_use]
    pub fn mode(&self) -> ViewerMode {
        self.inner.borrow().mode
    }

    /// Current zoom relative to the fitted scale, as a whole percentage.
    #[must_use]
    pub fn zoom_percent(&self) -> u32 {
        let viewer = self.inner.borrow();
        if viewer.mode.is_three_d() {
            let fit = viewer.fit_distance3d;
            let distance = viewer.camera3d.distance();
            if fit > 0.0 && fit.is_finite() && distance > 0.0 && distance.is_finite() {
                return (fit / distance * 100.0).round().clamp(1.0, 100_000.0) as u32;
            }
            return 100;
        }
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

/// One measured height-map cell projected into the viewer's work frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeightCell {
    /// Lower-left work-coordinate corner, in millimetres.
    pub min: Point2,
    /// Upper-right work-coordinate corner, in millimetres.
    pub max: Point2,
    /// Measured machine Z, or `None` for a missing/failed cell.
    pub z_mm: Option<f64>,
}

/// Measured height map ready for the 2D canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightOverlay {
    /// One cell per grid point, including missing cells.
    pub cells: Vec<HeightCell>,
    /// Lowest measured machine Z in the map.
    pub min_z_mm: f64,
    /// Highest measured machine Z in the map.
    pub max_z_mm: f64,
    /// Grid's nominal surface, in machine millimetres.
    pub nominal_z_mm: f64,
}

/// Number of colour steps in the height-map overlay legend.
pub const HEIGHT_OVERLAY_STEPS: usize = 5;

/// One legend swatch colour, low to high.
#[must_use]
pub fn height_overlay_color(index: usize, palette: Palette) -> Hsla {
    let colors = [
        palette.info,
        palette.accent,
        palette.success,
        palette.warning,
        palette.danger,
    ];
    colors[index.min(HEIGHT_OVERLAY_STEPS - 1)]
}

/// Colour step for a measured Z within a map's range.
///
/// A flat map (or a map with one measured point) uses the middle step, so a
/// nominal surface does not look like either an error or a high spot.
#[must_use]
pub fn height_overlay_step(z_mm: f64, min_z_mm: f64, max_z_mm: f64) -> usize {
    let span = max_z_mm - min_z_mm;
    if !z_mm.is_finite() || !span.is_finite() || span.abs() <= 1.0e-9 {
        return HEIGHT_OVERLAY_STEPS / 2;
    }
    let t = ((z_mm - min_z_mm) / span).clamp(0.0, 1.0);
    (t * (HEIGHT_OVERLAY_STEPS - 1) as f64).round() as usize
}

/// Draws the interactive job canvas.
///
/// `program` is the parsed program from `JobLoaded`; `None` draws the synthetic
/// preview. `marker` is the tool position in world millimetres (X, Y, Z); it is
/// projected through whichever camera is active. `overlay` is a measured height
/// map already converted into the same work frame; it is drawn above the
/// toolpath and below the marker in both modes.
pub fn view<T: HasViewer + 'static>(
    handle: &ViewerHandle,
    palette: Palette,
    program: Option<Arc<Program>>,
    marker: Option<Point3>,
    overlay: Option<HeightOverlay>,
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
                paint(
                    bounds,
                    &paint_handle,
                    program.clone(),
                    marker,
                    overlay.clone(),
                    palette,
                    window,
                );
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
                let pan = event.modifiers.shift;
                view.update(cx, |this, cx| {
                    let viewer = this.viewer_handle();
                    let changed = if double_click {
                        viewer.fit()
                    } else if pan {
                        viewer.begin_pan_drag(at)
                    } else {
                        viewer.begin_drag(at)
                    };
                    if changed {
                        cx.notify();
                    }
                });
            }
        })
        .on_mouse_down(MouseButton::Middle, {
            let view = view.clone();
            move |event, _, cx| {
                let at = event.position;
                view.update(cx, |this, cx| {
                    if this.viewer_handle().begin_pan_drag(at) {
                        cx.notify();
                    }
                });
            }
        })
        .on_mouse_move({
            let view = view.clone();
            move |event, _, cx| {
                if !matches!(
                    event.pressed_button,
                    Some(MouseButton::Left | MouseButton::Middle)
                ) {
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
        .on_mouse_up(MouseButton::Middle, {
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
///
/// Z is included for the 3D view; the 2D painter only reads X and Y.
#[must_use]
pub fn marker_mm3(units: Units, dro: &[f64]) -> Option<Point3> {
    let x = units.to_mm(*dro.first()?);
    let y = units.to_mm(*dro.get(1)?);
    // A profile with only two reported axes still gets a marker on the work
    // plane instead of losing it entirely.
    let z = dro.get(2).map_or(0.0, |z| units.to_mm(*z));
    (x.is_finite() && y.is_finite() && z.is_finite())
        .then(|| Point3::new(x as f32, y as f32, z as f32))
}

/// Projects the DRO work position into viewer millimetres, dropping Z.
///
/// Used by the tests that only care about the top view.
#[cfg(test)]
#[must_use]
fn marker_mm(units: Units, dro: &[f64]) -> Option<Point2> {
    marker_mm3(units, dro).map(Point3::xy)
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

/// Cached 3D render plan and the camera/viewport it was built for.
#[derive(Default)]
struct PlanCache3D {
    key: Option<PlanKey3D>,
    plan: RenderPlan,
}

/// Identity of one 3D render plan.
///
/// `Camera3D::project` depends on the eye (target, yaw, pitch, distance) and
/// the viewport height; the viewport width only affects culling. Nothing here
/// changes on a status tick, so the marker can move without a rebuild.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlanKey3D {
    program: usize,
    segments: usize,
    target_x: u32,
    target_y: u32,
    target_z: u32,
    yaw: u32,
    pitch: u32,
    distance: u32,
    width: u32,
    height: u32,
}

impl PlanKey3D {
    fn new(program: usize, segments: usize, camera: &Camera3D) -> Self {
        let target = camera.target();
        let viewport = camera.viewport();
        Self {
            program,
            segments,
            target_x: target.x.to_bits(),
            target_y: target.y.to_bits(),
            target_z: target.z.to_bits(),
            yaw: camera.yaw().to_bits(),
            pitch: camera.pitch().to_bits(),
            distance: camera.distance().to_bits(),
            width: viewport.width().to_bits(),
            height: viewport.height().to_bits(),
        }
    }
}

/// Mutable viewer state shared through [`ViewerHandle`].
#[derive(Default)]
struct Viewer {
    /// Active projection; 2D is the default.
    mode: ViewerMode,
    camera: Camera2D,
    camera3d: Camera3D,
    /// World bounds of the current toolpath, for the 2D envelope outline.
    path_bounds: Bounds2,
    /// World bounds of the current toolpath, for the 3D envelope box.
    path_bounds3: Bounds3,
    /// Identity of the program the camera is fitted to; `0` is synthetic.
    identity: usize,
    /// Camera scale produced by the last 2D fit, for the zoom readout.
    fit_scale: f32,
    /// Orbit distance produced by the last 3D fit, for the zoom readout.
    fit_distance3d: f32,
    /// A resize, program change, or explicit FIT asks for a 2D refit.
    pending_fit: bool,
    /// A resize, program change, mode switch, or FIT asks for a 3D refit.
    pending_fit_3d: bool,
    /// Canvas rectangle in window coordinates, refreshed on every prepaint.
    rect: Option<Bounds<Pixels>>,
    /// In-flight drag gesture, in window coordinates.
    drag: Option<DragState>,
    /// Synthetic preview, built once and reused.
    synthetic: Option<Arc<Toolpath>>,
    cache: PlanCache,
    cache3d: PlanCache3D,
    /// Number of 2D plans built; a test seam for the caching contract.
    #[cfg(test)]
    plan_builds: u32,
    /// Number of 3D plans built; a test seam for the caching contract.
    #[cfg(test)]
    plan_builds_3d: u32,
}

impl Viewer {
    /// Updates the cameras for the current viewport and program, then ensures
    /// the active render plan matches. Returns the 2D camera used for the
    /// zoom readout and the 2D painter.
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
        if viewport != self.camera3d.viewport() {
            self.camera3d.set_viewport(viewport);
            self.pending_fit_3d = true;
        }
        if identity != self.identity {
            self.identity = identity;
            self.pending_fit = true;
            self.pending_fit_3d = true;
        }
        self.path_bounds = path.bounds().xy_bounds();
        self.path_bounds3 = path.bounds();
        if self.pending_fit && !self.path_bounds.is_empty() && !viewport.is_empty() {
            self.camera.fit(self.path_bounds, VIEW_PADDING_PX);
            self.fit_scale = self.camera.scale();
            self.pending_fit = false;
        }
        if self.pending_fit_3d && !self.path_bounds3.is_empty() && !viewport.is_empty() {
            self.camera3d.fit_bounds(self.path_bounds3, VIEW_PADDING_PX);
            self.fit_distance3d = self.camera3d.distance();
            self.pending_fit_3d = false;
        }

        if self.mode.is_three_d() {
            let key3d = PlanKey3D::new(identity, path.segment_count(), &self.camera3d);
            if self.cache3d.key != Some(key3d) {
                self.cache3d.plan =
                    RenderPlan::from_3d(path, &self.camera3d, RenderOptions::default());
                self.cache3d.key = Some(key3d);
                #[cfg(test)]
                {
                    self.plan_builds_3d += 1;
                }
            }
        } else {
            let key = PlanKey::new(identity, path.segment_count(), &self.camera);
            if self.cache.key != Some(key) {
                self.cache.plan = RenderPlan::from_2d(path, &self.camera, RenderOptions::default());
                self.cache.key = Some(key);
                #[cfg(test)]
                {
                    self.plan_builds += 1;
                }
            }
        }
        self.camera
    }
}

fn paint(
    bounds: Bounds<Pixels>,
    handle: &ViewerHandle,
    program: Option<Arc<Program>>,
    marker: Option<Point3>,
    overlay: Option<HeightOverlay>,
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

    if viewer.mode.is_three_d() {
        let camera3d = viewer.camera3d;
        paint_ground_grid_3d(&camera3d, viewer.path_bounds3, bounds, palette, window);
        if !viewer.path_bounds3.is_empty() {
            paint_envelope_3d(&camera3d, viewer.path_bounds3, bounds, palette, window);
        }
        paint_plan_3d(bounds, &viewer.cache3d.plan, palette, window);
        if let Some(overlay) = overlay.as_ref() {
            paint_height_overlay_3d(&camera3d, bounds, overlay, palette, window);
        }
        if let Some(marker) = marker {
            paint_marker_3d(&camera3d, bounds, marker, palette, window);
        }
        return;
    }

    paint_grid(&camera, bounds, palette, window);
    if !viewer.path_bounds.is_empty() {
        paint_envelope(&camera, viewer.path_bounds, bounds, palette, window);
    }
    paint_plan(bounds, &viewer.cache.plan, palette, window);
    if let Some(overlay) = overlay.as_ref() {
        paint_height_overlay(&camera, bounds, overlay, palette, window);
    }
    if let Some(marker) = marker {
        paint_marker(&camera, bounds, marker.xy(), palette, window);
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

/// Projects one world point to window coordinates, adding the canvas origin.
///
/// [`Camera3D::project`] returns viewport-local pixels; every GPUI painter in
/// this module works in window coordinates, so the canvas origin is added here
/// exactly once.
fn project_window(camera: &Camera3D, bounds: Bounds<Pixels>, world: Point3) -> Option<(f32, f32)> {
    let projected = camera.project(world)?;
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    Some((left + projected.screen.x, top + projected.screen.y))
}

/// Appends one projected world-space line to `builder`.
fn push_world_line(
    builder: &mut PathBuilder,
    camera: &Camera3D,
    bounds: Bounds<Pixels>,
    start: Point3,
    end: Point3,
) -> bool {
    let (Some((x0, y0)), Some((x1, y1))) = (
        project_window(camera, bounds, start),
        project_window(camera, bounds, end),
    ) else {
        return false;
    };
    builder.move_to(point(px(x0), px(y0)));
    builder.line_to(point(px(x1), px(y1)));
    true
}

/// Paints a subdued ground grid on the work Z=0 plane.
///
/// The grid is centred on the toolpath footprint and grows or snaps to a
/// 1/2/5-decade step chosen for the current screen scale, so it stays readable
/// at any zoom. Lines are bounded by [`MAX_GRID_LINES`] on each axis.
fn paint_ground_grid_3d(
    camera: &Camera3D,
    path: Bounds3,
    bounds: Bounds<Pixels>,
    palette: Palette,
    window: &mut Window,
) {
    // An empty program still gets a work-plane reference.
    let footprint = if path.is_empty() {
        Bounds2::new(Point2::new(-50.0, -50.0), Point2::new(50.0, 50.0))
    } else {
        path.xy_bounds()
    };
    if footprint.is_empty() {
        return;
    }

    let units_per_pixel = camera.units_per_pixel();
    let px_per_unit = if units_per_pixel.is_finite() && units_per_pixel > 0.0 {
        1.0 / units_per_pixel
    } else {
        1.0
    };
    let mut step = grid_step_mm(px_per_unit);
    let margin = (footprint.width().max(footprint.height()) * GRID_MARGIN_FRACTION).max(step * 2.0);
    let mut min_x = footprint.min().x - margin;
    let mut max_x = footprint.max().x + margin;
    let mut min_y = footprint.min().y - margin;
    let mut max_y = footprint.max().y + margin;

    // Snap to the world lattice, then grow the step until the line count is
    // bounded; the loop is capped so a pathological camera cannot spin here.
    for _ in 0..16 {
        min_x = (min_x / step).floor() * step;
        max_x = (max_x / step).ceil() * step;
        min_y = (min_y / step).floor() * step;
        max_y = (max_y / step).ceil() * step;
        let count_x = ((max_x - min_x) / step).round() as i64 + 1;
        let count_y = ((max_y - min_y) / step).round() as i64 + 1;
        if count_x <= MAX_GRID_LINES && count_y <= MAX_GRID_LINES {
            break;
        }
        step *= 2.0;
        if !step.is_finite() || step <= 0.0 {
            return;
        }
    }

    let mut minor = PathBuilder::stroke(px(1.0));
    let mut major = PathBuilder::stroke(px(1.0));
    let count_x = (((max_x - min_x) / step).round() as i64).clamp(0, MAX_GRID_LINES);
    let count_y = (((max_y - min_y) / step).round() as i64).clamp(0, MAX_GRID_LINES);
    for index in 0..=count_x {
        let x = min_x + index as f32 * step;
        let target = if ((x / step).round() as i64).rem_euclid(5) == 0 {
            &mut major
        } else {
            &mut minor
        };
        push_world_line(
            target,
            camera,
            bounds,
            Point3::new(x, min_y, 0.0),
            Point3::new(x, max_y, 0.0),
        );
    }
    for index in 0..=count_y {
        let y = min_y + index as f32 * step;
        let target = if ((y / step).round() as i64).rem_euclid(5) == 0 {
            &mut major
        } else {
            &mut minor
        };
        push_world_line(
            target,
            camera,
            bounds,
            Point3::new(min_x, y, 0.0),
            Point3::new(max_x, y, 0.0),
        );
    }
    paint_path(minor, palette.grid_minor.alpha(0.8), window);
    paint_path(major, palette.grid_major.alpha(0.9), window);

    // Work axes at the origin when the grid covers them.
    let mut axes = PathBuilder::stroke(px(1.0));
    if min_y <= 0.0 && max_y >= 0.0 {
        push_world_line(
            &mut axes,
            camera,
            bounds,
            Point3::new(min_x, 0.0, 0.0),
            Point3::new(max_x, 0.0, 0.0),
        );
    }
    if min_x <= 0.0 && max_x >= 0.0 {
        push_world_line(
            &mut axes,
            camera,
            bounds,
            Point3::new(0.0, min_y, 0.0),
            Point3::new(0.0, max_y, 0.0),
        );
    }
    paint_path(axes, palette.border_strong.alpha(0.9), window);
}

/// Paints the toolpath bounds as a dashed 3D box.
///
/// The box gives the orbit view a depth reference without hiding the path: it
/// uses the same subdued border tone as the 2D envelope.
fn paint_envelope_3d(
    camera: &Camera3D,
    world: Bounds3,
    bounds: Bounds<Pixels>,
    palette: Palette,
    window: &mut Window,
) {
    if world.is_empty() {
        return;
    }
    let min = world.min();
    let max = world.max();
    let mut outline = PathBuilder::stroke(px(1.0)).dash_array(&[px(7.0), px(5.0)]);

    for z in [min.z, max.z] {
        push_world_line(
            &mut outline,
            camera,
            bounds,
            Point3::new(min.x, min.y, z),
            Point3::new(max.x, min.y, z),
        );
        push_world_line(
            &mut outline,
            camera,
            bounds,
            Point3::new(max.x, min.y, z),
            Point3::new(max.x, max.y, z),
        );
        push_world_line(
            &mut outline,
            camera,
            bounds,
            Point3::new(max.x, max.y, z),
            Point3::new(min.x, max.y, z),
        );
        push_world_line(
            &mut outline,
            camera,
            bounds,
            Point3::new(min.x, max.y, z),
            Point3::new(min.x, min.y, z),
        );
    }
    for (x, y) in [
        (min.x, min.y),
        (max.x, min.y),
        (max.x, max.y),
        (min.x, max.y),
    ] {
        push_world_line(
            &mut outline,
            camera,
            bounds,
            Point3::new(x, y, min.z),
            Point3::new(x, y, max.z),
        );
    }
    paint_path(outline, palette.border_strong.alpha(0.8), window);
}

/// Stroke style for one motion kind.
fn path_builder_for(kind: SegmentKind) -> PathBuilder {
    match kind {
        SegmentKind::Rapid => PathBuilder::stroke(px(1.1)).dash_array(&[px(5.0), px(4.0)]),
        SegmentKind::Feed | SegmentKind::Arc => PathBuilder::stroke(px(1.6)),
        SegmentKind::Plunge => PathBuilder::stroke(px(1.9)),
        SegmentKind::Probe => PathBuilder::stroke(px(1.2)).dash_array(&[px(2.0), px(3.0)]),
    }
}

/// Colour for one motion kind.
fn path_color(kind: SegmentKind, palette: Palette) -> Hsla {
    match kind {
        SegmentKind::Rapid => palette.path_rapid,
        SegmentKind::Feed | SegmentKind::Arc => palette.path_cut,
        SegmentKind::Plunge => palette.path_plunge,
        SegmentKind::Probe => palette.info,
    }
}

/// Paints the 3D plan in the plan's far-to-near order.
///
/// Consecutive polylines of the same motion style are batched into one path,
/// which keeps the painter order intact while reducing path builds. A
/// single-point polyline is painted as a dot at its place in the order, so a
/// plunge seen end-on still reads correctly.
fn paint_plan_3d(bounds: Bounds<Pixels>, plan: &RenderPlan, palette: Palette, window: &mut Window) {
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let mut batch: Option<(SegmentKind, PathBuilder)> = None;

    for polyline in plan.polylines() {
        if polyline.points.is_empty() {
            continue;
        }
        if polyline.points.len() == 1 {
            if let Some((kind, builder)) = batch.take() {
                paint_path(builder, path_color(kind, palette), window);
            }
            let single = polyline.points[0];
            paint_dot(
                window,
                path_color(polyline.kind, palette),
                left + single.x,
                top + single.y,
                1.8,
            );
            continue;
        }
        let continues = batch
            .as_ref()
            .is_some_and(|(kind, _)| *kind == polyline.kind);
        if !continues {
            if let Some((kind, builder)) = batch.take() {
                paint_path(builder, path_color(kind, palette), window);
            }
            batch = Some((polyline.kind, path_builder_for(polyline.kind)));
        }
        if let Some((_, builder)) = batch.as_mut() {
            for (index, screen) in polyline.points.iter().enumerate() {
                let world = point(px(left + screen.x), px(top + screen.y));
                if index == 0 {
                    builder.move_to(world);
                } else {
                    builder.line_to(world);
                }
            }
        }
    }
    if let Some((kind, builder)) = batch.take() {
        paint_path(builder, path_color(kind, palette), window);
    }
}

/// Paints measured height cells in the 3D work frame.
///
/// Cells are drawn at their measured Z relative to the map's nominal surface,
/// so a nominal map sits on the Z=0 work plane. Valid cells are translucent
/// fills with an outline; missing cells are dashed outlines only. Cells are
/// sorted far-to-near and never contribute to the plan cache or fitted bounds.
fn paint_height_overlay_3d(
    camera: &Camera3D,
    bounds: Bounds<Pixels>,
    overlay: &HeightOverlay,
    palette: Palette,
    window: &mut Window,
) {
    let nominal = overlay.nominal_z_mm as f32;
    let mut cells: Vec<(f32, [Point2; 4], Option<Hsla>)> = Vec::with_capacity(overlay.cells.len());
    for cell in &overlay.cells {
        // `HeightCell::z_mm` is machine Z; the 2D overlay only uses it for
        // colour. In 3D, shift it to the nominal surface so the map lines up
        // with the work Z=0 plane rather than the machine origin.
        let z = match cell.z_mm {
            Some(z_mm) => z_mm as f32 - nominal,
            None => 0.0,
        };
        let corners = [
            Point3::new(cell.min.x, cell.min.y, z),
            Point3::new(cell.max.x, cell.min.y, z),
            Point3::new(cell.max.x, cell.max.y, z),
            Point3::new(cell.min.x, cell.max.y, z),
        ];
        let mut projected = [Point2::ZERO; 4];
        let mut depth_sum = 0.0_f32;
        let mut visible = true;
        for (slot, corner) in projected.iter_mut().zip(corners) {
            let Some(point) = camera.project(corner) else {
                visible = false;
                break;
            };
            *slot = point.screen;
            depth_sum += point.depth;
        }
        if !visible {
            continue;
        }
        let color = cell.z_mm.map(|z_mm| {
            let step = height_overlay_step(z_mm, overlay.min_z_mm, overlay.max_z_mm);
            height_overlay_color(step, palette)
        });
        cells.push((depth_sum * 0.25, projected, color));
    }
    cells.sort_by(|a, b| b.0.total_cmp(&a.0));

    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    for (_, corners, color) in cells {
        if let Some(color) = color {
            let mut fill_path = PathBuilder::fill();
            fill_path.move_to(point(px(left + corners[0].x), px(top + corners[0].y)));
            for corner in &corners[1..] {
                fill_path.line_to(point(px(left + corner.x), px(top + corner.y)));
            }
            fill_path.close();
            paint_path(fill_path, color.alpha(0.28), window);

            let mut outline = PathBuilder::stroke(px(1.0));
            outline.move_to(point(px(left + corners[0].x), px(top + corners[0].y)));
            for corner in &corners[1..] {
                outline.line_to(point(px(left + corner.x), px(top + corner.y)));
            }
            outline.close();
            paint_path(outline, color.alpha(0.85), window);

            let center_x = corners.iter().map(|point| point.x).sum::<f32>() * 0.25;
            let center_y = corners.iter().map(|point| point.y).sum::<f32>() * 0.25;
            paint_dot(window, color, left + center_x, top + center_y, 1.8);
        } else {
            let mut outline = PathBuilder::stroke(px(1.0)).dash_array(&[px(3.0), px(3.0)]);
            outline.move_to(point(px(left + corners[0].x), px(top + corners[0].y)));
            for corner in &corners[1..] {
                outline.line_to(point(px(left + corner.x), px(top + corner.y)));
            }
            outline.close();
            paint_path(outline, palette.text_faint.alpha(0.7), window);
        }
    }
}

/// Screen position of the 3D tool marker, or `None` when it is behind the near
/// plane or outside the canvas plus a small margin for the crosshair.
///
/// This is the single projection used by [`paint_marker_3d`], so the painted
/// position always includes the canvas origin exactly once.
fn marker_screen_3d(
    camera: &Camera3D,
    bounds: Bounds<Pixels>,
    world: Point3,
) -> Option<(f32, f32)> {
    if !world.is_finite() {
        return None;
    }
    let projected = camera.project(world)?;
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let right = left + f32::from(bounds.size.width);
    let bottom = top + f32::from(bounds.size.height);
    let x = left + projected.screen.x;
    let y = top + projected.screen.y;
    if x < left - 12.0 || x > right + 12.0 || y < top - 12.0 || y > bottom + 12.0 {
        return None;
    }
    Some((x, y))
}

/// Paints the live tool marker in 3D: a dropline to the work plane, a ground
/// shadow, then the marker halo, ring, centre dot, and crosshair on top.
///
/// The marker is painted after the plan, so it always reads above the path.
fn paint_marker_3d(
    camera: &Camera3D,
    bounds: Bounds<Pixels>,
    world: Point3,
    palette: Palette,
    window: &mut Window,
) {
    let Some((x, y)) = marker_screen_3d(camera, bounds, world) else {
        return;
    };

    if world.z.abs() > 1.0e-3
        && let Some((plane_x, plane_y)) =
            project_window(camera, bounds, Point3::new(world.x, world.y, 0.0))
    {
        let mut dropline = PathBuilder::stroke(px(1.0)).dash_array(&[px(3.0), px(3.0)]);
        dropline.move_to(point(px(x), px(y)));
        dropline.line_to(point(px(plane_x), px(plane_y)));
        paint_path(dropline, palette.accent.alpha(0.35), window);
        paint_dot(window, palette.accent.alpha(0.22), plane_x, plane_y, 4.0);
    }

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

/// Paints measured height cells in the work frame.
///
/// A valid cell is a translucent filled square coloured by its measured Z; a
/// missing cell is only a dashed outline, so it is visibly not a measurement.
/// The overlay is projected through the same camera as the toolpath and never
/// contributes to the plan cache or the fitted bounds.
fn paint_height_overlay(
    camera: &Camera2D,
    bounds: Bounds<Pixels>,
    overlay: &HeightOverlay,
    palette: Palette,
    window: &mut Window,
) {
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);

    for cell in &overlay.cells {
        let min = camera.world_to_screen(cell.min);
        let max = camera.world_to_screen(cell.max);
        if !min.is_finite() || !max.is_finite() {
            continue;
        }
        // Screen Y is flipped, so the world minimum is the bottom-left corner.
        let x0 = left + min.x.min(max.x);
        let x1 = left + min.x.max(max.x);
        let y0 = top + min.y.min(max.y);
        let y1 = top + min.y.max(max.y);
        if x1 < left || x0 > left + width || y1 < top || y0 > top + height {
            continue;
        }
        let cell_bounds = Bounds::new(
            point(px(x0), px(y0)),
            size(px((x1 - x0).max(1.0)), px((y1 - y0).max(1.0))),
        );
        match cell.z_mm {
            Some(z_mm) => {
                let step = height_overlay_step(z_mm, overlay.min_z_mm, overlay.max_z_mm);
                let color = height_overlay_color(step, palette);
                window.paint_quad(fill(cell_bounds, color.alpha(0.32)).corner_radii(px(1.0)));
                let mut outline = PathBuilder::stroke(px(1.0));
                outline.move_to(point(px(x0), px(y0)));
                outline.line_to(point(px(x1), px(y0)));
                outline.line_to(point(px(x1), px(y1)));
                outline.line_to(point(px(x0), px(y1)));
                outline.close();
                paint_path(outline, color.alpha(0.85), window);
                paint_dot(window, color, (x0 + x1) * 0.5, (y0 + y1) * 0.5, 1.8);
            }
            None => {
                let mut outline = PathBuilder::stroke(px(1.0)).dash_array(&[px(3.0), px(3.0)]);
                outline.move_to(point(px(x0), px(y0)));
                outline.line_to(point(px(x1), px(y0)));
                outline.line_to(point(px(x1), px(y1)));
                outline.line_to(point(px(x0), px(y1)));
                outline.close();
                paint_path(outline, palette.text_faint.alpha(0.7), window);
            }
        }
    }
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

        // A two-axis profile keeps the marker on the work plane.
        assert_eq!(
            marker_mm3(Units::Millimetre, &[3.0, 4.0]),
            Some(Point3::new(3.0, 4.0, 0.0))
        );

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

    // --------------------------------------------------------------- 3D view

    fn corners(bounds: Bounds3) -> Vec<Point3> {
        let min = bounds.min();
        let max = bounds.max();
        let mut out = Vec::with_capacity(8);
        for x in [min.x, max.x] {
            for y in [min.y, max.y] {
                for z in [min.z, max.z] {
                    out.push(Point3::new(x, y, z));
                }
            }
        }
        out
    }

    #[test]
    fn mode_switch_keeps_the_top_camera_and_plan_valid() {
        let handle = ViewerHandle::new();
        let program = program("G21 G90\nG0 X0 Y0 Z5\nG1 Z-2 F200\nG1 X40 F800\nG1 Y20\n");
        let bounds = test_bounds(800.0, 600.0);
        let (camera_before, key_before, builds_before) = {
            let mut viewer = handle.inner.borrow_mut();
            let camera = viewer.prepare(bounds.size, Some(&program));
            (camera, viewer.cache.key, viewer.plan_builds)
        };
        assert!(builds_before > 0);

        assert!(handle.set_mode(ViewerMode::ThreeD));
        {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(bounds.size, Some(&program));
            viewer.camera3d.orbit(0.5, 0.2);
            viewer.prepare(bounds.size, Some(&program));
            // A 3D orbit must not disturb the 2D camera or force a 2D rebuild.
            assert_eq!(viewer.camera, camera_before);
            assert_eq!(viewer.cache.key, key_before);
            assert_eq!(viewer.plan_builds, builds_before);
        }

        assert!(handle.set_mode(ViewerMode::Top2D));
        {
            let mut viewer = handle.inner.borrow_mut();
            let camera = viewer.prepare(bounds.size, Some(&program));
            assert_eq!(camera, camera_before);
            assert_eq!(viewer.cache.key, key_before);
            assert_eq!(viewer.plan_builds, builds_before);
        }
    }

    #[test]
    fn three_d_fit_contains_the_toolpath_bounds() {
        let handle = ViewerHandle::new();
        assert_eq!(handle.mode(), ViewerMode::Top2D);
        assert!(handle.set_mode(ViewerMode::ThreeD));
        let program = program("G21 G90\nG0 X0 Y0 Z5\nG1 Z-2 F200\nG1 X40 F800\nG1 Y20\nG0 Z5\n");
        let bounds = test_bounds(800.0, 600.0);
        let (camera, world) = {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(bounds.size, Some(&program));
            (viewer.camera3d, viewer.path_bounds3)
        };
        assert!(!world.is_empty());
        assert!(world.contains_bounds(&program.toolpath.bounds()));
        for corner in corners(world) {
            let projected = camera.project(corner).expect("fitted corner is visible");
            assert!(
                projected.screen.x >= -0.01
                    && projected.screen.x <= f32::from(bounds.size.width)
                    && projected.screen.y >= -0.01
                    && projected.screen.y <= f32::from(bounds.size.height),
                "corner projected outside the canvas: {:?}",
                projected.screen
            );
        }
        assert_eq!(handle.zoom_percent(), 100);
    }

    #[test]
    fn three_d_gestures_move_only_the_three_d_camera() {
        let handle = ViewerHandle::new();
        let program = program("G0 X0 Y0 Z5\nG1 Z-2 F200\nG1 X40 F800\nG1 Y20\n");
        let bounds = test_bounds(800.0, 600.0);
        handle.inner.borrow_mut().rect = Some(Bounds::new(point(px(100.0), px(50.0)), bounds.size));
        assert!(handle.set_mode(ViewerMode::ThreeD));
        let (camera2d, fitted3d) = {
            let mut viewer = handle.inner.borrow_mut();
            let camera2d = viewer.prepare(bounds.size, Some(&program));
            (camera2d, viewer.camera3d)
        };

        assert!(!handle.begin_drag(point(px(500.0), px(350.0))));
        assert!(handle.drag_to(point(px(540.0), px(330.0))));
        handle.end_drag();
        let orbited = handle.inner.borrow().camera3d;
        assert!((orbited.yaw() - fitted3d.yaw()).abs() > 1.0e-4);
        assert!((orbited.pitch() - fitted3d.pitch()).abs() > 1.0e-4);

        assert!(!handle.begin_pan_drag(point(px(500.0), px(350.0))));
        assert!(handle.drag_to(point(px(520.0), px(370.0))));
        handle.end_drag();
        let panned = handle.inner.borrow().camera3d;
        assert!(panned.target() != orbited.target());

        assert!(handle.zoom_at(point(px(400.0), px(300.0)), 1.5));
        let zoomed = handle.inner.borrow().camera3d;
        assert!(zoomed.distance() < panned.distance());
        assert!(handle.zoom_percent() > 100);

        // Neither gesture touched the top view.
        assert_eq!(handle.inner.borrow().camera, camera2d);
    }

    #[test]
    fn marker_projects_in_three_d_and_hides_off_screen() {
        let world = Bounds3::new(Point3::new(0.0, 0.0, -5.0), Point3::new(50.0, 40.0, 5.0));
        let camera = Camera3D::fitted(Viewport::new(800.0, 600.0), world, VIEW_PADDING_PX);
        let origin = (120.0, 60.0);
        let bounds = Bounds::new(
            point(px(origin.0), px(origin.1)),
            size(px(800.0), px(600.0)),
        );

        let (x, y) = marker_screen_3d(&camera, bounds, camera.target())
            .expect("the orbit target projects to the viewport center");
        assert!(approx(x, origin.0 + 400.0, 0.01));
        assert!(approx(y, origin.1 + 300.0, 0.01));

        // Off-screen and behind-the-near-plane markers are skipped, not
        // clamped to an edge.
        assert!(marker_screen_3d(&camera, bounds, Point3::new(10_000.0, 10_000.0, 0.0)).is_none());
        assert!(marker_screen_3d(&camera, bounds, camera.eye()).is_none());
        assert!(
            marker_screen_3d(&camera, bounds, camera.eye() - camera.forward() * 10.0).is_none()
        );
    }

    #[test]
    fn three_d_plan_cache_ignores_marker_ticks_but_tracks_the_camera() {
        let handle = ViewerHandle::new();
        assert!(handle.set_mode(ViewerMode::ThreeD));
        let program = program("G0 X0 Y0 Z5\nG1 Z-2 F200\nG1 X40 F800\nG1 Y20\nG0 Z5\n");
        let bounds = test_bounds(800.0, 600.0);
        {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(bounds.size, Some(&program));
            assert_eq!(viewer.plan_builds_3d, 1);
            assert!(!viewer.cache3d.plan.is_empty());
            assert_eq!(viewer.plan_builds, 0, "3D mode built the 2D plan");

            // The same camera with a new marker is a marker-only status tick.
            viewer.prepare(bounds.size, Some(&program));
            viewer.prepare(bounds.size, Some(&program));
            assert_eq!(viewer.plan_builds_3d, 1, "a static tick rebuilt the plan");

            viewer.camera3d.orbit(0.2, 0.1);
            viewer.prepare(bounds.size, Some(&program));
            assert_eq!(
                viewer.plan_builds_3d, 2,
                "an orbit did not rebuild the plan"
            );

            viewer.camera3d.zoom(2.0);
            viewer.prepare(bounds.size, Some(&program));
            assert_eq!(viewer.plan_builds_3d, 3, "a zoom did not rebuild the plan");
        }
        assert!(handle.set_mode(ViewerMode::Top2D));
        {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(bounds.size, Some(&program));
            assert_eq!(
                viewer.plan_builds, 1,
                "the 2D plan was not built after the switch"
            );
        }
    }

    #[test]
    fn three_d_plan_is_ordered_far_to_near() {
        let handle = ViewerHandle::new();
        assert!(handle.set_mode(ViewerMode::ThreeD));
        let program = program(
            "G0 X0 Y0 Z5\nG1 Z-2 F200\nG1 X0 Y40 F800\nG1 X40\nG1 Y0\n\
             G0 Z5\nG0 X80 Y40\nG1 Z-2 F200\nG1 X40 Y0 F800\n",
        );
        let bounds = test_bounds(800.0, 600.0);
        let mut viewer = handle.inner.borrow_mut();
        viewer.prepare(bounds.size, Some(&program));
        let plan = &viewer.cache3d.plan;
        assert!(plan.len() > 2, "expected several depth-separated polylines");
        assert!(
            plan.polylines()
                .windows(2)
                .all(|pair| pair[0].depth >= pair[1].depth),
            "polylines are not sorted far-to-near"
        );
    }

    #[test]
    fn tiny_degenerate_and_empty_paths_do_not_panic_in_three_d() {
        for source in [
            "(empty program)\n",
            "G0 X5 Y5 Z0\n",
            "G1 X5.0001 Y5.0001 Z0 F1\n",
        ] {
            let handle = ViewerHandle::new();
            handle.inner.borrow_mut().rect = Some(test_bounds(800.0, 600.0));
            assert!(handle.set_mode(ViewerMode::ThreeD));
            let program = program(source);
            {
                let mut viewer = handle.inner.borrow_mut();
                viewer.prepare(test_bounds(800.0, 600.0).size, Some(&program));
                // A zero-sized viewport must not poison the camera either.
                viewer.prepare(test_bounds(0.0, 0.0).size, Some(&program));
                viewer.prepare(test_bounds(800.0, 600.0).size, Some(&program));
                assert!(viewer.camera3d.eye().is_finite());
                assert!(viewer.camera3d.target().is_finite());
                assert!(viewer.cache3d.plan.point_count() < 16);
                assert!(viewer.fit_distance3d.is_finite());
            }
            assert!(handle.zoom_percent() >= 1);
            assert!(handle.fit());
            let _ = handle.zoom_at(point(px(400.0), px(300.0)), 1.25);
            let _ = handle.set_mode(ViewerMode::Top2D);
        }

        let handle = ViewerHandle::new();
        assert!(handle.set_mode(ViewerMode::ThreeD));
        {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(test_bounds(800.0, 600.0).size, None);
            assert!(viewer.camera3d.project(viewer.camera3d.target()).is_some());
            assert!(!viewer.cache3d.plan.is_empty());
        }
    }

    #[test]
    fn zoom_readout_is_relative_to_the_fitted_scale_in_both_modes() {
        let handle = ViewerHandle::new();
        let program = program("G0 X0 Y0 Z5\nG1 Z-2 F200\nG1 X40 F800\nG1 Y20\n");
        let bounds = test_bounds(800.0, 600.0);
        {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(bounds.size, Some(&program));
        }
        assert_eq!(handle.zoom_percent(), 100);

        handle.inner.borrow_mut().rect = Some(test_bounds(800.0, 600.0));
        assert!(handle.zoom_at(point(px(400.0), px(300.0)), 2.0));
        let zoomed_2d = handle.zoom_percent();
        assert_eq!(zoomed_2d, 200);

        assert!(handle.set_mode(ViewerMode::ThreeD));
        {
            let mut viewer = handle.inner.borrow_mut();
            viewer.prepare(bounds.size, Some(&program));
        }
        assert_eq!(handle.zoom_percent(), 100);
        assert!(handle.zoom_at(point(px(400.0), px(300.0)), 2.0));
        let zoomed_3d = handle.zoom_percent();
        assert_eq!(zoomed_3d, 200);

        // Switching back keeps the 2D zoom, and its readout.
        assert!(handle.set_mode(ViewerMode::Top2D));
        assert_eq!(handle.zoom_percent(), zoomed_2d);
    }
}
