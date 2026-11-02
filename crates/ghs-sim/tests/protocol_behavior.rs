//! Protocol behaviour tests for the simulator.

use std::time::Duration;

use ghs_core::{Axis, BufferState, MachineState, Overrides, Position, Wcs};
use ghs_proto::{
    Response, parse_offset_line, parse_offsets, parse_parser_state, parse_setting_description,
    parse_status,
};
use ghs_sim::{SimProfile, Simulator};

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("ASCII controller output")
}

fn single_line(bytes: &[u8]) -> String {
    text(bytes).lines().next().unwrap_or_default().to_string()
}

fn settle(sim: &mut Simulator) {
    let _ = sim.tick(Duration::from_secs(60));
}

/// Asserted pins from one realtime status report.
fn probe_pins(sim: &mut Simulator) -> ghs_core::Signals {
    parse_status(text(&sim.feed(b"?")).trim())
        .expect("status")
        .pins
        .expect("pins")
}

#[test]
fn status_report_round_trips_through_the_parser() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let bytes = sim.feed(b"?");
    let report = parse_status(text(&bytes).trim()).expect("status report");
    assert_eq!(report.state, MachineState::Idle);
    assert_eq!(report.mpos, Some(Position::xyz(0.0, 0.0, 0.0)));
    assert_eq!(report.wpos, Some(Position::xyz(0.0, 0.0, 0.0)));
    assert_eq!(report.wco, Some(Position::xyz(0.0, 0.0, 0.0)));
    assert_eq!(
        report.buffer,
        Some(BufferState {
            planner_free: 64,
            rx_free: 1024,
        })
    );
    assert_eq!(report.wcs, Some(Wcs::G54));
    assert_eq!(report.overrides, Some(Overrides::DEFAULT));
    assert_eq!(report.tool, Some(0));
    assert_eq!(report.line_number, Some(0));
    assert!(report.unknown.is_empty(), "{:?}", report.unknown);
    assert!(report.malformed.is_empty(), "{:?}", report.malformed);

    let extended = text(&sim.feed(&[0x80]));
    assert!(extended.starts_with("<Idle"), "{extended}");
}

#[test]
fn full_status_report_contains_every_documented_field() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let line = text(&sim.feed(&[0x87]));
    for field in [
        "MPos:", "WPos:", "WCO:", "DTG:", "Bf:", "Ln:", "FS:", "Pn:", "WCS:", "Ov:", "A:", "H:",
        "D:", "T:", "P:", "TLR:", "In:", "FW:", "$C:", "Sc:",
    ] {
        assert!(line.contains(field), "missing {field}: {line}");
    }
    let report = parse_status(line.trim()).expect("full status report");
    assert_eq!(report.firmware.as_deref(), Some("grblHAL"));
    assert_eq!(report.check_mode, Some(false));
    assert_eq!(report.m66_result, Some(0));
    assert_eq!(report.tlo_reference_set, Some(false));
    assert_eq!(report.diameter_mode, Some(false));
    assert_eq!(report.tool, Some(0));
    let homing = report.homing.expect("homing status");
    assert!(!homing.complete);
    assert_eq!(homing.homed_axes, Some(ghs_core::Axes::empty()));
    assert!(report.unknown.is_empty(), "{:?}", report.unknown);
    assert!(report.malformed.is_empty(), "{:?}", report.malformed);
}

#[test]
fn generic_full_reports_sd_mpg_and_extra_spindles() {
    let mut sim = Simulator::new(SimProfile::GenericFull);
    let line = text(&sim.feed(&[0x87]));
    assert!(line.contains("|SD:1"), "{line}");
    assert!(line.contains("|MPG:0"), "{line}");
    assert!(line.contains("|SP1:"), "{line}");
    let report = parse_status(line.trim()).expect("status report");
    assert_eq!(report.mpos.expect("position").axes().to_string(), "XYZABC");
    assert!(report.malformed.is_empty(), "{:?}", report.malformed);
}

#[test]
fn settings_read_write_read_back_and_validate() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);

    assert_eq!(single_line(&sim.feed(b"$100\n")), "$100=250.000");

    let out = sim.feed(b"$100=300.000\n");
    assert_eq!(single_line(&out), "ok");
    assert_eq!(single_line(&sim.feed(b"$100\n")), "$100=300.000");
    assert_eq!(sim.setting(100), Some(300.0));

    let out = sim.feed(b"$100=0\n");
    assert_eq!(single_line(&out), "error:39");
    assert_eq!(single_line(&sim.feed(b"$100\n")), "$100=300.000");

    assert_eq!(single_line(&sim.feed(b"$23=1.5\n")), "error:23");
    assert_eq!(single_line(&sim.feed(b"$100=abc\n")), "error:2");
    assert_eq!(single_line(&sim.feed(b"$9999=1\n")), "error:3");

    let all = text(&sim.feed(b"$$\n"));
    assert!(all.contains("$100=300.000"));
    assert!(all.contains("$110=5000.000"));
    assert!(all.ends_with("ok\r\n"));
    let plus = text(&sim.feed(b"$+\n"));
    assert_eq!(all, plus);

    let details = text(&sim.feed(b"$$=100\n"));
    assert!(
        details.starts_with("$100=300.000 (X steps per millimetre)"),
        "{details}"
    );
    assert!(details.ends_with("ok\r\n"));
    assert_eq!(single_line(&sim.feed(b"$$=999\n")), "error:3");
}

#[test]
fn setting_metadata_is_parseable_and_covers_the_table() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let out = text(&sim.feed(b"$ES\n"));
    let mut count = 0;
    for line in out.lines().filter(|line| line.starts_with("[SETTING:")) {
        let description = parse_setting_description(line).expect("setting metadata");
        assert!(!description.name.is_empty());
        assert!(description.max.is_some());
        count += 1;
    }
    assert!(count >= 20);

    let steps = out
        .lines()
        .find(|line| line.starts_with("[SETTING:100|"))
        .expect("steps per millimetre metadata");
    let description = parse_setting_description(steps).expect("metadata");
    assert_eq!(description.id, 100);
    assert_eq!(description.name, "X steps per millimetre");
    assert_eq!(description.unit.as_deref(), Some("step/mm"));
    assert_eq!(description.min, Some(1.0));
    assert_eq!(description.max, Some(10_000.0));
    assert!(description.reboot_required);

    let groups = text(&sim.feed(b"$EG\n"));
    assert!(groups.contains("[SETTINGGROUP:0|N/A|General]"));
    assert!(groups.contains("[SETTINGGROUP:4|0|Axes]"));

    let description = text(&sim.feed(b"$SED=100\n"));
    assert!(description.starts_with("[SETTINGDESC:100|"));
    assert!(description.ends_with("ok\r\n"));
}

#[test]
fn offsets_parser_state_and_startup_lines() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G10 L2 P1 X10 Y20 Z-5\n")), "ok");

    let out = text(&sim.feed(b"$#\n"));
    let lines: Vec<&str> = out.lines().collect();
    let offsets = parse_offsets(&lines[..lines.len() - 1]);
    assert_eq!(offsets.wcs(Wcs::G54), Some(Position::xyz(10.0, 20.0, -5.0)));
    assert_eq!(offsets.home_mask, Some(ghs_core::Axes::empty()));
    assert!(offsets.probe.is_some());
    assert!(out.contains("[TLO:0.000,0.000,0.000]"));
    assert!(out.contains("[TLR:0.000]"));
    assert!(out.contains("[PRB:0.000,0.000,0.000:0]"));

    let parser_state = parse_parser_state(&single_line(&sim.feed(b"$G\n"))).expect("$G");
    assert_eq!(parser_state.wcs, Some(Wcs::G54));
    assert_eq!(parser_state.motion.as_deref(), Some("G0"));
    assert_eq!(parser_state.units, Some(ghs_core::Units::Mm));

    assert_eq!(single_line(&sim.feed(b"$N0=G54 G17\n")), "ok");
    assert_eq!(single_line(&sim.feed(b"$N0\n")), "$N0=G54 G17");
    let listing = text(&sim.feed(b"$N\n"));
    assert!(listing.contains("$N0=G54 G17"));
    assert!(listing.contains("$N1="));

    let reset = text(&sim.feed(&[0x18]));
    assert!(reset.starts_with("GrblHAL 1.1f"));
    assert!(reset.contains(">N0=G54 G17:ok"));
}

#[test]
fn homing_unlock_and_soft_reset_paths() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"$H\n")), "ok");
    let snapshot = sim.snapshot();
    assert_eq!(snapshot.state, MachineState::Idle);
    assert_eq!(snapshot.homed.to_string(), "XYZ");
    assert!(snapshot.homing_complete);
    assert!(text(&sim.feed(b"$#\n")).contains("[HOME:0.000,0.000,0.000:7]"));

    let alarm = text(&sim.raise_alarm(1));
    assert_eq!(alarm, "ALARM:1\r\n");
    assert_eq!(sim.snapshot().state, MachineState::Alarm);
    assert_eq!(single_line(&sim.feed(b"G0 X1\n")), "error:9");

    let status = parse_status(text(&sim.feed(b"?")).trim()).expect("status");
    assert_eq!(status.state, MachineState::Alarm);
    assert_eq!(status.alarm.map(|code| code.code()), Some(1));

    let unlocked = text(&sim.feed(b"$X\n"));
    assert!(unlocked.contains("[MSG:Caution: Unlocked]"));
    assert!(unlocked.ends_with("ok\r\n"));
    assert_eq!(sim.snapshot().state, MachineState::Idle);
    assert_eq!(sim.snapshot().alarm, None);

    let mut single_axis = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&single_axis.feed(b"$HX\n")), "ok");
    assert_eq!(single_axis.snapshot().homed.to_string(), "X");
    assert!(!single_axis.snapshot().homing_complete);
    assert_eq!(single_line(&single_axis.feed(b"$H\n")), "ok");
    assert_eq!(single_axis.snapshot().homed.to_string(), "XYZ");

    let reset = text(&sim.feed(&[0x18]));
    assert!(reset.starts_with("GrblHAL 1.1f"));
    assert_eq!(sim.snapshot().state, MachineState::Idle);
}

#[test]
fn jog_accepts_explicit_units_without_changing_modal_units() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);

    assert_eq!(single_line(&sim.feed(b"$J=G21 G91 X1 F1000\n")), "ok");
    settle(&mut sim);
    assert_eq!(sim.position()[Axis::X], 1.0);

    // 0.5 inch is 12.7 mm; the G20 is local to the jog line.
    assert_eq!(single_line(&sim.feed(b"$J=G20 G91 X0.5 F10\n")), "ok");
    settle(&mut sim);
    assert!((sim.position()[Axis::X] - 13.7).abs() < 1.0e-6);

    let parser = text(&sim.feed(b"$G\n"));
    assert!(parser.contains("G21"), "modal units changed: {parser}");
}

#[test]
fn jog_moves_over_ticks_and_cancels() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"$J=G91 X10 F1000\n")), "ok");
    assert_eq!(sim.snapshot().state, MachineState::Jog);

    let _ = sim.tick(Duration::from_millis(300));
    let partial = sim.position().get(Axis::X);
    assert!((partial - 5.0).abs() < 0.6, "partial X = {partial}");

    let _ = sim.feed(&[0x85]);
    assert_eq!(sim.snapshot().state, MachineState::Idle);
    let cancelled = sim.position().get(Axis::X);
    assert!((cancelled - 5.0).abs() < 0.6, "cancelled X = {cancelled}");

    let _ = sim.tick(Duration::from_secs(5));
    assert_eq!(sim.position().get(Axis::X), cancelled);
}

#[test]
fn realtime_overrides_change_and_reset() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let _ = sim.feed(&[0x91]);
    assert_eq!(sim.snapshot().overrides.feed, 110);
    let _ = sim.feed(&[0x93]);
    assert_eq!(sim.snapshot().overrides.feed, 111);
    let _ = sim.feed(&[0x92]);
    let _ = sim.feed(&[0x94]);
    assert_eq!(sim.snapshot().overrides.feed, 100);

    let _ = sim.feed(&[0x96]);
    assert_eq!(sim.snapshot().overrides.rapid, 50);
    let _ = sim.feed(&[0x97]);
    assert_eq!(sim.snapshot().overrides.rapid, 25);
    let _ = sim.feed(&[0x98]);
    assert_eq!(sim.snapshot().overrides.rapid, 5);
    let _ = sim.feed(&[0x95]);
    assert_eq!(sim.snapshot().overrides.rapid, 100);

    let _ = sim.feed(&[0x9A]);
    assert_eq!(sim.snapshot().overrides.spindle, 110);
    let _ = sim.feed(&[0x99]);
    assert_eq!(sim.snapshot().overrides.spindle, 100);

    let status = text(&sim.feed(b"$J=G91 X1 F1000\n"));
    assert!(status.ends_with("ok\r\n"));
    let line = text(&sim.feed(b"?"));
    assert!(line.contains("|Ov:100,100,100|"), "{line}");

    let _ = sim.feed(&[0x91]);
    assert_eq!(sim.snapshot().overrides.feed, 110);
    let _ = sim.feed(&[0x18]);
    assert_eq!(sim.snapshot().overrides, Overrides::DEFAULT);
}

#[test]
fn feed_hold_freezes_motion_and_cycle_start_resumes() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G1 X10 F600\n")), "ok");
    assert_eq!(sim.snapshot().state, MachineState::Run);

    let _ = sim.feed(b"!");
    assert_eq!(sim.snapshot().state, MachineState::Hold);
    let _ = sim.tick(Duration::from_secs(5));
    assert_eq!(sim.position().get(Axis::X), 0.0);

    let _ = sim.feed(b"~");
    assert_eq!(sim.snapshot().state, MachineState::Run);
    let _ = sim.tick(Duration::from_secs(3));
    assert_eq!(sim.snapshot().state, MachineState::Idle);
    assert!((sim.position().get(Axis::X) - 10.0).abs() < 1e-9);
}

#[test]
fn optional_stop_and_single_block_hold_the_program() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let _ = sim.feed(&[0x88]);
    assert!(sim.snapshot().optional_stop);
    assert_eq!(single_line(&sim.feed(b"M1\n")), "ok");
    assert_eq!(sim.snapshot().state, MachineState::Hold);
    let _ = sim.feed(&[0x81]);
    assert_eq!(sim.snapshot().state, MachineState::Idle);

    let _ = sim.feed(&[0x89]);
    assert!(sim.snapshot().single_block);
    assert_eq!(single_line(&sim.feed(b"G1 X1 F600\n")), "ok");
    let _ = sim.tick(Duration::from_secs(3));
    assert_eq!(sim.snapshot().state, MachineState::Hold);
    let _ = sim.feed(&[0x81]);
    assert_eq!(sim.snapshot().state, MachineState::Idle);
}

#[test]
fn soft_reset_clears_the_runtime_job_toggles() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G10 L2 P1 X10\n")), "ok");
    assert_eq!(single_line(&sim.feed(b"T3\n")), "ok");
    assert_eq!(single_line(&sim.feed(b"$N0=G54 G17\n")), "ok");

    // Set all three through the `$` toggles, the counterpart of the realtime
    // codes a sender may use instead.
    assert_eq!(single_line(&sim.feed(b"$O\n")), "ok");
    assert_eq!(single_line(&sim.feed(b"$S\n")), "ok");
    assert_eq!(single_line(&sim.feed(b"$B\n")), "ok");
    let before = sim.snapshot();
    assert!(before.optional_stop);
    assert!(before.single_block);
    assert!(before.block_delete);

    let reset = text(&sim.feed(&[0x18]));
    assert!(reset.starts_with("GrblHAL 1.1f"), "{reset}");
    assert!(reset.contains(">N0=G54 G17:ok"), "{reset}");

    // grblHAL's `sys_init()` clears the job toggles on reset.
    let after = sim.snapshot();
    assert!(!after.optional_stop, "optional stop survived the reset");
    assert!(!after.single_block, "single block survived the reset");
    assert!(!after.block_delete, "block delete survived the reset");

    // The rest of the runtime state is untouched by this change.
    assert_eq!(after.state, MachineState::Idle);
    assert_eq!(after.tool, 3);
    assert_eq!(after.startup_lines[0], "G54 G17");
    assert!(
        text(&sim.feed(b"$#\n")).contains("[G54:10.000,0.000,0.000]"),
        "offsets were lost"
    );
}

#[test]
fn runtime_job_toggles_rearm_after_a_soft_reset() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let _ = sim.feed(&[0x88]);
    let _ = sim.feed(&[0x89]);
    let _ = sim.feed(b"$B\n");
    let _ = sim.feed(&[0x18]);
    assert!(!sim.snapshot().optional_stop);
    assert!(!sim.snapshot().single_block);
    assert!(!sim.snapshot().block_delete);

    // Optional stop off: `M1` does not hold. The realtime toggle re-arms it.
    assert_eq!(single_line(&sim.feed(b"M1\n")), "ok");
    assert_eq!(sim.snapshot().state, MachineState::Idle);
    let _ = sim.feed(&[0x88]);
    assert!(sim.snapshot().optional_stop);
    assert_eq!(single_line(&sim.feed(b"M1\n")), "ok");
    assert_eq!(sim.snapshot().state, MachineState::Hold);
    let _ = sim.feed(&[0x81]);
    assert_eq!(sim.snapshot().state, MachineState::Idle);

    // Block delete off: a `/` line runs. `$B` flips it and the same line is
    // discarded without moving.
    assert_eq!(single_line(&sim.feed(b"/G1 X2 F600\n")), "ok");
    settle(&mut sim);
    assert!((sim.position().get(Axis::X) - 2.0).abs() < 1.0e-9);
    assert_eq!(single_line(&sim.feed(b"$B\n")), "ok");
    assert!(sim.snapshot().block_delete);
    assert_eq!(single_line(&sim.feed(b"/G1 X9 F600\n")), "ok");
    settle(&mut sim);
    assert!(
        (sim.position().get(Axis::X) - 2.0).abs() < 1.0e-9,
        "a deleted block moved the machine"
    );

    // Single block off: a block completes directly. The realtime toggle then
    // holds at the end of the next block.
    assert_eq!(sim.snapshot().state, MachineState::Idle);
    assert_eq!(single_line(&sim.feed(b"G1 X3 F600\n")), "ok");
    settle(&mut sim);
    assert_eq!(sim.snapshot().state, MachineState::Idle);
    let _ = sim.feed(&[0x89]);
    assert!(sim.snapshot().single_block);
    assert_eq!(single_line(&sim.feed(b"G1 X4 F600\n")), "ok");
    settle(&mut sim);
    assert_eq!(sim.snapshot().state, MachineState::Hold);
}

#[test]
fn alarm_door_stop_and_sleep_preserve_the_runtime_job_toggles() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let _ = sim.feed(&[0x88]);
    let _ = sim.feed(&[0x89]);
    let _ = sim.feed(b"$B\n");
    let assert_set = |sim: &Simulator, where_: &str| {
        let snapshot = sim.snapshot();
        assert!(snapshot.optional_stop, "optional stop cleared by {where_}");
        assert!(snapshot.single_block, "single block cleared by {where_}");
        assert!(snapshot.block_delete, "block delete cleared by {where_}");
    };

    assert_eq!(text(&sim.raise_alarm(1)), "ALARM:1\r\n");
    assert_set(&sim, "an alarm");
    assert!(text(&sim.feed(b"$X\n")).ends_with("ok\r\n"));

    let _ = sim.feed(&[0x84]);
    assert_eq!(sim.snapshot().state, MachineState::Door);
    assert_set(&sim, "the safety door");
    let _ = sim.feed(&[0x84]);

    let _ = sim.feed(&[0x19]);
    assert_set(&sim, "a stop");

    assert_eq!(single_line(&sim.feed(b"$SLP\n")), "ok");
    assert_eq!(sim.snapshot().state, MachineState::Sleep);
    assert_set(&sim, "sleep");

    // Only a reset clears them.
    let _ = sim.feed(&[0x18]);
    assert_eq!(sim.snapshot().state, MachineState::Idle);
    let after = sim.snapshot();
    assert!(!after.optional_stop);
    assert!(!after.single_block);
    assert!(!after.block_delete);
}

#[test]
fn tool_change_waits_for_acknowledge_and_cycle_start() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"T3 M6\n")), "ok");
    let snapshot = sim.snapshot();
    assert_eq!(snapshot.state, MachineState::Tool);
    assert_eq!(snapshot.tool, 3);
    assert!(snapshot.tool_change_pending);
    assert!(!snapshot.tool_change_acknowledged);

    assert_eq!(single_line(&sim.feed(b"G0 X1\n")), "error:40");

    // Jogging and `$` commands are allowed while the change is pending.
    assert_eq!(single_line(&sim.feed(b"$J=G91 X1 F1000\n")), "ok");
    let _ = sim.tick(Duration::from_secs(2));
    assert_eq!(sim.snapshot().state, MachineState::Tool);
    assert!(sim.snapshot().tool_change_pending);

    // Cycle start before the acknowledge does nothing.
    let _ = sim.feed(&[0x81]);
    assert_eq!(sim.snapshot().state, MachineState::Tool);

    let _ = sim.feed(&[0xA3]);
    assert!(sim.snapshot().tool_change_acknowledged);
    let _ = sim.feed(&[0x81]);
    let snapshot = sim.snapshot();
    assert_eq!(snapshot.state, MachineState::Idle);
    assert!(!snapshot.tool_change_pending);
    assert_eq!(snapshot.tool, 3);

    let status = text(&sim.feed(&[0x87]));
    assert!(status.contains("|T:3|"), "{status}");
    assert!(!status.contains("|A:T"), "{status}");
}

#[test]
fn mpg_takeover_is_capability_gated() {
    let mut sim = Simulator::new(SimProfile::GenericFull);
    let _ = sim.feed(&[0x8B]);
    assert!(sim.snapshot().mpg_active);
    assert!(text(&sim.feed(b"?")).contains("|MPG:1|"));
    let _ = sim.feed(&[0x8B]);
    assert!(!sim.snapshot().mpg_active);
    assert!(text(&sim.feed(b"?")).contains("|MPG:0|"));

    let mut flexihal = Simulator::new(SimProfile::FlexiHal);
    let before = flexihal.snapshot();
    let _ = flexihal.feed(&[0x8B]);
    assert_eq!(flexihal.snapshot(), before);
    assert!(!text(&flexihal.feed(b"?")).contains("MPG:"));
}

#[test]
fn sticky_errors_require_a_blank_line_or_system_command() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G999\n")), "error:20");
    assert_eq!(sim.snapshot().error, Some(20));

    assert_eq!(single_line(&sim.feed(b"G0 X1\n")), "error:20");
    settle(&mut sim);
    assert_eq!(sim.position().get(Axis::X), 0.0);

    assert_eq!(single_line(&sim.feed(b"\n")), "ok");
    assert_eq!(sim.snapshot().error, None);
    assert_eq!(single_line(&sim.feed(b"G0 X1\n")), "ok");
    settle(&mut sim);
    assert!((sim.position().get(Axis::X) - 1.0).abs() < 1e-9);
}

#[test]
fn partial_lines_and_interleaved_realtime_bytes() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert!(sim.feed(b"G0 X").is_empty());
    assert!(sim.feed(b"10 F").is_empty());

    let status = text(&sim.feed(b"?"));
    assert!(status.starts_with("<Idle"), "{status}");

    let out = sim.feed(b"100\n");
    assert_eq!(single_line(&out), "ok");
    settle(&mut sim);
    assert!((sim.position().get(Axis::X) - 10.0).abs() < 1e-9);
}

#[test]
fn cr_lf_crlf_and_lfcr_terminators() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let out = text(&sim.feed(b"$100\r\n$101\n\r$102\r"));
    let oks = out.matches("ok\r\n").count();
    assert_eq!(oks, 3, "{out}");
    assert_eq!(out.matches("$100=250.000").count(), 1);
    assert_eq!(out.matches("$101=250.000").count(), 1);
    assert_eq!(out.matches("$102=250.000").count(), 1);
}

#[test]
fn reset_settings_reset_offsets_and_reset_tools() {
    let mut sim = Simulator::new(SimProfile::GenericFull);
    assert_eq!(single_line(&sim.feed(b"$110=9000\n")), "ok");
    assert_eq!(single_line(&sim.feed(b"G10 L2 P2 X5\n")), "ok");

    assert_eq!(single_line(&sim.feed(b"$RST=$\n")), "ok");
    assert_eq!(single_line(&sim.feed(b"$110\n")), "$110=5000.000");

    assert_eq!(single_line(&sim.feed(b"$RST=#\n")), "ok");
    let out = text(&sim.feed(b"$#\n"));
    let lines: Vec<&str> = out.lines().collect();
    let offsets = parse_offsets(&lines[..lines.len() - 1]);
    assert_eq!(
        offsets.wcs(Wcs::G55),
        Some(Position::new(
            ghs_core::Axes::parse("XYZABC").expect("six axes")
        ))
    );

    assert_eq!(single_line(&sim.feed(b"$RST=*\n")), "ok");
    assert_eq!(single_line(&sim.feed(b"$100\n")), "$100=250.000");
}

#[test]
fn enumeration_and_diagnostic_commands_are_supported() {
    let mut sim = Simulator::new(SimProfile::GenericFull);
    for command in [
        b"$EA\n".as_slice(),
        b"$EE\n",
        b"$ES\n",
        b"$EG\n",
        b"$EAG\n",
        b"$EEG\n",
        b"$ESG\n",
        b"$ESH\n",
        b"$E\n",
        b"$PINS\n",
        b"$PINSTATE\n",
        b"$LIM\n",
        b"$LEV\n",
        b"$PORTS\n",
        b"$SPINDLES\n",
        b"$SPINDLESH\n",
        b"$HSS\n",
        b"$RTC\n",
        b"$HELP\n",
        b"$HELP commands\n",
        b"$SED\n",
    ] {
        let out = text(&sim.feed(command));
        assert!(out.ends_with("ok\r\n"), "{command:?} returned {out:?}");
        assert!(out.len() > 4);
    }

    let pins = text(&sim.feed(b"$PINS\n"));
    assert!(pins.contains("|input]"));
    assert!(pins.contains("|output]"));
    let spindles = text(&sim.feed(b"$SPINDLES\n"));
    assert_eq!(spindles.matches("[SPINDLE:").count(), 2);

    let alarms = text(&sim.feed(b"$EA\n"));
    let line = alarms.lines().next().expect("alarm description");
    let description = ghs_proto::parse_code_description(line).expect("alarm code");
    assert_eq!(description.code, 1);
    assert!(alarms.contains("[ALARMCODE:10|"));
}

#[test]
fn reboot_prints_the_banner_and_startup_results() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"$N0=G54\n")), "ok");
    let out = text(&sim.feed(b"$REBOOT\n"));
    assert!(out.starts_with("GrblHAL 1.1f"));
    assert!(out.contains(">N0=G54:ok"));
}

#[test]
fn a_flexihal_controller_has_no_controller_tool_table() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let out = text(&sim.feed(b"$#\n"));
    assert!(!out.contains("[T:"));

    let mut full = Simulator::new(SimProfile::GenericFull);
    let out = text(&full.feed(b"$#\n"));
    assert_eq!(out.matches("[T:").count(), 8);
    let tool_line = out
        .lines()
        .find(|line| line.starts_with("[T:"))
        .expect("tool");
    let tool = ghs_proto::parse_tool_table_line(tool_line).expect("tool entry");
    assert_eq!(tool.number, 0);
}

#[test]
fn responses_are_framed_like_the_controller() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let out = sim.feed(b"$I\n");
    assert!(out.ends_with(b"ok\r\n"));
    assert!(out.starts_with(b"[VER:"));
    assert!(out.windows(2).any(|pair| pair == b"\r\n"));

    let response = Response::parse("[PRB:1.000,2.000,3.000:1]");
    assert!(matches!(response, Response::Probe(_)));
    let offset = parse_offset_line("[G54:1.000,2.000,3.000]").expect("offset");
    assert_eq!(offset.values, vec![1.0, 2.0, 3.0]);
}

#[test]
fn units_incremental_and_work_coordinate_systems_are_modal() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G20 G91 G0 X1\n")), "ok");
    settle(&mut sim);
    assert_eq!(sim.snapshot().units, ghs_core::Units::Inch);
    assert!((sim.position().get(Axis::X) - 25.4).abs() < 1e-6);

    assert_eq!(single_line(&sim.feed(b"G21 G90 G0 X10 Y0\n")), "ok");
    settle(&mut sim);
    assert_eq!(sim.snapshot().units, ghs_core::Units::Mm);
    assert!((sim.position().get(Axis::X) - 10.0).abs() < 1e-6);

    assert_eq!(single_line(&sim.feed(b"G10 L2 P2 X5 Y6 Z0\n")), "ok");
    assert_eq!(single_line(&sim.feed(b"G55 G0 X0 Y0\n")), "ok");
    settle(&mut sim);
    assert_eq!(sim.snapshot().wcs, Wcs::G55);
    assert_eq!(sim.snapshot().wco, Position::xyz(5.0, 6.0, 0.0));
    assert!((sim.position().get(Axis::X) - 5.0).abs() < 1e-6);
}

#[test]
fn g92_offsets_the_work_coordinate_system() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G0 X10 Y20\n")), "ok");
    settle(&mut sim);

    assert_eq!(single_line(&sim.feed(b"G92 X0 Y0\n")), "ok");
    let snapshot = sim.snapshot();
    assert_eq!(snapshot.wpos, Position::xyz(0.0, 0.0, 0.0));
    assert_eq!(snapshot.wco, Position::xyz(10.0, 20.0, 0.0));
    assert_eq!(sim.position(), Position::xyz(10.0, 20.0, 0.0));

    let status = parse_status(text(&sim.feed(b"?")).trim()).expect("status");
    assert_eq!(status.wpos, Some(Position::xyz(0.0, 0.0, 0.0)));
    assert_eq!(status.wco, Some(Position::xyz(10.0, 20.0, 0.0)));
}

#[test]
fn spindle_and_coolant_modal_state_is_reported() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"M3 S12000\n")), "ok");
    let snapshot = sim.snapshot();
    assert!(snapshot.spindle_on);
    assert!(snapshot.spindle_clockwise);
    assert_eq!(snapshot.spindle_rpm, 12_000.0);

    assert_eq!(single_line(&sim.feed(b"M7\n")), "ok");
    assert!(sim.snapshot().mist);
    let status = parse_status(text(&sim.feed(b"?")).trim()).expect("status");
    let accessory = status.accessory.expect("accessories");
    assert!(accessory.spindle_on);
    assert!(accessory.clockwise);
    assert!(accessory.mist);
    assert!(!accessory.flood);
    assert_eq!(status.spindle_rpm, Some(12_000.0));

    assert_eq!(single_line(&sim.feed(b"M4 M8\n")), "ok");
    let snapshot = sim.snapshot();
    assert!(!snapshot.spindle_clockwise);
    assert!(snapshot.flood);
    assert!(!snapshot.mist);

    assert_eq!(single_line(&sim.feed(b"M5 M9\n")), "ok");
    let snapshot = sim.snapshot();
    assert!(!snapshot.spindle_on);
    assert!(!snapshot.flood);
    assert!(!snapshot.mist);
}

#[test]
fn arcs_move_to_their_endpoint_and_validate_offsets() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G2 X10 Y10 I5 J0 F600\n")), "ok");
    settle(&mut sim);
    assert!((sim.position().get(Axis::X) - 10.0).abs() < 1e-6);
    assert!((sim.position().get(Axis::Y) - 10.0).abs() < 1e-6);

    let out = single_line(&sim.feed(b"G3 X0 Y0 F600\n"));
    assert_eq!(out, "error:35");
}

#[test]
fn probing_reports_a_prb_result() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G38.2 Z-5 F100\n")), "ok");
    let out = text(&sim.tick(Duration::from_secs(10)));
    assert!(out.contains("[PRB:"), "{out}");
    let snapshot = sim.snapshot();
    assert!(snapshot.probe_result.is_some_and(|probe| probe.success));
    assert!((sim.position().get(Axis::Z) + 5.0).abs() < 1e-6);

    let offsets = text(&sim.feed(b"$#\n"));
    assert!(offsets.contains("[PRB:0.000,0.000,-5.000:1]"), "{offsets}");
}

#[test]
fn a_retract_clears_probe_contact_but_keeps_the_last_prb() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G38.2 Z-5 F100\n")), "ok");
    let out = text(&sim.tick(Duration::from_secs(10)));
    assert!(out.contains("[PRB:0.000,0.000,-5.000:1]"), "{out}");
    assert!(sim.snapshot().probe_triggered);
    assert!(probe_pins(&mut sim).contains(ghs_core::Signals::PROBE));

    assert_eq!(single_line(&sim.feed(b"G0 Z5\n")), "ok");
    assert!(!sim.snapshot().probe_triggered);
    assert!(!probe_pins(&mut sim).contains(ghs_core::Signals::PROBE));

    // The live input opened, but the last result stays available to `$#`.
    let offsets = text(&sim.feed(b"$#\n"));
    assert!(offsets.contains("[PRB:0.000,0.000,-5.000:1]"), "{offsets}");
    assert_eq!(
        sim.snapshot().probe_result.map(|probe| probe.position),
        Some(Position::xyz(0.0, 0.0, -5.0))
    );
}

#[test]
fn a_second_probe_after_a_retract_replaces_the_prb() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G38.2 Z-5 F100\n")), "ok");
    let first = text(&sim.tick(Duration::from_secs(10)));
    assert!(first.contains("[PRB:0.000,0.000,-5.000:1]"), "{first}");

    assert_eq!(single_line(&sim.feed(b"G0 Z5\n")), "ok");
    settle(&mut sim);
    assert!(!sim.snapshot().probe_triggered);

    assert_eq!(single_line(&sim.feed(b"G38.2 Z-3 F100\n")), "ok");
    let second = text(&sim.tick(Duration::from_secs(10)));
    assert!(second.contains("[PRB:0.000,0.000,-3.000:1]"), "{second}");
    assert!(sim.snapshot().probe_triggered);
    assert!(probe_pins(&mut sim).contains(ghs_core::Signals::PROBE));

    let offsets = text(&sim.feed(b"$#\n"));
    assert!(offsets.contains("[PRB:0.000,0.000,-3.000:1]"), "{offsets}");
    assert!(!offsets.contains("[PRB:0.000,0.000,-5.000:1]"), "{offsets}");
}

#[test]
fn reset_clears_probe_contact_and_the_last_prb() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G38.2 Z-5 F100\n")), "ok");
    let out = text(&sim.tick(Duration::from_secs(10)));
    assert!(out.contains("[PRB:"), "{out}");
    assert!(sim.snapshot().probe_triggered);

    let reset = text(&sim.feed(&[0x18]));
    assert!(reset.starts_with("GrblHAL 1.1f"), "{reset}");
    assert!(!sim.snapshot().probe_triggered);
    assert_eq!(sim.snapshot().probe_result, None);
    assert!(!probe_pins(&mut sim).contains(ghs_core::Signals::PROBE));
    let offsets = text(&sim.feed(b"$#\n"));
    assert!(offsets.contains("[PRB:0.000,0.000,0.000:0]"), "{offsets}");

    // An offset reset also drops the last result and any live contact.
    assert_eq!(single_line(&sim.feed(b"G38.2 Z-4 F100\n")), "ok");
    let out = text(&sim.tick(Duration::from_secs(10)));
    assert!(out.contains("[PRB:0.000,0.000,-4.000:1]"), "{out}");
    assert_eq!(single_line(&sim.feed(b"$RST=#\n")), "ok");
    assert!(!sim.snapshot().probe_triggered);
    assert_eq!(sim.snapshot().probe_result, None);
    let offsets = text(&sim.feed(b"$#\n"));
    assert!(offsets.contains("[PRB:0.000,0.000,0.000:0]"), "{offsets}");
}

#[test]
fn unlock_keeps_the_probe_result_and_live_contact() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G38.2 Z-5 F100\n")), "ok");
    let out = text(&sim.tick(Duration::from_secs(10)));
    assert!(out.contains("[PRB:"), "{out}");

    let alarm = text(&sim.feed(&[0x9F]));
    assert_eq!(alarm, "ALARM:10\r\n");
    assert!(text(&sim.feed(b"$X\n")).ends_with("ok\r\n"));
    assert!(sim.snapshot().probe_triggered);
    assert!(sim.snapshot().probe_result.is_some());

    // Retracting after unlock opens the input and keeps the result.
    assert_eq!(single_line(&sim.feed(b"G0 Z5\n")), "ok");
    assert!(!sim.snapshot().probe_triggered);
    assert!(sim.snapshot().probe_result.is_some());
}

#[test]
fn an_alarm_mid_probe_does_not_emit_or_latch_a_result() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G38.2 Z-5 F100\n")), "ok");
    let _ = sim.tick(Duration::from_secs(1));

    let alarm = text(&sim.raise_alarm(4));
    assert_eq!(alarm, "ALARM:4\r\n");
    assert_eq!(sim.snapshot().state, MachineState::Alarm);
    assert!(!sim.snapshot().probe_triggered);
    assert_eq!(sim.snapshot().probe_result, None);
    assert!(!probe_pins(&mut sim).contains(ghs_core::Signals::PROBE));

    let drained = text(&sim.tick(Duration::from_secs(10)));
    assert!(!drained.contains("[PRB:"), "{drained}");
    assert_eq!(sim.snapshot().probe_result, None);
}

#[test]
fn auto_reporting_is_timer_driven() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert!(sim.feed(b"$481=100\n").ends_with(b"ok\r\n"));
    let _ = sim.feed(&[0x8C]);
    assert!(sim.snapshot().auto_report);

    let out = text(&sim.tick(Duration::from_millis(250)));
    assert!(out.matches("<Idle").count() >= 2, "{out}");
    assert!(out.contains("AR:100"), "{out}");

    let _ = sim.feed(&[0x8C]);
    assert!(!sim.snapshot().auto_report);
    assert!(sim.tick(Duration::from_secs(1)).is_empty());
}

#[test]
fn parser_state_and_probe_toggle_realtime_commands() {
    let mut sim = Simulator::new(SimProfile::GenericFull);
    let line = single_line(&sim.feed(&[0x83]));
    let state = parse_parser_state(&line).expect("parser state");
    assert_eq!(state.motion.as_deref(), Some("G0"));
    assert_eq!(state.wcs, Some(Wcs::G54));

    assert!(!sim.snapshot().probe_connected);
    let _ = sim.feed(&[0xA4]);
    assert!(sim.snapshot().probe_connected);
    let _ = sim.feed(&[0xA4]);
    assert!(!sim.snapshot().probe_connected);

    // A build without the PC capability ignores the toggle.
    let mut flexihal = Simulator::new(SimProfile::FlexiHal);
    let _ = flexihal.feed(&[0xA4]);
    assert!(!flexihal.snapshot().probe_connected);
}

#[test]
fn soft_estop_alarms_and_locks_motion() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let out = text(&sim.feed(&[0x9F]));
    assert_eq!(out, "ALARM:10\r\n");
    let snapshot = sim.snapshot();
    assert_eq!(snapshot.state, MachineState::Alarm);
    assert_eq!(snapshot.alarm, Some(10));
    assert_eq!(single_line(&sim.feed(b"G0 X1\n")), "error:9");

    let status = parse_status(text(&sim.feed(b"?")).trim()).expect("status");
    assert_eq!(status.state, MachineState::Alarm);
    assert_eq!(status.alarm.map(|alarm| alarm.code()), Some(10));
    assert!(
        status
            .pins
            .expect("pins")
            .contains(ghs_core::Signals::ESTOP)
    );
}

#[test]
fn input_signals_reflect_in_pn_and_lim() {
    let mut sim = Simulator::new(SimProfile::GenericFull);
    sim.set_signal(ghs_core::Signals::X_LIMIT, true);
    sim.set_signal(ghs_core::Signals::PROBE, true);
    let status = parse_status(text(&sim.feed(b"?")).trim()).expect("status");
    let pins = status.pins.expect("pins");
    assert!(pins.contains(ghs_core::Signals::X_LIMIT));
    assert!(pins.contains(ghs_core::Signals::PROBE));

    let limits = text(&sim.feed(b"$LIM\n"));
    assert!(limits.contains("[LIM:X]"), "{limits}");

    sim.set_signal(ghs_core::Signals::X_LIMIT, false);
    let status = parse_status(text(&sim.feed(b"?")).trim()).expect("status");
    assert!(
        !status
            .pins
            .expect("pins")
            .contains(ghs_core::Signals::X_LIMIT)
    );
}

#[test]
fn planner_free_space_tracks_the_active_block() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G1 X100 F600\n")), "ok");
    let status = parse_status(text(&sim.feed(b"?")).trim()).expect("status");
    assert_eq!(
        status.buffer,
        Some(BufferState {
            planner_free: 63,
            rx_free: 1024,
        })
    );
    assert_eq!(status.state, MachineState::Run);
    settle(&mut sim);
    assert_eq!(sim.snapshot().planner_free, 64);
}

#[test]
fn alarm_11_survives_reset_until_homing() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let _ = sim.raise_alarm(11);
    let _ = sim.feed(&[0x18]);
    assert_eq!(sim.snapshot().state, MachineState::Alarm);
    assert_eq!(sim.snapshot().alarm, Some(11));

    assert_eq!(single_line(&sim.feed(b"$H\n")), "ok");
    assert_eq!(sim.snapshot().state, MachineState::Idle);
    assert_eq!(sim.snapshot().alarm, None);
}

#[test]
fn stop_exit_and_safety_door_realtime_commands() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G1 X10 F600\n")), "ok");
    let _ = sim.feed(&[0x19]);
    assert_eq!(sim.snapshot().state, MachineState::Idle);
    assert_eq!(sim.position().get(Axis::X), 0.0);
    assert!(sim.tick(Duration::from_secs(5)).is_empty());

    assert_eq!(single_line(&sim.feed(b"G1 X10 F600\n")), "ok");
    let _ = sim.feed(&[0x03]);
    assert_eq!(sim.snapshot().state, MachineState::Idle);

    assert_eq!(single_line(&sim.feed(b"G1 X10 F600\n")), "ok");
    let _ = sim.feed(&[0x84]);
    assert_eq!(sim.snapshot().state, MachineState::Door);
    assert_eq!(single_line(&sim.feed(b"G0 X0\n")), "error:13");
    let _ = sim.feed(&[0x84]);
    assert!(matches!(
        sim.snapshot().state,
        MachineState::Run | MachineState::Hold
    ));
    settle(&mut sim);
}

#[test]
fn setting_writes_are_idle_only() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"G1 X10 F600\n")), "ok");
    assert_eq!(single_line(&sim.feed(b"$100=300\n")), "error:8");
    assert_eq!(sim.setting(100), Some(250.0));
    settle(&mut sim);
    assert_eq!(single_line(&sim.feed(b"$100=300\n")), "ok");
    assert_eq!(sim.setting(100), Some(300.0));
}

#[test]
fn a_failed_g10_reports_only_the_error() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let out = text(&sim.feed(b"G10 L2 X1\n"));
    assert_eq!(out, "error:20\r\n");
}

#[test]
fn check_mode_reports_without_moving() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"$C\n")), "ok");
    assert_eq!(sim.snapshot().state, MachineState::Check);

    assert_eq!(single_line(&sim.feed(b"G0 X10\n")), "ok");
    settle(&mut sim);
    assert_eq!(sim.position().get(Axis::X), 0.0);
    let status = parse_status(text(&sim.feed(&[0x87])).trim()).expect("status");
    assert_eq!(status.check_mode, Some(true));
    assert!(text(&sim.feed(b"?")).contains("|$C:1"));

    assert_eq!(single_line(&sim.feed(b"$C\n")), "ok");
    assert_eq!(sim.snapshot().state, MachineState::Idle);
}

#[test]
fn sleep_ignores_gcode_until_reset() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"$SLP\n")), "ok");
    assert_eq!(sim.snapshot().state, MachineState::Sleep);
    assert!(sim.feed(b"G0 X10\n").is_empty());
    assert_eq!(sim.position().get(Axis::X), 0.0);

    let out = text(&sim.feed(&[0x18]));
    assert!(out.starts_with("GrblHAL 1.1f"));
    assert_eq!(sim.snapshot().state, MachineState::Idle);
}

#[test]
fn tool_length_reference_and_tool_plate_probe() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let status = parse_status(text(&sim.feed(&[0x87])).trim()).expect("status");
    assert_eq!(status.tlo_reference_set, Some(false));

    assert_eq!(single_line(&sim.feed(b"G43.1 Z3\n")), "ok");
    let offsets = text(&sim.feed(b"$#\n"));
    assert!(offsets.contains("[TLO:0.000,0.000,3.000]"), "{offsets}");
    assert!(single_line(&sim.feed(b"$G\n")).contains("G43.1"));

    assert_eq!(single_line(&sim.feed(b"$TLR\n")), "ok");
    let status = parse_status(text(&sim.feed(&[0x87])).trim()).expect("status");
    assert_eq!(status.tlo_reference_set, Some(true));
    assert!(text(&sim.feed(b"$#\n")).contains("[TLR:3.000]"));

    assert_eq!(single_line(&sim.feed(b"G49\n")), "ok");
    assert!(text(&sim.feed(b"$#\n")).contains("[TLO:0.000,0.000,0.000]"));

    assert_eq!(single_line(&sim.feed(b"$TPW\n")), "ok");
    assert!(sim.snapshot().probe_result.is_some());
}

#[test]
fn program_pause_resumes_with_cycle_start() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert_eq!(single_line(&sim.feed(b"M0\n")), "ok");
    assert_eq!(sim.snapshot().state, MachineState::Hold);
    let _ = sim.feed(&[0x81]);
    assert_eq!(sim.snapshot().state, MachineState::Idle);
}

#[test]
fn reset_discards_a_partial_input_line() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    assert!(sim.feed(b"G0 X10").is_empty());
    let _ = sim.feed(&[0x18]);
    assert_eq!(text(&sim.feed(b"\n")), "ok\r\n");
    assert_eq!(sim.position().get(Axis::X), 0.0);
}
