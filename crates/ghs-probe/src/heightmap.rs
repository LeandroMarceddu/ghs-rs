//! Height-map grid, probing plan, and bilinear result matrix.

use ghs_core::{Axis, Position, ProbeResult};
use ghs_viewer::{Bounds2, Point2};

use crate::ProbePlan;
use crate::error::{ProbeError, ValidationError};
use crate::gcode::GcodeBuilder;
use crate::params::{ProbeParams, XyPoint, check_coordinate, check_finite, check_range};

/// Largest number of height-map points in one grid.
pub const MAX_HEIGHT_MAP_POINTS: usize = 4_096;

/// Largest number of columns or rows in a height map.
pub const MAX_HEIGHT_MAP_AXIS_POINTS: u16 = 128;

/// Estimated fixed overhead per height-map point, in seconds.
///
/// Covers the relative retract, the move to safe Z, the rapid to the next point
/// and the descent to the search start. It is an estimate for the preflight
/// sheet, not a controller timing model.
pub const HEIGHT_MAP_POINT_OVERHEAD_SECONDS: f64 = 3.0;

const MAX_STEP_MM: f64 = 1_000.0;

/// A rectangular height-map grid.
#[derive(Debug, Clone, PartialEq)]
pub struct HeightMapGrid {
    /// First point, in machine millimetres.
    pub origin_mm: XyPoint,
    /// Column spacing, in millimetres.
    pub step_x_mm: f64,
    /// Row spacing, in millimetres.
    pub step_y_mm: f64,
    /// Number of columns, at least two.
    pub columns: u16,
    /// Number of rows, at least two.
    pub rows: u16,
    /// Lowest expected surface Z, in machine millimetres, used as every search
    /// target.
    ///
    /// Height-map probes travel from [`ProbeParams::safe_z_mm`] down to this
    /// target, so the rapid plane is above every point regardless of local
    /// height. Set [`ProbeParams::safe_z_mm`] above the highest point on the
    /// grid and keep `safe_z - surface_z` within
    /// [`ProbeParams::search_distance_mm`].
    pub surface_z_mm: f64,
}

impl HeightMapGrid {
    /// Number of points in the grid.
    #[must_use]
    pub fn point_count(&self) -> usize {
        usize::from(self.columns) * usize::from(self.rows)
    }

    /// Validate the grid bounds and size.
    pub fn validate(&self) -> Result<(), ValidationError> {
        check_coordinate("origin X", self.origin_mm.x_mm)?;
        check_coordinate("origin Y", self.origin_mm.y_mm)?;
        check_range("step_x_mm", self.step_x_mm, 0.0, MAX_STEP_MM)?;
        check_range("step_y_mm", self.step_y_mm, 0.0, MAX_STEP_MM)?;
        if self.step_x_mm <= 0.0 {
            return Err(ValidationError::NotPositive {
                field: "step_x_mm",
                value: self.step_x_mm,
            });
        }
        if self.step_y_mm <= 0.0 {
            return Err(ValidationError::NotPositive {
                field: "step_y_mm",
                value: self.step_y_mm,
            });
        }
        if !(2..=MAX_HEIGHT_MAP_AXIS_POINTS).contains(&self.columns) {
            return Err(ValidationError::OutOfRange {
                field: "columns",
                value: f64::from(self.columns),
                min: 2.0,
                max: f64::from(MAX_HEIGHT_MAP_AXIS_POINTS),
            });
        }
        if !(2..=MAX_HEIGHT_MAP_AXIS_POINTS).contains(&self.rows) {
            return Err(ValidationError::OutOfRange {
                field: "rows",
                value: f64::from(self.rows),
                min: 2.0,
                max: f64::from(MAX_HEIGHT_MAP_AXIS_POINTS),
            });
        }
        if self.point_count() > MAX_HEIGHT_MAP_POINTS {
            return Err(ValidationError::GridTooLarge {
                columns: u32::from(self.columns),
                rows: u32::from(self.rows),
                max_points: MAX_HEIGHT_MAP_POINTS,
            });
        }
        check_coordinate("surface_z_mm", self.surface_z_mm)?;
        let span_x = self.step_x_mm * f64::from(self.columns - 1);
        let span_y = self.step_y_mm * f64::from(self.rows - 1);
        check_coordinate("grid X span", span_x)?;
        check_coordinate("grid Y span", span_y)?;
        check_coordinate("grid maximum X", self.origin_mm.x_mm + span_x)?;
        check_coordinate("grid maximum Y", self.origin_mm.y_mm + span_y)?;
        Ok(())
    }

    /// Validate against shared probe parameters and the safe Z plane.
    ///
    /// The whole grid shares one vertical search: every point starts at safe Z
    /// and targets `surface_z_mm`. The travel `safe_z - surface_z` must be
    /// positive and no longer than `search_distance_mm`.
    pub fn validate_with_params(&self, params: &ProbeParams) -> Result<(), ValidationError> {
        params.validate()?;
        self.validate()?;
        let travel = params.safe_z_mm - self.surface_z_mm;
        check_finite("height-map travel", travel)?;
        if travel <= 0.0 {
            return Err(ValidationError::NotPositive {
                field: "height-map travel",
                value: travel,
            });
        }
        if travel > params.search_distance_mm {
            return Err(ValidationError::HeightMapTravel {
                travel_mm: travel,
                search_mm: params.search_distance_mm,
            });
        }
        Ok(())
    }

    /// Grid point for a column and row, or `None` when out of bounds.
    #[must_use]
    pub fn point(&self, column: u16, row: u16) -> Option<XyPoint> {
        if column >= self.columns || row >= self.rows {
            return None;
        }
        Some(XyPoint::new(
            self.origin_mm.x_mm + self.step_x_mm * f64::from(column),
            self.origin_mm.y_mm + self.step_y_mm * f64::from(row),
        ))
    }

    /// Linear index for a column and row.
    #[must_use]
    pub fn index(&self, column: u16, row: u16) -> Option<usize> {
        if column >= self.columns || row >= self.rows {
            return None;
        }
        Some(usize::from(row) * usize::from(self.columns) + usize::from(column))
    }

    /// Inclusive XY bounds of the grid.
    #[must_use]
    pub fn bounds(&self) -> Bounds2 {
        if self.columns == 0 || self.rows == 0 {
            return Bounds2::empty();
        }
        let max = XyPoint::new(
            self.origin_mm.x_mm + self.step_x_mm * f64::from(self.columns - 1),
            self.origin_mm.y_mm + self.step_y_mm * f64::from(self.rows - 1),
        );
        Bounds2::new(self.origin_mm.to_viewer(), max.to_viewer())
    }

    /// Returns `true` when the XY point is inside or on the grid.
    #[must_use]
    pub fn contains_xy(&self, x_mm: f64, y_mm: f64) -> bool {
        if !x_mm.is_finite() || !y_mm.is_finite() {
            return false;
        }
        let Some(max) = self.point(self.columns.saturating_sub(1), self.rows.saturating_sub(1))
        else {
            return false;
        };
        x_mm >= self.origin_mm.x_mm
            && x_mm <= max.x_mm
            && y_mm >= self.origin_mm.y_mm
            && y_mm <= max.y_mm
    }

    /// Build the complete height-map probing program.
    ///
    /// Points are visited row by row, X fastest. Every point retracts to safe Z
    /// (the rapid plane above the whole grid), moves in XY at safe Z, probes
    /// down with `G38.2` to `surface_z_mm`, then retracts and returns to safe Z.
    /// No rapid descends toward the work, so a locally higher surface is met by
    /// the probe feed, not by a G0. A missed point raises a controller alarm
    /// and stops the job, so later points never run on a bad probe.
    pub fn plan(&self, params: &ProbeParams) -> Result<ProbePlan, ProbeError> {
        self.validate_with_params(params)?;
        let mut builder = GcodeBuilder::new(params);
        for row in 0..self.rows {
            for column in 0..self.columns {
                let point = self
                    .point(column, row)
                    .ok_or(ProbeError::Degenerate("height-map grid point"))?;
                builder.rapid_xy(point.x_mm, point.y_mm)?;
                let from = Position::xyz(point.x_mm, point.y_mm, params.safe_z_mm);
                let to = Position::xyz(point.x_mm, point.y_mm, self.surface_z_mm);
                builder.probe(format!("p{row}-c{column}"), from, to)?;
                builder.retract()?;
            }
        }
        builder.finish(crate::CycleKind::HeightMap, "height-map")
    }

    /// Estimate probing time for the preflight sheet.
    pub fn estimate(&self, params: &ProbeParams) -> Result<HeightMapEstimate, ProbeError> {
        self.validate_with_params(params)?;
        let points = self.point_count();
        let travel = params.safe_z_mm - self.surface_z_mm;
        let probe_seconds = travel / params.feed_mm_min * 60.0;
        let probing_seconds = probe_seconds * points as f64;
        let overhead_seconds = HEIGHT_MAP_POINT_OVERHEAD_SECONDS * points as f64;
        let total_seconds = probing_seconds + overhead_seconds;
        for (field, value) in [
            ("probing_seconds", probing_seconds),
            ("overhead_seconds", overhead_seconds),
            ("total_seconds", total_seconds),
        ] {
            check_finite(field, value)?;
        }
        Ok(HeightMapEstimate {
            points,
            probing_distance_mm: travel * points as f64,
            probing_seconds,
            overhead_seconds,
            total_seconds,
        })
    }
}

/// Estimated height-map duration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightMapEstimate {
    /// Number of points.
    pub points: usize,
    /// Total probing travel, in millimetres.
    pub probing_distance_mm: f64,
    /// Probing travel time, in seconds.
    pub probing_seconds: f64,
    /// Fixed per-point overhead, in seconds.
    pub overhead_seconds: f64,
    /// Estimated total, in seconds.
    pub total_seconds: f64,
}

/// Measured height grid with per-point validity.
///
/// Heights are machine Z coordinates from `PRB`. `None` means the point was
/// not probed, the probe failed, or the reported position was not finite.
/// Bilinear interpolation never bridges a missing point: a cell with a missing
/// corner returns `None`. The grid origin and the resulting bounds are also
/// machine XY.
#[derive(Debug, Clone, PartialEq)]
pub struct HeightMap {
    grid: HeightMapGrid,
    samples: Vec<Option<f64>>,
}

impl HeightMap {
    /// Build an empty, all-invalid map.
    pub fn new(grid: HeightMapGrid) -> Result<Self, ProbeError> {
        grid.validate()?;
        let samples = vec![None; grid.point_count()];
        Ok(HeightMap { grid, samples })
    }

    /// Build a map from explicit samples.
    pub fn from_samples(
        grid: HeightMapGrid,
        samples: Vec<Option<f64>>,
    ) -> Result<Self, ProbeError> {
        grid.validate()?;
        if samples.len() != grid.point_count() {
            return Err(ProbeError::ResultCount {
                expected: grid.point_count(),
                actual: samples.len(),
            });
        }
        for sample in samples.iter().flatten() {
            check_finite("height sample", *sample)?;
        }
        Ok(HeightMap { grid, samples })
    }

    /// Build a map from ordered probe results.
    ///
    /// A shorter result list is accepted as a partial map, for example when the
    /// operator stopped the cycle. Failed and non-finite results become invalid
    /// cells. Extra results are rejected.
    pub fn from_results(grid: HeightMapGrid, results: &[ProbeResult]) -> Result<Self, ProbeError> {
        grid.validate()?;
        let count = grid.point_count();
        if results.len() > count {
            return Err(ProbeError::ResultCount {
                expected: count,
                actual: results.len(),
            });
        }
        let mut map = HeightMap::new(grid)?;
        for (index, result) in results.iter().enumerate() {
            if result.success
                && result.position.is_finite()
                && result.position.axes().contains(Axis::Z)
            {
                map.samples[index] = Some(result.position.get(Axis::Z));
            }
        }
        Ok(map)
    }

    /// Grid definition.
    #[must_use]
    pub fn grid(&self) -> &HeightMapGrid {
        &self.grid
    }

    /// Every sample in row-major order; `None` is invalid.
    #[must_use]
    pub fn samples(&self) -> &[Option<f64>] {
        &self.samples
    }

    /// Number of cells.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Always `false`; present for the standard collection API.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Sample at a grid point.
    #[must_use]
    pub fn z(&self, column: u16, row: u16) -> Option<f64> {
        let index = self.grid.index(column, row)?;
        self.samples.get(index).copied().flatten()
    }

    /// Set or invalidate one cell.
    pub fn set_z(&mut self, column: u16, row: u16, value: Option<f64>) -> Result<(), ProbeError> {
        let index = self
            .grid
            .index(column, row)
            .ok_or(ProbeError::Degenerate("height-map index"))?;
        if let Some(height) = value {
            check_finite("height sample", height)?;
        }
        self.samples[index] = value;
        Ok(())
    }

    /// Invalidate one cell.
    pub fn invalidate(&mut self, column: u16, row: u16) -> Result<(), ProbeError> {
        self.set_z(column, row, None)
    }

    /// Number of valid cells.
    #[must_use]
    pub fn valid_count(&self) -> usize {
        self.samples.iter().flatten().count()
    }

    /// Number of invalid cells.
    #[must_use]
    pub fn missing_count(&self) -> usize {
        self.len() - self.valid_count()
    }

    /// Returns `true` when every cell is valid.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.missing_count() == 0
    }

    /// Inclusive XY bounds of the grid, in machine coordinates.
    #[must_use]
    pub fn bounds(&self) -> Bounds2 {
        self.grid.bounds()
    }

    /// Returns `true` when the XY point is inside or on the grid.
    #[must_use]
    pub fn contains(&self, x_mm: f64, y_mm: f64) -> bool {
        self.grid.contains_xy(x_mm, y_mm)
    }

    /// Bilinearly interpolated height.
    ///
    /// Returns `None` outside the grid or when a required sample is invalid.
    /// On a grid line only the samples that actually contribute are required.
    #[must_use]
    pub fn height_at(&self, x_mm: f64, y_mm: f64) -> Option<f64> {
        if !self.grid.contains_xy(x_mm, y_mm) {
            return None;
        }
        let grid = &self.grid;
        let max_column = f64::from(grid.columns.saturating_sub(1));
        let max_row = f64::from(grid.rows.saturating_sub(1));
        let fx = ((x_mm - grid.origin_mm.x_mm) / grid.step_x_mm).clamp(0.0, max_column);
        let fy = ((y_mm - grid.origin_mm.y_mm) / grid.step_y_mm).clamp(0.0, max_row);
        let column0 = fx.floor();
        let row0 = fy.floor();
        let column0 = column0 as u16;
        let row0 = row0 as u16;
        let column1 = (column0 + 1).min(grid.columns.saturating_sub(1));
        let row1 = (row0 + 1).min(grid.rows.saturating_sub(1));
        let tx = fx - f64::from(column0);
        let ty = fy - f64::from(row0);
        let epsilon = 1e-12;

        let z00 = self.z(column0, row0)?;
        if tx.abs() <= epsilon && ty.abs() <= epsilon {
            return Some(z00);
        }
        if tx.abs() <= epsilon {
            let z01 = self.z(column0, row1)?;
            return Some(z00 * (1.0 - ty) + z01 * ty);
        }
        if ty.abs() <= epsilon {
            let z10 = self.z(column1, row0)?;
            return Some(z00 * (1.0 - tx) + z10 * tx);
        }
        let z10 = self.z(column1, row0)?;
        let z01 = self.z(column0, row1)?;
        let z11 = self.z(column1, row1)?;
        let bottom = z00 * (1.0 - tx) + z10 * tx;
        let top = z01 * (1.0 - tx) + z11 * tx;
        Some(bottom * (1.0 - ty) + top * ty)
    }

    /// Z compensation at an XY point: measured surface minus nominal surface.
    #[must_use]
    pub fn compensation_at(&self, x_mm: f64, y_mm: f64) -> Option<f64> {
        let height = self.height_at(x_mm, y_mm)?;
        Some(height - self.grid.surface_z_mm)
    }

    /// Apply height compensation to a nominal program Z.
    ///
    /// `x_mm` and `y_mm` are machine coordinates for the lookup. `z_mm` is the
    /// nominal program Z; only the measured deviation is added, so its frame
    /// cancels. This is the primitive for streamed compensation; a corrected
    /// G-code export is a follow-up.
    #[must_use]
    pub fn apply_to_z(&self, x_mm: f64, y_mm: f64, z_mm: f64) -> Option<f64> {
        if !z_mm.is_finite() {
            return None;
        }
        Some(z_mm + self.compensation_at(x_mm, y_mm)?)
    }

    /// Valid samples as machine-XY viewer overlay points with their height.
    #[must_use]
    pub fn valid_points(&self) -> Vec<(Point2, f64)> {
        let mut points = Vec::with_capacity(self.valid_count());
        for row in 0..self.grid.rows {
            for column in 0..self.grid.columns {
                let Some(height) = self.z(column, row) else {
                    continue;
                };
                let Some(point) = self.grid.point(column, row) else {
                    continue;
                };
                points.push((point.to_viewer(), height));
            }
        }
        points
    }

    /// Lowest and highest valid height, if any.
    #[must_use]
    pub fn z_range(&self) -> Option<(f64, f64)> {
        let mut range: Option<(f64, f64)> = None;
        for height in self.samples.iter().flatten() {
            range = Some(match range {
                Some((low, high)) => (low.min(*height), high.max(*height)),
                None => (*height, *height),
            });
        }
        range
    }

    /// One row of samples.
    #[must_use]
    pub fn row(&self, row: u16) -> Option<&[Option<f64>]> {
        if row >= self.grid.rows {
            return None;
        }
        let start = usize::from(row) * usize::from(self.grid.columns);
        let end = start + usize::from(self.grid.columns);
        self.samples.get(start..end)
    }
}
