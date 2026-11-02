//! Pin map, auxiliary pin state, limit state and last-event accessors.

mod support;

use std::time::{Duration, Instant};

use ghs_engine::{
    Axes, Command, ConnectTarget, Engine, EngineConfig, EngineEvent, EngineHandle, ExtendedCommand,
    LogLevel, MAX_PIN_MAPPINGS, PinDirection, Signals, SystemCommand,
};
use support::{Control, ScriptedTransport, standard_script, wait_until};

fn config() -> EngineConfig {
    EngineConfig {
        handshake_listen: Duration::from_millis(5),
        handshake_response: Duration::from_millis(200),
        discovery_timeout: Duration::from_secs(2),
        status_poll_active: Duration::from_millis(10),
        status_poll_idle: Duration::from_millis(20),
        full_status_interval: Duration::from_secs(5),
        watchdog_pause: Duration::from_secs(2),
        watchdog_disconnect: Duration::from_secs(5),
        tick_interval: Duration::from_millis(2),
        command_timeout: Duration::from_millis(500),
        event_queue_capacity: 8_192,
        command_queue_capacity: 64,
        default_rx_buffer: 128,
        max_command_line: 4_096,
    }
}

fn connect(handle: &EngineHandle, script: impl FnOnce(&Control)) -> Control {
    let (transport, control) = ScriptedTransport::new();
    script(&control);
    handle
        .connect(ConnectTarget::Transport(Box::new(transport)))
        .expect("connect transport");
    assert!(
        wait_until(|| handle.is_ready(), Duration::from_secs(2)),
        "engine did not become ready"
    );
    control
}

fn drain_events(handle: &EngineHandle) {
    while handle.events().try_recv().is_ok() {}
}

fn connected_engine() -> (EngineHandle, Control) {
    let handle = Engine::spawn(config()).expect("spawn engine");
    let control = connect(&handle, standard_script);
    drain_events(&handle);
    (handle, control)
}

/// Request one diagnostic report and collect every event up to its `ok`.
fn send_report(
    handle: &EngineHandle,
    control: &Control,
    command: ExtendedCommand,
    responses: &[&str],
) -> Vec<EngineEvent> {
    control.expect_line(command.as_str(), responses);
    handle
        .send(Command::System(SystemCommand::Extended(command)))
        .unwrap_or_else(|error| panic!("request {}: {error}", command.as_str()));

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut events = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for the {} acknowledgement",
            command.as_str()
        );
        match handle.events().recv_timeout(remaining) {
            Ok(EngineEvent::Log {
                level: LogLevel::Ok,
                ..
            }) => return events,
            Ok(event) => events.push(event),
            Err(error) => panic!("engine event channel failure: {error}"),
        }
    }
}

fn populate_reports(handle: &EngineHandle, control: &Control) {
    send_report(
        handle,
        control,
        ExtendedCommand::Pins,
        &[
            "[PIN:0|X limit|input]",
            "[PIN:1|Y limit|input]",
            "[PIN:2|Aux output 0|output]",
            "ok",
        ],
    );
    send_report(
        handle,
        control,
        ExtendedCommand::PinState,
        &["[PINSTATE:HSDEP|0101|1010]", "ok"],
    );
    send_report(
        handle,
        control,
        ExtendedCommand::Limits,
        &["[LIM:XZ]", "ok"],
    );
    send_report(
        handle,
        control,
        ExtendedCommand::Events,
        &["[LEV:17|HSDEP]", "ok"],
    );
}

#[test]
fn diagnostic_reports_update_accessors_and_emit_events() {
    let (handle, control) = connected_engine();

    let events = send_report(
        &handle,
        &control,
        ExtendedCommand::Pins,
        &[
            "[PIN:0|X limit|input]",
            "[PIN:1|Y limit|input]",
            "[PIN:2|Aux output 0|output|future]",
            "ok",
        ],
    );
    let map = handle.pin_map();
    assert_eq!(map.len(), 3);
    assert_eq!(map[0].id, 0);
    assert_eq!(map[0].name, "X limit");
    assert_eq!(map[0].direction, Some(PinDirection::Input));
    assert_eq!(map[1].id, 1);
    assert_eq!(map[2].id, 2);
    assert_eq!(map[2].direction, Some(PinDirection::Output));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, EngineEvent::PinMap(map) if map.len() == 3))
    );

    let events = send_report(
        &handle,
        &control,
        ExtendedCommand::PinState,
        &["[PINSTATE:HSDEP|0101|1010]", "ok"],
    );
    let state = handle.pin_state().expect("pin state");
    assert_eq!(state.mask, Signals::parse("HSDEP"));
    assert_eq!(state.inputs, vec![false, true, false, true]);
    assert_eq!(state.outputs, vec![true, false, true, false]);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, EngineEvent::PinState(_)))
    );

    let events = send_report(
        &handle,
        &control,
        ExtendedCommand::Limits,
        &["[LIM:XZ]", "ok"],
    );
    let limits = handle.limit_state().expect("limit state");
    assert_eq!(limits.mask, 0b101);
    assert_eq!(
        limits.axes,
        Axes::from(ghs_engine::Axis::X) | Axes::from(ghs_engine::Axis::Z)
    );
    assert!(limits.is_asserted(ghs_engine::Axis::X));
    assert!(!limits.is_asserted(ghs_engine::Axis::Y));
    assert!(limits.is_asserted(ghs_engine::Axis::Z));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, EngineEvent::LimitState(_)))
    );

    let events = send_report(
        &handle,
        &control,
        ExtendedCommand::Events,
        &["[LEV:17|HSDEP]", "ok"],
    );
    let last = handle.limit_events().expect("last events");
    assert_eq!(last.count, 17);
    assert_eq!(last.signals, Signals::parse("HSDEP"));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, EngineEvent::LastEvents(_)))
    );

    handle.shutdown();
}

#[test]
fn a_new_pins_report_replaces_the_map_and_the_map_is_bounded() {
    let (handle, control) = connected_engine();

    send_report(
        &handle,
        &control,
        ExtendedCommand::Pins,
        &["[PIN:0|X limit|input]", "[PIN:1|Y limit|input]", "ok"],
    );
    assert_eq!(handle.pin_map().len(), 2);

    send_report(
        &handle,
        &control,
        ExtendedCommand::Pins,
        &["[PIN:9|Probe|input]", "ok"],
    );
    let map = handle.pin_map();
    assert_eq!(map.len(), 1);
    assert_eq!(map[0].id, 9);
    assert_eq!(map[0].name, "Probe");

    let mut lines: Vec<String> = (0..MAX_PIN_MAPPINGS + 1)
        .map(|id| format!("[PIN:{id}|Aux {id}|input]"))
        .collect();
    lines.push("ok".into());
    let responses: Vec<&str> = lines.iter().map(String::as_str).collect();
    send_report(&handle, &control, ExtendedCommand::Pins, &responses);
    assert_eq!(handle.pin_map().len(), MAX_PIN_MAPPINGS);
    assert_eq!(handle.pin_map()[0].id, 0);
    assert_eq!(
        handle.pin_map()[MAX_PIN_MAPPINGS - 1].id,
        u16::try_from(MAX_PIN_MAPPINGS - 1).expect("bound fits")
    );

    handle.shutdown();
}

#[test]
fn malformed_reports_are_logged_and_do_not_replace_valid_data() {
    let (handle, control) = connected_engine();

    send_report(
        &handle,
        &control,
        ExtendedCommand::Pins,
        &[
            "[PIN:not-a-number|X limit|input]",
            "[PIN:1|Z limit|input]",
            "ok",
        ],
    );
    let map = handle.pin_map();
    assert_eq!(map.len(), 1);
    assert_eq!(map[0].id, 1);

    send_report(
        &handle,
        &control,
        ExtendedCommand::Pins,
        &["[PIN:1|Z limit|input]", "ok"],
    );
    let events = send_report(
        &handle,
        &control,
        ExtendedCommand::PinState,
        &["[PINSTATE:bad|0101|1010]", "ok"],
    );
    assert!(handle.pin_state().is_none());
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, EngineEvent::PinState(_)))
    );

    handle.shutdown();
}

#[test]
fn a_soft_reset_clears_live_reports_but_keeps_the_pin_map() {
    let (handle, control) = connected_engine();
    populate_reports(&handle, &control);
    assert_eq!(handle.pin_map().len(), 3);
    assert!(handle.pin_state().is_some());
    assert!(handle.limit_state().is_some());
    assert!(handle.limit_events().is_some());

    handle.send(Command::Reset).expect("soft reset");
    assert!(
        wait_until(
            || handle.pin_state().is_none()
                && handle.limit_state().is_none()
                && handle.limit_events().is_none(),
            Duration::from_secs(1)
        ),
        "a soft reset must clear the live pin reports"
    );
    assert_eq!(
        handle.pin_map().len(),
        3,
        "the static pin map survives a soft reset"
    );

    handle.shutdown();
}

#[test]
fn disconnect_clears_live_reports_and_a_new_session_clears_the_map() {
    let handle = Engine::spawn(config()).expect("spawn engine");
    let control = connect(&handle, standard_script);
    drain_events(&handle);
    populate_reports(&handle, &control);
    assert_eq!(handle.pin_map().len(), 3);
    assert!(handle.pin_state().is_some());

    handle.disconnect();
    assert!(
        wait_until(|| !handle.is_connected(), Duration::from_secs(2)),
        "engine did not disconnect"
    );
    assert!(
        handle.pin_state().is_none(),
        "live pin levels must not outlive the link"
    );
    assert_eq!(
        handle.pin_map().len(),
        3,
        "the static pin map is kept across a disconnect"
    );

    let _control = connect(&handle, standard_script);
    assert!(
        handle.pin_map().is_empty(),
        "a new session starts with no controller pin map"
    );
    assert!(handle.pin_state().is_none());
    assert!(handle.limit_state().is_none());
    assert!(handle.limit_events().is_none());

    handle.shutdown();
}
