//! Whole-program rotation: geometry, units, preservation, limits, and errors.

use ghs_gcode::{
    MAX_ROTATED_OUTPUT_LINES, Parser, RotationError, RotationOptions, RotationWarningKind,
    apply_rotation, checksum_xor, parse_program,
};

fn options(angle_deg: f64, pivot_mm: (f64, f64)) -> RotationOptions {
    RotationOptions {
        angle_deg,
        pivot_mm,
    }
}

fn rotated(source: &str, angle_deg: f64, pivot_mm: (f64, f64)) -> String {
    apply_rotation(source, &options(angle_deg, pivot_mm))
        .expect("rotation succeeds")
        .source
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1.0e-9
}

/// Asserts an unsupported-construct error with a matching tag.
fn expect_unsupported(source: &str, tag: &str) {
    let error = apply_rotation(source, &options(90.0, (0.0, 0.0)))
        .expect_err("the program must be rejected");
    match error {
        RotationError::UnsupportedConstruct { construct, .. } => {
            assert_eq!(construct, tag, "source: {source:?}");
        }
        other => panic!("expected UnsupportedConstruct, got {other:?}"),
    }
}

#[test]
fn zero_degrees_is_identity_and_keeps_r_arcs() {
    let source = "\
(header)
G21 G90 G17

N10 G0 X1 Y0 (start)
G2 X0 Y1 R1 F100
M3 S12000
";
    let result = apply_rotation(source, &options(0.0, (7.0, -7.0))).expect("identity");

    assert_eq!(result.source, source);
    assert_eq!(result.stats.input_lines, 6);
    assert_eq!(result.stats.output_lines, 6);
    assert_eq!(result.stats.rotated_moves, 0);
    assert_eq!(result.stats.rotated_arcs, 0);
    assert!(result.warnings.is_empty());
}

#[test]
fn zero_degrees_still_validates_the_program() {
    let error = apply_rotation("G21 G90\nG91\nG1 X10 F100\n", &options(0.0, (0.0, 0.0)))
        .expect_err("identity must still reject unsupported constructs");
    assert!(matches!(
        error,
        RotationError::UnsupportedConstruct {
            construct: "G91",
            ..
        }
    ));

    let error = apply_rotation(
        "G21 G90 G17\nG0 X10 Y0 Z0\nG2 X0 Y-10 Z-1 I-10 J0 F100\n",
        &options(0.0, (0.0, 0.0)),
    )
    .expect_err("identity must still reject a helical arc");
    assert!(matches!(
        error,
        RotationError::UnsupportedConstruct {
            construct: "helical arc",
            ..
        }
    ));
}

#[test]
fn ninety_degrees_rotates_a_square_about_the_origin() {
    let source = "\
G21 G90
G0 X0 Y0
G1 X10 Y0 F100
G1 X10 Y10
G1 X0 Y10
G1 X0 Y0
";
    let result = apply_rotation(source, &options(90.0, (0.0, 0.0))).expect("rotate");

    assert_eq!(
        result.source,
        "\
G21 G90
G0 X0 Y0
G1 X0 Y10 F100
G1 X-10 Y10
G1 X-10 Y0
G1 X0 Y0
"
    );
    assert_eq!(result.stats.rotated_moves, 5);
    assert_eq!(result.stats.rotated_arcs, 0);
    assert_eq!(result.stats.output_lines, 6);

    let program = parse_program(&result.source).expect("output reparses");
    assert!(close(program.state.position[0], 0.0));
    assert!(close(program.state.position[1], 0.0));
}

#[test]
fn one_hundred_eighty_degrees_rotates_a_square_about_the_origin() {
    let source = "\
G21 G90
G0 X0 Y0
G1 X10 Y0 F100
G1 X10 Y10
G1 X0 Y10
G1 X0 Y0
";
    let output = rotated(source, 180.0, (0.0, 0.0));

    assert_eq!(
        output,
        "\
G21 G90
G0 X0 Y0
G1 X-10 Y0 F100
G1 X-10 Y-10
G1 X0 Y-10
G1 X0 Y0
"
    );
}

#[test]
fn rotates_about_a_non_zero_pivot() {
    let source = "G21 G90\nG0 X10 Y5\nG1 X10 Y10 F100\n";
    let output = rotated(source, 90.0, (5.0, 5.0));

    assert_eq!(output, "G21 G90\nG0 X5 Y10\nG1 X0 Y10 F100\n");
}

#[test]
fn non_moving_absolute_endpoints_are_still_rotated() {
    // Every absolute endpoint is rotated, including a line that returns to the
    // programmed origin. Copying it verbatim would cut the unrotated path.
    let source = "G21 G90\nG0 X0 Y0\nG1 X10 Y0 F100\nG1 X0 Y0\n";
    let result = apply_rotation(source, &options(180.0, (10.0, 0.0))).expect("rotate");

    assert_eq!(
        result.source,
        "G21 G90\nG0 X20 Y0\nG1 X10 Y0 F100\nG1 X20 Y0\n"
    );
    assert_eq!(result.stats.rotated_moves, 3);
}

#[test]
fn g2_and_g3_endpoints_and_ij_offsets_rotate() {
    let source = "\
G21 G90 G17
G0 X10 Y0 Z1
G2 X0 Y-10 I-10 J0 F200
G3 X10 Y0 I10 J0
";
    let result = apply_rotation(source, &options(90.0, (0.0, 0.0))).expect("rotate");

    assert_eq!(
        result.source,
        "\
G21 G90 G17
G0 X0 Y10 Z1
G2 X10 Y0 I0 J-10 F200
G3 X0 Y10 I0 J10
"
    );
    assert_eq!(result.stats.rotated_moves, 1);
    assert_eq!(result.stats.rotated_arcs, 2);
    assert!(result.warnings.is_empty());

    let program = parse_program(&result.source).expect("output reparses");
    assert_eq!(program.stats.arc_moves, 2);
}

#[test]
fn r_arc_is_resolved_to_ij_with_a_warning() {
    let source = "G21 G90 G17\nG0 X1 Y0 Z0\nG2 X0 Y1 R1 F100\n";
    let result = apply_rotation(source, &options(90.0, (0.0, 0.0))).expect("rotate");

    assert_eq!(
        result.source,
        "G21 G90 G17\nG0 X0 Y1 Z0\nG2 X-1 Y0 I-1 J0 F100\n"
    );
    assert_eq!(result.stats.rotated_arcs, 1);
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.warnings[0].line, 3);
    assert_eq!(
        result.warnings[0].kind,
        RotationWarningKind::RadiusArcResolved
    );
    assert!(!result.source.contains('R'));
}

#[test]
fn full_circle_without_endpoint_rotates_its_center_offsets() {
    let source = "G21 G90\nG0 X5 Y0\nG2 I-5 J0 F100\n";
    let result = apply_rotation(source, &options(90.0, (0.0, 0.0))).expect("rotate");

    assert_eq!(result.source, "G21 G90\nG0 X0 Y5\nG2 I0 J-5 F100\n");
    assert_eq!(result.stats.rotated_arcs, 1);
    assert!(result.warnings.is_empty());
}

#[test]
fn g20_program_rotates_and_stays_in_inches() {
    let source = "G20 G90\nG0 X1 Y0\nG1 X1 Y1 F10\n";
    let result = apply_rotation(source, &options(90.0, (0.0, 0.0))).expect("rotate");

    assert_eq!(result.source, "G20 G90\nG0 X0 Y1\nG1 X-1 Y1 F10\n");
    assert!(result.source.contains("G20"));
    assert!(!result.source.contains("G21"));

    let program = parse_program(&result.source).expect("output reparses");
    assert!(close(program.state.position[0], -25.4));
    assert!(close(program.state.position[1], 25.4));
    assert_eq!(program.stats.max_feed_mm_per_min, Some(254.0));
}

#[test]
fn g20_arc_rotates_inches_and_stays_in_g20() {
    let source = "G20 G90 G17\nG0 X1 Y0 Z0\nG2 X0 Y-1 I-1 J0 F10\n";
    let result = apply_rotation(source, &options(90.0, (0.0, 0.0))).expect("rotate");

    assert_eq!(
        result.source,
        "G20 G90 G17\nG0 X0 Y1 Z0\nG2 X1 Y0 I0 J-1 F10\n"
    );
    assert!(result.warnings.is_empty());

    let program = parse_program(&result.source).expect("output reparses");
    assert_eq!(program.stats.arc_moves, 1);
    assert!(close(program.state.position[0], 25.4));
    assert!(close(program.state.position[1], 0.0));
}

#[test]
fn non_motion_content_and_n_words_are_preserved_verbatim() {
    let source = "\
(header)
G21 G90 G17
G43.1 Z-0.5
G49

N10 G0 X10 Y0 (rapid to start)
/ G1 X10 Y10 F100
G4 P0.5
T2 M6
S12000 M3
M8
G1 X0 Y10
M5
M9
G0 Z5
M30
";
    let result = apply_rotation(source, &options(90.0, (0.0, 0.0))).expect("rotate");
    let lines: Vec<&str> = result.source.lines().collect();

    assert_eq!(result.stats.input_lines, 16);
    assert_eq!(result.stats.output_lines, 16);
    assert_eq!(lines.len(), 16);
    for unchanged in [
        "(header)",
        "G21 G90 G17",
        "G43.1 Z-0.5",
        "G49",
        "",
        "G4 P0.5",
        "T2 M6",
        "S12000 M3",
        "M8",
        "M5",
        "M9",
        "G0 Z5",
        "M30",
    ] {
        assert!(
            lines.contains(&unchanged),
            "missing verbatim line {unchanged:?} in {:?}",
            result.source
        );
    }
    assert!(lines.contains(&"N10 G0 X0 Y10 (rapid to start)"));
    assert!(lines.contains(&"/ G1 X-10 Y10 F100"));
    assert!(lines.contains(&"G1 X-10 Y0"));
    assert!(lines[5].starts_with("N10 "));
}

#[test]
fn rejects_incremental_motion() {
    expect_unsupported("G21 G90\nG91\nG1 X10 F100\n", "G91");
    expect_unsupported("G21 G90\nG91 G1 X10 F100\n", "G91");
}

#[test]
fn rejects_machine_and_home_moves() {
    expect_unsupported("G21 G90\nG53 G0 X10 Y0\n", "G53");
    expect_unsupported("G21 G90\nG28\n", "G28/G30");
    expect_unsupported("G21 G90\nG30\n", "G28/G30");
    expect_unsupported("G21 G90\nG28.1\n", "G28/G30");
}

#[test]
fn rejects_probing_and_canned_cycles() {
    expect_unsupported("G21 G90\nG38.2 X10 F100\n", "G38.x");
    expect_unsupported("G21 G90\nG81 X0 Y0 Z-1 R1 F100\n", "canned cycle");
}

#[test]
fn rejects_frame_changing_commands() {
    expect_unsupported("G21 G90\nG10 L2 P1 X0 Y0\n", "G10");
    expect_unsupported("G21 G90\nG92 X0 Y0\n", "G92");
    expect_unsupported("G21 G90\nG51 P2\n", "G51");
    expect_unsupported("G21 G90\nG93 G1 X10 F10\n", "G93");
    expect_unsupported("G21 G90 G54\nG55\nG0 X10 Y0\n", "work coordinate system");
}

#[test]
fn rejects_non_xy_and_helical_arcs() {
    expect_unsupported("G21 G90\nG18 G2 X10 Z0 I5 K0 F100\n", "G18/G19 arc");
    expect_unsupported("G21 G90\nG19 G3 Y10 Z0 J5 K0 F100\n", "G18/G19 arc");
    expect_unsupported(
        "G21 G90 G17\nG0 X10 Y0 Z0\nG2 X0 Y-10 Z-1 I-10 J0 F100\n",
        "helical arc",
    );
}

#[test]
fn rejects_checksummed_input() {
    let body = "G0 X10 Y0";
    let sum = checksum_xor(body.as_bytes());
    let source = format!("G21 G90\n{body}*{sum:02}\n");
    expect_unsupported(&source, "checksum");
}

#[test]
fn rejects_malformed_programs_without_panicking() {
    for source in ["G21 G90\nG1 X\n", "G21 G90\n(open\n", "G0 X1.2.3\n"] {
        let error = apply_rotation(source, &options(90.0, (0.0, 0.0)))
            .expect_err("malformed input must be rejected");
        match error {
            RotationError::Parse { errors } => assert!(!errors.is_empty()),
            other => panic!("expected Parse, got {other:?}"),
        }
    }
}

#[test]
fn invalid_options_are_rejected() {
    for angle_deg in [f64::NAN, f64::INFINITY, 3_600.001, -4_000.0] {
        let error = apply_rotation("G21 G90\nG0 X1 Y0\n", &options(angle_deg, (0.0, 0.0)))
            .expect_err("bad angle");
        assert!(matches!(error, RotationError::InvalidOptions { .. }));
    }
    for pivot_mm in [(f64::NAN, 0.0), (0.0, f64::NEG_INFINITY), (1.0e8, 0.0)] {
        let error =
            apply_rotation("G21 G90\nG0 X1 Y0\n", &options(90.0, pivot_mm)).expect_err("bad pivot");
        assert!(matches!(error, RotationError::InvalidOptions { .. }));
    }
}

#[test]
fn extreme_rotated_coordinates_return_a_typed_error() {
    let source = "G21 G90\nG0 X10000000 Y0\n";
    let error = apply_rotation(source, &options(180.0, (1.0e7, 1.0e7)))
        .expect_err("rotated Y would be outside the coordinate range");
    match error {
        RotationError::CoordinateOutOfRange { line, y_mm, .. } => {
            assert_eq!(line, 2);
            assert!(close(y_mm, 2.0e7));
        }
        other => panic!("expected CoordinateOutOfRange, got {other:?}"),
    }
}

#[test]
fn output_line_cap_is_enforced() {
    let source = "\n".repeat(MAX_ROTATED_OUTPUT_LINES + 1);
    let error = apply_rotation(&source, &options(90.0, (0.0, 0.0)))
        .expect_err("the program is larger than the cap");
    assert!(matches!(
        error,
        RotationError::OutputTooLong {
            limit: MAX_ROTATED_OUTPUT_LINES
        }
    ));
}

#[test]
fn source_is_untouched_when_the_transform_fails() {
    let source = String::from("G21 G90\nG91\nG1 X10 F100\n");
    let before = source.clone();
    assert!(apply_rotation(&source, &options(90.0, (0.0, 0.0))).is_err());
    assert_eq!(source, before);
}

/// Replays every line and returns the parser position after each line.
fn positions(output: &str) -> Vec<[f64; 3]> {
    let mut parser = Parser::new();
    let mut positions = Vec::new();
    for (index, line) in output.lines().enumerate() {
        parser
            .parse_line_at(index + 1, line)
            .expect("emitted line reparses");
        positions.push(parser.state().position);
    }
    positions
}

#[test]
fn replayed_output_reaches_the_rotated_endpoint() {
    let source = "G21 G90\nG0 X0 Y0\nG1 X10 Y0 F100\n";
    let output = rotated(source, 90.0, (0.0, 0.0));
    let positions = positions(&output);

    assert_eq!(positions.len(), 3);
    assert!(close(positions[2][0], 0.0));
    assert!(close(positions[2][1], 10.0));
}
