//! Height-map toolpath compensation, summary, and CSV export tests.

use ghs_probe::{HeightMap, HeightMapGrid, XyPoint};
use ghs_viewer::{Point2, Point3, Segment, SegmentKind, Toolpath};

fn grid(columns: u16, rows: u16, step_mm: f64) -> HeightMapGrid {
    HeightMapGrid {
        origin_mm: XyPoint::new(0.0, 0.0),
        step_x_mm: step_mm,
        step_y_mm: step_mm,
        columns,
        rows,
        surface_z_mm: 0.0,
    }
}

fn flat_map(columns: u16, rows: u16, step_mm: f64, height_mm: f64) -> HeightMap {
    let count = usize::from(columns) * usize::from(rows);
    HeightMap::from_samples(grid(columns, rows, step_mm), vec![Some(height_mm); count])
        .expect("valid flat map")
}

fn approx(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, got {actual}"
    );
}

fn nominal_z(start_z: f32, end_z: f32, start_x: f32, end_x: f32, x: f32) -> f64 {
    if start_x == end_x {
        return f64::from(start_z);
    }
    let t = (f64::from(x) - f64::from(start_x)) / (f64::from(end_x) - f64::from(start_x));
    f64::from(start_z) + (f64::from(end_z) - f64::from(start_z)) * t
}

#[test]
fn flat_map_at_nominal_leaves_z_unchanged_and_reports_full_compensation() {
    let map = flat_map(3, 3, 10.0, 0.0);
    let mut source = Toolpath::new();
    assert!(source.append_line(
        Point3::new(0.0, 0.0, -1.0),
        Point3::new(20.0, 20.0, -3.0),
        SegmentKind::Feed,
        4,
        2,
    ));
    assert!(source.append_line(
        Point3::new(20.0, 20.0, -3.0),
        Point3::new(20.0, 20.0, 5.0),
        SegmentKind::Rapid,
        4,
        2,
    ));

    let result = map.compensate_toolpath(&source, Point2::ZERO);
    assert_eq!(result.uncompensated_segments, 0);
    assert_eq!(result.compensated_segments, result.toolpath.len());
    assert!(result.compensated_segments > source.len());
    assert!(result.is_fully_compensated());
    approx(result.uncompensated_length_mm, 0.0, 1e-12);
    approx(
        result.total_length_mm(),
        source
            .segments()
            .iter()
            .map(|segment| f64::from(segment.length()))
            .sum::<f64>(),
        1e-3,
    );

    for segment in result.toolpath.segments() {
        assert!(segment.is_valid());
        assert_eq!(segment.layer, 4);
        assert_eq!(segment.tool, 2);
        if segment.kind == SegmentKind::Rapid {
            continue;
        }
        approx(
            f64::from(segment.start.z),
            nominal_z(-1.0, -3.0, 0.0, 20.0, segment.start.x),
            1e-5,
        );
        approx(
            f64::from(segment.end.z),
            nominal_z(-1.0, -3.0, 0.0, 20.0, segment.end.x),
            1e-5,
        );
    }
    let last = result.toolpath.segments().last().expect("output");
    assert_eq!(last.kind, SegmentKind::Rapid);
    approx(f64::from(last.start.z), -3.0, 1e-6);
    approx(f64::from(last.end.z), 5.0, 1e-6);
}

#[test]
fn linear_ramp_shifts_endpoints_and_midpoints_by_the_interpolated_deviation() {
    let map = HeightMap::from_samples(
        grid(2, 2, 10.0),
        vec![Some(0.0), Some(1.0), Some(2.0), Some(3.0)],
    )
    .expect("valid ramp");

    let mut horizontal = Toolpath::new();
    horizontal.append_line(
        Point3::new(0.0, 5.0, -5.0),
        Point3::new(10.0, 5.0, -5.0),
        SegmentKind::Feed,
        0,
        0,
    );
    let result = map.compensate_toolpath(&horizontal, Point2::ZERO);
    assert_eq!(result.toolpath.len(), 2);
    assert_eq!(result.compensated_segments, 2);
    let first = result.toolpath.segments()[0];
    let second = result.toolpath.segments()[1];
    approx(f64::from(first.start.x), 0.0, 1e-6);
    approx(f64::from(first.start.z), -4.0, 1e-6);
    approx(f64::from(first.end.x), 5.0, 1e-6);
    approx(f64::from(first.end.z), -3.5, 1e-6);
    approx(f64::from(second.start.z), -3.5, 1e-6);
    approx(f64::from(second.end.z), -3.0, 1e-6);

    let mut diagonal = Toolpath::new();
    diagonal.append_line(
        Point3::new(0.0, 0.0, -5.0),
        Point3::new(10.0, 10.0, -5.0),
        SegmentKind::Feed,
        0,
        0,
    );
    let result = map.compensate_toolpath(&diagonal, Point2::ZERO);
    assert_eq!(result.toolpath.len(), 3);
    for segment in result.toolpath.segments() {
        for point in [segment.start, segment.end] {
            let expected = -5.0 + 0.1 * f64::from(point.x) + 0.2 * f64::from(point.y);
            approx(f64::from(point.z), expected, 1e-5);
        }
    }
}

#[test]
fn long_segments_are_split_into_ordered_finite_pieces() {
    let columns = 11;
    let samples: Vec<Option<f64>> = (0..columns * 2)
        .map(|index| Some(f64::from(index % columns) * 0.1))
        .collect();
    let map = HeightMap::from_samples(grid(columns, 2, 10.0), samples).expect("valid map");
    let mut source = Toolpath::new();
    source.append_line(
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(100.0, 0.0, -1.0),
        SegmentKind::Feed,
        7,
        3,
    );

    let result = map.compensate_toolpath(&source, Point2::ZERO);
    assert_eq!(result.toolpath.len(), 20);
    assert_eq!(result.compensated_segments, 20);
    assert_eq!(result.uncompensated_segments, 0);
    assert_eq!(
        result.toolpath.segments()[0].start,
        source.segments()[0].start
    );
    let last_output = result.toolpath.segments()[19];
    let source_end = source.segments()[0].end;
    assert_eq!(last_output.end.x, source_end.x);
    assert_eq!(last_output.end.y, source_end.y);
    approx(f64::from(last_output.end.z), 0.0, 1e-6);

    let mut previous: Option<Segment> = None;
    for segment in result.toolpath.segments() {
        assert!(segment.is_valid());
        assert_eq!(segment.kind, SegmentKind::Feed);
        assert_eq!(segment.layer, 7);
        assert_eq!(segment.tool, 3);
        if let Some(previous) = previous {
            assert_eq!(previous.end, segment.start);
            assert!(segment.start.x >= previous.start.x);
        }
        previous = Some(*segment);
    }
}

#[test]
fn work_offset_is_used_for_lookups_but_geometry_stays_in_work_coordinates() {
    let map = HeightMap::from_samples(
        grid(2, 2, 10.0),
        vec![Some(0.0), Some(1.0), Some(2.0), Some(3.0)],
    )
    .expect("valid ramp");
    let mut source = Toolpath::new();
    source.append_line(
        Point3::new(100.0, 205.0, -5.0),
        Point3::new(110.0, 205.0, -5.0),
        SegmentKind::Feed,
        0,
        0,
    );

    // Work (100..110, 205) maps to machine (0..10, 5).
    let result = map.compensate_toolpath(&source, Point2::new(-100.0, -200.0));
    assert_eq!(result.toolpath.len(), 2);
    let first = result.toolpath.segments()[0];
    let second = result.toolpath.segments()[1];
    approx(f64::from(first.start.x), 100.0, 1e-6);
    approx(f64::from(first.start.y), 205.0, 1e-6);
    approx(f64::from(first.start.z), -4.0, 1e-6);
    approx(f64::from(first.end.z), -3.5, 1e-6);
    approx(f64::from(second.end.x), 110.0, 1e-6);
    approx(f64::from(second.end.z), -3.0, 1e-6);
}

#[test]
fn outside_and_invalid_samples_are_left_unchanged_and_counted() {
    let mut map = flat_map(3, 3, 10.0, 0.0);
    map.invalidate(1, 0).expect("valid cell");

    let mut source = Toolpath::new();
    source.append_line(
        Point3::new(0.0, 10.0, -2.0),
        Point3::new(20.0, 10.0, -2.0),
        SegmentKind::Feed,
        1,
        1,
    );
    source.append_line(
        Point3::new(0.0, 0.0, -2.0),
        Point3::new(20.0, 0.0, -2.0),
        SegmentKind::Feed,
        1,
        1,
    );
    source.append_line(
        Point3::new(100.0, 100.0, 1.0),
        Point3::new(110.0, 100.0, 0.0),
        SegmentKind::Rapid,
        1,
        1,
    );

    let result = map.compensate_toolpath(&source, Point2::ZERO);
    assert_eq!(result.compensated_segments, 4);
    assert_eq!(result.uncompensated_segments, 5);
    approx(result.compensated_length_mm, 20.0, 1e-6);
    approx(
        result.uncompensated_length_mm,
        20.0 + 101.0_f64.sqrt(),
        1e-6,
    );
    assert!(!result.is_fully_compensated());

    let segments = result.toolpath.segments();
    for segment in &segments[..4] {
        assert_eq!(segment.kind, SegmentKind::Feed);
    }
    for segment in &segments[4..8] {
        assert_eq!(segment.kind, SegmentKind::Feed);
        approx(f64::from(segment.start.z), -2.0, 1e-6);
        approx(f64::from(segment.end.z), -2.0, 1e-6);
    }
    assert_eq!(segments[8].kind, SegmentKind::Rapid);
    approx(f64::from(segments[8].start.z), 1.0, 1e-6);
    approx(f64::from(segments[8].end.z), 0.0, 1e-6);
}

#[test]
fn z_only_and_rapid_moves_keep_kind_layer_and_tool() {
    let map = flat_map(2, 2, 10.0, 1.5);
    let mut source = Toolpath::new();
    source.append_line(
        Point3::new(5.0, 5.0, 10.0),
        Point3::new(5.0, 5.0, 2.0),
        SegmentKind::Rapid,
        5,
        9,
    );
    source.append_line(
        Point3::new(5.0, 5.0, 2.0),
        Point3::new(5.0, 5.0, -1.0),
        SegmentKind::Plunge,
        6,
        9,
    );
    source.append_line(
        Point3::new(5.0, 5.0, -1.0),
        Point3::new(5.0, 5.0, -1.0),
        SegmentKind::Probe,
        7,
        9,
    );

    let result = map.compensate_toolpath(&source, Point2::ZERO);
    assert_eq!(result.toolpath.len(), 3);
    assert_eq!(result.compensated_segments, 3);
    assert_eq!(result.uncompensated_segments, 0);
    let segments = result.toolpath.segments();
    assert_eq!(segments[0].kind, SegmentKind::Rapid);
    assert_eq!(segments[0].layer, 5);
    assert_eq!(segments[0].tool, 9);
    approx(f64::from(segments[0].start.z), 11.5, 1e-6);
    approx(f64::from(segments[0].end.z), 3.5, 1e-6);
    assert_eq!(segments[1].kind, SegmentKind::Plunge);
    assert_eq!(segments[1].layer, 6);
    assert_eq!(segments[1].tool, 9);
    approx(f64::from(segments[1].start.z), 3.5, 1e-6);
    approx(f64::from(segments[1].end.z), 0.5, 1e-6);
    assert_eq!(segments[2].kind, SegmentKind::Probe);
    assert_eq!(segments[2].layer, 7);
    assert_eq!(segments[2].tool, 9);
    approx(f64::from(segments[2].start.z), 0.5, 1e-6);
}

#[test]
fn empty_non_finite_offset_and_huge_inputs_do_not_panic() {
    let map = flat_map(3, 3, 10.0, 0.5);
    let empty = map.compensate_toolpath(&Toolpath::new(), Point2::ZERO);
    assert!(empty.toolpath.is_empty());
    assert_eq!(empty.total_segments(), 0);
    approx(empty.total_length_mm(), 0.0, 1e-12);
    assert!(empty.is_fully_compensated());

    let mut source = Toolpath::new();
    source.append_line(
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(10.0, 0.0, -1.0),
        SegmentKind::Feed,
        0,
        0,
    );
    let non_finite = map.compensate_toolpath(&source, Point2::new(f32::NAN, 0.0));
    assert_eq!(non_finite.toolpath.len(), 1);
    assert_eq!(non_finite.uncompensated_segments, 1);
    assert!(non_finite.toolpath.segments()[0].is_valid());
    approx(non_finite.total_length_mm(), 10.0_f64.hypot(1.0), 1e-6);

    let mut huge = Toolpath::new();
    huge.append_line(
        Point3::new(-3.0e38, 1.0e6, 0.0),
        Point3::new(3.0e38, 1.0e6, -1.0),
        SegmentKind::Feed,
        0,
        0,
    );
    let huge_result = map.compensate_toolpath(&huge, Point2::ZERO);
    assert_eq!(huge_result.toolpath.len(), 1);
    assert_eq!(huge_result.uncompensated_segments, 1);
    assert!(huge_result.toolpath.segments()[0].is_valid());
    assert!(huge_result.total_length_mm().is_finite());
}

#[test]
fn summary_reports_complete_partial_and_all_invalid_maps() {
    let partial = HeightMap::from_samples(
        HeightMapGrid {
            surface_z_mm: 0.5,
            ..grid(2, 2, 10.0)
        },
        vec![Some(1.0), None, Some(-1.0), Some(3.0)],
    )
    .expect("valid map");
    let summary = partial.summary();
    assert_eq!(summary.points, 4);
    assert_eq!(summary.valid_points, 3);
    assert_eq!(summary.missing_points, 1);
    assert_eq!(summary.min_z_mm, Some(-1.0));
    assert_eq!(summary.max_z_mm, Some(3.0));
    assert_eq!(summary.min_compensation_mm, Some(-1.5));
    assert_eq!(summary.max_compensation_mm, Some(2.5));
    assert!(!summary.is_complete());
    assert!(!summary.is_all_invalid());

    let all_invalid = HeightMap::new(grid(2, 2, 10.0)).expect("valid grid");
    let summary = all_invalid.summary();
    assert_eq!(summary.points, 4);
    assert_eq!(summary.valid_points, 0);
    assert_eq!(summary.missing_points, 4);
    assert_eq!(summary.min_z_mm, None);
    assert_eq!(summary.max_z_mm, None);
    assert_eq!(summary.min_compensation_mm, None);
    assert_eq!(summary.max_compensation_mm, None);
    assert!(summary.is_all_invalid());
    assert!(!summary.is_complete());
}

#[test]
fn csv_is_stable_row_major_and_round_trips_the_samples() {
    let map = HeightMap::from_samples(
        grid(2, 2, 10.0),
        vec![Some(0.0), None, Some(2.5), Some(-1.25)],
    )
    .expect("valid map");
    let csv = map.csv();
    assert_eq!(csv, map.csv());
    assert!(csv.ends_with('\n'));
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "column,row,x_mm,y_mm,z_mm,valid");
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[1], "0,0,0.000000,0.000000,0.000000,1");
    assert_eq!(lines[2], "1,0,10.000000,0.000000,,0");
    assert_eq!(lines[3], "0,1,0.000000,10.000000,2.500000,1");
    assert_eq!(lines[4], "1,1,10.000000,10.000000,-1.250000,1");

    let expected_cells = [(0_u16, 0_u16), (1, 0), (0, 1), (1, 1)];
    let mut parsed = Vec::new();
    for (line, (column, row)) in lines[1..].iter().zip(expected_cells) {
        let fields: Vec<&str> = line.split(',').collect();
        assert_eq!(fields.len(), 6);
        assert_eq!(fields[0].parse::<u16>().expect("column"), column);
        assert_eq!(fields[1].parse::<u16>().expect("row"), row);
        let x: f64 = fields[2].parse().expect("x");
        let y: f64 = fields[3].parse().expect("y");
        let point = map.grid().point(column, row).expect("grid point");
        approx(x, point.x_mm, 1e-9);
        approx(y, point.y_mm, 1e-9);
        match fields[4] {
            "" => {
                assert_eq!(fields[5], "0");
                parsed.push(None);
            }
            z => {
                assert_eq!(fields[5], "1");
                parsed.push(Some(z.parse::<f64>().expect("z")));
            }
        }
    }
    assert_eq!(parsed, map.samples());
    let round_trip = HeightMap::from_samples(grid(2, 2, 10.0), parsed).expect("round trip");
    assert_eq!(round_trip.samples(), map.samples());
}
