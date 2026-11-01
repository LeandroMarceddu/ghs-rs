//! Golden tests against the recorded FlexiHAL `$I+` response.

use ghs_core::{Axis, ControllerProfile};
use ghs_proto::{
    Response, ResponseParser, Streamer, StreamerConfig, parse_controller_info, parse_offsets,
};

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

#[test]
fn parses_the_recorded_reference_capabilities() {
    let info = parse_controller_info(&fixture_lines());

    assert_eq!(info.version, "1.1f");
    assert_eq!(info.build_info, "20240604");
    assert_eq!(info.firmware.as_deref(), Some("grblHAL"));
    assert!(info.is_grblhal());

    assert_eq!(info.axes.to_string(), "XYZ");
    assert_eq!(info.axis_count(), 3);
    assert_eq!(info.axis_list(), vec![Axis::X, Axis::Y, Axis::Z]);

    // The build reports a 1024-byte RX buffer; the streamer must never assume
    // the 128-byte legacy default. Its 64 is the planner block buffer.
    assert_eq!(info.rx_buffer_size, Some(1024));
    assert_eq!(info.rx_buffer_size_or(128), 1024);
    assert_eq!(info.planner_blocks, Some(64));
    assert_eq!(info.tool_table_entries, Some(0));
    assert!(!info.has_tool_table());

    assert_eq!(info.option_flags, "VNMSL");
    assert_eq!(
        info.new_options,
        vec!["ENUMS", "RT+", "HOME", "ES", "EXPR", "TC", "SED"]
    );
    assert!(info.rt_plus());
    assert!(info.has_sed());
    assert!(info.homing());
    assert!(info.estop());
    assert!(info.expressions());
    assert!(info.manual_tool_change());
    assert!(!info.has_atc());
    assert!(!info.has_sd());
    assert!(!info.has_mpg());
    assert!(!info.has_probe_connected_signal());

    assert_eq!(info.signals.to_string(), "HSDEP");
    assert!(info.has_signal(ghs_core::Signals::FEED_HOLD));
    assert!(info.has_signal(ghs_core::Signals::ESTOP));
    assert!(info.has_signal(ghs_core::Signals::PROBE));

    assert_eq!(info.nvs.as_deref(), Some("EEPROM"));
    assert_eq!(info.free_memory_kb, Some(80));
    assert_eq!(info.driver.as_deref(), Some("STM32F446"));
    assert_eq!(info.driver_version.as_deref(), Some("240428"));
    assert_eq!(info.board.as_deref(), Some("FlexiHAL_20240604_210624"));
    assert_eq!(info.spindle.as_deref(), Some("Huanyang v1"));

    let aux = info.aux_io.expect("aux io");
    assert_eq!((aux.digital_in, aux.digital_out), (4, 4));
    assert_eq!((aux.analog_in, aux.analog_out), (0, 0));
    assert!(info.has_aux_io());

    assert_eq!(info.plugins.len(), 14);
    assert!(
        info.plugins
            .iter()
            .any(|plugin| plugin.starts_with("MODBUS"))
    );
    assert!(
        info.plugins
            .iter()
            .any(|plugin| plugin.starts_with("Probe Protection"))
    );
}

#[test]
fn profile_is_data_and_a_subset_of_a_full_profile() {
    let flexihal = ControllerProfile::from_info_lines("flexihal", &fixture_lines());
    assert_eq!(flexihal.capabilities().rx_buffer_size, Some(1024));
    assert_eq!(flexihal.capabilities().planner_blocks, Some(64));

    let full_lines = [
        "[VER:1.1f.20240604:]",
        "[OPT:VNMSL+2,128,2048,9,16]",
        "[AXS:9:XYZABCUVW]",
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED,SD,YM,ATC=1,MPG,PC,OS,BD,SS,ODO]",
        "[FIRMWARE:grblHAL]",
        "[SIGNALS:XYZABCDRHSELTMFQPIJK]",
        "[AUX IO:8,8,2,2]",
    ];
    let full = ControllerProfile::from_info_lines("generic-full", &full_lines);
    assert!(flexihal.is_subset_of(&full));
    assert!(!full.is_subset_of(&flexihal));
}

#[test]
fn every_fixture_line_decodes_without_panicking() {
    let mut parser = ResponseParser::new();
    let mut payload = Vec::new();
    for line in fixture_lines() {
        payload.extend_from_slice(line.as_bytes());
        payload.extend_from_slice(b"\r\n");
    }
    payload.extend_from_slice(b"ok\r\n");

    let responses = parser.push(&payload);
    assert_eq!(responses.len(), fixture_lines().len() + 1);
    assert!(matches!(responses.last(), Some(Response::Ok)));
    assert!(
        responses[..responses.len() - 1]
            .iter()
            .all(|response| matches!(response, Response::Feedback(_)))
    );
}

#[test]
fn streamer_uses_the_reported_buffer_size() {
    let info = parse_controller_info(&fixture_lines());
    let mut streamer = Streamer::new(StreamerConfig {
        rx_buffer_size: info.rx_buffer_size_or(128),
        ..StreamerConfig::default()
    });
    assert_eq!(streamer.config().rx_buffer_size, 1024);

    let line = "G1 X0 Y0 Z0 F100";
    for _ in 0..4 {
        streamer.enqueue(line).expect("fits the reported buffer");
    }
    let output = streamer.poll_output();
    assert_eq!(output.len(), 68);
    assert!(output.len() <= 1024);
}

#[test]
fn parses_a_recorded_offset_and_parser_state_session() {
    let lines = [
        "[G54:-357.000,-757.000,-148.306]",
        "[G55:0.000,0.000,0.000]",
        "[G28:-1043.330,-801.000,-2.000]",
        "[G92:0.000,0.000,0.000]",
        "[HOME:-299.000,-353.000,-2.000:7]",
        "[TLO:0.000,0.000,3.000]",
        "[PRB:-293.004,-16.995,-78.005:1]",
        "[GC:G0 G54 G17 G21 G90 G94 G43.1 G98 G50 M5 M9 T0 F0 S0.]",
        "ok",
    ];
    let mut parser = ResponseParser::new();
    let mut payload = lines.join("\r\n").into_bytes();
    payload.extend_from_slice(b"\r\n");
    let responses = parser.push(&payload);
    assert_eq!(responses.len(), lines.len());

    let offsets = parse_offsets(&lines[..7]);
    assert_eq!(
        offsets.wcs(ghs_core::Wcs::G54),
        Some(ghs_core::Position::xyz(-357.0, -757.0, -148.306))
    );
    assert_eq!(offsets.home_mask, Some(ghs_core::Axes::xyz()));
    assert_eq!(offsets.tlo.as_deref(), Some(&[0.0, 0.0, 3.0][..]));
    assert!(offsets.probe.is_some_and(|probe| probe.success));

    let parser_state = responses
        .iter()
        .find_map(|response| match response {
            Response::ParserState(state) => Some(state),
            _ => None,
        })
        .expect("parser state");
    assert_eq!(parser_state.wcs, Some(ghs_core::Wcs::G54));
}
