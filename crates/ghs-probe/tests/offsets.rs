//! Probe-tip XY offset and height-map Z offset tests.

use ghs_core::{Axis, Position, ProbeResult};
use ghs_probe::{
    CornerProbe, EdgeMode, EdgeSpec, HeightMap, HeightMapGrid, MAX_HEIGHT_MAP_Z_OFFSET_MM,
    MAX_PROBE_OFFSET_MM, ProbeAxis, ProbeCycle, ProbeError, ProbeParams, ProbeValue, RotationProbe,
    Sign, ValidationError, XyPoint,
};

fn params() -> ProbeParams {
    ProbeParams {
        probe_diameter_mm: 4.0,
        position: Some(Position::xyz(0.0, 0.0, -2.0)),
        ..ProbeParams::default()
    }
}

fn edge_cycle() -> ProbeCycle {
    ProbeCycle::Edge(EdgeSpec::new(
        ProbeAxis::X,
        EdgeMode::Outside,
        Sign::Positive,
        20.0,
    ))
}

fn corner_cycle() -> ProbeCycle {
    ProbeCycle::Corner(CornerProbe::new(
        EdgeSpec::new(ProbeAxis::X, EdgeMode::Outside, Sign::Positive, 20.0),
        EdgeSpec::new(ProbeAxis::Y, EdgeMode::Outside, Sign::Positive, 30.0),
    ))
}

fn approx(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, got {actual}"
    );
}

fn assert_no_spindle_or_coolant_start(program: &str) {
    for line in program.lines() {
        let trimmed = line.trim();
        assert!(!trimmed.starts_with("M3"), "spindle start in {trimmed}");
        assert!(!trimmed.starts_with("M4"), "spindle start in {trimmed}");
        assert!(!trimmed.starts_with("M7"), "coolant on in {trimmed}");
        assert!(!trimmed.starts_with("M8"), "coolant on in {trimmed}");
    }
}

#[test]
fn zero_offset_matches_the_existing_cycle_and_height_map_apis() {
    let params = ProbeParams {
        search_distance_mm: 2.0,
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        radius_mm: Some(5.0),
        ..params()
    };
    let zero = XyPoint::new(0.0, 0.0);
    let cycles = [
        ProbeCycle::ZTouchPlate {
            expected_plate_z_mm: 0.0,
            plate_thickness_mm: 1.0,
        },
        ProbeCycle::ToolLength {
            reference_z_mm: 0.0,
        },
        edge_cycle(),
        corner_cycle(),
        ProbeCycle::CenterCircle {
            touch_count: 3,
            mode: EdgeMode::Inside,
        },
        ProbeCycle::CenterRectangle {
            width_mm: 20.0,
            height_mm: 10.0,
            mode: EdgeMode::Inside,
        },
        ProbeCycle::Rotation(RotationProbe {
            separation_mm: 10.0,
            edge_axis: ProbeAxis::X,
            approach: Sign::Positive,
            expected_edge_mm: 5.0,
        }),
        ProbeCycle::XyTouchPlate(CornerProbe::new(
            EdgeSpec::new(ProbeAxis::X, EdgeMode::Outside, Sign::Positive, 20.0),
            EdgeSpec::new(ProbeAxis::Y, EdgeMode::Outside, Sign::Positive, 30.0),
        )),
    ];
    for cycle in cycles {
        assert_eq!(
            cycle.plan(&params).expect("zero-offset plan"),
            cycle
                .plan_with_offset(&params, zero)
                .expect("explicit zero"),
            "{cycle:?}"
        );
    }

    let grid = HeightMapGrid {
        origin_mm: XyPoint::new(0.0, 0.0),
        step_x_mm: 10.0,
        step_y_mm: 10.0,
        columns: 2,
        rows: 2,
        surface_z_mm: 0.0,
    };
    let grid_params = ProbeParams {
        search_distance_mm: 10.0,
        ..params.clone()
    };
    assert_eq!(
        grid.plan(&grid_params).expect("zero-offset grid"),
        grid.plan_with_offset(&grid_params, zero)
            .expect("explicit zero")
    );
}

fn shifted(position: Position, offset: XyPoint) -> Position {
    let mut shifted = position;
    shifted.set(Axis::X, position.get(Axis::X) - offset.x_mm);
    shifted.set(Axis::Y, position.get(Axis::Y) - offset.y_mm);
    shifted
}

#[test]
fn every_cycle_shifts_its_probe_positions_by_the_offset() {
    let params = ProbeParams {
        search_distance_mm: 2.0,
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        radius_mm: Some(5.0),
        ..params()
    };
    let offset = XyPoint::new(3.0, -2.0);
    let cycles = [
        ProbeCycle::ZTouchPlate {
            expected_plate_z_mm: 0.0,
            plate_thickness_mm: 0.0,
        },
        ProbeCycle::ToolLength {
            reference_z_mm: 0.0,
        },
        edge_cycle(),
        corner_cycle(),
        ProbeCycle::CenterCircle {
            touch_count: 4,
            mode: EdgeMode::Inside,
        },
        ProbeCycle::CenterRectangle {
            width_mm: 20.0,
            height_mm: 10.0,
            mode: EdgeMode::Inside,
        },
        ProbeCycle::Rotation(RotationProbe {
            separation_mm: 10.0,
            edge_axis: ProbeAxis::X,
            approach: Sign::Positive,
            expected_edge_mm: 20.0,
        }),
        ProbeCycle::XyTouchPlate(CornerProbe::new(
            EdgeSpec::new(ProbeAxis::X, EdgeMode::Outside, Sign::Positive, 20.0),
            EdgeSpec::new(ProbeAxis::Y, EdgeMode::Outside, Sign::Positive, 30.0),
        )),
    ];
    for cycle in cycles {
        let zero = cycle.plan(&params).expect("zero-offset plan");
        let offset_plan = cycle
            .plan_with_offset(&params, offset)
            .expect("offset plan");
        assert_eq!(zero.probes.len(), offset_plan.probes.len());
        for (zero_probe, offset_probe) in zero.probes.iter().zip(&offset_plan.probes) {
            assert_eq!(offset_probe.from, shifted(zero_probe.from, offset));
            assert_eq!(offset_probe.to, shifted(zero_probe.to, offset));
        }
        assert_no_spindle_or_coolant_start(&offset_plan.program);
    }
}

#[test]
fn zero_offset_results_match_the_existing_outcome_api() {
    let params = ProbeParams {
        edge_offset_mm: 1.0,
        ..params()
    };
    let results = [ProbeResult::new(true, Position::xyz(18.0, 0.0, -2.0))];
    assert_eq!(
        ghs_probe::ProbeOutcome::from_results(&edge_cycle(), &params, &results)
            .expect("zero-offset outcome"),
        ghs_probe::ProbeOutcome::from_results_with_offset(
            &edge_cycle(),
            &params,
            &results,
            XyPoint::new(0.0, 0.0),
        )
        .expect("explicit zero")
    );
}

#[test]
fn edge_plan_targets_the_controlled_point_and_results_add_the_offset() {
    let params = ProbeParams {
        edge_offset_mm: 1.0,
        ..params()
    };
    let offset = XyPoint::new(3.0, -2.0);
    let plan = edge_cycle()
        .plan_with_offset(&params, offset)
        .expect("valid plan");

    assert_eq!(plan.probes[0].from, Position::xyz(10.0, 2.0, -2.0));
    assert_eq!(plan.probes[0].to, Position::xyz(15.0, 2.0, -2.0));
    assert!(plan.program.contains("G0 X10.000 Y2.000"));
    assert!(plan.program.contains("G38.2 X15.000 F100.000"));
    assert_no_spindle_or_coolant_start(&plan.program);

    let raw = ProbeResult::new(true, Position::xyz(15.0, 2.0, -2.0));
    let outcome =
        ghs_probe::ProbeOutcome::from_results_with_offset(&edge_cycle(), &params, &[raw], offset)
            .expect("valid outcome");
    match outcome.value {
        ProbeValue::Edge {
            edge_mm, zero_mm, ..
        } => {
            approx(edge_mm, 20.0, 1e-12);
            approx(zero_mm, 21.0, 1e-12);
        }
        other => panic!("unexpected value: {other:?}"),
    }
    assert_eq!(outcome.samples[0].result.position, raw.position);
}

#[test]
fn corner_results_add_the_offset_before_combining_edges() {
    let params = ProbeParams {
        edge_offset_mm: 1.0,
        ..params()
    };
    let offset = XyPoint::new(3.0, -2.0);
    let results = [
        ProbeResult::new(true, Position::xyz(15.0, 2.0, -2.0)),
        ProbeResult::new(true, Position::xyz(-3.0, 30.0, -2.0)),
    ];
    let outcome = ghs_probe::ProbeOutcome::from_results_with_offset(
        &corner_cycle(),
        &params,
        &results,
        offset,
    )
    .expect("valid outcome");
    match outcome.value {
        ProbeValue::Corner {
            x_edge_mm,
            y_edge_mm,
            zero,
        } => {
            approx(x_edge_mm, 20.0, 1e-12);
            approx(y_edge_mm, 30.0, 1e-12);
            assert_eq!(zero, XyPoint::new(21.0, 31.0));
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn circle_results_fit_in_feature_coordinates() {
    let params = ProbeParams {
        probe_diameter_mm: 2.0,
        center: Some(Position::xyz(10.0, 20.0, 0.0)),
        radius_mm: Some(5.0),
        ..params()
    };
    let offset = XyPoint::new(2.0, -1.0);
    let results = [
        ProbeResult::new(true, Position::xyz(12.0, 21.0, 0.0)),
        ProbeResult::new(true, Position::xyz(6.0, 24.464_101_615_137_754, 0.0)),
        ProbeResult::new(true, Position::xyz(6.0, 17.535_898_384_862_246, 0.0)),
    ];
    let cycle = ProbeCycle::CenterCircle {
        touch_count: 3,
        mode: EdgeMode::Inside,
    };
    let outcome =
        ghs_probe::ProbeOutcome::from_results_with_offset(&cycle, &params, &results, offset)
            .expect("valid outcome");
    match outcome.value {
        ProbeValue::Center {
            center,
            contact_radius_mm,
            feature_radius_mm,
            rms_error_mm,
        } => {
            assert!(center.distance(XyPoint::new(10.0, 20.0)) < 1e-9);
            approx(contact_radius_mm, 4.0, 1e-9);
            approx(feature_radius_mm, 5.0, 1e-9);
            assert!(rms_error_mm < 1e-9);
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn rectangle_results_fit_in_feature_coordinates() {
    let params = ProbeParams {
        probe_diameter_mm: 2.0,
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        ..params()
    };
    let offset = XyPoint::new(2.0, -1.0);
    let results = [
        ProbeResult::new(true, Position::xyz(7.0, 1.0, 0.0)),
        ProbeResult::new(true, Position::xyz(-11.0, 1.0, 0.0)),
        ProbeResult::new(true, Position::xyz(-2.0, 5.0, 0.0)),
        ProbeResult::new(true, Position::xyz(-2.0, -3.0, 0.0)),
    ];
    let cycle = ProbeCycle::CenterRectangle {
        width_mm: 20.0,
        height_mm: 10.0,
        mode: EdgeMode::Inside,
    };
    let outcome =
        ghs_probe::ProbeOutcome::from_results_with_offset(&cycle, &params, &results, offset)
            .expect("valid outcome");
    match outcome.value {
        ProbeValue::RectangleCenter {
            center,
            width_mm,
            height_mm,
        } => {
            assert!(center.distance(XyPoint::new(0.0, 0.0)) < 1e-9);
            approx(width_mm, 20.0, 1e-12);
            approx(height_mm, 10.0, 1e-12);
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn rotation_results_use_offset_edge_points() {
    let params = ProbeParams {
        probe_diameter_mm: 2.0,
        ..params()
    };
    let offset = XyPoint::new(1.0, 2.0);
    let cycle = ProbeCycle::Rotation(RotationProbe {
        separation_mm: 20.0,
        edge_axis: ProbeAxis::X,
        approach: Sign::Positive,
        expected_edge_mm: 5.0,
    });
    let results = [
        ProbeResult::new(true, Position::xyz(-1.0, 2.0, -2.0)),
        ProbeResult::new(true, Position::xyz(19.0, 3.0, -2.0)),
    ];
    let outcome =
        ghs_probe::ProbeOutcome::from_results_with_offset(&cycle, &params, &results, offset)
            .expect("valid outcome");
    match outcome.value {
        ProbeValue::Rotation {
            p1,
            p2,
            angle_deg,
            deviation_deg,
            ..
        } => {
            assert_eq!(p1, XyPoint::new(0.0, 5.0));
            assert_eq!(p2, XyPoint::new(20.0, 6.0));
            approx(angle_deg, 2.862_405_226_111_747_4, 1e-9);
            approx(deviation_deg, angle_deg, 1e-9);
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn xy_plate_results_use_offset_edge_points() {
    let params = ProbeParams {
        edge_offset_mm: 2.0,
        ..params()
    };
    let offset = XyPoint::new(-4.0, 5.0);
    let cycle = ProbeCycle::XyTouchPlate(CornerProbe::new(
        EdgeSpec::new(ProbeAxis::X, EdgeMode::Outside, Sign::Positive, 20.0),
        EdgeSpec::new(ProbeAxis::Y, EdgeMode::Outside, Sign::Positive, 30.0),
    ));
    let results = [
        ProbeResult::new(true, Position::xyz(22.0, -5.0, -2.0)),
        ProbeResult::new(true, Position::xyz(4.0, 23.0, -2.0)),
    ];
    let outcome =
        ghs_probe::ProbeOutcome::from_results_with_offset(&cycle, &params, &results, offset)
            .expect("valid outcome");
    match outcome.value {
        ProbeValue::XyZero { zero, .. } => {
            assert_eq!(zero, XyPoint::new(22.0, 32.0));
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn z_cycles_translate_plan_xy_but_leave_z_math_alone() {
    let params = ProbeParams {
        position: Some(Position::xyz(10.0, 10.0, -2.0)),
        ..params()
    };
    let offset = XyPoint::new(2.0, 3.0);
    let cycle = ProbeCycle::ZTouchPlate {
        expected_plate_z_mm: 0.0,
        plate_thickness_mm: 1.0,
    };
    let plan = cycle.plan_with_offset(&params, offset).expect("valid plan");
    assert!(plan.program.contains("G0 X8.000 Y7.000"));
    assert!(plan.program.contains("G38.2 Z0.000 F100.000"));
    assert_eq!(plan.probes[0].from, Position::xyz(8.0, 7.0, 5.0));
    assert_eq!(plan.probes[0].to, Position::xyz(8.0, 7.0, 0.0));
    assert_no_spindle_or_coolant_start(&plan.program);

    let raw = ProbeResult::new(true, Position::xyz(8.0, 7.0, -5.0));
    let outcome =
        ghs_probe::ProbeOutcome::from_results_with_offset(&cycle, &params, &[raw], offset)
            .expect("valid outcome");
    match outcome.value {
        ProbeValue::ZZero {
            contact_z_mm,
            zero_z_mm,
            ..
        } => {
            approx(contact_z_mm, -5.0, 1e-12);
            approx(zero_z_mm, -6.0, 1e-12);
        }
        other => panic!("unexpected value: {other:?}"),
    }
    assert_eq!(outcome.samples[0].result.position, raw.position);
}

#[test]
fn z_only_plan_ignores_the_xy_offset() {
    let params = ProbeParams {
        position: None,
        ..params()
    };
    let cycle = ProbeCycle::ZTouchPlate {
        expected_plate_z_mm: 0.0,
        plate_thickness_mm: 0.0,
    };
    let zero = cycle.plan(&params).expect("zero-offset plan");
    let offset = cycle
        .plan_with_offset(&params, XyPoint::new(25.0, -25.0))
        .expect("offset plan");
    assert_eq!(zero.program, offset.program);
    assert_eq!(zero.probes, offset.probes);
    for probe in &offset.probes {
        assert_eq!(probe.from.axes().to_string(), "Z");
        assert_eq!(probe.to.axes().to_string(), "Z");
    }
}

#[test]
fn height_map_plan_shifts_every_grid_point() {
    let params = ProbeParams {
        search_distance_mm: 10.0,
        ..params()
    };
    let grid = HeightMapGrid {
        origin_mm: XyPoint::new(100.0, 200.0),
        step_x_mm: 10.0,
        step_y_mm: 10.0,
        columns: 2,
        rows: 2,
        surface_z_mm: 0.0,
    };
    let offset = XyPoint::new(5.0, -2.0);
    let plan = grid.plan_with_offset(&params, offset).expect("valid plan");
    assert_eq!(plan.expected_probes, 4);
    let expected_from = [
        Position::xyz(95.0, 202.0, 10.0),
        Position::xyz(105.0, 202.0, 10.0),
        Position::xyz(95.0, 212.0, 10.0),
        Position::xyz(105.0, 212.0, 10.0),
    ];
    let expected_to = [
        Position::xyz(95.0, 202.0, 0.0),
        Position::xyz(105.0, 202.0, 0.0),
        Position::xyz(95.0, 212.0, 0.0),
        Position::xyz(105.0, 212.0, 0.0),
    ];
    for (index, probe) in plan.probes.iter().enumerate() {
        assert_eq!(probe.from, expected_from[index]);
        assert_eq!(probe.to, expected_to[index]);
    }
    for line in [
        "G0 X95.000 Y202.000",
        "G0 X105.000 Y202.000",
        "G0 X95.000 Y212.000",
        "G0 X105.000 Y212.000",
    ] {
        assert!(plan.program.contains(line), "missing {line}");
    }
    assert_no_spindle_or_coolant_start(&plan.program);
}

#[test]
fn invalid_probe_offsets_are_typed_errors() {
    let params = ProbeParams {
        search_distance_mm: 10.0,
        ..params()
    };
    let grid = HeightMapGrid {
        origin_mm: XyPoint::new(0.0, 0.0),
        step_x_mm: 10.0,
        step_y_mm: 10.0,
        columns: 2,
        rows: 2,
        surface_z_mm: 0.0,
    };
    let results = [ProbeResult::new(true, Position::xyz(18.0, 0.0, -2.0))];
    for offset in [
        XyPoint::new(f64::NAN, 0.0),
        XyPoint::new(0.0, f64::INFINITY),
        XyPoint::new(MAX_PROBE_OFFSET_MM + 0.1, 0.0),
        XyPoint::new(0.0, -MAX_PROBE_OFFSET_MM - 0.1),
    ] {
        assert!(matches!(
            edge_cycle().plan_with_offset(&params, offset),
            Err(ProbeError::Validation(_))
        ));
        assert!(matches!(
            grid.plan_with_offset(&params, offset),
            Err(ProbeError::Validation(_))
        ));
        assert!(matches!(
            ghs_probe::ProbeOutcome::from_results_with_offset(
                &edge_cycle(),
                &params,
                &results,
                offset,
            ),
            Err(ProbeError::Validation(_))
        ));
    }

    assert!(matches!(
        edge_cycle().plan_with_offset(&params, XyPoint::new(f64::NAN, 0.0)),
        Err(ProbeError::Validation(ValidationError::NonFinite { .. }))
    ));
    assert!(matches!(
        edge_cycle().plan_with_offset(&params, XyPoint::new(MAX_PROBE_OFFSET_MM + 0.1, 0.0)),
        Err(ProbeError::Validation(ValidationError::OutOfRange { .. }))
    ));
}

fn grid() -> HeightMapGrid {
    HeightMapGrid {
        origin_mm: XyPoint::new(0.0, 0.0),
        step_x_mm: 10.0,
        step_y_mm: 10.0,
        columns: 2,
        rows: 2,
        surface_z_mm: 0.0,
    }
}

fn ramp_map() -> HeightMap {
    HeightMap::from_samples(grid(), vec![Some(0.0), Some(1.0), Some(2.0), Some(3.0)])
        .expect("valid map")
}

#[test]
fn default_height_map_has_a_zero_z_offset() {
    let map = ramp_map();
    assert_eq!(map.z_offset_mm(), 0.0);
    assert_eq!(map, map.clone().with_z_offset(0.0).expect("zero offset"));
    assert_eq!(map.height_at(5.0, 5.0), Some(1.5));
    assert_eq!(map.compensation_at(5.0, 5.0), Some(1.5));
}

#[test]
fn height_map_z_offset_shifts_display_and_compensation() {
    let map = ramp_map().with_z_offset(10.0).expect("valid offset");
    assert_eq!(map.z_offset_mm(), 10.0);
    assert_eq!(map.samples(), &[Some(0.0), Some(1.0), Some(2.0), Some(3.0)]);
    assert_eq!(map.row(0), Some(&[Some(0.0), Some(1.0)][..]));
    assert_eq!(map.z(0, 0), Some(10.0));
    assert_eq!(map.z(1, 1), Some(13.0));
    assert_eq!(map.height_at(5.0, 5.0), Some(11.5));
    assert_eq!(map.height_at(2.5, 7.5), Some(11.75));
    assert_eq!(map.compensation_at(5.0, 5.0), Some(11.5));
    assert_eq!(map.apply_to_z(5.0, 5.0, -2.0), Some(9.5));
    assert_eq!(map.z_range(), Some((10.0, 13.0)));

    let points = map.valid_points();
    assert_eq!(points.len(), 4);
    assert_eq!(
        points.iter().map(|(_, z)| *z).collect::<Vec<_>>(),
        vec![10.0, 11.0, 12.0, 13.0]
    );

    let summary = map.summary();
    assert_eq!(summary.valid_points, 4);
    assert_eq!(summary.min_z_mm, Some(10.0));
    assert_eq!(summary.max_z_mm, Some(13.0));
    assert_eq!(summary.min_compensation_mm, Some(10.0));
    assert_eq!(summary.max_compensation_mm, Some(13.0));
}

#[test]
fn height_map_csv_reports_effective_heights() {
    let raw = ramp_map();
    let shifted = raw.clone().with_z_offset(10.0).expect("valid offset");
    let raw_csv = raw.csv();
    let shifted_csv = shifted.csv();
    let raw_lines: Vec<&str> = raw_csv.lines().collect();
    let shifted_lines: Vec<&str> = shifted_csv.lines().collect();
    assert_eq!(raw_lines.len(), shifted_lines.len());
    assert_eq!(raw_lines[0], shifted_lines[0]);
    for (raw_line, shifted_line) in raw_lines[1..].iter().zip(&shifted_lines[1..]) {
        let raw_fields: Vec<&str> = raw_line.split(',').collect();
        let shifted_fields: Vec<&str> = shifted_line.split(',').collect();
        assert_eq!(raw_fields[5], shifted_fields[5]);
        if raw_fields[5] == "0" {
            assert_eq!(raw_fields[4], "");
            assert_eq!(shifted_fields[4], "");
        } else {
            let raw_z: f64 = raw_fields[4].parse().expect("raw z");
            let shifted_z: f64 = shifted_fields[4].parse().expect("shifted z");
            approx(shifted_z, raw_z + 10.0, 1e-6);
        }
    }
}

#[test]
fn height_map_z_offset_uses_effective_toolpath_compensation() {
    let map = HeightMap::from_samples(grid(), vec![Some(0.0); 4])
        .expect("valid map")
        .with_z_offset(5.0)
        .expect("valid offset");
    let mut source = ghs_viewer::Toolpath::new();
    source.append_line(
        ghs_viewer::Point3::new(0.0, 0.0, 0.0),
        ghs_viewer::Point3::new(10.0, 0.0, 0.0),
        ghs_viewer::SegmentKind::Feed,
        0,
        0,
    );
    let result = map.compensate_toolpath(&source, ghs_viewer::Point2::ZERO);
    assert_eq!(result.compensated_segments, result.toolpath.len());
    assert_eq!(result.uncompensated_segments, 0);
    for segment in result.toolpath.segments() {
        approx(f64::from(segment.start.z), 5.0, 1e-6);
        approx(f64::from(segment.end.z), 5.0, 1e-6);
    }
}

#[test]
fn height_map_z_offset_validation_is_bounded_and_finite() {
    let map = ramp_map();
    for offset in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        MAX_HEIGHT_MAP_Z_OFFSET_MM + 0.1,
        -MAX_HEIGHT_MAP_Z_OFFSET_MM - 0.1,
    ] {
        assert!(map.clone().with_z_offset(offset).is_err());
    }
    assert!(
        map.clone()
            .with_z_offset(MAX_HEIGHT_MAP_Z_OFFSET_MM)
            .is_ok()
    );
    assert!(
        map.clone()
            .with_z_offset(-MAX_HEIGHT_MAP_Z_OFFSET_MM)
            .is_ok()
    );

    let mut mutable = ramp_map();
    assert!(mutable.set_z_offset(f64::NAN).is_err());
    assert_eq!(mutable.z_offset_mm(), 0.0);
    mutable.set_z_offset(1.25).expect("valid offset");
    assert_eq!(mutable.z_offset_mm(), 1.25);
    assert_eq!(
        mutable.samples(),
        &[Some(0.0), Some(1.0), Some(2.0), Some(3.0)]
    );
}

#[test]
fn height_map_effective_z_stays_within_the_grid_and_requires_all_corners() {
    let map = HeightMap::from_samples(grid(), vec![Some(0.0), Some(1.0), Some(2.0), None])
        .expect("valid map")
        .with_z_offset(-4.0)
        .expect("valid offset");
    assert_eq!(map.height_at(10.0, 0.0), Some(-3.0));
    assert_eq!(map.height_at(0.0, 10.0), Some(-2.0));
    assert_eq!(map.height_at(5.0, 0.0), Some(-3.5));
    assert_eq!(map.height_at(5.0, 5.0), None);
    assert_eq!(map.height_at(-0.001, 0.0), None);
    assert_eq!(map.height_at(10.001, 0.0), None);
    assert_eq!(map.compensation_at(5.0, 5.0), None);
    assert_eq!(map.samples(), &[Some(0.0), Some(1.0), Some(2.0), None]);
}
