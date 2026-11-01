//! Parameter and cycle validation tests: bad input returns typed errors.

use ghs_core::{Axes, Position};
use ghs_probe::{
    CornerProbe, EdgeMode, EdgeSpec, MAX_EDGE_OFFSET_MM, MAX_PROBE_DIAMETER_MM,
    MAX_PROBE_FEED_MM_MIN, MAX_RADIUS_MM, MAX_RETRACT_MM, MAX_SEARCH_DISTANCE_MM, ProbeAxis,
    ProbeCycle, ProbeParams, RotationProbe, Sign,
};

fn base_params() -> ProbeParams {
    ProbeParams {
        probe_diameter_mm: 4.0,
        position: Some(Position::xyz(0.0, 0.0, -2.0)),
        ..ProbeParams::default()
    }
}

fn edge_cycle() -> ProbeCycle {
    ProbeCycle::Edge(EdgeSpec {
        axis: ProbeAxis::X,
        mode: EdgeMode::Outside,
        approach: Sign::Positive,
        expected_edge_mm: 20.0,
    })
}

#[test]
fn defaults_are_valid() {
    assert!(ProbeParams::default().validate().is_ok());
    assert!(base_params().validate().is_ok());
}

#[test]
fn non_finite_and_out_of_range_parameters_are_rejected() {
    type Mutation = fn(&mut ProbeParams);
    let cases: &[(&str, Mutation)] = &[
        ("safe NaN", |p| p.safe_z_mm = f64::NAN),
        ("safe inf", |p| p.safe_z_mm = f64::INFINITY),
        ("safe huge", |p| p.safe_z_mm = 1.0e9),
        ("search zero", |p| p.search_distance_mm = 0.0),
        ("search negative", |p| p.search_distance_mm = -1.0),
        ("search NaN", |p| p.search_distance_mm = f64::NAN),
        ("search infinite", |p| p.search_distance_mm = f64::INFINITY),
        ("search too large", |p| {
            p.search_distance_mm = MAX_SEARCH_DISTANCE_MM + 0.1;
        }),
        ("feed zero", |p| p.feed_mm_min = 0.0),
        ("feed negative", |p| p.feed_mm_min = -1.0),
        ("feed NaN", |p| p.feed_mm_min = f64::NAN),
        ("feed too large", |p| {
            p.feed_mm_min = MAX_PROBE_FEED_MM_MIN + 0.1;
        }),
        ("retract zero", |p| p.retract_mm = 0.0),
        ("retract negative", |p| p.retract_mm = -1.0),
        ("retract NaN", |p| p.retract_mm = f64::NAN),
        ("retract too large", |p| p.retract_mm = MAX_RETRACT_MM + 0.1),
        ("diameter negative", |p| p.probe_diameter_mm = -1.0),
        ("diameter NaN", |p| p.probe_diameter_mm = f64::NAN),
        ("diameter too large", |p| {
            p.probe_diameter_mm = MAX_PROBE_DIAMETER_MM + 0.1;
        }),
        ("offset negative", |p| p.edge_offset_mm = -1.0),
        ("offset NaN", |p| p.edge_offset_mm = f64::NAN),
        ("offset too large", |p| {
            p.edge_offset_mm = MAX_EDGE_OFFSET_MM + 0.1
        }),
        ("position NaN", |p| {
            p.position = Some(Position::xyz(f64::NAN, 0.0, 0.0));
        }),
        ("position infinite", |p| {
            p.position = Some(Position::xyz(0.0, f64::INFINITY, 0.0));
        }),
        ("center NaN", |p| {
            p.center = Some(Position::xyz(0.0, f64::NAN, 0.0));
        }),
        ("work offset NaN", |p| {
            p.wcs_offset_mm = Position::xyz(f64::NAN, 0.0, 0.0);
        }),
        ("radius zero", |p| p.radius_mm = Some(0.0)),
        ("radius negative", |p| p.radius_mm = Some(-1.0)),
        ("radius NaN", |p| p.radius_mm = Some(f64::NAN)),
        ("radius too large", |p| {
            p.radius_mm = Some(MAX_RADIUS_MM + 0.1)
        }),
    ];
    for (name, mutate) in cases {
        let mut candidate = base_params();
        mutate(&mut candidate);
        assert!(
            candidate.validate().is_err(),
            "case '{name}' should be rejected"
        );
    }
}

#[test]
fn exact_upper_bounds_are_accepted_by_shared_validation() {
    let candidate = ProbeParams {
        safe_z_mm: 10.0,
        search_distance_mm: MAX_SEARCH_DISTANCE_MM,
        feed_mm_min: MAX_PROBE_FEED_MM_MIN,
        retract_mm: MAX_RETRACT_MM,
        probe_diameter_mm: MAX_PROBE_DIAMETER_MM,
        edge_offset_mm: MAX_EDGE_OFFSET_MM,
        radius_mm: Some(MAX_RADIUS_MM),
        ..base_params()
    };
    assert!(candidate.validate().is_ok());
}

#[test]
fn vertical_cycles_reject_a_safe_z_below_the_search_start() {
    let low = ProbeParams {
        safe_z_mm: 4.0,
        search_distance_mm: 5.0,
        ..base_params()
    };
    assert!(
        ProbeCycle::ZTouchPlate {
            expected_plate_z_mm: 0.0,
            plate_thickness_mm: 0.0,
        }
        .plan(&low)
        .is_err()
    );
    assert!(
        ProbeCycle::ToolLength {
            reference_z_mm: 0.0,
        }
        .plan(&low)
        .is_err()
    );
}

#[test]
fn z_and_tool_parameters_are_validated() {
    let params = base_params();
    assert!(
        ProbeCycle::ZTouchPlate {
            expected_plate_z_mm: f64::NAN,
            plate_thickness_mm: 0.0,
        }
        .plan(&params)
        .is_err()
    );
    assert!(
        ProbeCycle::ZTouchPlate {
            expected_plate_z_mm: 0.0,
            plate_thickness_mm: -1.0,
        }
        .plan(&params)
        .is_err()
    );
    assert!(
        ProbeCycle::ZTouchPlate {
            expected_plate_z_mm: 0.0,
            plate_thickness_mm: 1.0e6,
        }
        .plan(&params)
        .is_err()
    );
    assert!(
        ProbeCycle::ToolLength {
            reference_z_mm: f64::INFINITY,
        }
        .plan(&params)
        .is_err()
    );
}

#[test]
fn xy_cycles_require_a_position_and_a_probing_height_below_safe_z() {
    let no_position = ProbeParams {
        position: None,
        ..base_params()
    };
    assert!(edge_cycle().plan(&no_position).is_err());
    assert!(
        ProbeCycle::Corner(CornerProbe::new(
            EdgeSpec::new(ProbeAxis::X, EdgeMode::Outside, Sign::Positive, 20.0),
            EdgeSpec::new(ProbeAxis::Y, EdgeMode::Outside, Sign::Positive, 20.0),
        ))
        .plan(&no_position)
        .is_err()
    );
    assert!(
        ProbeCycle::Rotation(RotationProbe {
            separation_mm: 10.0,
            edge_axis: ProbeAxis::X,
            approach: Sign::Positive,
            expected_edge_mm: 5.0,
        })
        .plan(&no_position)
        .is_err()
    );
    assert!(
        ProbeCycle::XyTouchPlate(CornerProbe::new(
            EdgeSpec::new(ProbeAxis::X, EdgeMode::Outside, Sign::Positive, 20.0),
            EdgeSpec::new(ProbeAxis::Y, EdgeMode::Outside, Sign::Positive, 20.0),
        ))
        .plan(&no_position)
        .is_err()
    );

    let above_safe = ProbeParams {
        position: Some(Position::xyz(0.0, 0.0, 20.0)),
        ..base_params()
    };
    assert!(edge_cycle().plan(&above_safe).is_err());
}

#[test]
fn xy_cycles_require_a_z_axis_in_their_anchor() {
    let xy_only = Position::new(Axes::parse("XY").expect("valid axes"));
    let no_position_z = ProbeParams {
        position: Some(xy_only),
        ..base_params()
    };
    assert!(edge_cycle().plan(&no_position_z).is_err());

    let no_center_z = ProbeParams {
        center: Some(xy_only),
        radius_mm: Some(5.0),
        ..base_params()
    };
    assert!(
        ProbeCycle::CenterCircle {
            touch_count: 3,
            mode: EdgeMode::Inside,
        }
        .plan(&no_center_z)
        .is_err()
    );
    assert!(
        ProbeCycle::CenterRectangle {
            width_mm: 20.0,
            height_mm: 10.0,
            mode: EdgeMode::Inside,
        }
        .plan(&no_center_z)
        .is_err()
    );
}

#[test]
fn edge_and_corner_geometry_is_validated() {
    let params = base_params();
    assert!(
        ProbeCycle::Edge(EdgeSpec {
            axis: ProbeAxis::X,
            mode: EdgeMode::Outside,
            approach: Sign::Positive,
            expected_edge_mm: f64::NAN,
        })
        .plan(&params)
        .is_err()
    );
    assert!(
        ProbeCycle::Corner(CornerProbe::new(
            EdgeSpec::new(ProbeAxis::Y, EdgeMode::Outside, Sign::Positive, 20.0),
            EdgeSpec::new(ProbeAxis::Y, EdgeMode::Outside, Sign::Positive, 20.0),
        ))
        .plan(&params)
        .is_err()
    );
    assert!(
        ProbeCycle::Corner(CornerProbe::new(
            EdgeSpec::new(ProbeAxis::X, EdgeMode::Outside, Sign::Positive, 20.0),
            EdgeSpec::new(ProbeAxis::X, EdgeMode::Outside, Sign::Positive, 20.0),
        ))
        .plan(&params)
        .is_err()
    );
}

#[test]
fn circle_geometry_is_validated() {
    let params = base_params();
    assert!(
        ProbeCycle::CenterCircle {
            touch_count: 3,
            mode: EdgeMode::Inside,
        }
        .plan(&params)
        .is_err()
    );

    let missing_radius = ProbeParams {
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        ..base_params()
    };
    assert!(
        ProbeCycle::CenterCircle {
            touch_count: 3,
            mode: EdgeMode::Inside,
        }
        .plan(&missing_radius)
        .is_err()
    );

    let zero_radius = ProbeParams {
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        radius_mm: Some(0.0),
        ..base_params()
    };
    assert!(
        ProbeCycle::CenterCircle {
            touch_count: 3,
            mode: EdgeMode::Inside,
        }
        .plan(&zero_radius)
        .is_err()
    );

    let small_radius = ProbeParams {
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        radius_mm: Some(1.0),
        ..base_params()
    };
    assert!(
        ProbeCycle::CenterCircle {
            touch_count: 3,
            mode: EdgeMode::Inside,
        }
        .plan(&small_radius)
        .is_err()
    );

    let valid_center = ProbeParams {
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        radius_mm: Some(5.0),
        ..base_params()
    };
    for touch_count in [0, 1, 2, 9, 255] {
        assert!(
            ProbeCycle::CenterCircle {
                touch_count,
                mode: EdgeMode::Inside,
            }
            .plan(&valid_center)
            .is_err(),
            "touch count {touch_count} should be rejected"
        );
    }
}

#[test]
fn rectangle_geometry_is_validated() {
    let params = base_params();
    assert!(
        ProbeCycle::CenterRectangle {
            width_mm: 20.0,
            height_mm: 10.0,
            mode: EdgeMode::Inside,
        }
        .plan(&params)
        .is_err()
    );

    let center = ProbeParams {
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        ..base_params()
    };
    for (width, height) in [(0.0, 10.0), (20.0, 0.0), (-1.0, 10.0), (20.0, f64::NAN)] {
        assert!(
            ProbeCycle::CenterRectangle {
                width_mm: width,
                height_mm: height,
                mode: EdgeMode::Inside,
            }
            .plan(&center)
            .is_err()
        );
    }
    assert!(
        ProbeCycle::CenterRectangle {
            width_mm: 4.0,
            height_mm: 10.0,
            mode: EdgeMode::Inside,
        }
        .plan(&center)
        .is_err()
    );
    assert!(
        ProbeCycle::CenterRectangle {
            width_mm: 20.0,
            height_mm: 4.0,
            mode: EdgeMode::Inside,
        }
        .plan(&center)
        .is_err()
    );
}

#[test]
fn rotation_geometry_is_validated() {
    let params = base_params();
    for separation in [0.0, -1.0, f64::NAN, 1.0e9] {
        assert!(
            ProbeCycle::Rotation(RotationProbe {
                separation_mm: separation,
                edge_axis: ProbeAxis::X,
                approach: Sign::Positive,
                expected_edge_mm: 5.0,
            })
            .plan(&params)
            .is_err(),
            "separation {separation} should be rejected"
        );
    }
    assert!(
        ProbeCycle::Rotation(RotationProbe {
            separation_mm: 10.0,
            edge_axis: ProbeAxis::X,
            approach: Sign::Positive,
            expected_edge_mm: f64::NAN,
        })
        .plan(&params)
        .is_err()
    );
}

#[test]
fn bad_cycles_return_errors_without_panicking() {
    let params = base_params();
    let cases = [
        ProbeCycle::ZTouchPlate {
            expected_plate_z_mm: f64::NAN,
            plate_thickness_mm: 0.0,
        },
        ProbeCycle::ToolLength {
            reference_z_mm: f64::NEG_INFINITY,
        },
        ProbeCycle::Edge(EdgeSpec::new(
            ProbeAxis::X,
            EdgeMode::Outside,
            Sign::Positive,
            f64::NAN,
        )),
        ProbeCycle::Corner(CornerProbe::new(
            EdgeSpec::new(ProbeAxis::Y, EdgeMode::Inside, Sign::Negative, -1.0e12),
            EdgeSpec::new(ProbeAxis::X, EdgeMode::Inside, Sign::Negative, 1.0e12),
        )),
        ProbeCycle::CenterCircle {
            touch_count: 255,
            mode: EdgeMode::Inside,
        },
        ProbeCycle::CenterRectangle {
            width_mm: -20.0,
            height_mm: f64::INFINITY,
            mode: EdgeMode::Outside,
        },
        ProbeCycle::Rotation(RotationProbe {
            separation_mm: f64::NAN,
            edge_axis: ProbeAxis::X,
            approach: Sign::Positive,
            expected_edge_mm: 5.0,
        }),
    ];
    for cycle in cases {
        assert!(cycle.plan(&params).is_err());
    }
}
