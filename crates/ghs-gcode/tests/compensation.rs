//! Height-map program rewrite: geometry, modes, preservation, and errors.

use std::cell::Cell;

use ghs_core::Units;
use ghs_gcode::{
    HeightCompensationError, HeightCompensationOptions, HeightCompensationWarningKind, OutsideMap,
    Parser, apply_height_compensation, checksum_xor, parse_program,
};
use ghs_viewer::SegmentKind;

fn options(step_mm: f64) -> HeightCompensationOptions {
    HeightCompensationOptions {
        work_offset_xy_mm: [0.0, 0.0],
        max_step_mm: step_mm,
        outside: OutsideMap::Leave,
    }
}

fn flat(_x: f64, _y: f64) -> Option<f64> {
    Some(0.0)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0e-3
}

fn close_mm(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1.0e-6
}

/// Replays the emitted program through the parser and returns every arc as
/// `(start, end, center)` in millimetres.
fn replayed_arcs(output: &str) -> Vec<([f64; 3], [f64; 3], [f64; 3])> {
    let mut parser = Parser::new();
    let mut arcs = Vec::new();
    for (index, source) in output.lines().enumerate() {
        let start = parser.state().position;
        let line = parser
            .parse_line_at(index + 1, source)
            .expect("emitted line must reparse");
        let is_arc = line
            .words
            .iter()
            .any(|word| word.letter == 'G' && (word.value == 2.0 || word.value == 3.0));
        if is_arc {
            let i = line
                .words
                .iter()
                .find(|word| word.letter == 'I')
                .map_or(0.0, |word| word.value);
            let j = line
                .words
                .iter()
                .find(|word| word.letter == 'J')
                .map_or(0.0, |word| word.value);
            let center = [start[0] + i, start[1] + j, start[2]];
            arcs.push((start, parser.state().position, center));
        }
    }
    arcs
}

#[test]
fn flat_map_leaves_geometry_and_z_unchanged() {
    let source = "\
G21 G90
G0 X0 Y0 Z5
G1 Z0 F100
G1 X20 Y0
G2 X20 Y10 I0 J5
G3 X10 Y10 I-5 J0
G1 X0 Y0
";
    let before = parse_program(source).expect("source parses");
    let result = apply_height_compensation(source, &options(1000.0), flat).expect("transform");
    let after = parse_program(&result.source).expect("output reparses");

    assert_eq!(after.state.position, before.state.position);
    assert!(close(after.bounds.min().x, before.bounds.min().x));
    assert!(close(after.bounds.max().x, before.bounds.max().x));
    assert!(close(after.bounds.min().y, before.bounds.min().y));
    assert!(close(after.bounds.max().y, before.bounds.max().y));
    assert!(close(after.bounds.min().z, before.bounds.min().z));
    assert!(close(after.bounds.max().z, before.bounds.max().z));
    assert_eq!(result.stats.uncompensated_motions, 0);
    assert_eq!(result.stats.split_motions, 0);
    assert!(result.stats.compensated_motions > 0);
}

#[test]
fn ramp_compensation_shifts_z_at_endpoints_and_split_points() {
    let source = "G21 G90\nG0 X0 Y0 Z5\nG1 Z0 F100\nG1 X10\n";
    let result =
        apply_height_compensation(source, &options(5.0), |x, _y| Some(0.1 * x)).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    let segments = program.toolpath.segments();
    assert_eq!(segments.last().expect("last").end.z, 1.0);
    assert!(close(segments[segments.len() - 2].end.z, 0.5));
    assert_eq!(program.state.position, [10.0, 0.0, 1.0]);

    assert_eq!(result.stats.split_motions, 1);
    assert_eq!(result.stats.compensated_motions, 4);
    assert_eq!(result.stats.min_compensation_mm, Some(0.0));
    assert_eq!(result.stats.max_compensation_mm, Some(1.0));
}

#[test]
fn sloped_line_interpolates_z_and_adds_compensation() {
    let source = "G21 G90\nG1 X10 Z10 F100\n";
    let result =
        apply_height_compensation(source, &options(5.0), |x, _y| Some(0.1 * x)).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");
    let segments = program.toolpath.segments();

    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].end.z, 5.5);
    assert_eq!(segments[1].end.z, 11.0);
}

#[test]
fn work_offset_is_added_before_the_callback() {
    let seen = Cell::new(f64::NAN);
    let options = HeightCompensationOptions {
        work_offset_xy_mm: [100.0, 20.0],
        max_step_mm: 1000.0,
        outside: OutsideMap::Leave,
    };
    let result = apply_height_compensation("G21 G90\nG1 X10 F100\n", &options, |x, _y| {
        seen.set(x);
        Some(x * 0.01)
    })
    .expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    assert_eq!(seen.get(), 110.0);
    assert_eq!(program.toolpath.segments().last().expect("last").end.z, 1.1);
}

#[test]
fn long_line_is_split_contiguous_and_keeps_feed_and_kind() {
    let source = "G21 G90\nG0 X0 Y0 Z1\nG1 X10 F600\n";
    let result = apply_height_compensation(source, &options(2.5), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    let feed: Vec<_> = program
        .toolpath
        .segments()
        .iter()
        .filter(|segment| segment.kind == SegmentKind::Feed)
        .collect();
    assert_eq!(feed.len(), 4);
    let expected_x = [2.5, 5.0, 7.5, 10.0];
    for (segment, expected) in feed.iter().zip(expected_x) {
        assert!(close(segment.end.x, expected as f32));
        assert!(close(segment.end.z, 1.0));
    }
    for pair in feed.windows(2) {
        assert_eq!(pair[0].end, pair[1].start);
    }
    assert!(result.stats.max_output_step_mm <= 2.5 + 1.0e-9);
    assert_eq!(result.stats.split_motions, 1);
    assert_eq!(program.stats.max_feed_mm_per_min, Some(600.0));
}

#[test]
fn g2_arc_splits_into_sub_arcs_with_the_same_center_and_radius() {
    let source = "G21 G90\nG0 X10 Y0 Z0\nG2 X-10 Y0 I-10 J0 F200\n";
    let result = apply_height_compensation(source, &options(2.0), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");
    let arcs = replayed_arcs(&result.source);

    assert!(arcs.len() > 1);
    assert_eq!(result.stats.arc_motions, 1);
    assert_eq!(result.stats.split_motions, 2);
    assert_eq!(program.stats.arc_moves, arcs.len());
    for (start, end, center) in arcs {
        assert!(close_mm(center[0], 0.0));
        assert!(close_mm(center[1], 0.0));
        let start_radius = (start[0] - center[0]).hypot(start[1] - center[1]);
        let end_radius = (end[0] - center[0]).hypot(end[1] - center[1]);
        assert!(close_mm(start_radius, 10.0), "start radius {start_radius}");
        assert!(close_mm(end_radius, 10.0), "end radius {end_radius}");
    }
    assert_eq!(program.state.position, [-10.0, 0.0, 0.0]);
    assert!(close(program.bounds.min().y, -10.0));
    assert!(close(program.bounds.max().y, 0.0));
}

#[test]
fn g3_arc_splits_in_the_other_direction() {
    let source = "G21 G90\nG0 X10 Y0 Z0\nG3 X-10 Y0 I-10 J0 F200\n";
    let result = apply_height_compensation(source, &options(2.0), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");
    let arcs = replayed_arcs(&result.source);

    assert!(arcs.len() > 1);
    for (_, _, center) in &arcs {
        assert!(close_mm(center[0], 0.0));
        assert!(close_mm(center[1], 0.0));
    }
    assert!(close(program.bounds.max().y, 10.0));
    assert!(close(program.bounds.min().y, 0.0));
    assert_eq!(program.state.position, [-10.0, 0.0, 0.0]);
}

#[test]
fn r_arc_is_resolved_to_a_center_and_split() {
    let source = "G21 G90\nG0 X10 Y0 Z0\nG2 X0 Y10 R10 F200\n";
    let result = apply_height_compensation(source, &options(2.0), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");
    let arcs = replayed_arcs(&result.source);

    assert!(arcs.len() > 1);
    for (start, end, center) in arcs {
        assert!(close_mm(center[0], 10.0));
        assert!(close_mm(center[1], 10.0));
        assert!(close_mm(
            (start[0] - center[0]).hypot(start[1] - center[1]),
            10.0
        ));
        assert!(close_mm(
            (end[0] - center[0]).hypot(end[1] - center[1]),
            10.0
        ));
    }
    assert_eq!(program.state.position, [0.0, 10.0, 0.0]);
}

#[test]
fn full_circle_is_split_and_returns_to_its_start() {
    let source = "G21 G90\nG0 X0 Y0 Z0\nG2 I5 J0 F200\n";
    let result = apply_height_compensation(source, &options(2.0), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    assert!(replayed_arcs(&result.source).len() > 2);
    assert_eq!(program.state.position, [0.0, 0.0, 0.0]);
    assert!(close(program.bounds.min().x, 0.0));
    assert!(close(program.bounds.max().x, 10.0));
    assert!(close(program.bounds.min().y, -5.0));
    assert!(close(program.bounds.max().y, 5.0));
}

#[test]
fn arc_endpoints_receive_compensation_without_moving_the_center() {
    let source = "G21 G90\nG0 X10 Y0 Z0\nG3 X-10 Y0 I-10 J0 F200\n";
    let result =
        apply_height_compensation(source, &options(2.0), |_x, _y| Some(0.25)).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    assert!(close(program.bounds.min().z, 0.0));
    assert!(close(program.bounds.max().z, 0.25));
    assert_eq!(program.state.position, [-10.0, 0.0, 0.25]);
    assert_eq!(result.stats.uncompensated_motions, 0);
}

#[test]
fn arc_with_varying_compensation_stays_parseable() {
    let source = "G21 G90\nG0 X10 Y0 Z0\nG2 X-10 Y0 I-10 J0 F200\n";
    let result = apply_height_compensation(source, &options(2.0), |x, _y| Some(0.01 * (x + 10.0)))
        .expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    assert!(replayed_arcs(&result.source).len() > 1);
    assert!(close_mm(program.state.position[2], 0.0));
    assert!(close(program.bounds.min().z, 0.0));
    assert!(close(program.bounds.max().z, 0.2));
}

#[test]
fn incremental_moves_are_emitted_as_deltas() {
    let source = "G21 G91\nG1 X10 F100\n";
    let result = apply_height_compensation(source, &options(4.0), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    assert!(close_mm(program.state.position[0], 10.0));
    assert_eq!(program.stats.feed_segments, 3);
    assert!(result.stats.max_output_step_mm <= 4.0 + 1.0e-9);

    let mut total = 0.0;
    for line in result.source.lines().filter(|line| line.starts_with("G1 ")) {
        let x = line
            .split_whitespace()
            .find_map(|word| word.strip_prefix('X'))
            .expect("X word");
        total += x.parse::<f64>().expect("number");
    }
    assert!(close_mm(total, 10.0));
}

#[test]
fn incremental_full_circle_stays_a_full_circle() {
    let source = "G21 G91\nG2 X0 Y0 I5 J0 F200\n";
    let result = apply_height_compensation(source, &options(2.0), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    assert!(close_mm(program.state.position[0], 0.0));
    assert!(close_mm(program.state.position[1], 0.0));
    assert!(replayed_arcs(&result.source).len() > 2);
    assert!(close(program.bounds.max().x, 10.0));
    assert!(close(program.bounds.min().y, -5.0));
}

#[test]
fn inch_programs_stay_in_inches() {
    let source = "G20 G90\nG0 X0 Y0 Z0.2\nG1 X2 F10\n";
    let result =
        apply_height_compensation(source, &options(6.35), |_x, _y| Some(0.1)).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    assert_eq!(program.state.units, Units::Inch);
    assert!(close_mm(program.state.position[0], 50.8));
    assert!(close_mm(program.state.position[2], 5.18));
    assert!(close(program.bounds.max().x, 50.8));
    assert!(close(program.bounds.max().z, 5.18));
    assert_eq!(program.stats.max_feed_mm_per_min, Some(254.0));
    assert!(result.source.contains("G20"));
    assert!(!result.source.contains("G21"));
}

#[test]
fn inch_incremental_arc_reparses() {
    let source = "G20 G91\nG2 X-0.5 Y0 I-0.25 J0 F10\n";
    let result = apply_height_compensation(source, &options(2.54), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    assert!(program.stats.arc_moves > 1);
    assert!(close_mm(program.state.position[0], -12.7));
    assert!(close_mm(program.state.position[1], 0.0));
    assert!(close(program.bounds.min().y, -6.35));
    assert!(close(program.bounds.max().y, 0.0));
    assert_eq!(program.state.units, Units::Inch);
}

#[test]
fn f_only_and_nonmoving_blocks_are_never_compensated() {
    let source = "\
G21 G90
G0 X0 Y0 Z5
F100
G1 X0
G1 X0 Y0
G1 F200
G1 X10 F300
";
    let calls = Cell::new(0usize);
    let result = apply_height_compensation(source, &options(10.0), |_x, _y| {
        calls.set(calls.get() + 1);
        Some(1.0)
    })
    .expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    // Only the pure-Z rapid and the real X move are sampled.
    assert_eq!(calls.get(), 2);
    assert_eq!(result.stats.compensated_motions, 2);
    assert_eq!(result.stats.input_motion_blocks, 2);
    assert_eq!(program.toolpath.segments().last().expect("last").end.z, 6.0);
    for preserved in ["F100", "G1 X0", "G1 X0 Y0", "G1 F200"] {
        assert!(
            result.source.lines().any(|line| line == preserved),
            "missing preserved line {preserved}"
        );
    }
}

#[test]
fn outside_leave_counts_uncompensated_moves() {
    let source = "G21 G90\nG1 X10 F100\n";
    let result = apply_height_compensation(source, &options(2.5), |x, _y| {
        if x < 5.0 { Some(0.5) } else { None }
    })
    .expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    assert_eq!(result.stats.compensated_motions, 1);
    assert_eq!(result.stats.uncompensated_motions, 3);
    assert_eq!(result.stats.output_motion_blocks, 4);
    assert_eq!(program.state.position, [10.0, 0.0, 0.0]);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.kind == HeightCompensationWarningKind::OutsideMap)
    );
}

#[test]
fn outside_reject_fails_with_the_line_and_machine_point() {
    let source = "G21 G90\nG1 X10 F100\n";
    let options = HeightCompensationOptions {
        outside: OutsideMap::Reject,
        ..options(2.5)
    };
    let error = apply_height_compensation(
        source,
        &options,
        |x, _y| {
            if x < 5.0 { Some(0.5) } else { None }
        },
    )
    .expect_err("outside map");

    match error {
        HeightCompensationError::OutsideMap { line, x_mm, y_mm } => {
            assert_eq!(line, 2);
            assert_eq!(x_mm, 5.0);
            assert_eq!(y_mm, 0.0);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn comments_spindle_coolant_speed_and_feed_are_preserved() {
    let source = "\
(header)
T1 M6
M3 S12000
M8
G4 P0.1
G21 G90 G0 Z5
G1 X10 F300 M7 (cut) ; finish (roughing)
M5
M9
M30
";
    let result = apply_height_compensation(source, &options(5.0), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    assert!(result.source.contains("(header)"));
    assert!(result.source.contains("T1 M6"));
    assert!(result.source.contains("M3 S12000"));
    assert!(result.source.contains("G4 P0.1"));
    assert!(result.source.contains("; cut"));
    assert!(result.source.contains("; finish (roughing)"));
    for code in ["M3", "M7", "M8"] {
        let count = result
            .source
            .lines()
            .filter(|line| line.split_whitespace().any(|word| word == code))
            .count();
        assert_eq!(count, 1, "{code} must appear exactly once");
    }
    assert_eq!(program.stats.max_spindle_rpm, 12000.0);
    assert_eq!(program.stats.max_feed_mm_per_min, Some(300.0));
    assert_eq!(program.stats.tool_changes, 1);
    assert_eq!(program.state.spindle, ghs_gcode::SpindleMode::Off);
    assert_eq!(program.state.coolant, ghs_gcode::CoolantMode::Off);
    let first_motion = result
        .source
        .lines()
        .find(|line| line.starts_with("G1 "))
        .expect("split feed move");
    assert!(first_motion.split_whitespace().any(|word| word == "M7"));
}

#[test]
fn straight_moves_in_non_xy_planes_are_allowed() {
    let source = "G21 G18 G90\nG0 X1 Z1\nG1 X2 Z2 F100\n";
    let result = apply_height_compensation(source, &options(10.0), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    assert_eq!(program.state.position, [2.0, 0.0, 2.0]);
    assert!(result.source.contains("G18"));
}

#[test]
fn spindle_start_is_first_and_stop_is_last() {
    let source = "G21 G90\nG1 X10 F100 M3 S12000\nG1 X20 M5\n";
    let result = apply_height_compensation(source, &options(5.0), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");
    let motions: Vec<_> = result
        .source
        .lines()
        .filter(|line| line.starts_with("G1 "))
        .collect();

    assert_eq!(motions.len(), 4);
    assert!(motions[0].split_whitespace().any(|word| word == "M3"));
    assert!(motions[0].split_whitespace().any(|word| word == "S12000"));
    assert!(!motions[1].split_whitespace().any(|word| word == "M3"));
    assert!(!motions[2].split_whitespace().any(|word| word == "M5"));
    assert!(motions[3].split_whitespace().any(|word| word == "M5"));
    assert_eq!(program.state.spindle, ghs_gcode::SpindleMode::Off);
}

#[test]
fn program_end_stays_on_the_last_sub_move() {
    let source = "G21 G90\nG1 X10 F100 M30\n";
    let result = apply_height_compensation(source, &options(5.0), flat).expect("transform");
    let motions: Vec<_> = result
        .source
        .lines()
        .filter(|line| line.starts_with("G1 "))
        .collect();

    assert_eq!(motions.len(), 2);
    assert!(!motions[0].split_whitespace().any(|word| word == "M30"));
    assert!(motions[1].split_whitespace().any(|word| word == "M30"));
    parse_program(&result.source).expect("output reparses");
}

#[test]
fn tool_length_frame_change_keeps_incremental_deltas() {
    let source = "G21 G91\nG43.1 Z10\nG1 Z-5 F100\n";
    let result =
        apply_height_compensation(source, &options(10.0), |_x, _y| Some(1.0)).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    // G43.1 moves the program frame to -10; the nominal target is -15 and the
    // compensated target is -14, so the incremental word is Z-4, not Z-14.
    assert!(close_mm(program.state.position[2], -14.0));
    let move_line = result
        .source
        .lines()
        .find(|line| line.starts_with("G1 "))
        .expect("feed move");
    assert!(
        move_line.split_whitespace().any(|word| word == "Z-4"),
        "unexpected move line: {move_line}"
    );
}

#[test]
fn non_xy_arcs_g53_helical_and_checksums_are_rejected() {
    let cases = [
        ("G18 G2 X10 Z0 I5 K0 F100\n", "G18/G19 arc"),
        ("G19 G3 Y10 Z0 J5 K0 F100\n", "G18/G19 arc"),
        ("G53 G0 X0\n", "G53"),
        ("G2 X10 Y0 Z-1 I5 J0 F100\n", "helical arc"),
        ("G93 G1 X10 F0.5\n", "G93"),
        ("G10 L2 P1 X0\n", "G10"),
        ("G0 X1\nG92 X0\n", "G92"),
        ("G54 G0 X1\nG55 G0 X1\n", "work coordinate system"),
        ("G51 P2\n", "G51"),
    ];
    for (source, expected) in cases {
        let error =
            apply_height_compensation(source, &options(10.0), flat).expect_err("must reject");
        match error {
            HeightCompensationError::UnsupportedConstruct { construct, .. } => {
                assert_eq!(construct, expected, "source {source:?}");
            }
            other => panic!("unexpected error for {source:?}: {other:?}"),
        }
    }
}

#[test]
fn checksummed_input_is_rejected() {
    let body = "G0 X1";
    let sum = checksum_xor(body.as_bytes());
    let source = format!("{body}*{sum:02}\n");
    let error =
        apply_height_compensation(&source, &options(10.0), flat).expect_err("checksummed input");
    assert!(matches!(
        error,
        HeightCompensationError::UnsupportedConstruct {
            construct: "checksum",
            ..
        }
    ));
}

#[test]
fn absolute_arc_centers_are_rejected_by_the_parser() {
    let error = apply_height_compensation("G90.1\n", &options(10.0), flat)
        .expect_err("absolute arc centers");
    let errors = error.parse_errors().expect("parse error");
    assert_eq!(errors[0].kind, ghs_gcode::ParseErrorKind::UnsupportedCode);
}

#[test]
fn line_numbers_are_dropped_with_one_warning() {
    let source = "N10 G21 G90\nN20 G0 X0 Y0 Z5\nN30 G1 X10 F100\n";
    let result = apply_height_compensation(source, &options(1000.0), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    assert_eq!(program.lines.len(), 3);
    for line in &program.lines {
        assert!(line.number.is_none(), "N word survived: {}", line.source);
    }
    assert_eq!(
        result.warnings[0].kind,
        HeightCompensationWarningKind::LineNumbersDropped
    );
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.stats.input_lines, 3);
    assert_eq!(result.stats.output_lines, 3);
}

#[test]
fn a_line_number_on_a_motion_line_still_warns_when_dropped() {
    let source = "G21 G90\nN20 G1 X10 F100\n";
    let result = apply_height_compensation(source, &options(10.0), flat).expect("transform");

    assert_eq!(
        result.warnings[0].kind,
        HeightCompensationWarningKind::LineNumbersDropped
    );
    assert!(!result.source.contains("N20"));
}

#[test]
fn block_delete_applies_to_every_generated_sub_move() {
    let source = "/G1 X10 F100\n";
    let result = apply_height_compensation(source, &options(5.0), flat).expect("transform");
    let program = parse_program(&result.source).expect("output reparses");

    let motion_lines: Vec<_> = result
        .source
        .lines()
        .filter(|line| line.contains("G1"))
        .collect();
    assert_eq!(motion_lines.len(), 2);
    assert!(motion_lines.iter().all(|line| line.starts_with('/')));
    assert_eq!(
        program
            .lines
            .iter()
            .filter(|line| line.block_delete)
            .count(),
        2
    );
}

#[test]
fn malformed_input_returns_parse_errors_without_panicking() {
    for source in ["G1 X\n", "G1 X1\n", "G0 X1*99\n", "IF [#1 GT 2]\n"] {
        let error = apply_height_compensation(source, &options(10.0), flat).expect_err("malformed");
        let errors = error.parse_errors().expect("parse errors");
        assert!(!errors.is_empty());
    }
}

#[test]
fn empty_and_comment_only_programs_round_trip() {
    let empty = apply_height_compensation("", &options(10.0), flat).expect("empty");
    assert!(empty.source.is_empty());
    assert_eq!(empty.stats.input_lines, 0);
    assert_eq!(empty.stats.output_lines, 0);

    let source = "%\n(header)\n\n; note\n%\n";
    let comments = apply_height_compensation(source, &options(10.0), flat).expect("comments");
    assert_eq!(comments.source, source);
    assert_eq!(comments.stats.input_lines, 5);
    assert_eq!(comments.stats.output_lines, 5);
}

#[test]
fn options_are_validated_before_parsing() {
    for step in [f64::NAN, 0.0, -1.0, 2000.0, f64::INFINITY] {
        let error = apply_height_compensation("not gcode\n", &options(step), flat)
            .expect_err("invalid step");
        assert!(matches!(
            error,
            HeightCompensationError::InvalidOptions { .. }
        ));
    }
    let offset = HeightCompensationOptions {
        work_offset_xy_mm: [f64::INFINITY, 0.0],
        max_step_mm: 1.0,
        outside: OutsideMap::Leave,
    };
    assert!(matches!(
        apply_height_compensation("G21 G90\n", &offset, flat),
        Err(HeightCompensationError::InvalidOptions { .. })
    ));
}

#[test]
fn a_compensated_z_outside_the_parser_range_is_an_error() {
    let error = apply_height_compensation(
        "G21 G90\nG1 Z10000000 F100\n",
        &options(1000.0),
        |_x, _y| Some(1.0),
    )
    .expect_err("coordinate overflow");
    assert!(matches!(
        error,
        HeightCompensationError::CoordinateOutOfRange { .. }
    ));
}

#[test]
fn a_nonfinite_callback_value_is_an_error() {
    let error = apply_height_compensation("G21 G90\nG1 X10 F100\n", &options(1000.0), |_x, _y| {
        Some(f64::NAN)
    })
    .expect_err("nan compensation");
    assert!(matches!(
        error,
        HeightCompensationError::InvalidCompensation { .. }
    ));
}
