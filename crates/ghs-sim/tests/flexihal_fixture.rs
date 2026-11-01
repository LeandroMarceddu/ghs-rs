//! Golden tests against the recorded FlexiHAL controller.

use ghs_core::{Axis, ControllerInfo};
use ghs_proto::{Response, ResponseParser};
use ghs_sim::{SimProfile, Simulator, profile_flexihal, profile_generic_full};

/// Raw transcription of the reference controller's `$I+` report.
const CONTROLLER_INFO: &str =
    include_str!("../../../fixtures/controller/flexihal-1.1f.20240604.txt");

fn fixture_lines() -> Vec<&'static str> {
    CONTROLLER_INFO
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn output_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8(bytes.to_vec())
        .expect("ASCII controller output")
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn banner_matches_the_reference_controller() {
    let sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(sim.banner(), b"GrblHAL 1.1f ['$' for help]\r\n");
    assert_eq!(sim.profile_name(), "flexihal");
}

#[test]
fn extended_info_reproduces_the_recorded_fixture_exactly() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let bytes = sim.feed(b"$I+\r\n");
    let lines = output_lines(&bytes);

    let expected = fixture_lines();
    assert_eq!(lines.len(), expected.len() + 1);
    for (actual, expected) in lines.iter().zip(expected.iter()) {
        assert_eq!(actual, expected);
    }
    assert_eq!(lines.last().map(String::as_str), Some("ok"));
}

#[test]
fn extended_info_parses_back_to_the_reference_capabilities() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let bytes = sim.feed(b"$I+\n");
    let lines = output_lines(&bytes);
    let info = ghs_proto::parse_controller_info(&lines[..lines.len() - 1]);

    assert_eq!(info.version, "1.1f");
    assert_eq!(info.build_info, "20240604");
    assert_eq!(info.firmware.as_deref(), Some("grblHAL"));
    assert_eq!(info.axis_list(), vec![Axis::X, Axis::Y, Axis::Z]);
    assert_eq!(info.rx_buffer_size, Some(1024));
    assert_eq!(info.planner_blocks, Some(64));
    assert_eq!(info.tool_table_entries, Some(0));
    assert!(info.rt_plus());
    assert!(info.has_sed());
    assert!(info.homing());
    assert!(info.estop());
    assert!(info.manual_tool_change());
    assert!(!info.has_sd());
    assert!(!info.has_atc());
    assert!(!info.has_mpg());
    assert!(!info.has_probe_connected_signal());
    assert_eq!(info.signals.to_string(), "HSDEP");
    assert_eq!(info.plugins.len(), 14);
    assert_eq!(info.spindle.as_deref(), Some("Huanyang v1"));
}

#[test]
fn short_info_is_a_subset_of_the_extended_report() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let short = output_lines(&sim.feed(b"$I\n"));
    let extended = output_lines(&sim.feed(b"$I+\n"));
    assert!(short.len() < extended.len());
    assert_eq!(short.last().map(String::as_str), Some("ok"));
    for line in &short[..short.len() - 1] {
        assert!(extended.contains(line), "missing from $I+: {line}");
    }
    assert!(short[0].starts_with("[VER:1.1f.20240604:]"));
}

#[test]
fn every_info_line_decodes_as_a_controller_response() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let bytes = sim.feed(b"$I+\n");
    let mut parser = ResponseParser::new();
    let responses = parser.push(&bytes);
    assert_eq!(responses.len(), fixture_lines().len() + 1);
    assert!(matches!(responses.last(), Some(Response::Ok)));
    assert!(
        responses[..responses.len() - 1]
            .iter()
            .all(|response| matches!(response, Response::Feedback(_)))
    );
}

#[test]
fn flexihal_is_a_strict_subset_of_generic_full() {
    let flexihal = profile_flexihal();
    let full = profile_generic_full();
    assert!(flexihal.is_subset_of(&full));
    assert!(!full.is_subset_of(&flexihal));
    assert_eq!(flexihal.capabilities().axes.to_string(), "XYZ");
    assert_eq!(full.capabilities().axes.to_string(), "XYZABC");
    assert!(full.capabilities().has_sd());
    assert!(full.capabilities().has_mpg());
    assert!(full.capabilities().has_atc());
    assert!(full.capabilities().has_probe_connected_signal());
    assert!(full.capabilities().has_tool_table());
}

#[test]
fn generic_full_info_is_capability_rich() {
    let mut sim = Simulator::new(SimProfile::GenericFull);
    let bytes = sim.feed(b"$I+\n");
    let lines = output_lines(&bytes);
    let info = ghs_proto::parse_controller_info(&lines[..lines.len() - 1]);
    assert_eq!(info.axes.to_string(), "XYZABC");
    assert_eq!(info.rx_buffer_size, Some(2048));
    assert_eq!(info.planner_blocks, Some(128));
    assert_eq!(info.tool_table_entries, Some(8));
    assert!(info.has_sd());
    assert!(info.has_mpg());
    assert!(info.has_atc());
    assert!(info.has_probe_connected_signal());
    assert_eq!(info.plugins.len(), 3);
    assert_eq!(info.spindle.as_deref(), Some("PWM v1"));
    let _: ControllerInfo = info;
}

#[test]
fn enum_and_profile_constructors_are_equivalent() {
    let mut from_enum = Simulator::new(SimProfile::GenericFull);
    let mut from_data = Simulator::from_profile(profile_generic_full());
    assert_eq!(from_enum.banner(), from_data.banner());
    for command in [b"$I+\n".as_slice(), b"$ES\n", b"$#\n", b"?", b"$G\n"] {
        assert_eq!(from_enum.feed(command), from_data.feed(command));
    }
    assert_eq!(from_enum.snapshot(), from_data.snapshot());
}
