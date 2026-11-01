//! Result math and outcome computation tests.

use ghs_core::{Position, ProbeResult, Units, Wcs};
use ghs_probe::{
    CircleFit, CornerProbe, EdgeMode, EdgeSpec, ProbeAxis, ProbeCycle, ProbeParams, ProbeValue,
    RotationProbe, Sign, XyPoint, circle_feature_radius, corner_work_zero, edge_from_contact,
    edge_work_zero, fit_circle, rectangle_center_from_edges, rectangle_center_from_touches,
    rotation_from_points, tool_length_offset, xy_plate_zero, z_zero_from_contact,
};

fn params() -> ProbeParams {
    ProbeParams {
        probe_diameter_mm: 4.0,
        position: Some(Position::xyz(0.0, 0.0, -2.0)),
        ..ProbeParams::default()
    }
}

#[test]
fn z_zero_subtracts_plate_thickness() {
    assert!((z_zero_from_contact(-5.0, 1.5).unwrap() + 6.5).abs() < 1e-12);
    assert!((z_zero_from_contact(-5.0, 0.0).unwrap() + 5.0).abs() < 1e-12);
    assert!(z_zero_from_contact(f64::NAN, 0.0).is_err());
    assert!(z_zero_from_contact(0.0, -1.0).is_err());
}

#[test]
fn tool_length_offset_keeps_the_documented_sign() {
    assert!((tool_length_offset(-10.0, -8.0).unwrap() - 2.0).abs() < 1e-12);
    assert!((tool_length_offset(-10.0, -13.0).unwrap() + 3.0).abs() < 1e-12);
    assert!(tool_length_offset(f64::INFINITY, 0.0).is_err());
}

#[test]
fn edge_correction_uses_the_approach_direction() {
    // Moving +X: contact is one radius before the edge.
    assert!((edge_from_contact(18.0, 2.0, Sign::Positive).unwrap() - 20.0).abs() < 1e-12);
    // Moving -X: contact is one radius after the edge.
    assert!((edge_from_contact(22.0, 2.0, Sign::Negative).unwrap() - 20.0).abs() < 1e-12);
    assert!((edge_work_zero(20.0, Sign::Positive, 1.5).unwrap() - 21.5).abs() < 1e-12);
    assert!((edge_work_zero(20.0, Sign::Negative, 1.5).unwrap() - 18.5).abs() < 1e-12);
    assert!(edge_from_contact(f64::NAN, 2.0, Sign::Positive).is_err());
    assert!(edge_work_zero(20.0, Sign::Positive, -1.0).is_err());
}

#[test]
fn corner_zero_combines_both_edges_and_offsets() {
    let zero = corner_work_zero(20.0, 30.0, Sign::Positive, Sign::Positive, 1.0).unwrap();
    assert_eq!(zero, XyPoint::new(21.0, 31.0));
    let zero = corner_work_zero(20.0, 30.0, Sign::Negative, Sign::Negative, 1.0).unwrap();
    assert_eq!(zero, XyPoint::new(19.0, 29.0));
    let plate = xy_plate_zero(20.0, 30.0, Sign::Positive, Sign::Positive, 0.0).unwrap();
    assert_eq!(plate, XyPoint::new(20.0, 30.0));
}

#[test]
fn circle_fit_recovers_a_known_centre_and_radius() {
    let points = [
        XyPoint::new(14.0, 20.0),
        XyPoint::new(8.0, 23.464_101_615_137_754),
        XyPoint::new(8.0, 16.535_898_384_862_246),
    ];
    let fit = fit_circle(&points).unwrap();
    assert!((fit.center.x_mm - 10.0).abs() < 1e-9);
    assert!((fit.center.y_mm - 20.0).abs() < 1e-9);
    assert!((fit.radius_mm - 4.0).abs() < 1e-9);
    assert!(fit.rms_error_mm < 1e-9);
}

#[test]
fn circle_fit_uses_least_squares_for_more_points() {
    let points = [
        XyPoint::new(10.0, 5.0),
        XyPoint::new(15.0, 10.0),
        XyPoint::new(10.0, 15.0),
        XyPoint::new(5.0, 10.0),
    ];
    let fit = fit_circle(&points).unwrap();
    assert!((fit.center.x_mm - 10.0).abs() < 1e-9);
    assert!((fit.center.y_mm - 10.0).abs() < 1e-9);
    assert!((fit.radius_mm - 5.0).abs() < 1e-9);
    assert!(fit.rms_error_mm < 1e-9);
}

#[test]
fn circle_fit_rejects_degenerate_input() {
    assert!(fit_circle(&[]).is_err());
    assert!(fit_circle(&[XyPoint::new(0.0, 0.0); 3]).is_err());
    assert!(
        fit_circle(&[
            XyPoint::new(0.0, 0.0),
            XyPoint::new(1.0, 1.0),
            XyPoint::new(2.0, 2.0),
        ])
        .is_err()
    );
    assert!(
        fit_circle(&[
            XyPoint::new(f64::NAN, 0.0),
            XyPoint::new(1.0, 0.0),
            XyPoint::new(0.0, 1.0),
        ])
        .is_err()
    );
}

#[test]
fn circle_feature_radius_corrects_for_inside_and_outside() {
    assert!((circle_feature_radius(4.0, 1.0, EdgeMode::Inside).unwrap() - 5.0).abs() < 1e-12);
    assert!((circle_feature_radius(6.0, 1.0, EdgeMode::Outside).unwrap() - 5.0).abs() < 1e-12);
    assert!(circle_feature_radius(0.5, 1.0, EdgeMode::Outside).is_err());
}

#[test]
fn rectangle_center_from_edges_checks_order() {
    let fit = rectangle_center_from_edges(-10.0, 10.0, -5.0, 5.0).unwrap();
    assert_eq!(fit.center, XyPoint::new(0.0, 0.0));
    assert!((fit.width_mm - 20.0).abs() < 1e-12);
    assert!((fit.height_mm - 10.0).abs() < 1e-12);
    assert!(rectangle_center_from_edges(10.0, -10.0, -5.0, 5.0).is_err());
    assert!(rectangle_center_from_edges(-10.0, 10.0, 5.0, -5.0).is_err());
}

#[test]
fn rectangle_center_from_four_symmetric_touches() {
    let center = rectangle_center_from_touches([
        XyPoint::new(-10.0, -5.0),
        XyPoint::new(10.0, -5.0),
        XyPoint::new(10.0, 5.0),
        XyPoint::new(-10.0, 5.0),
    ])
    .unwrap();
    assert_eq!(center, XyPoint::new(0.0, 0.0));

    let center = rectangle_center_from_touches([
        XyPoint::new(0.0, 5.0),
        XyPoint::new(10.0, 0.0),
        XyPoint::new(0.0, -5.0),
        XyPoint::new(-10.0, 0.0),
    ])
    .unwrap();
    assert_eq!(center, XyPoint::new(0.0, 0.0));

    assert!(
        rectangle_center_from_touches([
            XyPoint::new(f64::NAN, 0.0),
            XyPoint::new(0.0, 0.0),
            XyPoint::new(0.0, 0.0),
            XyPoint::new(0.0, 0.0),
        ])
        .is_err()
    );
}

#[test]
fn rotation_reports_angle_and_deviation_from_the_nominal_axis() {
    let fit = rotation_from_points(
        XyPoint::new(0.0, 0.0),
        XyPoint::new(10.0, 1.0),
        ProbeAxis::X,
    )
    .unwrap();
    assert!((fit.angle_deg - 5.710_593_137_499_642).abs() < 1e-9);
    assert_eq!(fit.nominal_deg, 0.0);
    assert!((fit.deviation_deg - fit.angle_deg).abs() < 1e-9);

    let fit = rotation_from_points(
        XyPoint::new(0.0, 0.0),
        XyPoint::new(1.0, 10.0),
        ProbeAxis::Y,
    )
    .unwrap();
    assert!((fit.angle_deg - 84.289_406_862_500_37).abs() < 1e-9);
    assert_eq!(fit.nominal_deg, 90.0);
    assert!((fit.deviation_deg + 5.710_593_137_499_642).abs() < 1e-9);

    let reversed = rotation_from_points(
        XyPoint::new(10.0, 0.0),
        XyPoint::new(0.0, 0.0),
        ProbeAxis::X,
    )
    .unwrap();
    assert!(reversed.deviation_deg.abs() < 1e-9);
    assert!(
        rotation_from_points(XyPoint::new(0.0, 0.0), XyPoint::new(0.0, 0.0), ProbeAxis::X).is_err()
    );
}

#[test]
fn edge_outcome_computes_edge_and_zero() {
    let params = ProbeParams {
        edge_offset_mm: 1.0,
        ..params()
    };
    let cycle = ProbeCycle::Edge(EdgeSpec {
        axis: ProbeAxis::X,
        mode: EdgeMode::Outside,
        approach: Sign::Positive,
        expected_edge_mm: 20.0,
    });
    let results = [ProbeResult::new(true, Position::xyz(18.0, 0.0, -2.0))];
    let outcome = ghs_probe::ProbeOutcome::from_results(&cycle, &params, &results).unwrap();
    assert_eq!(outcome.cycle, ghs_probe::CycleKind::Edge);
    assert_eq!(outcome.wcs, Wcs::G54);
    assert_eq!(outcome.units, Units::Mm);
    assert_eq!(outcome.samples.len(), 1);
    assert_eq!(outcome.samples[0].label, "edge");
    match outcome.value {
        ProbeValue::Edge {
            axis,
            mode,
            edge_mm,
            zero_mm,
        } => {
            assert_eq!(axis, ProbeAxis::X);
            assert_eq!(mode, EdgeMode::Outside);
            assert!((edge_mm - 20.0).abs() < 1e-12);
            assert!((zero_mm - 21.0).abs() < 1e-12);
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn results_stay_in_machine_coordinates_when_a_work_offset_is_set() {
    let params = ProbeParams {
        wcs_offset_mm: Position::xyz(100.0, 200.0, 10.0),
        ..params()
    };
    let cycle = ProbeCycle::Edge(EdgeSpec {
        axis: ProbeAxis::X,
        mode: EdgeMode::Outside,
        approach: Sign::Positive,
        expected_edge_mm: 20.0,
    });
    let results = [ProbeResult::new(true, Position::xyz(18.0, 0.0, -2.0))];
    let outcome = ghs_probe::ProbeOutcome::from_results(&cycle, &params, &results).unwrap();
    match outcome.value {
        ProbeValue::Edge {
            edge_mm, zero_mm, ..
        } => {
            assert!((edge_mm - 20.0).abs() < 1e-12);
            assert!((zero_mm - 20.0).abs() < 1e-12);
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn corner_outcome_combines_two_edges() {
    let params = ProbeParams {
        edge_offset_mm: 1.0,
        ..params()
    };
    let cycle = ProbeCycle::Corner(CornerProbe::new(
        EdgeSpec {
            axis: ProbeAxis::X,
            mode: EdgeMode::Outside,
            approach: Sign::Positive,
            expected_edge_mm: 20.0,
        },
        EdgeSpec {
            axis: ProbeAxis::Y,
            mode: EdgeMode::Outside,
            approach: Sign::Negative,
            expected_edge_mm: 30.0,
        },
    ));
    let results = [
        ProbeResult::new(true, Position::xyz(18.0, 0.0, -2.0)),
        ProbeResult::new(true, Position::xyz(0.0, 32.0, -2.0)),
    ];
    let outcome = ghs_probe::ProbeOutcome::from_results(&cycle, &params, &results).unwrap();
    match outcome.value {
        ProbeValue::Corner {
            x_edge_mm,
            y_edge_mm,
            zero,
        } => {
            assert!((x_edge_mm - 20.0).abs() < 1e-12);
            assert!((y_edge_mm - 30.0).abs() < 1e-12);
            assert_eq!(zero, XyPoint::new(21.0, 29.0));
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn circle_outcome_fits_centre_and_corrects_radius() {
    let params = ProbeParams {
        probe_diameter_mm: 2.0,
        center: Some(Position::xyz(10.0, 20.0, 0.0)),
        radius_mm: Some(5.0),
        ..params()
    };
    let cycle = ProbeCycle::CenterCircle {
        touch_count: 3,
        mode: EdgeMode::Inside,
    };
    let results = [
        ProbeResult::new(true, Position::xyz(14.0, 20.0, 0.0)),
        ProbeResult::new(true, Position::xyz(8.0, 23.464_101_615_137_754, 0.0)),
        ProbeResult::new(true, Position::xyz(8.0, 16.535_898_384_862_246, 0.0)),
    ];
    let outcome = ghs_probe::ProbeOutcome::from_results(&cycle, &params, &results).unwrap();
    match outcome.value {
        ProbeValue::Center {
            center,
            contact_radius_mm,
            feature_radius_mm,
            rms_error_mm,
        } => {
            assert!(center.distance(XyPoint::new(10.0, 20.0)) < 1e-9);
            assert!((contact_radius_mm - 4.0).abs() < 1e-9);
            assert!((feature_radius_mm - 5.0).abs() < 1e-9);
            assert!(rms_error_mm < 1e-9);
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn outside_circle_outcome_reports_the_boss_radius() {
    let params = ProbeParams {
        probe_diameter_mm: 2.0,
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        radius_mm: Some(5.0),
        ..params()
    };
    let cycle = ProbeCycle::CenterCircle {
        touch_count: 3,
        mode: EdgeMode::Outside,
    };
    let results = [
        ProbeResult::new(true, Position::xyz(6.0, 0.0, 0.0)),
        ProbeResult::new(true, Position::xyz(-3.0, 5.196_152_422_706_632, 0.0)),
        ProbeResult::new(true, Position::xyz(-3.0, -5.196_152_422_706_632, 0.0)),
    ];
    let outcome = ghs_probe::ProbeOutcome::from_results(&cycle, &params, &results).unwrap();
    match outcome.value {
        ProbeValue::Center {
            center,
            feature_radius_mm,
            ..
        } => {
            assert!(center.distance(XyPoint::new(0.0, 0.0)) < 1e-9);
            assert!((feature_radius_mm - 5.0).abs() < 1e-9);
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn rectangle_outcome_fits_centre_and_size() {
    let params = ProbeParams {
        probe_diameter_mm: 2.0,
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        ..params()
    };
    let cycle = ProbeCycle::CenterRectangle {
        width_mm: 20.0,
        height_mm: 10.0,
        mode: EdgeMode::Inside,
    };
    let results = [
        ProbeResult::new(true, Position::xyz(9.0, 0.0, 0.0)),
        ProbeResult::new(true, Position::xyz(-9.0, 0.0, 0.0)),
        ProbeResult::new(true, Position::xyz(0.0, 4.0, 0.0)),
        ProbeResult::new(true, Position::xyz(0.0, -4.0, 0.0)),
    ];
    let outcome = ghs_probe::ProbeOutcome::from_results(&cycle, &params, &results).unwrap();
    match outcome.value {
        ProbeValue::RectangleCenter {
            center,
            width_mm,
            height_mm,
        } => {
            assert!(center.distance(XyPoint::new(0.0, 0.0)) < 1e-9);
            assert!((width_mm - 20.0).abs() < 1e-12);
            assert!((height_mm - 10.0).abs() < 1e-12);
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn outside_rectangle_outcome_uses_outward_contacts() {
    let params = ProbeParams {
        probe_diameter_mm: 2.0,
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        ..params()
    };
    let cycle = ProbeCycle::CenterRectangle {
        width_mm: 20.0,
        height_mm: 10.0,
        mode: EdgeMode::Outside,
    };
    let results = [
        ProbeResult::new(true, Position::xyz(11.0, 0.0, 0.0)),
        ProbeResult::new(true, Position::xyz(-11.0, 0.0, 0.0)),
        ProbeResult::new(true, Position::xyz(0.0, 6.0, 0.0)),
        ProbeResult::new(true, Position::xyz(0.0, -6.0, 0.0)),
    ];
    let outcome = ghs_probe::ProbeOutcome::from_results(&cycle, &params, &results).unwrap();
    match outcome.value {
        ProbeValue::RectangleCenter {
            center,
            width_mm,
            height_mm,
        } => {
            assert!(center.distance(XyPoint::new(0.0, 0.0)) < 1e-9);
            assert!((width_mm - 20.0).abs() < 1e-12);
            assert!((height_mm - 10.0).abs() < 1e-12);
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn rotation_outcome_uses_edge_corrected_points() {
    let params = ProbeParams {
        probe_diameter_mm: 2.0,
        ..params()
    };
    let cycle = ProbeCycle::Rotation(RotationProbe {
        separation_mm: 20.0,
        edge_axis: ProbeAxis::X,
        approach: Sign::Positive,
        expected_edge_mm: 5.0,
    });
    let results = [
        ProbeResult::new(true, Position::xyz(0.0, 4.0, -2.0)),
        ProbeResult::new(true, Position::xyz(20.0, 5.0, -2.0)),
    ];
    let outcome = ghs_probe::ProbeOutcome::from_results(&cycle, &params, &results).unwrap();
    match outcome.value {
        ProbeValue::Rotation {
            axis,
            p1,
            p2,
            angle_deg,
            nominal_deg,
            deviation_deg,
            ..
        } => {
            assert_eq!(axis, ghs_probe::RotationAxis::Z);
            assert_eq!(p1, XyPoint::new(0.0, 5.0));
            assert_eq!(p2, XyPoint::new(20.0, 6.0));
            assert!((angle_deg - 2.862_405_226_111_747_4).abs() < 1e-9);
            assert_eq!(nominal_deg, 0.0);
            assert!((deviation_deg - angle_deg).abs() < 1e-9);
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn xy_plate_outcome_computes_the_offset_zero() {
    let params = ProbeParams {
        edge_offset_mm: 2.0,
        ..params()
    };
    let cycle = ProbeCycle::XyTouchPlate(CornerProbe::new(
        EdgeSpec {
            axis: ProbeAxis::X,
            mode: EdgeMode::Outside,
            approach: Sign::Positive,
            expected_edge_mm: 20.0,
        },
        EdgeSpec {
            axis: ProbeAxis::Y,
            mode: EdgeMode::Outside,
            approach: Sign::Positive,
            expected_edge_mm: 30.0,
        },
    ));
    let results = [
        ProbeResult::new(true, Position::xyz(18.0, 0.0, -2.0)),
        ProbeResult::new(true, Position::xyz(0.0, 28.0, -2.0)),
    ];
    let outcome = ghs_probe::ProbeOutcome::from_results(&cycle, &params, &results).unwrap();
    match outcome.value {
        ProbeValue::XyZero { zero, .. } => {
            assert_eq!(zero, XyPoint::new(22.0, 32.0));
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn z_and_tool_length_outcomes() {
    let params = params();
    let z_cycle = ProbeCycle::ZTouchPlate {
        expected_plate_z_mm: -5.0,
        plate_thickness_mm: 1.5,
    };
    let z_outcome = ghs_probe::ProbeOutcome::from_results(
        &z_cycle,
        &params,
        &[ProbeResult::new(true, Position::xyz(0.0, 0.0, -5.0))],
    )
    .unwrap();
    match z_outcome.value {
        ProbeValue::ZZero {
            contact_z_mm,
            plate_thickness_mm,
            zero_z_mm,
        } => {
            assert!((contact_z_mm + 5.0).abs() < 1e-12);
            assert!((plate_thickness_mm - 1.5).abs() < 1e-12);
            assert!((zero_z_mm + 6.5).abs() < 1e-12);
        }
        other => panic!("unexpected value: {other:?}"),
    }

    let tool_cycle = ProbeCycle::ToolLength {
        reference_z_mm: -10.0,
    };
    let tool_outcome = ghs_probe::ProbeOutcome::from_results(
        &tool_cycle,
        &params,
        &[ProbeResult::new(true, Position::xyz(0.0, 0.0, -8.0))],
    )
    .unwrap();
    match tool_outcome.value {
        ProbeValue::ToolLength { offset_mm, .. } => {
            assert!((offset_mm - 2.0).abs() < 1e-12);
        }
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn failed_and_mismatched_results_are_typed_errors() {
    let params = params();
    let cycle = ProbeCycle::Edge(EdgeSpec {
        axis: ProbeAxis::X,
        mode: EdgeMode::Outside,
        approach: Sign::Positive,
        expected_edge_mm: 20.0,
    });
    assert!(matches!(
        ghs_probe::ProbeOutcome::from_results(
            &cycle,
            &params,
            &[ProbeResult::new(false, Position::zero())]
        ),
        Err(ghs_probe::ProbeError::ProbeFailed { index: 0 })
    ));
    assert!(matches!(
        ghs_probe::ProbeOutcome::from_results(&cycle, &params, &[]),
        Err(ghs_probe::ProbeError::ResultCount {
            expected: 1,
            actual: 0
        })
    ));
    assert!(matches!(
        ghs_probe::ProbeOutcome::from_results(
            &cycle,
            &params,
            &[ProbeResult::new(true, Position::xyz(f64::NAN, 0.0, 0.0))]
        ),
        Err(ghs_probe::ProbeError::NonFiniteProbe { index: 0 })
    ));
}

#[test]
fn outcome_raw_positions_match_the_inputs() {
    let params = params();
    let cycle = ProbeCycle::Edge(EdgeSpec {
        axis: ProbeAxis::X,
        mode: EdgeMode::Outside,
        approach: Sign::Positive,
        expected_edge_mm: 20.0,
    });
    let raw = Position::xyz(18.0, 1.0, -2.0);
    let outcome =
        ghs_probe::ProbeOutcome::from_results(&cycle, &params, &[ProbeResult::new(true, raw)])
            .unwrap();
    let positions: Vec<Position> = outcome.raw_positions().collect();
    assert_eq!(positions, vec![raw]);
}

#[test]
fn circle_fit_type_is_public() {
    let fit: CircleFit = fit_circle(&[
        XyPoint::new(1.0, 0.0),
        XyPoint::new(0.0, 1.0),
        XyPoint::new(-1.0, 0.0),
    ])
    .unwrap();
    assert!(fit.center.distance(XyPoint::new(0.0, 0.0)) < 1e-9);
}
