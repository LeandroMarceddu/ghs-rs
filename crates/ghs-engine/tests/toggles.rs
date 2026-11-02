//! Optional-stop, single-block and block-delete toggle state.

mod support;

use std::time::{Duration, Instant};

use ghs_engine::{
    Command, ConnectTarget, Engine, EngineConfig, EngineError, EngineEvent, EngineHandle,
    JobPauseReason, LogLevel,
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

fn connected_engine() -> (EngineHandle, Control) {
    let handle = Engine::spawn(config()).expect("spawn engine");
    let control = connect(&handle, standard_script);
    (handle, control)
}

fn drain_events(handle: &EngineHandle) {
    while handle.events().try_recv().is_ok() {}
}

fn wait_for_ok(handle: &EngineHandle) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for the controller ok"
        );
        match handle.events().recv_timeout(remaining) {
            Ok(EngineEvent::Log {
                level: LogLevel::Ok,
                ..
            }) => return,
            Ok(_) => {}
            Err(error) => panic!("engine event channel closed while waiting for ok: {error}"),
        }
    }
}

fn start(handle: &EngineHandle) {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match handle.send(Command::StartProgram { from_line: None }) {
            Err(EngineError::Busy) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2));
            }
            result => {
                result.expect("start program");
                return;
            }
        }
    }
}

fn wait_event(
    handle: &EngineHandle,
    timeout: Duration,
    predicate: impl Fn(&EngineEvent) -> bool,
) -> EngineEvent {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for an engine event"
        );
        match handle.events().recv_timeout(remaining) {
            Ok(event) => {
                if predicate(&event) {
                    return event;
                }
            }
            Err(error) => panic!("engine event channel failure: {error}"),
        }
    }
}

#[test]
fn a_new_session_starts_with_all_toggles_off() {
    let (handle, _control) = connected_engine();
    assert!(!handle.optional_stop());
    assert!(!handle.single_block());
    assert!(!handle.block_delete());
    handle.shutdown();
}

#[test]
fn each_toggle_updates_only_its_own_accessor() {
    let (handle, control) = connected_engine();

    handle
        .send(Command::SetOptionalStop(true))
        .expect("optional stop on");
    assert!(handle.optional_stop());
    assert!(!handle.single_block());
    assert!(!handle.block_delete());

    handle
        .send(Command::SetSingleBlock(true))
        .expect("single block on");
    assert!(handle.optional_stop());
    assert!(handle.single_block());
    assert!(!handle.block_delete());

    drain_events(&handle);
    control.expect_line("$B", &["ok"]);
    handle
        .send(Command::SetBlockDelete(true))
        .expect("block delete on");
    assert!(handle.optional_stop());
    assert!(handle.single_block());
    assert!(handle.block_delete());

    handle
        .send(Command::SetOptionalStop(false))
        .expect("optional stop off");
    assert!(!handle.optional_stop());
    assert!(handle.single_block());
    assert!(handle.block_delete());

    handle
        .send(Command::SetSingleBlock(false))
        .expect("single block off");
    assert!(!handle.optional_stop());
    assert!(!handle.single_block());
    assert!(handle.block_delete());

    // The `$B` line may still be in flight; wait for its acknowledgement
    // before toggling block delete again.
    wait_for_ok(&handle);
    control.expect_line("$B", &["ok"]);
    handle
        .send(Command::SetBlockDelete(false))
        .expect("block delete off");
    assert!(!handle.block_delete());
    handle.shutdown();
}

#[test]
fn setting_the_same_value_twice_sends_no_duplicate_toggle() {
    let (handle, control) = connected_engine();
    control.clear_sent();

    handle
        .send(Command::SetOptionalStop(false))
        .expect("optional stop already off");
    handle
        .send(Command::SetOptionalStop(false))
        .expect("optional stop still off");
    assert!(!handle.optional_stop());
    assert!(!control.sent_bytes().contains(&0x88));

    handle
        .send(Command::SetOptionalStop(true))
        .expect("optional stop on");
    handle
        .send(Command::SetOptionalStop(true))
        .expect("optional stop still on");
    assert!(handle.optional_stop());
    assert_eq!(
        control
            .sent_bytes()
            .into_iter()
            .filter(|byte| *byte == 0x88)
            .count(),
        1,
        "the second optional-stop command must not toggle the controller"
    );

    handle
        .send(Command::SetSingleBlock(true))
        .expect("single block on");
    handle
        .send(Command::SetSingleBlock(true))
        .expect("single block still on");
    assert!(handle.single_block());
    assert_eq!(
        control
            .sent_bytes()
            .into_iter()
            .filter(|byte| *byte == 0x89)
            .count(),
        1,
        "the second single-block command must not toggle the controller"
    );

    drain_events(&handle);
    control.expect_line("$B", &["ok"]);
    handle
        .send(Command::SetBlockDelete(true))
        .expect("block delete on");
    handle
        .send(Command::SetBlockDelete(true))
        .expect("block delete still on");
    assert!(handle.block_delete());
    assert_eq!(
        control
            .sent_lines()
            .into_iter()
            .filter(|line| line == "$B")
            .count(),
        1,
        "the second block-delete command must not toggle the controller"
    );
    handle.shutdown();
}

#[test]
fn a_rejected_command_leaves_the_accessor_unchanged() {
    let handle = Engine::spawn(config()).expect("spawn engine");
    for command in [
        Command::SetOptionalStop(true),
        Command::SetSingleBlock(true),
        Command::SetBlockDelete(true),
    ] {
        let error = handle.send(command).expect_err("command before connect");
        assert!(
            matches!(error, EngineError::NotConnected),
            "unexpected error: {error:?}"
        );
    }
    assert!(!handle.optional_stop());
    assert!(!handle.single_block());
    assert!(!handle.block_delete());

    let control = connect(&handle, standard_script);
    let lines = ["G0 X1", "M0", "M2"];
    for line in lines {
        control.expect_line(line, &["ok"]);
    }
    handle
        .send(Command::LoadProgram {
            name: "toggles.nc".into(),
            source: lines.join("\n"),
        })
        .expect("load program");
    start(&handle);
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(
            event,
            EngineEvent::JobPaused {
                reason: JobPauseReason::ProgramStop,
                ..
            }
        )
    });

    let error = handle
        .send(Command::SetBlockDelete(true))
        .expect_err("block delete while the job is paused");
    assert!(
        matches!(error, EngineError::JobState { .. }),
        "unexpected error: {error:?}"
    );
    assert!(!handle.block_delete());
    handle.shutdown();
}

#[test]
fn a_soft_reset_clears_the_toggles_and_the_job_flags() {
    let (handle, control) = connected_engine();
    drain_events(&handle);
    control.expect_line("$B", &["ok"]);
    control.expect_line("$B", &["ok"]);

    handle
        .send(Command::SetOptionalStop(true))
        .expect("optional stop on");
    handle
        .send(Command::SetSingleBlock(true))
        .expect("single block on");
    handle
        .send(Command::SetBlockDelete(true))
        .expect("block delete on");
    assert!(handle.optional_stop());
    assert!(handle.single_block());
    assert!(handle.block_delete());

    handle.send(Command::Reset).expect("soft reset");
    assert!(!handle.optional_stop());
    assert!(!handle.single_block());
    assert!(!handle.block_delete());

    // A toggle after the reset must be sent again: the job flags were cleared
    // too, so this is not treated as a same-value duplicate.
    handle
        .send(Command::SetOptionalStop(true))
        .expect("optional stop after reset");
    handle
        .send(Command::SetSingleBlock(true))
        .expect("single block after reset");
    handle
        .send(Command::SetBlockDelete(true))
        .expect("block delete after reset");
    assert!(handle.optional_stop());
    assert!(handle.single_block());
    assert!(handle.block_delete());
    assert_eq!(
        control
            .sent_bytes()
            .into_iter()
            .filter(|byte| *byte == 0x88)
            .count(),
        2
    );
    assert_eq!(
        control
            .sent_bytes()
            .into_iter()
            .filter(|byte| *byte == 0x89)
            .count(),
        2
    );
    assert_eq!(
        control
            .sent_lines()
            .into_iter()
            .filter(|line| line == "$B")
            .count(),
        2
    );
    handle.shutdown();
}

#[test]
fn disconnect_and_reconnect_clear_the_toggles() {
    let handle = Engine::spawn(config()).expect("spawn engine");
    let control = connect(&handle, standard_script);
    drain_events(&handle);
    control.expect_line("$B", &["ok"]);

    handle
        .send(Command::SetOptionalStop(true))
        .expect("optional stop on");
    handle
        .send(Command::SetSingleBlock(true))
        .expect("single block on");
    handle
        .send(Command::SetBlockDelete(true))
        .expect("block delete on");
    assert!(handle.optional_stop());
    assert!(handle.single_block());
    assert!(handle.block_delete());

    handle.disconnect();
    assert!(
        wait_until(|| !handle.is_connected(), Duration::from_secs(2)),
        "engine did not disconnect"
    );
    assert!(!handle.optional_stop());
    assert!(!handle.single_block());
    assert!(!handle.block_delete());

    let _control = connect(&handle, standard_script);
    assert!(
        !handle.optional_stop(),
        "a new session starts with the toggles off"
    );
    assert!(!handle.single_block());
    assert!(!handle.block_delete());
    handle.shutdown();
}
