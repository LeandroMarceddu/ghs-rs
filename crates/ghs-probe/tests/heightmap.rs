//! Height-map bounds, interpolation, validity, and plan tests.

use ghs_core::{Position, ProbeResult};
use ghs_probe::{HeightMap, HeightMapGrid, MAX_HEIGHT_MAP_POINTS, ProbeParams, XyPoint};

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

fn params() -> ProbeParams {
    ProbeParams {
        search_distance_mm: 10.0,
        ..ProbeParams::default()
    }
}

fn map() -> HeightMap {
    HeightMap::from_samples(grid(), vec![Some(0.0), Some(1.0), Some(2.0), Some(3.0)])
        .expect("valid map")
}

#[test]
fn bilinear_interpolation_matches_a_plane() {
    let map = map();
    assert_eq!(map.height_at(0.0, 0.0), Some(0.0));
    assert_eq!(map.height_at(10.0, 0.0), Some(1.0));
    assert_eq!(map.height_at(0.0, 10.0), Some(2.0));
    assert_eq!(map.height_at(10.0, 10.0), Some(3.0));
    assert_eq!(map.height_at(5.0, 0.0), Some(0.5));
    assert_eq!(map.height_at(0.0, 5.0), Some(1.0));
    assert_eq!(map.height_at(10.0, 5.0), Some(2.0));
    assert_eq!(map.height_at(5.0, 5.0), Some(1.5));
    assert_eq!(map.height_at(2.5, 7.5), Some(1.75));
    assert_eq!(map.height_at(-0.001, 0.0), None);
    assert_eq!(map.height_at(10.001, 0.0), None);
    assert_eq!(map.height_at(0.0, 10.001), None);
    assert_eq!(map.height_at(f64::NAN, 0.0), None);
}

#[test]
fn exact_grid_points_do_not_need_irrelevant_neighbours() {
    let map = HeightMap::from_samples(grid(), vec![Some(0.0), Some(1.0), Some(2.0), None]).unwrap();
    assert_eq!(map.height_at(10.0, 0.0), Some(1.0));
    assert_eq!(map.height_at(0.0, 10.0), Some(2.0));
    assert_eq!(map.height_at(5.0, 0.0), Some(0.5));
    assert_eq!(map.height_at(0.0, 5.0), Some(1.0));
    assert_eq!(map.height_at(5.0, 5.0), None);
    assert_eq!(map.height_at(10.0, 5.0), None);
}

#[test]
fn missing_points_are_not_bridged() {
    let map = HeightMap::from_samples(grid(), vec![Some(0.0), None, Some(2.0), Some(3.0)]).unwrap();
    assert_eq!(map.height_at(5.0, 5.0), None);
    assert_eq!(map.height_at(5.0, 0.0), None);
    assert_eq!(map.height_at(0.0, 5.0), Some(1.0));
    assert_eq!(map.valid_count(), 3);
    assert_eq!(map.missing_count(), 1);
    assert!(!map.is_complete());
}

#[test]
fn bounds_and_containment_are_inclusive() {
    let map = map();
    let bounds = map.bounds();
    assert!(!bounds.is_empty());
    assert_eq!(bounds.width(), 10.0);
    assert_eq!(bounds.height(), 10.0);
    assert!(map.contains(0.0, 0.0));
    assert!(map.contains(10.0, 10.0));
    assert!(!map.contains(10.0001, 10.0));
    assert!(!map.contains(f64::NAN, 0.0));
}

#[test]
fn from_results_records_success_failure_and_partial_maps() {
    let results = [
        ProbeResult::new(true, Position::xyz(0.0, 0.0, 1.0)),
        ProbeResult::new(false, Position::xyz(10.0, 0.0, 9.0)),
        ProbeResult::new(true, Position::xyz(0.0, 10.0, 3.0)),
    ];
    let map = HeightMap::from_results(grid(), &results).unwrap();
    assert_eq!(map.samples(), &[Some(1.0), None, Some(3.0), None]);
    assert_eq!(map.valid_count(), 2);
    assert_eq!(map.missing_count(), 2);

    let too_many = vec![ProbeResult::new(true, Position::zero()); 5];
    assert!(HeightMap::from_results(grid(), &too_many).is_err());
}

#[test]
fn non_finite_probe_results_become_invalid_cells() {
    let results = [
        ProbeResult::new(true, Position::xyz(f64::NAN, 0.0, 1.0)),
        ProbeResult::new(true, Position::xyz(10.0, 0.0, f64::INFINITY)),
        ProbeResult::new(true, Position::xyz(0.0, 10.0, 3.0)),
        ProbeResult::new(true, Position::xyz(10.0, 10.0, 4.0)),
    ];
    let map = HeightMap::from_results(grid(), &results).unwrap();
    assert_eq!(map.valid_count(), 2);
    assert_eq!(map.z(0, 0), None);
    assert_eq!(map.z(1, 0), None);
}

#[test]
fn samples_can_be_invalidated_and_repaired() {
    let mut map = map();
    assert_eq!(map.z(1, 1), Some(3.0));
    map.invalidate(1, 1).unwrap();
    assert_eq!(map.z(1, 1), None);
    assert_eq!(map.height_at(5.0, 5.0), None);
    map.set_z(1, 1, Some(7.0)).unwrap();
    assert_eq!(map.z(1, 1), Some(7.0));
    assert!(map.set_z(2, 2, Some(1.0)).is_err());
    assert!(map.set_z(0, 0, Some(f64::NAN)).is_err());
    assert!(map.invalidate(5, 5).is_err());
}

#[test]
fn compensation_uses_measured_minus_nominal_surface() {
    let map = map();
    assert_eq!(map.compensation_at(5.0, 5.0), Some(1.5));
    assert_eq!(map.apply_to_z(5.0, 5.0, -2.0), Some(-0.5));
    assert_eq!(map.apply_to_z(5.0, 5.0, f64::NAN), None);
    assert_eq!(map.compensation_at(-1.0, 0.0), None);

    let map = HeightMap::from_samples(
        HeightMapGrid {
            surface_z_mm: 1.0,
            ..grid()
        },
        vec![Some(1.0), Some(2.0), Some(3.0), Some(4.0)],
    )
    .unwrap();
    assert_eq!(map.compensation_at(0.0, 0.0), Some(0.0));
    assert_eq!(map.apply_to_z(10.0, 10.0, 0.0), Some(3.0));
}

#[test]
fn rows_ranges_and_overlay_points() {
    let map = map();
    assert_eq!(map.row(0), Some(&[Some(0.0), Some(1.0)][..]));
    assert_eq!(map.row(1), Some(&[Some(2.0), Some(3.0)][..]));
    assert_eq!(map.row(2), None);
    assert_eq!(map.z_range(), Some((0.0, 3.0)));
    let points = map.valid_points();
    assert_eq!(points.len(), 4);
    assert_eq!(points[0].1, 0.0);
    assert_eq!(points[3].1, 3.0);
    assert_eq!(map.len(), 4);
    assert!(!map.is_empty());
}

#[test]
fn empty_map_is_all_invalid() {
    let map = HeightMap::new(grid()).unwrap();
    assert_eq!(map.valid_count(), 0);
    assert_eq!(map.missing_count(), 4);
    assert!(!map.is_complete());
    assert_eq!(map.height_at(0.0, 0.0), None);
    assert_eq!(map.z_range(), None);
    assert!(map.valid_points().is_empty());
}

#[test]
fn grid_bounds_are_enforced() {
    let cases = [
        HeightMapGrid {
            columns: 1,
            ..grid()
        },
        HeightMapGrid { rows: 1, ..grid() },
        HeightMapGrid {
            columns: 129,
            ..grid()
        },
        HeightMapGrid { rows: 0, ..grid() },
        HeightMapGrid {
            step_x_mm: 0.0,
            ..grid()
        },
        HeightMapGrid {
            step_y_mm: -1.0,
            ..grid()
        },
        HeightMapGrid {
            step_x_mm: f64::NAN,
            ..grid()
        },
        HeightMapGrid {
            surface_z_mm: f64::NAN,
            ..grid()
        },
        HeightMapGrid {
            origin_mm: XyPoint::new(f64::INFINITY, 0.0),
            ..grid()
        },
        HeightMapGrid {
            columns: 65,
            rows: 64,
            ..grid()
        },
        HeightMapGrid {
            columns: 128,
            step_x_mm: 1_000.0,
            ..grid()
        },
    ];
    for candidate in cases {
        assert!(
            candidate.validate().is_err(),
            "grid should be rejected: {candidate:?}"
        );
    }
    assert!(grid().validate().is_ok());
    assert!(grid().point_count() <= MAX_HEIGHT_MAP_POINTS);
}

#[test]
fn grid_plan_rejects_a_zero_or_too_long_vertical_travel() {
    let zero_travel = ProbeParams {
        safe_z_mm: 0.0,
        ..params()
    };
    assert!(grid().plan(&zero_travel).is_err());

    let too_tight = ProbeParams {
        search_distance_mm: 3.0,
        ..params()
    };
    assert!(grid().plan(&too_tight).is_err());

    let estimate = grid().estimate(&params()).unwrap();
    assert_eq!(estimate.points, 4);
    assert_eq!(estimate.probing_distance_mm, 40.0);
    assert_eq!(estimate.probing_seconds, 24.0);
    assert_eq!(estimate.overhead_seconds, 12.0);
    assert_eq!(estimate.total_seconds, 36.0);
}

#[test]
fn grid_plan_subtracts_the_work_offset() {
    let offset = ProbeParams {
        wcs_offset_mm: Position::xyz(100.0, 200.0, 10.0),
        ..params()
    };
    let plan = grid().plan(&offset).unwrap();
    assert!(plan.program.contains("G0 X-100.000 Y-200.000"));
    assert!(!plan.program.contains("G0 Z-5.000"));
    assert!(plan.program.contains("G38.2 Z-10.000 F100.000"));
    assert!(plan.program.contains("G0 Z0.000"));
}

#[test]
fn grid_plan_lists_one_bounded_probe_per_point() {
    let plan = grid().plan(&params()).unwrap();
    assert_eq!(plan.expected_probes, 4);
    assert_eq!(plan.probes.len(), 4);
    assert_eq!(
        plan.probes
            .iter()
            .map(|probe| probe.label.as_str())
            .collect::<Vec<_>>(),
        vec!["p0-c0", "p0-c1", "p1-c0", "p1-c1"]
    );
    for probe in &plan.probes {
        assert!((probe.travel_mm - params().search_distance_mm).abs() < 1e-9);
    }
}

#[test]
fn every_height_map_probe_starts_without_a_downward_rapid() {
    let plan = grid().plan(&params()).unwrap();
    let lines: Vec<&str> = plan.program.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if !line.starts_with("G38.2") {
            continue;
        }
        assert!(index > 0);
        assert!(
            !lines[index - 1].starts_with("G0 Z"),
            "height-map probe was preceded by a Z rapid: {}",
            lines[index - 1]
        );
        assert!(lines[index - 1].starts_with("G0 X"));
    }
}

#[test]
fn height_map_samples_length_must_match_the_grid() {
    assert!(HeightMap::from_samples(grid(), vec![Some(0.0)]).is_err());
    assert!(HeightMap::from_samples(grid(), vec![Some(f64::NAN); 4]).is_err());
    assert!(HeightMap::from_samples(grid(), vec![Some(0.0); 4]).is_ok());
}

#[test]
fn point_and_index_helpers_reject_out_of_bounds() {
    let grid = grid();
    assert_eq!(grid.point(0, 0), Some(XyPoint::new(0.0, 0.0)));
    assert_eq!(grid.point(1, 1), Some(XyPoint::new(10.0, 10.0)));
    assert_eq!(grid.point(2, 0), None);
    assert_eq!(grid.index(0, 0), Some(0));
    assert_eq!(grid.index(1, 0), Some(1));
    assert_eq!(grid.index(0, 1), Some(2));
    assert_eq!(grid.index(1, 1), Some(3));
    assert_eq!(grid.index(2, 0), None);
}
