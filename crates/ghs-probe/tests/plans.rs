//! Golden G-code and safety invariants for every cycle.

use ghs_core::{Position, Units, Wcs};
use ghs_probe::{
    CornerProbe, CycleKind, EdgeMode, EdgeSpec, HeightMapGrid, ProbeAxis, ProbeCycle,
    ProbeMoveCode, ProbeParams, RotationProbe, Sign, XyPoint,
};

fn base_params() -> ProbeParams {
    ProbeParams {
        probe_diameter_mm: 4.0,
        position: Some(Position::xyz(0.0, 0.0, -2.0)),
        ..ProbeParams::default()
    }
}

fn program(lines: &[&str]) -> String {
    lines.join("\n")
}

fn assert_safe(plan: &ghs_probe::ProbePlan, params: &ProbeParams) {
    let lines: Vec<&str> = plan.program.lines().collect();
    assert_eq!(lines.first().copied(), Some("G21 G90 G17 G94"));
    assert_eq!(lines.get(1).copied(), Some(plan.wcs.as_str()));
    assert_eq!(lines.get(2).copied(), Some("M5"));
    assert_eq!(lines.get(3).copied(), Some("M9"));
    assert!(lines.get(4).is_some_and(|line| line.starts_with("G0 Z")));
    let expected_last = format!("G0 Z{:.3}", plan.safe_z_mm);
    assert_eq!(lines.last().copied(), Some(expected_last.as_str()));
    for (index, line) in lines.iter().enumerate() {
        if !line.starts_with("G38.2") {
            continue;
        }
        assert_eq!(lines.get(index + 1).copied(), Some("G91"));
        assert!(
            lines
                .get(index + 2)
                .is_some_and(|next| next.starts_with("G0 Z")),
            "probe at {index} is not followed by a relative retract"
        );
        assert_eq!(lines.get(index + 3).copied(), Some("G90"));
        assert_eq!(lines.get(index + 4).copied(), Some(expected_last.as_str()));
    }
    assert_eq!(plan.expected_probes, plan.probes.len());
    assert!(plan.preconditions.requires_probe);
    assert!(plan.preconditions.requires_probe_verification);
    assert!(plan.preconditions.requires_spindle_off);
    assert!(plan.preconditions.requires_coolant_off);
    assert!(plan.preconditions.requires_idle);
    assert!(plan.preconditions.requires_operator_confirmation);
    assert_eq!(plan.preconditions.safe_z_mm, params.safe_z_mm);
    assert_eq!(
        plan.preconditions.search_distance_mm,
        params.search_distance_mm
    );

    for line in &lines {
        let trimmed = line.trim();
        assert!(!trimmed.starts_with("M3"), "spindle start in {trimmed}");
        assert!(!trimmed.starts_with("M4"), "spindle start in {trimmed}");
        assert!(!trimmed.starts_with("M7"), "coolant on in {trimmed}");
        assert!(!trimmed.starts_with("M8"), "coolant on in {trimmed}");
        assert!(!trimmed.contains('~'), "cycle start in {trimmed}");
        assert!(!trimmed.starts_with("G38.3"));
        assert!(!trimmed.starts_with("G38.4"));
        assert!(!trimmed.starts_with("G38.5"));
    }

    for probe in &plan.probes {
        assert_eq!(probe.code, ProbeMoveCode::G38_2);
        assert!(probe.travel_mm > 0.0);
        assert!(
            probe.travel_mm <= params.search_distance_mm + 1e-9,
            "travel {} exceeds search {}",
            probe.travel_mm,
            params.search_distance_mm
        );
        assert!((probe.direction.length() - 1.0).abs() < 1e-9);
        assert_eq!(probe.feed_mm_min, params.feed_mm_min);
    }
}

#[test]
fn z_touch_plate_golden() {
    let params = ProbeParams {
        position: Some(Position::xyz(10.0, 10.0, -2.0)),
        ..base_params()
    };
    let cycle = ProbeCycle::ZTouchPlate {
        expected_plate_z_mm: 0.0,
        plate_thickness_mm: 1.0,
    };
    let plan = cycle.plan(&params).expect("valid plan");
    assert_safe(&plan, &params);
    assert_eq!(plan.name, "z-touch-plate");
    assert_eq!(plan.expected_probes, 1);
    assert_eq!(
        plan.program,
        program(&[
            "G21 G90 G17 G94",
            "G54",
            "M5",
            "M9",
            "G0 Z10.000",
            "G0 X10.000 Y10.000",
            "G0 Z5.000",
            "G38.2 Z0.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
        ])
    );
}

#[test]
fn z_touch_plate_without_position_uses_current_xy() {
    let params = ProbeParams {
        position: None,
        ..base_params()
    };
    let plan = ProbeCycle::ZTouchPlate {
        expected_plate_z_mm: 0.0,
        plate_thickness_mm: 0.0,
    }
    .plan(&params)
    .expect("valid plan");
    assert_safe(&plan, &params);
    assert!(!plan.program.contains("G0 X"));
    assert!(plan.program.contains("G38.2 Z0.000 F100.000"));
}

#[test]
fn tool_length_golden() {
    let params = ProbeParams {
        position: Some(Position::xyz(5.0, 6.0, 0.0)),
        ..base_params()
    };
    let plan = ProbeCycle::ToolLength {
        reference_z_mm: 0.0,
    }
    .plan(&params)
    .expect("valid plan");
    assert_safe(&plan, &params);
    assert_eq!(plan.name, "tool-length");
    assert_eq!(
        plan.program,
        program(&[
            "G21 G90 G17 G94",
            "G54",
            "M5",
            "M9",
            "G0 Z10.000",
            "G0 X5.000 Y6.000",
            "G0 Z5.000",
            "G38.2 Z0.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
        ])
    );
}

#[test]
fn edge_x_golden() {
    let params = base_params();
    let plan = ProbeCycle::Edge(EdgeSpec {
        axis: ProbeAxis::X,
        mode: EdgeMode::Outside,
        approach: Sign::Positive,
        expected_edge_mm: 20.0,
    })
    .plan(&params)
    .expect("valid plan");
    assert_safe(&plan, &params);
    assert_eq!(plan.name, "edge-x-outside-positive");
    assert_eq!(
        plan.program,
        program(&[
            "G21 G90 G17 G94",
            "G54",
            "M5",
            "M9",
            "G0 Z10.000",
            "G0 X13.000 Y0.000",
            "G0 Z-2.000",
            "G38.2 X18.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
        ])
    );
}

#[test]
fn edge_y_inside_golden() {
    let params = base_params();
    let plan = ProbeCycle::Edge(EdgeSpec {
        axis: ProbeAxis::Y,
        mode: EdgeMode::Inside,
        approach: Sign::Negative,
        expected_edge_mm: 20.0,
    })
    .plan(&params)
    .expect("valid plan");
    assert_safe(&plan, &params);
    assert_eq!(plan.name, "edge-y-inside-negative");
    assert_eq!(
        plan.program,
        program(&[
            "G21 G90 G17 G94",
            "G54",
            "M5",
            "M9",
            "G0 Z10.000",
            "G0 X0.000 Y27.000",
            "G0 Z-2.000",
            "G38.2 Y22.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
        ])
    );
}

#[test]
fn corner_golden() {
    let params = base_params();
    let plan = ProbeCycle::Corner(CornerProbe::new(
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
    ))
    .plan(&params)
    .expect("valid plan");
    assert_safe(&plan, &params);
    assert_eq!(plan.name, "corner");
    assert_eq!(plan.expected_probes, 2);
    assert_eq!(
        plan.program,
        program(&[
            "G21 G90 G17 G94",
            "G54",
            "M5",
            "M9",
            "G0 Z10.000",
            "G0 X13.000 Y0.000",
            "G0 Z-2.000",
            "G38.2 X18.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
            "G0 X0.000 Y23.000",
            "G0 Z-2.000",
            "G38.2 Y28.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
        ])
    );
}

#[test]
fn circle_inside_golden() {
    let params = ProbeParams {
        search_distance_mm: 2.0,
        probe_diameter_mm: 2.0,
        center: Some(Position::xyz(10.0, 20.0, 0.0)),
        radius_mm: Some(5.0),
        ..base_params()
    };
    let plan = ProbeCycle::CenterCircle {
        touch_count: 3,
        mode: EdgeMode::Inside,
    }
    .plan(&params)
    .expect("valid plan");
    assert_safe(&plan, &params);
    assert_eq!(plan.name, "center-circle-inside");
    assert_eq!(plan.expected_probes, 3);
    assert_eq!(
        plan.program,
        program(&[
            "G21 G90 G17 G94",
            "G54",
            "M5",
            "M9",
            "G0 Z10.000",
            "G0 X12.000 Y20.000",
            "G0 Z0.000",
            "G38.2 X14.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
            "G0 X9.000 Y21.732",
            "G0 Z0.000",
            "G38.2 X8.000 Y23.464 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
            "G0 X9.000 Y18.268",
            "G0 Z0.000",
            "G38.2 X8.000 Y16.536 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
        ])
    );
}

#[test]
fn circle_outside_golden() {
    let params = ProbeParams {
        search_distance_mm: 2.0,
        probe_diameter_mm: 2.0,
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        radius_mm: Some(5.0),
        ..base_params()
    };
    let plan = ProbeCycle::CenterCircle {
        touch_count: 4,
        mode: EdgeMode::Outside,
    }
    .plan(&params)
    .expect("valid plan");
    assert_safe(&plan, &params);
    assert_eq!(plan.name, "center-circle-outside");
    assert_eq!(plan.expected_probes, 4);
    let g38: Vec<&str> = plan
        .program
        .lines()
        .filter(|line| line.starts_with("G38.2"))
        .collect();
    assert_eq!(
        g38,
        vec![
            "G38.2 X6.000 F100.000",
            "G38.2 Y6.000 F100.000",
            "G38.2 X-6.000 F100.000",
            "G38.2 Y-6.000 F100.000",
        ]
    );
    assert!(plan.program.contains("G0 X8.000 Y0.000"));
    assert!(plan.program.contains("G0 X0.000 Y8.000"));
}

#[test]
fn rectangle_inside_golden() {
    let params = ProbeParams {
        search_distance_mm: 3.0,
        probe_diameter_mm: 2.0,
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        ..base_params()
    };
    let plan = ProbeCycle::CenterRectangle {
        width_mm: 20.0,
        height_mm: 10.0,
        mode: EdgeMode::Inside,
    }
    .plan(&params)
    .expect("valid plan");
    assert_safe(&plan, &params);
    assert_eq!(plan.name, "center-rectangle-inside");
    assert_eq!(plan.expected_probes, 4);
    assert_eq!(
        plan.program,
        program(&[
            "G21 G90 G17 G94",
            "G54",
            "M5",
            "M9",
            "G0 Z10.000",
            "G0 X6.000 Y0.000",
            "G0 Z0.000",
            "G38.2 X9.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
            "G0 X-6.000 Y0.000",
            "G0 Z0.000",
            "G38.2 X-9.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
            "G0 X0.000 Y1.000",
            "G0 Z0.000",
            "G38.2 Y4.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
            "G0 X0.000 Y-1.000",
            "G0 Z0.000",
            "G38.2 Y-4.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
        ])
    );
}

#[test]
fn rectangle_outside_plan_moves_inward_from_outside() {
    let params = ProbeParams {
        search_distance_mm: 3.0,
        probe_diameter_mm: 2.0,
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        ..base_params()
    };
    let plan = ProbeCycle::CenterRectangle {
        width_mm: 20.0,
        height_mm: 10.0,
        mode: EdgeMode::Outside,
    }
    .plan(&params)
    .expect("valid plan");
    assert_safe(&plan, &params);
    let g38: Vec<&str> = plan
        .program
        .lines()
        .filter(|line| line.starts_with("G38.2"))
        .collect();
    assert_eq!(
        g38,
        vec![
            "G38.2 X11.000 F100.000",
            "G38.2 X-11.000 F100.000",
            "G38.2 Y6.000 F100.000",
            "G38.2 Y-6.000 F100.000",
        ]
    );
    assert!(plan.program.contains("G0 X14.000 Y0.000"));
    assert!(plan.program.contains("G0 X-14.000 Y0.000"));
    assert!(plan.program.contains("G0 X0.000 Y9.000"));
    assert!(plan.program.contains("G0 X0.000 Y-9.000"));
}

#[test]
fn rotation_golden() {
    let params = ProbeParams {
        search_distance_mm: 2.0,
        probe_diameter_mm: 2.0,
        ..base_params()
    };
    let plan = ProbeCycle::Rotation(RotationProbe {
        separation_mm: 20.0,
        edge_axis: ProbeAxis::X,
        approach: Sign::Positive,
        expected_edge_mm: 5.0,
    })
    .plan(&params)
    .expect("valid plan");
    assert_safe(&plan, &params);
    assert_eq!(plan.name, "rotation");
    assert_eq!(plan.expected_probes, 2);
    assert_eq!(
        plan.program,
        program(&[
            "G21 G90 G17 G94",
            "G54",
            "M5",
            "M9",
            "G0 Z10.000",
            "G0 X0.000 Y2.000",
            "G0 Z-2.000",
            "G38.2 Y4.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
            "G0 X20.000 Y2.000",
            "G0 Z-2.000",
            "G38.2 Y4.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
        ])
    );
}

#[test]
fn xy_touch_plate_golden() {
    let params = base_params();
    let plan = ProbeCycle::XyTouchPlate(CornerProbe::new(
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
    ))
    .plan(&params)
    .expect("valid plan");
    assert_safe(&plan, &params);
    assert_eq!(plan.name, "xy-touch-plate");
    assert_eq!(plan.expected_probes, 2);
    assert!(plan.program.contains("G38.2 X18.000 F100.000"));
    assert!(plan.program.contains("G38.2 Y28.000 F100.000"));
}

#[test]
fn height_map_golden() {
    let params = ProbeParams {
        search_distance_mm: 10.0,
        position: None,
        ..base_params()
    };
    let grid = HeightMapGrid {
        origin_mm: XyPoint::new(0.0, 0.0),
        step_x_mm: 10.0,
        step_y_mm: 10.0,
        columns: 2,
        rows: 2,
        surface_z_mm: 0.0,
    };
    let plan = grid.plan(&params).expect("valid plan");
    assert_safe(&plan, &params);
    assert_eq!(plan.name, "height-map");
    assert_eq!(plan.kind, CycleKind::HeightMap);
    assert_eq!(plan.expected_probes, 4);
    assert_eq!(
        plan.program,
        program(&[
            "G21 G90 G17 G94",
            "G54",
            "M5",
            "M9",
            "G0 Z10.000",
            "G0 X0.000 Y0.000",
            "G38.2 Z0.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
            "G0 X10.000 Y0.000",
            "G38.2 Z0.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
            "G0 X0.000 Y10.000",
            "G38.2 Z0.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
            "G0 X10.000 Y10.000",
            "G38.2 Z0.000 F100.000",
            "G91",
            "G0 Z2.000",
            "G90",
            "G0 Z10.000",
        ])
    );
}

#[test]
fn units_and_wcs_do_not_change_the_millimetre_program() {
    let mm = ProbeCycle::Edge(EdgeSpec {
        axis: ProbeAxis::X,
        mode: EdgeMode::Outside,
        approach: Sign::Positive,
        expected_edge_mm: 20.0,
    })
    .plan(&base_params())
    .expect("valid plan");

    let inch_params = ProbeParams {
        units: Units::Inch,
        wcs: Wcs::G55,
        ..base_params()
    };
    let inch = ProbeCycle::Edge(EdgeSpec {
        axis: ProbeAxis::X,
        mode: EdgeMode::Outside,
        approach: Sign::Positive,
        expected_edge_mm: 20.0,
    })
    .plan(&inch_params)
    .expect("valid plan");
    assert!(inch.program.starts_with("G21 G90 G17 G94"));
    assert!(inch.program.contains("\nG55\n"));
    assert_eq!(inch.program, mm.program.replace("\nG54\n", "\nG55\n"));
}

#[test]
fn plan_subtracts_the_active_work_offset_but_records_machine_positions() {
    let params = ProbeParams {
        position: Some(Position::xyz(0.0, 0.0, -2.0)),
        wcs_offset_mm: Position::xyz(100.0, 200.0, 10.0),
        wcs: Wcs::G55,
        ..base_params()
    };
    let plan = ProbeCycle::Edge(EdgeSpec {
        axis: ProbeAxis::X,
        mode: EdgeMode::Outside,
        approach: Sign::Positive,
        expected_edge_mm: 20.0,
    })
    .plan(&params)
    .expect("valid plan");
    assert!(plan.program.contains("\nG55\n"));
    assert!(plan.program.contains("G0 Z0.000"));
    assert!(plan.program.contains("G0 X-87.000 Y-200.000"));
    assert!(plan.program.contains("G0 Z-12.000"));
    assert!(plan.program.contains("G38.2 X-82.000 F100.000"));
    assert_eq!(plan.probes[0].from, Position::xyz(13.0, 0.0, -2.0));
    assert_eq!(plan.probes[0].to, Position::xyz(18.0, 0.0, -2.0));
}

#[test]
fn z_only_probe_with_a_work_offset_stays_z_only() {
    let params = ProbeParams {
        position: None,
        wcs_offset_mm: Position::xyz(100.0, 200.0, 10.0),
        ..base_params()
    };
    let plan = ProbeCycle::ZTouchPlate {
        expected_plate_z_mm: 0.0,
        plate_thickness_mm: 0.0,
    }
    .plan(&params)
    .expect("valid plan");
    assert!(plan.program.contains("G0 Z0.000"));
    assert!(plan.program.contains("G0 Z-5.000"));
    assert!(plan.program.contains("G38.2 Z-10.000 F100.000"));
    assert_eq!(plan.probes[0].from.axes().to_string(), "Z");
    assert_eq!(plan.probes[0].to.axes().to_string(), "Z");
    assert_eq!(plan.probes[0].from.get(ghs_core::Axis::Z), 5.0);
    assert_eq!(plan.probes[0].to.get(ghs_core::Axis::Z), 0.0);
}

#[test]
fn every_cycle_plan_has_the_same_safety_preamble_and_retract() {
    let params = ProbeParams {
        search_distance_mm: 2.0,
        center: Some(Position::xyz(0.0, 0.0, 0.0)),
        radius_mm: Some(5.0),
        ..base_params()
    };
    let cycles = [
        ProbeCycle::ZTouchPlate {
            expected_plate_z_mm: 0.0,
            plate_thickness_mm: 0.0,
        },
        ProbeCycle::ToolLength {
            reference_z_mm: 0.0,
        },
        ProbeCycle::Edge(EdgeSpec::new(
            ProbeAxis::X,
            EdgeMode::Outside,
            Sign::Positive,
            20.0,
        )),
        ProbeCycle::Corner(CornerProbe::new(
            EdgeSpec::new(ProbeAxis::X, EdgeMode::Outside, Sign::Positive, 20.0),
            EdgeSpec::new(ProbeAxis::Y, EdgeMode::Outside, Sign::Positive, 20.0),
        )),
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
            EdgeSpec::new(ProbeAxis::Y, EdgeMode::Outside, Sign::Positive, 20.0),
        )),
    ];
    for cycle in cycles {
        let plan = cycle.plan(&params).expect("valid plan");
        assert_safe(&plan, &params);
        assert_eq!(plan.kind, cycle.kind());
        assert_eq!(plan.expected_probes, cycle.expected_probes());
        assert_eq!(cycle.labels().len(), cycle.expected_probes());
    }
}
