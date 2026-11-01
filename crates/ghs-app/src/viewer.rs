//! Synthetic toolpath canvas for the Job workspace.
//!
//! This is a rendering spike, not a geometry pipeline: the shape is a fixed
//! normalized sketch drawn with GPUI's `canvas` and `PathBuilder`. The real
//! viewer (`ghs-viewer`, M4) will replace it with a flattened segment stream,
//! pan/zoom, hit testing, and a live marker driven by engine status.

use gpui_kit::{
    Bounds, Hsla, IntoElement, PathBuilder, Pixels, Styled as _, Window, canvas, point, px,
};

use crate::theme::Palette;

/// Draw the mock toolpath. The returned canvas fills its parent.
pub fn toolpath(palette: Palette) -> impl IntoElement {
    canvas(
        move |bounds, _, _| bounds,
        move |bounds, _, window, _| paint_toolpath(bounds, palette, window),
    )
    .size_full()
}

fn paint_toolpath(bounds: Bounds<Pixels>, palette: Palette, window: &mut Window) {
    let pad = 30.0;
    let left = bounds.left().as_f32() + pad;
    let top = bounds.top().as_f32() + pad;
    let right = (bounds.right().as_f32() - pad).max(left + 1.0);
    let bottom = (bounds.bottom().as_f32() - pad).max(top + 1.0);
    let width = right - left;
    let height = bottom - top;

    if width < 40.0 || height < 40.0 {
        return;
    }

    paint_grid(left, top, right, bottom, palette, window);

    let map = |nx: f32, ny: f32| point(px(left + nx * width), px(top + ny * height));

    // Work envelope / stock outline.
    let mut stock = PathBuilder::stroke(px(1.2)).dash_array(&[px(7.0), px(5.0)]);
    rounded_rect_px(
        &mut stock,
        left + 0.08 * width,
        top + 0.12 * height,
        0.84 * width,
        0.76 * height,
        0.02 * width.min(height),
    );
    paint_path(stock, palette.border_strong, window);

    // Rapids into the cut.
    let mut rapid = PathBuilder::stroke(px(1.1)).dash_array(&[px(5.0), px(4.0)]);
    rapid.move_to(map(0.04, 0.92));
    rapid.line_to(map(0.22, 0.70));
    rapid.move_to(map(0.78, 0.24));
    rapid.line_to(map(0.58, 0.42));
    paint_path(rapid, palette.path_rapid, window);

    // Outer pocket contour: rounded rectangle, two passes.
    for inset in [0.0_f32, 0.012] {
        let mut cut = PathBuilder::stroke(px(1.8));
        rounded_rect_norm(&mut cut, &map, 0.18 + inset, 0.22 + inset, 0.64, 0.56, 0.07);
        paint_path(cut, palette.path_cut, window);
    }

    // Inner island contour.
    let mut island = PathBuilder::stroke(px(1.8));
    rounded_rect_norm(&mut island, &map, 0.34, 0.36, 0.32, 0.28, 0.05);
    paint_path(island, palette.path_cut, window);

    // Circular boss, drawn as a sampled polyline so the spike does not depend
    // on arc tessellation details.
    let mut boss = PathBuilder::stroke(px(1.8));
    circle_norm(&mut boss, &map, 0.505, 0.50, 0.075);
    paint_path(boss, palette.path_cut, window);

    // Plunge and lead-in.
    let mut plunge = PathBuilder::stroke(px(2.0));
    plunge.move_to(map(0.50, 0.50));
    plunge.line_to(map(0.505, 0.575));
    paint_path(plunge, palette.path_plunge, window);

    // Origin arrows.
    let mut axes = PathBuilder::stroke(px(1.4));
    axes.move_to(map(0.03, 0.94));
    axes.line_to(map(0.14, 0.94));
    axes.move_to(map(0.12, 0.925));
    axes.line_to(map(0.14, 0.94));
    axes.line_to(map(0.12, 0.955));
    axes.move_to(map(0.03, 0.94));
    axes.line_to(map(0.03, 0.84));
    axes.move_to(map(0.015, 0.86));
    axes.line_to(map(0.03, 0.84));
    axes.line_to(map(0.045, 0.86));
    paint_path(axes, palette.text_faint, window);

    // Tool marker: filled dot, bright ring, crosshair.
    let mut marker = PathBuilder::fill();
    circle_norm(&mut marker, &map, 0.68, 0.40, 0.012);
    paint_path(marker, palette.accent, window);

    let mut ring = PathBuilder::stroke(px(1.6));
    circle_norm(&mut ring, &map, 0.68, 0.40, 0.022);
    paint_path(ring, palette.accent_hover, window);

    let mut cross = PathBuilder::stroke(px(1.0));
    cross.move_to(map(0.645, 0.40));
    cross.line_to(map(0.715, 0.40));
    cross.move_to(map(0.68, 0.365));
    cross.line_to(map(0.68, 0.435));
    paint_path(cross, palette.text, window);
}

fn paint_grid(left: f32, top: f32, right: f32, bottom: f32, palette: Palette, window: &mut Window) {
    let step = 24.0;
    let mut minor = PathBuilder::stroke(px(1.0));
    let mut major = PathBuilder::stroke(px(1.0));

    let mut index = 0;
    let mut x = left;
    while x <= right {
        let target = if index % 5 == 0 {
            &mut major
        } else {
            &mut minor
        };
        target.move_to(point(px(x), px(top)));
        target.line_to(point(px(x), px(bottom)));
        x += step;
        index += 1;
    }

    index = 0;
    let mut y = top;
    while y <= bottom {
        let target = if index % 5 == 0 {
            &mut major
        } else {
            &mut minor
        };
        target.move_to(point(px(left), px(y)));
        target.line_to(point(px(right), px(y)));
        y += step;
        index += 1;
    }

    paint_path(minor, palette.grid_minor, window);
    paint_path(major, palette.grid_major, window);
}

fn rounded_rect_norm(
    builder: &mut PathBuilder,
    map: &impl Fn(f32, f32) -> gpui_kit::Point<Pixels>,
    nx: f32,
    ny: f32,
    nw: f32,
    nh: f32,
    radius: f32,
) {
    let x0 = nx;
    let y0 = ny;
    let x1 = nx + nw;
    let y1 = ny + nh;
    let r = radius.min(nw * 0.5).min(nh * 0.5);

    builder.move_to(map(x0 + r, y0));
    builder.line_to(map(x1 - r, y0));
    builder.curve_to(map(x1, y0 + r), map(x1, y0));
    builder.line_to(map(x1, y1 - r));
    builder.curve_to(map(x1 - r, y1), map(x1, y1));
    builder.line_to(map(x0 + r, y1));
    builder.curve_to(map(x0, y1 - r), map(x0, y1));
    builder.line_to(map(x0, y0 + r));
    builder.curve_to(map(x0 + r, y0), map(x0, y0));
    builder.close();
}

fn circle_norm(
    builder: &mut PathBuilder,
    map: &impl Fn(f32, f32) -> gpui_kit::Point<Pixels>,
    cx: f32,
    cy: f32,
    radius: f32,
) {
    const STEPS: usize = 28;
    for i in 0..STEPS {
        let angle = std::f32::consts::TAU * (i as f32) / (STEPS as f32);
        let p = map(cx + angle.cos() * radius, cy + angle.sin() * radius);
        if i == 0 {
            builder.move_to(p);
        } else {
            builder.line_to(p);
        }
    }
    builder.close();
}

fn rounded_rect_px(builder: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32, r: f32) {
    builder.move_to(point(px(x + r), px(y)));
    builder.line_to(point(px(x + w - r), px(y)));
    builder.curve_to(point(px(x + w), px(y + r)), point(px(x + w), px(y)));
    builder.line_to(point(px(x + w), px(y + h - r)));
    builder.curve_to(point(px(x + w - r), px(y + h)), point(px(x + w), px(y + h)));
    builder.line_to(point(px(x + r), px(y + h)));
    builder.curve_to(point(px(x), px(y + h - r)), point(px(x), px(y + h)));
    builder.line_to(point(px(x), px(y + r)));
    builder.curve_to(point(px(x + r), px(y)), point(px(x), px(y)));
    builder.close();
}

fn paint_path(builder: PathBuilder, color: Hsla, window: &mut Window) {
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}
