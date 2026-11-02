//! End-to-end tests for the headless session engine over a scripted transport.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::RecvTimeoutError;
use ghs_engine::{
    Command, ConnectTarget, Engine, EngineConfig, EngineError, EngineEvent, EngineHandle,
    JogCommand, MachineSnapshot, MachineState, Position, RealtimeCommand, SettingValue, Signals,
    StreamerError, SystemCommand, Wcs,
};
use support::{Control, ScriptedTransport, wait_until};

const IDLE_STATUS: &str = "<Idle|MPos:0.000,0.000,0.000|WPos:0.000,0.000,0.000|WCO:0.000,0.000,0.000|Bf:15,1024|FS:0,0|Pn:|WCS:G54|Ov:100,100,100|A:|MPG:0|H:1|T:0|TLR:0>";

fn config() -> EngineConfig {
    EngineConfig {
        handshake_listen: Duration::from_millis(5),
        handshake_response: Duration::from_millis(200),
        discovery_timeout: Duration::from_secs(2),
        status_poll_active: Duration::from_millis(10),
        status_poll_idle: Duration::from_millis(20),
        full_status_interval: Duration::from_secs(5),
        watchdog_pause: Duration::from_millis(80),
        watchdog_disconnect: Duration::from_millis(250),
        tick_interval: Duration::from_millis(2),
        command_timeout: Duration::from_millis(500),
        event_queue_capacity: 4_096,
        command_queue_capacity: 64,
        default_rx_buffer: 128,
        max_command_line: 4_096,
    }
}

fn slow_poll_config() -> EngineConfig {
    EngineConfig {
        status_poll_active: Duration::from_secs(1),
        status_poll_idle: Duration::from_secs(1),
        full_status_interval: Duration::from_secs(10),
        watchdog_pause: Duration::from_secs(2),
        watchdog_disconnect: Duration::from_secs(5),
        ..config()
    }
}

fn full_report_config() -> EngineConfig {
    EngineConfig {
        full_status_interval: Duration::from_millis(50),
        ..config()
    }
}

fn standard_script(control: &Control) {
    standard_script_with_options(
        control,
        "[OPT:VNMSL+2,64,1024,3,0]",
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
        IDLE_STATUS,
    );
}

fn standard_script_with_opt(control: &Control, opt: &str) {
    standard_script_with_options(
        control,
        opt,
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
        IDLE_STATUS,
    );
}

fn standard_script_with_status(control: &Control, status: &str) {
    standard_script_with_options(
        control,
        "[OPT:VNMSL+2,64,1024,3,0]",
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
        status,
    );
}

fn standard_script_with_options(control: &Control, opt: &str, newopt: &str, status: &str) {
    standard_script_with_offsets(
        control,
        opt,
        newopt,
        status,
        &[
            "[G54:10.000,20.000,30.000]",
            "[G55:0.000,0.000,0.000]",
            "[G28:0.000,0.000,0.000]",
            "[G30:0.000,0.000,0.000]",
            "[G92:0.000,0.000,0.000]",
            "[TLO:0.000,0.000,0.000]",
            "[TLR:0.000]",
            "ok",
        ],
    );
}

fn standard_script_with_offsets(
    control: &Control,
    opt: &str,
    newopt: &str,
    status: &str,
    offsets: &[&str],
) {
    control.set_status(status);
    control.expect_line(
        "$I+",
        &[
            "[VER:1.1f.20240604:]",
            opt,
            "[AXS:3:XYZ]",
            newopt,
            "[FIRMWARE:grblHAL]",
            "[SIGNALS:HSDEP]",
            "[NVS STORAGE:*EEPROM]",
            "[FREE MEMORY:80K]",
            "[DRIVER:STM32F446]",
            "[DRIVER VERSION:240428]",
            "[BOARD:FlexiHAL_20240604_210624]",
            "[AUX IO:4,4,0,0]",
            "[PLUGIN:MODBUS v0.16]",
            "[SPINDLE:Huanyang v1]",
            "ok",
        ],
    );
    control.expect_line(
        "$$",
        &["$0=10", "$22=1", "$100=250.000", "$110=8000.000", "ok"],
    );
    control.expect_line("$#", offsets);
    control.expect_line("$G", &["[GC:G0 G54 G17 G21 G90 G94 M5 M9 T0 F0 S0]", "ok"]);
    if newopt.contains("ENUMS") {
        control.expect_line(
            "$EA",
            &["[ALARMCODE:1|Hard limit|Hard limit triggered.]", "ok"],
        );
        control.expect_line(
            "$EE",
            &["[ERRORCODE:20|Unsupported command|Invalid g-code.]", "ok"],
        );
        control.expect_line(
            "$ES",
            &[
                "[SETTING:0|0|Step pulse time|microseconds|5|####0|3|1000|0|N/A]",
                "[SETTING:100|1|X steps per mm|step/mm|6|#####0.000|1|100000|0|N/A]",
                "ok",
            ],
        );
        control.expect_line("$EG", &["[SETTINGGROUP:1|N/A|Axes]", "ok"]);
    }
}

/// Queue discovery whose `$#` response also carries controller tool entries.
fn tool_script(control: &Control, tools: &[&str]) {
    let mut lines: Vec<String> = [
        "[G54:10.000,20.000,30.000]",
        "[G55:0.000,0.000,0.000]",
        "[G28:0.000,0.000,0.000]",
        "[G30:0.000,0.000,0.000]",
        "[G92:0.000,0.000,0.000]",
        "[TLO:0.000,0.000,0.000]",
        "[TLR:0.000]",
    ]
    .iter()
    .map(|line| (*line).to_string())
    .collect();
    lines.extend(tools.iter().map(|line| (*line).to_string()));
    lines.push("ok".to_string());
    let offsets: Vec<&str> = lines.iter().map(String::as_str).collect();
    standard_script_with_offsets(
        control,
        "[OPT:VNMSL+2,64,1024,3,0]",
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
        IDLE_STATUS,
        &offsets,
    );
}

fn connected_engine_with(
    script: impl FnOnce(&Control),
    config: EngineConfig,
) -> (EngineHandle, Control) {
    let (transport, control) = ScriptedTransport::new();
    script(&control);
    let handle = Engine::spawn(config).expect("spawn engine");
    handle
        .connect(ConnectTarget::Transport(Box::new(transport)))
        .expect("connect transport");
    (handle, control)
}

fn connected_engine(script: impl FnOnce(&Control)) -> (EngineHandle, Control) {
    connected_engine_with(script, config())
}

fn wait_event(
    handle: &EngineHandle,
    timeout: Duration,
    predicate: impl Fn(&EngineEvent) -> bool,
) -> EngineEvent {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            panic!("timed out waiting for an engine event");
        }
        match handle.events().recv_timeout(remaining) {
            Ok(event) => {
                if predicate(&event) {
                    return event;
                }
            }
            Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for an engine event"),
            Err(RecvTimeoutError::Disconnected) => panic!("engine event channel disconnected"),
        }
    }
}

fn wait_ready(handle: &EngineHandle) {
    assert!(
        wait_until(|| handle.is_ready(), Duration::from_secs(2)),
        "engine did not become ready"
    );
    assert!(handle.is_connected());
}

fn wait_for_snapshot(
    handle: &EngineHandle,
    predicate: impl Fn(&MachineSnapshot) -> bool,
) -> Arc<MachineSnapshot> {
    assert!(
        wait_until(|| predicate(&handle.snapshot()), Duration::from_secs(2)),
        "snapshot condition was not met"
    );
    handle.snapshot()
}

fn assert_sent_line(control: &Control, line: &str) {
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|sent| sent == line),
            Duration::from_secs(1)
        ),
        "line was not sent: {line:?}; sent lines: {:?}",
        control.sent_lines()
    );
}

fn assert_sent_byte(control: &Control, byte: u8) {
    assert!(
        wait_until(
            || control.sent_bytes().contains(&byte),
            Duration::from_secs(1)
        ),
        "byte 0x{byte:02X} was not sent"
    );
}

fn send_retry(handle: &EngineHandle, command: Command) -> Result<(), EngineError> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match handle.send(command.clone()) {
            Err(EngineError::Busy) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2));
            }
            other => return other,
        }
    }
}

#[test]
fn handshake_discovers_capabilities_without_a_banner() {
    let (handle, control) = connected_engine(standard_script);

    let connected = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::Connected)
    });
    assert_eq!(connected, EngineEvent::Connected);
    wait_ready(&handle);

    let info_event = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::ControllerInfo(_))
    });
    let info = match info_event {
        EngineEvent::ControllerInfo(info) => info,
        other => panic!("expected controller info, got {other:?}"),
    };
    assert_eq!(info.version, "1.1f");
    assert_eq!(info.build_info, "20240604");
    assert_eq!(info.firmware.as_deref(), Some("grblHAL"));
    assert_eq!(info.driver.as_deref(), Some("STM32F446"));
    assert_eq!(info.board.as_deref(), Some("FlexiHAL_20240604_210624"));
    assert_eq!(info.axes.to_string(), "XYZ");
    assert_eq!(info.planner_blocks, Some(64));
    assert_eq!(info.rx_buffer_size, Some(1024));
    assert_eq!(info.tool_table_entries, Some(0));
    assert!(info.has_sed());
    assert!(info.rt_plus());
    assert!(info.manual_tool_change());

    let settings_event = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::Settings(_))
    });
    let settings = match settings_event {
        EngineEvent::Settings(settings) => settings,
        other => panic!("expected settings, got {other:?}"),
    };
    let steps = settings
        .iter()
        .find(|setting| setting.number == 100)
        .expect("setting 100");
    assert_eq!(steps.description.as_deref(), Some("X steps per mm"));
    assert_eq!(steps.unit.as_deref(), Some("step/mm"));
    assert_eq!(steps.group.as_deref(), Some("Axes"));
    assert_eq!(steps.value, SettingValue::Float(250.0));

    let tables_event = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::CodeTables { .. })
    });
    let (alarms, errors) = match tables_event {
        EngineEvent::CodeTables { alarms, errors } => (alarms, errors),
        other => panic!("expected code tables, got {other:?}"),
    };
    assert_eq!(
        alarms
            .get(1)
            .map(|description| description.message.as_str()),
        Some("Hard limit")
    );
    assert_eq!(
        errors
            .get(20)
            .map(|description| description.message.as_str()),
        Some("Unsupported command")
    );

    let offsets = handle.offsets();
    assert_eq!(offsets.wcs(Wcs::G54), Some(Position::xyz(10.0, 20.0, 30.0)));
    let parser = handle.parser_state().expect("parser state");
    assert_eq!(parser.wcs, Some(Wcs::G54));
    assert_eq!(parser.motion.as_deref(), Some("G0"));

    assert!(
        control.unexpected().is_empty(),
        "unexpected commands: {:?}",
        control.unexpected()
    );
    handle.shutdown();
}

#[test]
fn banner_is_reported_but_not_required() {
    let (transport, control) = ScriptedTransport::new();
    standard_script(&control);
    control.push_line("Grbl 1.1f ['$' for help]");
    let handle = Engine::spawn(config()).expect("spawn engine");
    handle
        .connect(ConnectTarget::Transport(Box::new(transport)))
        .expect("connect");

    let banner = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::Banner(_))
    });
    match banner {
        EngineEvent::Banner(text) => assert!(text.starts_with("Grbl")),
        other => panic!("expected banner, got {other:?}"),
    }
    wait_ready(&handle);
    handle.shutdown();
}

#[test]
fn mpg_takeover_waits_for_release_before_discovery() {
    let (handle, control) = connected_engine(|control| {
        standard_script_with_status(control, "<Idle|MPG:1|MPos:0.000,0.000,0.000>");
    });

    wait_for_snapshot(&handle, |snapshot| snapshot.state == MachineState::Idle);
    std::thread::sleep(Duration::from_millis(60));

    assert!(handle.is_connected());
    assert!(!handle.is_ready());
    assert!(
        !control.sent_lines().iter().any(|line| line == "$I+"),
        "discovery must not start while the pendant owns the input stream"
    );

    control.set_status("<Idle|MPG:0|MPos:0.000,0.000,0.000>");
    wait_ready(&handle);
    assert_sent_line(&control, "$I+");
    handle.shutdown();
}

#[test]
fn rx_buffer_is_the_second_opt_field_at_1024() {
    let (handle, control) =
        connected_engine(|control| standard_script_with_opt(control, "[OPT:V,64,1024,3,0]"));
    wait_ready(&handle);

    let info = handle.controller_info().expect("controller info");
    assert_eq!(info.planner_blocks, Some(64));
    assert_eq!(info.rx_buffer_size, Some(1024));

    let long_line = format!("G1 {}", "X1.000 ".repeat(30)).trim().to_string();
    assert!(long_line.len() > 64);
    handle
        .send(Command::Mdi(long_line.clone()))
        .expect("a line above 64 bytes fits the reported 1024-byte buffer");
    assert_sent_line(&control, &long_line);
    handle.shutdown();
}

#[test]
fn opt_planner_blocks_are_not_the_rx_buffer() {
    let (handle, control) =
        connected_engine(|control| standard_script_with_opt(control, "[OPT:V,1024,64,3,0]"));
    wait_ready(&handle);

    let info = handle.controller_info().expect("controller info");
    assert_eq!(info.planner_blocks, Some(1024));
    assert_eq!(info.rx_buffer_size, Some(64));

    let long_line = format!("G1 {}", "X1.000 ".repeat(30)).trim().to_string();
    let error = handle
        .send(Command::Mdi(long_line.clone()))
        .expect_err("line cannot fit a 64-byte receive buffer");
    assert!(
        matches!(
            error,
            EngineError::Streamer(StreamerError::LineTooLong { limit: 64, .. })
        ),
        "unexpected error: {error:?}"
    );
    assert!(!control.sent_lines().iter().any(|line| line == &long_line));
    handle.shutdown();
}

#[test]
fn status_reports_merge_into_the_snapshot() {
    let (handle, control) = connected_engine(standard_script);
    wait_ready(&handle);

    let detailed = concat!(
        "<Idle|MPos:1.000,2.000,3.000|WPos:11.000,12.000,13.000|",
        "WCO:10.000,10.000,10.000|DTG:0.500,0.000,0.000|Bf:15,1024|Ln:7|",
        "FS:0,0|Pn:X|WCS:G55|Ov:100,100,100|A:SF|MPG:0|H:1|T:3|TLR:1|SD:0>"
    );
    control.set_status(detailed);
    let snapshot = wait_for_snapshot(&handle, |snapshot| {
        snapshot.mpos == Position::xyz(1.0, 2.0, 3.0)
    });
    assert_eq!(snapshot.state, MachineState::Idle);
    assert_eq!(snapshot.wpos, Position::xyz(11.0, 12.0, 13.0));
    assert_eq!(snapshot.wco, Position::xyz(10.0, 10.0, 10.0));
    assert_eq!(snapshot.wcs, Wcs::G55);
    assert_eq!(snapshot.signals, Signals::X_LIMIT);
    assert_eq!(snapshot.buffer.planner_free, 15);
    assert_eq!(snapshot.buffer.rx_free, 1024);
    assert_eq!(snapshot.line_number, Some(7));
    assert!(snapshot.feed_spindle.spindle_on);
    assert!(snapshot.feed_spindle.clockwise);
    assert_eq!(snapshot.homing_complete, Some(true));
    assert_eq!(
        snapshot.overrides,
        ghs_engine::Overrides::new(100, 100, 100)
    );

    let raw = handle.last_status().expect("raw status");
    assert_eq!(raw.tool, Some(3));
    assert_eq!(raw.tlo_reference_set, Some(true));
    assert!(raw.dtg.is_some());
    assert!(matches!(raw.sd, Some(ghs_proto::SdStatus::Mount(0))));

    control
        .set_status("<Run|MPos:5.000,6.000,7.000|FS:1200,18000|Ov:125,50,90|A:CM|Pn:YH|WCS:G56>");
    let snapshot = wait_for_snapshot(&handle, |snapshot| snapshot.state == MachineState::Run);
    assert_eq!(snapshot.mpos, Position::xyz(5.0, 6.0, 7.0));
    assert_eq!(snapshot.wpos, Position::xyz(11.0, 12.0, 13.0));
    assert_eq!(snapshot.wco, Position::xyz(10.0, 10.0, 10.0));
    assert_eq!(snapshot.wcs, Wcs::G56);
    assert_eq!(snapshot.feed_spindle.feed, 1200.0);
    assert_eq!(snapshot.feed_spindle.spindle_rpm, 18000.0);
    assert!(snapshot.feed_spindle.spindle_on);
    assert!(!snapshot.feed_spindle.clockwise);
    assert_eq!(snapshot.signals, Signals::Y_LIMIT | Signals::FEED_HOLD);
    assert_eq!(snapshot.overrides, ghs_engine::Overrides::new(125, 50, 90));

    control.set_status("<Hold:1|FS:0,18000>");
    let snapshot = wait_for_snapshot(&handle, |snapshot| snapshot.state == MachineState::Hold);
    assert_eq!(snapshot.mpos, Position::xyz(5.0, 6.0, 7.0));

    control.set_status("<Alarm:1|MPos:9.000,9.000,9.000>");
    let snapshot = wait_for_snapshot(&handle, |snapshot| snapshot.state == MachineState::Alarm);
    assert_eq!(snapshot.alarm.map(|alarm| alarm.code()), Some(1));
    let alarm = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::Alarm(1))
    });
    assert_eq!(alarm, EngineEvent::Alarm(1));

    control.set_status("<Idle|MPos:9.000,9.000,9.000>");
    let snapshot = wait_for_snapshot(&handle, |snapshot| snapshot.state == MachineState::Idle);
    assert!(snapshot.alarm.is_none());
    handle.shutdown();
}

#[test]
fn mdi_error_stops_and_clears_before_the_next_line() {
    let (handle, control) = connected_engine(|control| {
        standard_script(control);
        control.expect_line("G1 X10 F500", &["ok"]);
        control.expect_line("G0 Z5", &["error:20"]);
        control.expect_line("", &["ok"]);
        control.expect_line("G0 X0", &["ok"]);
    });
    wait_ready(&handle);

    send_retry(&handle, Command::Mdi("G1 X10 F500".into())).expect("first MDI");
    assert_sent_line(&control, "G1 X10 F500");

    send_retry(&handle, Command::Mdi("G0 Z5".into())).expect("failing MDI");
    let error = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::Error(20))
    });
    assert_eq!(error, EngineEvent::Error(20));

    assert!(
        wait_until(
            || control.sent_lines().iter().any(String::is_empty),
            Duration::from_secs(1)
        ),
        "the engine must send a blank line to clear the sticky error"
    );

    send_retry(&handle, Command::Mdi("G0 X0".into())).expect("MDI after clearing");
    assert_sent_line(&control, "G0 X0");
    handle.shutdown();
}

#[test]
fn jog_cancel_and_motion_gating_in_alarm() {
    let (handle, control) = connected_engine(|control| {
        standard_script(control);
        control.expect_line("$J=G21 G91 X1.000 Y0.000 Z0.000 F1000.000", &["ok"]);
        control.expect_line("$H", &["ok"]);
        control.expect_line("$X", &["ok"]);
    });
    wait_ready(&handle);

    let jog = JogCommand::incremental(Position::xyz(1.0, 0.0, 0.0), 1000.0).expect("jog");
    send_retry(&handle, Command::Jog(jog.clone())).expect("jog");
    assert_sent_line(&control, "$J=G21 G91 X1.000 Y0.000 Z0.000 F1000.000");

    handle
        .send(Command::Realtime(RealtimeCommand::JogCancel))
        .expect("jog cancel");
    assert_sent_byte(&control, 0x85);

    control.set_status("<Alarm:1|MPos:0.000,0.000,0.000>");
    wait_for_snapshot(&handle, |snapshot| snapshot.state == MachineState::Alarm);

    let error = handle
        .send(Command::Jog(jog))
        .expect_err("jog must be blocked in alarm");
    assert!(
        matches!(
            error,
            EngineError::MotionBlocked {
                state: MachineState::Alarm
            }
        ),
        "unexpected error: {error:?}"
    );

    let error = handle
        .send(Command::Mdi("G0 X0".into()))
        .expect_err("MDI must be blocked in alarm");
    assert!(
        matches!(
            error,
            EngineError::MotionBlocked {
                state: MachineState::Alarm
            }
        ),
        "unexpected error: {error:?}"
    );

    let error = handle
        .send(Command::Realtime(RealtimeCommand::CycleStart))
        .expect_err("cycle start must be blocked in alarm");
    assert!(
        matches!(
            error,
            EngineError::MotionBlocked {
                state: MachineState::Alarm
            }
        ),
        "unexpected error: {error:?}"
    );

    send_retry(&handle, Command::Home).expect("home is an explicit intent in alarm");
    assert_sent_line(&control, "$H");
    send_retry(&handle, Command::Unlock).expect("unlock");
    assert_sent_line(&control, "$X");

    control.set_status(IDLE_STATUS);
    wait_for_snapshot(&handle, |snapshot| snapshot.state == MachineState::Idle);

    let jog = JogCommand::incremental(Position::xyz(0.0, 1.0, 0.0), 800.0).expect("jog");
    send_retry(&handle, Command::Jog(jog)).expect("jog after unlock");
    assert_sent_line(&control, "$J=G21 G91 X0.000 Y1.000 Z0.000 F800.000");
    handle.shutdown();
}

#[test]
fn overrides_home_unlock_sleep_and_reset() {
    let (handle, control) = connected_engine_with(
        |control| {
            standard_script(control);
            control.expect_line("$H", &["ok"]);
            control.expect_line("$X", &["ok"]);
            control.expect_line("$SLP", &["ok"]);
            control.expect_line("", &["ok"]);
        },
        slow_poll_config(),
    );
    wait_ready(&handle);
    std::thread::sleep(Duration::from_millis(20));

    handle
        .send(Command::SetFeedOverride(120))
        .expect("feed override");
    let bytes = control.sent_bytes();
    assert!(
        bytes.windows(2).any(|pair| pair == [0x91, 0x91]),
        "expected two coarse feed-override increases, got {bytes:?}"
    );
    assert_eq!(handle.snapshot().overrides.feed, 120);

    handle
        .send(Command::SetFeedOverride(119))
        .expect("fine feed override");
    assert!(control.sent_bytes().contains(&0x94));
    assert_eq!(handle.snapshot().overrides.feed, 119);

    handle
        .send(Command::SetSpindleOverride(80))
        .expect("spindle override");
    let bytes = control.sent_bytes();
    assert!(
        bytes.windows(2).any(|pair| pair == [0x9B, 0x9B]),
        "expected two coarse spindle-override decreases, got {bytes:?}"
    );
    assert_eq!(handle.snapshot().overrides.spindle, 80);

    handle
        .send(Command::SetRapidOverride(50))
        .expect("rapid override");
    assert!(control.sent_bytes().contains(&0x96));
    assert_eq!(handle.snapshot().overrides.rapid, 50);

    let error = handle
        .send(Command::SetRapidOverride(30))
        .expect_err("rapid override is discrete");
    assert!(matches!(error, EngineError::InvalidCommand(_)));

    let error = handle
        .send(Command::System(SystemCommand::WriteSetting {
            number: 110,
            value: SettingValue::Float(8000.0),
        }))
        .expect_err("unverified system writes are rejected");
    assert!(matches!(error, EngineError::InvalidCommand(_)));

    send_retry(&handle, Command::Home).expect("home");
    assert_sent_line(&control, "$H");
    send_retry(&handle, Command::Unlock).expect("unlock");
    assert_sent_line(&control, "$X");
    send_retry(&handle, Command::Sleep).expect("sleep");
    assert_sent_line(&control, "$SLP");
    assert!(wait_until(
        || handle.snapshot().state == MachineState::Sleep,
        Duration::from_secs(1)
    ));
    let error = handle
        .send(Command::SetFeedOverride(100))
        .expect_err("controls are blocked while the controller sleeps");
    assert!(matches!(error, EngineError::NotReady));

    handle.send(Command::Reset).expect("soft reset");
    assert_sent_byte(&control, 0x18);

    control.clear_sent();
    handle.send(Command::RequestStatus).expect("request status");
    assert_sent_byte(&control, 0x80);

    send_retry(&handle, Command::ClearError).expect("clear error");
    assert!(
        wait_until(
            || control.sent_lines().iter().any(String::is_empty),
            Duration::from_secs(1)
        ),
        "clear-error must send a blank line"
    );
    handle.shutdown();
}

#[test]
fn watchdog_pauses_motion_then_disconnects_without_resuming() {
    let (handle, control) = connected_engine(standard_script);
    wait_ready(&handle);

    control.set_status("<Run|MPos:0.000,0.000,0.000|FS:1200,12000>");
    wait_for_snapshot(&handle, |snapshot| snapshot.state == MachineState::Run);

    control.clear_sent();
    control.set_respond_status(false);

    assert!(
        wait_until(
            || control.sent_bytes().contains(&0x82),
            Duration::from_secs(1)
        ),
        "the watchdog must send a feed hold after telemetry stops during motion"
    );
    let disconnected = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::Disconnected)
    });
    assert_eq!(disconnected, EngineEvent::Disconnected);

    assert!(!handle.is_connected());
    assert!(!handle.is_ready());
    assert_eq!(handle.snapshot().state, MachineState::Unknown);

    let bytes = control.sent_bytes();
    assert!(
        !bytes.contains(&0x81) && !bytes.contains(&b'~'),
        "the engine must never auto-resume after a watchdog pause"
    );
}

#[test]
fn handshake_falls_back_to_printable_status_and_fails_cleanly() {
    let (transport, control) = ScriptedTransport::new();
    standard_script(&control);
    control.set_respond_status(false);
    let handle = Engine::spawn(config()).expect("spawn engine");
    handle
        .connect(ConnectTarget::Transport(Box::new(transport)))
        .expect("connect");

    let failed = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::ConnectFailed(_))
    });
    assert!(matches!(failed, EngineEvent::ConnectFailed(_)));
    wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::Disconnected)
    });
    assert!(!handle.is_connected());

    let bytes = control.sent_bytes();
    assert!(bytes.contains(&0x87), "expected the full status probe");
    assert!(bytes.contains(&b'?'), "expected the printable fallback");
    handle.shutdown();
}

#[test]
fn settings_write_is_read_back_and_verified() {
    let (handle, _control) = connected_engine(|control| {
        standard_script(control);
        control.expect_line("$110=8000", &["ok"]);
        control.expect_line("$110", &["$110=8000.000", "ok"]);
        control.expect_line("$110=7000", &["ok"]);
        control.expect_line("$110", &["$110=6500.000", "ok"]);
    });
    wait_ready(&handle);

    handle
        .send(Command::SetSetting {
            number: 110,
            value: SettingValue::Float(8000.0),
        })
        .expect("verified write");
    let written = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::SettingWritten { number: 110, .. })
    });
    match written {
        EngineEvent::SettingWritten {
            value, verified, ..
        } => {
            assert!(verified);
            assert_eq!(value, SettingValue::Float(8000.0));
        }
        other => panic!("expected setting written, got {other:?}"),
    }
    let setting = handle
        .settings()
        .iter()
        .find(|setting| setting.number == 110)
        .cloned()
        .expect("setting 110");
    assert_eq!(setting.value, SettingValue::Float(8000.0));

    send_retry(
        &handle,
        Command::SetSetting {
            number: 110,
            value: SettingValue::Float(7000.0),
        },
    )
    .expect("second write");
    let written = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::SettingWritten { number: 110, .. })
    });
    match written {
        EngineEvent::SettingWritten {
            value, verified, ..
        } => {
            assert!(!verified);
            assert_eq!(value, SettingValue::Float(6500.0));
        }
        other => panic!("expected setting written, got {other:?}"),
    }
    handle.shutdown();
}

#[test]
fn malformed_input_does_not_stop_telemetry() {
    let (handle, control) = connected_engine(standard_script);
    wait_ready(&handle);

    control.push_line("not a controller response");
    control.push_line("<Idle|MPos:bad,0,0|FS:bad>");
    control.push_line("[MSG:Warning: noisy line]");
    control.set_status("<Idle|MPos:7.000,8.000,9.000>");

    let snapshot = wait_for_snapshot(&handle, |snapshot| {
        snapshot.mpos == Position::xyz(7.0, 8.0, 9.0)
    });
    assert_eq!(snapshot.state, MachineState::Idle);
    assert!(handle.is_ready());
    handle.shutdown();
}

#[test]
fn probe_results_update_the_snapshot_and_offsets() {
    let (handle, control) = connected_engine(standard_script);
    wait_ready(&handle);

    control.push_line("[PRB:1.000,2.000,-3.000:1]");
    let probe = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::Probe(_))
    });
    match probe {
        EngineEvent::Probe(result) => {
            assert!(result.success);
            assert_eq!(result.position, Position::xyz(1.0, 2.0, -3.0));
        }
        other => panic!("expected probe, got {other:?}"),
    }
    assert_eq!(
        handle.snapshot().probe.map(|probe| probe.position),
        Some(Position::xyz(1.0, 2.0, -3.0))
    );
    assert_eq!(
        handle.offsets().probe.map(|probe| probe.position),
        Some(Position::xyz(1.0, 2.0, -3.0))
    );
    handle.shutdown();
}

#[test]
fn soft_reset_clears_the_cached_probe_result() {
    let (handle, control) = connected_engine(standard_script);
    wait_ready(&handle);

    control.push_line("[PRB:1.000,2.000,-3.000:1]");
    wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::Probe(_))
    });
    assert_eq!(
        handle.offsets().probe.map(|probe| probe.position),
        Some(Position::xyz(1.0, 2.0, -3.0))
    );
    assert!(handle.snapshot().probe.is_some());

    handle.send(Command::Reset).expect("soft reset");
    assert_sent_byte(&control, 0x18);

    assert!(
        wait_until(
            || handle.offsets().probe.is_none() && handle.snapshot().probe.is_none(),
            Duration::from_secs(1)
        ),
        "a soft reset must drop the cached probe result: offsets={:?}, snapshot={:?}",
        handle.offsets().probe,
        handle.snapshot().probe
    );

    // Only the probe result expires: the work offsets stay cached until the
    // controller repopulates them after the reset.
    let offsets = handle.offsets();
    assert_eq!(offsets.wcs(Wcs::G54), Some(Position::xyz(10.0, 20.0, 30.0)));
    assert_eq!(offsets.g92, Some(Position::xyz(0.0, 0.0, 0.0)));
    assert!(offsets.tlo.is_some());

    // A fresh probe after the reset must still replace the cleared result.
    control.push_line("[PRB:4.000,5.000,-6.000:1]");
    let probe = wait_event(
        &handle,
        Duration::from_secs(1),
        |event| matches!(event, EngineEvent::Probe(result) if result.success),
    );
    match probe {
        EngineEvent::Probe(result) => {
            assert_eq!(result.position, Position::xyz(4.0, 5.0, -6.0));
        }
        other => panic!("expected probe, got {other:?}"),
    }
    assert_eq!(
        handle.offsets().probe.map(|probe| probe.position),
        Some(Position::xyz(4.0, 5.0, -6.0))
    );
    assert_eq!(
        handle.snapshot().probe.map(|probe| probe.position),
        Some(Position::xyz(4.0, 5.0, -6.0))
    );
    handle.shutdown();
}

#[test]
fn destructive_offset_resets_clear_the_cached_probe_result() {
    let (handle, control) = connected_engine(|control| {
        standard_script(control);
        control.expect_line("$RST=#", &["ok"]);
        control.expect_line("$RST=*", &["ok"]);
    });
    wait_ready(&handle);

    for (target, line) in [
        (ghs_proto::ResetTarget::Offsets, "$RST=#"),
        (ghs_proto::ResetTarget::All, "$RST=*"),
    ] {
        control.push_line("[PRB:1.000,2.000,-3.000:1]");
        wait_event(&handle, Duration::from_secs(1), |event| {
            matches!(event, EngineEvent::Probe(_))
        });
        assert!(handle.offsets().probe.is_some());

        send_retry(&handle, Command::System(SystemCommand::Reset(target))).expect("reset command");
        assert_sent_line(&control, line);
        assert!(
            wait_until(
                || handle.offsets().probe.is_none() && handle.snapshot().probe.is_none(),
                Duration::from_secs(1)
            ),
            "{line} must drop the cached probe result"
        );
    }
    handle.shutdown();
}

#[test]
fn controller_tool_table_is_sorted_and_published_as_an_event() {
    let (handle, _control) = connected_engine(|control| {
        tool_script(
            control,
            &[
                "[T:7|0.000,0.000,-12.000|1.500|6,0,0|End mill|4]",
                "[T:2|10.000,20.000,-5.000|3.175]",
                "[T:5|0.000,0.000,-8.000|2.000|3,0,0|Spot drill|1]",
            ],
        );
    });
    wait_ready(&handle);
    assert!(
        wait_until(
            || handle.controller_tools().len() == 3,
            Duration::from_secs(1)
        ),
        "controller tools were not collected: {:?}",
        handle.controller_tools()
    );

    let table = handle.controller_tools();
    assert_eq!(
        table.iter().map(|tool| tool.number).collect::<Vec<_>>(),
        vec![2, 5, 7]
    );
    assert_eq!(table[0].offset, Position::xyz(10.0, 20.0, -5.0));
    assert_eq!(table[0].radius_mm, Some(3.175));
    assert_eq!(table[1].name.as_deref(), Some("Spot drill"));
    assert_eq!(table[2].pocket, Some(4));

    let event = wait_event(
        &handle,
        Duration::from_secs(1),
        |event| matches!(event, EngineEvent::ControllerTools(tools) if tools.as_slice() == table.as_slice()),
    );
    match event {
        EngineEvent::ControllerTools(tools) => assert_eq!(*tools, *table),
        other => panic!("expected controller tools, got {other:?}"),
    }
    handle.shutdown();
}

#[test]
fn duplicate_controller_tool_numbers_replace_the_previous_entry() {
    let (handle, _control) = connected_engine(|control| {
        tool_script(
            control,
            &[
                "[T:3|0.000,0.000,-1.000|1.000]",
                "[T:3|0.000,0.000,-2.000|2.000]",
            ],
        );
    });
    wait_ready(&handle);
    assert!(
        wait_until(
            || {
                handle
                    .controller_tools()
                    .iter()
                    .any(|tool| tool.number == 3 && tool.radius_mm == Some(2.0))
            },
            Duration::from_secs(1)
        ),
        "the second T3 entry must replace the first: {:?}",
        handle.controller_tools()
    );

    let table = handle.controller_tools();
    assert_eq!(table.len(), 1);
    assert_eq!(table[0].number, 3);
    assert_eq!(table[0].offset, Position::xyz(0.0, 0.0, -2.0));
    assert_eq!(table[0].radius_mm, Some(2.0));
    handle.shutdown();
}

#[test]
fn malformed_and_oversized_controller_tool_tables_are_bounded() {
    const BOUND: usize = 256;
    let mut lines: Vec<String> = Vec::new();
    lines.push("[T:not-a-number|0,0,0|1.0]".to_string());
    lines.push("[T:1|0,0,0]".to_string());
    for number in 0..(BOUND + 32) {
        lines.push(format!("[T:{number}|0.000,0.000,0.000|1.000]"));
    }
    let tools: Vec<&str> = lines.iter().map(String::as_str).collect();

    let (handle, _control) = connected_engine(|control| tool_script(control, &tools));
    wait_ready(&handle);
    assert!(
        wait_until(
            || handle.controller_tools().len() == BOUND,
            Duration::from_secs(2)
        ),
        "expected {BOUND} bounded entries, got {}",
        handle.controller_tools().len()
    );

    let table = handle.controller_tools();
    assert_eq!(table.len(), BOUND);
    assert_eq!(table.first().map(|tool| tool.number), Some(0));
    assert_eq!(
        table.last().map(|tool| tool.number),
        Some((BOUND - 1) as u32)
    );
    assert!(table.windows(2).all(|pair| pair[0].number < pair[1].number));
    handle.shutdown();
}

#[test]
fn manual_tool_table_refresh_replaces_the_previous_report() {
    let (handle, control) = connected_engine(|control| {
        tool_script(control, &["[T:1|0.000,0.000,-1.000|1.000]"]);
    });
    wait_ready(&handle);
    assert!(
        wait_until(
            || handle.controller_tools().len() == 1,
            Duration::from_secs(1)
        ),
        "the initial tool table was not collected"
    );

    control.expect_line("$#", &["[T:5|0.000,0.000,-5.000|2.500]", "ok"]);
    send_retry(&handle, Command::System(SystemCommand::Offsets)).expect("refresh offsets");
    assert!(
        wait_until(
            || {
                let table = handle.controller_tools();
                table.len() == 1 && table[0].number == 5
            },
            Duration::from_secs(1)
        ),
        "a fresh $# must replace the cached controller table: {:?}",
        handle.controller_tools()
    );
    handle.shutdown();
}

#[test]
fn controller_tool_table_reset_rules_are_explicit() {
    let (handle, control) = connected_engine(|control| {
        tool_script(control, &["[T:1|0.000,0.000,-1.000|1.000]"]);
        control.expect_byte(0x18, &[]);
        control.expect_line("$RST=#", &["ok"]);
    });
    wait_ready(&handle);
    assert!(
        wait_until(
            || handle.controller_tools().len() == 1,
            Duration::from_secs(1)
        ),
        "the initial tool table was not collected"
    );

    // A soft reset keeps the controller's stored tool table.
    handle.send(Command::Reset).expect("soft reset");
    assert_sent_byte(&control, 0x18);
    assert_eq!(handle.controller_tools().len(), 1);

    // Resetting the stored offsets drops the local copy; only a later `$#`
    // can repopulate it from the controller.
    send_retry(
        &handle,
        Command::System(SystemCommand::Reset(ghs_proto::ResetTarget::Offsets)),
    )
    .expect("reset offsets");
    assert_sent_line(&control, "$RST=#");
    assert!(
        wait_until(
            || handle.controller_tools().is_empty(),
            Duration::from_secs(1)
        ),
        "$RST=# must clear the cached controller tool table"
    );
    handle.shutdown();
}

#[test]
fn reconnect_replaces_the_controller_tool_table() {
    let (handle, _control) = connected_engine(|control| {
        tool_script(control, &["[T:4|0.000,0.000,-4.000|2.000]"]);
    });
    wait_ready(&handle);
    assert!(
        wait_until(
            || {
                let table = handle.controller_tools();
                table.len() == 1 && table[0].number == 4
            },
            Duration::from_secs(1)
        ),
        "the initial tool table was not collected"
    );

    handle.disconnect();
    wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::Disconnected)
    });
    // The cache survives a disconnect like the other discovery data; the next
    // session start clears it before the controller re-reports.
    assert_eq!(handle.controller_tools().len(), 1);

    let (transport, control2) = ScriptedTransport::new();
    tool_script(
        &control2,
        &[
            "[T:9|0.000,0.000,-9.000|3.000]",
            "[T:10|0.000,0.000,-10.000|4.000]",
        ],
    );
    handle
        .connect(ConnectTarget::Transport(Box::new(transport)))
        .expect("reconnect");
    wait_ready(&handle);
    assert!(
        wait_until(
            || {
                let table = handle.controller_tools();
                table.len() == 2 && table[0].number == 9 && table[1].number == 10
            },
            Duration::from_secs(1)
        ),
        "reconnect must replace the controller table: {:?}",
        handle.controller_tools()
    );
    assert!(
        !handle
            .controller_tools()
            .iter()
            .any(|tool| tool.number == 4)
    );
    handle.shutdown();
}

#[test]
fn disconnect_then_reconnect_works_on_the_same_engine() {
    let (handle, control) = connected_engine(standard_script);
    wait_ready(&handle);

    handle.disconnect();
    wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::Disconnected)
    });
    assert!(!handle.is_connected());
    assert!(!handle.is_ready());

    let (transport, control2) = ScriptedTransport::new();
    standard_script(&control2);
    handle
        .connect(ConnectTarget::Transport(Box::new(transport)))
        .expect("reconnect");
    wait_ready(&handle);
    assert_eq!(
        handle.controller_info().expect("info").rx_buffer_size,
        Some(1024)
    );
    assert!(!control.is_open());
    handle.shutdown();
}

#[test]
fn shutdown_closes_the_transport_and_stops_the_thread() {
    let (handle, control) = connected_engine(standard_script);
    wait_ready(&handle);
    assert!(control.is_open());

    handle.shutdown();
    assert!(!control.is_open());
}

#[test]
fn rt_plus_uses_the_extended_status_code() {
    let (handle, control) = connected_engine(standard_script);
    wait_ready(&handle);
    control.clear_sent();
    handle.send(Command::RequestStatus).expect("request status");
    assert_sent_byte(&control, 0x80);
    assert!(!control.sent_bytes().contains(&b'?'));
    handle.shutdown();
}

#[test]
fn rt_minus_uses_the_printable_status_code() {
    let (handle, control) = connected_engine(|control| {
        standard_script_with_options(
            control,
            "[OPT:V,64,1024,3,0]",
            "[NEWOPT:ENUMS,HOME,ES,TC]",
            IDLE_STATUS,
        );
    });
    wait_ready(&handle);
    control.clear_sent();
    handle.send(Command::RequestStatus).expect("request status");
    assert_sent_byte(&control, b'?');
    assert!(!control.sent_bytes().contains(&0x80));
    handle.shutdown();
}

#[test]
fn full_status_reports_are_requested_periodically() {
    let (handle, control) = connected_engine_with(standard_script, full_report_config());
    wait_ready(&handle);
    control.clear_sent();
    assert_sent_byte(&control, 0x87);
    handle.shutdown();
}

#[test]
fn motion_switches_polling_to_the_active_rate() {
    let mut config = config();
    config.status_poll_idle = Duration::from_millis(500);
    config.status_poll_active = Duration::from_millis(10);
    config.watchdog_pause = Duration::from_secs(2);
    config.watchdog_disconnect = Duration::from_secs(5);

    let (handle, control) = connected_engine_with(standard_script, config);
    wait_ready(&handle);

    control.set_status("<Run|MPos:0.000,0.000,0.000|FS:1200,12000>");
    control.push_line("<Run|MPos:0.000,0.000,0.000|FS:1200,12000>");
    wait_for_snapshot(&handle, |snapshot| snapshot.state == MachineState::Run);

    control.clear_sent();
    std::thread::sleep(Duration::from_millis(100));
    let polls = control
        .sent_bytes()
        .iter()
        .filter(|byte| **byte == 0x80)
        .count();
    assert!(
        polls >= 3,
        "expected the active poll rate after a Run status, got {polls} requests"
    );
    handle.shutdown();
}

#[test]
fn sleep_suspends_polling_until_status_or_reset() {
    let (handle, control) = connected_engine(|control| {
        standard_script(control);
        control.expect_line("$SLP", &["ok"]);
        control.expect_line("$SLP", &["ok"]);
    });
    wait_ready(&handle);

    send_retry(&handle, Command::Sleep).expect("sleep");
    assert_sent_line(&control, "$SLP");
    wait_for_snapshot(&handle, |snapshot| snapshot.state == MachineState::Sleep);
    assert!(!handle.is_ready());

    control.clear_sent();
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        control.sent_bytes().is_empty(),
        "status polling must stop while the controller sleeps: {:?}",
        control.sent_bytes()
    );

    handle
        .send(Command::RequestStatus)
        .expect("status while asleep");
    assert_sent_byte(&control, 0x80);
    assert!(wait_until(|| handle.is_ready(), Duration::from_secs(1)));
    assert!(wait_until(
        || handle.snapshot().state == MachineState::Idle,
        Duration::from_secs(1)
    ));

    send_retry(&handle, Command::Sleep).expect("sleep again");
    assert!(wait_until(
        || handle.snapshot().state == MachineState::Sleep,
        Duration::from_secs(1)
    ));
    handle
        .send(Command::Reset)
        .expect("reset wakes the controller");
    assert_sent_byte(&control, 0x18);
    assert!(wait_until(|| handle.is_ready(), Duration::from_secs(1)));
    assert!(wait_until(
        || handle.snapshot().state == MachineState::Idle,
        Duration::from_secs(1)
    ));
    handle.shutdown();
}

#[test]
fn door_state_stops_a_pending_mdi_sequence() {
    let (handle, control) = connected_engine(|control| {
        standard_script(control);
        control.expect_line("G1 X1", &[]);
    });
    wait_ready(&handle);

    handle
        .send(Command::Mdi("G1 X1\nG1 X2".into()))
        .expect("multi-line MDI");
    assert_sent_line(&control, "G1 X1");

    control.set_status("<Door:1|MPos:0.000,0.000,0.000>");
    wait_for_snapshot(&handle, |snapshot| snapshot.state == MachineState::Door);

    control.push_line("ok");
    std::thread::sleep(Duration::from_millis(40));
    assert!(
        !control.sent_lines().iter().any(|line| line == "G1 X2"),
        "remaining MDI lines must be dropped when the machine enters Door"
    );
    let error = handle
        .send(Command::Mdi("G0 X0".into()))
        .expect_err("MDI is blocked in Door");
    assert!(
        matches!(
            error,
            EngineError::MotionBlocked {
                state: MachineState::Door
            }
        ),
        "unexpected error: {error:?}"
    );
    handle.shutdown();
}

#[test]
fn engine_handle_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<EngineHandle>();
}

#[test]
fn bf_rx_free_space_can_only_reduce_the_send_budget() {
    let (handle, control) = connected_engine(standard_script);
    wait_ready(&handle);

    control.set_status("<Idle|MPos:0.000,0.000,0.000|Bf:15,10>");
    wait_for_snapshot(&handle, |snapshot| snapshot.buffer.rx_free == 10);

    handle
        .send(Command::Mdi("G1 X10 F500".into()))
        .expect("the line is accepted into the queue");
    std::thread::sleep(Duration::from_millis(40));
    assert!(
        !control
            .sent_lines()
            .iter()
            .any(|line| line == "G1 X10 F500"),
        "Bf must block the line while only 10 bytes are free"
    );

    control.set_status("<Idle|MPos:0.000,0.000,0.000|Bf:15,1024>");
    assert_sent_line(&control, "G1 X10 F500");
    handle.shutdown();
}
