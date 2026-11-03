//! End-to-end check-mode (`$C`) tests over the in-process simulator.
//!
//! The scripted suite proves `$C` ordering with a byte-exact controller; this
//! suite proves the same flow against the simulator that ships with demo mode,
//! including that the controller really suppresses motion and returns to
//! `Idle`.

use std::thread;
use std::time::{Duration, Instant};

use ghs_demo::{SimProfile, start};
use ghs_engine::{
    Axis, Command, ConnectTarget, Engine, EngineConfig, EngineError, EngineEvent, EngineHandle,
    JobAbortReason, JobState, MachineState, RealtimeCommand,
};

fn fast_config() -> EngineConfig {
    EngineConfig {
        handshake_listen: Duration::from_millis(50),
        handshake_response: Duration::from_millis(500),
        discovery_timeout: Duration::from_millis(2_000),
        status_poll_active: Duration::from_millis(20),
        status_poll_idle: Duration::from_millis(20),
        full_status_interval: Duration::from_millis(500),
        watchdog_pause: Duration::from_millis(1_000),
        watchdog_disconnect: Duration::from_millis(2_000),
        tick_interval: Duration::from_millis(5),
        command_timeout: Duration::from_millis(2_000),
        ..EngineConfig::default()
    }
}

fn connect_simulator() -> (EngineHandle, ghs_demo::SimSession) {
    let (endpoint, session) = start(SimProfile::FlexiHal).expect("start simulator");
    let engine = Engine::spawn(fast_config()).expect("spawn engine");
    engine
        .connect(ConnectTarget::Transport(Box::new(endpoint)))
        .expect("connect");
    assert!(
        wait_until(|| engine.is_ready(), Duration::from_secs(8)),
        "engine did not become ready"
    );
    (engine, session)
}

fn wait_for_event(
    engine: &EngineHandle,
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
        match engine.events().recv_timeout(remaining) {
            Ok(event) => {
                if predicate(&event) {
                    return event;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                panic!("timed out waiting for an engine event");
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                panic!("engine event channel disconnected");
            }
        }
    }
}

fn wait_until(mut predicate: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if predicate() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn load(engine: &EngineHandle, source: &str) {
    engine
        .send(Command::LoadProgram {
            name: "check.nc".into(),
            source: source.into(),
        })
        .expect("load program");
}

fn long_program(lines: usize) -> String {
    let mut source = String::from("G21 G90\n");
    for index in 0..lines {
        source.push_str(&format!("G1 X{} F100\n", index % 10));
    }
    source.push_str("M2\n");
    source
}

fn check_mode_off(engine: &EngineHandle) -> bool {
    engine
        .last_status()
        .is_none_or(|status| status.check_mode != Some(true))
}

/// Wait until the controller reports `Idle` with the `$C` flag clear.
fn wait_for_idle(engine: &EngineHandle) {
    assert!(
        wait_until(
            || engine.snapshot().state == MachineState::Idle && check_mode_off(engine),
            Duration::from_secs(5)
        ),
        "controller did not return to Idle without check mode: state {:?}, status {:?}",
        engine.snapshot().state,
        engine
            .last_status()
            .map(|status| (status.state, status.check_mode))
    );
}

#[test]
fn check_run_completes_without_motion_and_returns_to_idle() {
    let (engine, _session) = connect_simulator();
    let initial = engine.snapshot().wpos;

    load(&engine, "G21 G90\nG0 X10 Y5\nG1 Z-2 F100\nG1 X20\nM2\n");
    engine
        .send(Command::StartCheckRun)
        .expect("start check run");
    let started = wait_for_event(&engine, Duration::from_secs(5), |event| {
        matches!(event, EngineEvent::CheckStarted { .. })
    });
    assert_eq!(started, EngineEvent::CheckStarted { total_lines: 5 });
    assert!(engine.check_run(), "the run must be labelled as a dry run");

    wait_for_event(&engine, Duration::from_secs(10), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    wait_for_event(&engine, Duration::from_secs(10), |event| {
        matches!(event, EngineEvent::CheckCompleted { .. })
    });
    wait_for_idle(&engine);

    let after = engine.snapshot().wpos;
    assert!(
        (after[Axis::X] - initial[Axis::X]).abs() < 1.0e-6
            && (after[Axis::Y] - initial[Axis::Y]).abs() < 1.0e-6
            && (after[Axis::Z] - initial[Axis::Z]).abs() < 1.0e-6,
        "check mode moved an axis: before {initial:?}, after {after:?}"
    );
    assert_eq!(engine.snapshot().job.state, JobState::Completed);
    assert!(engine.check_run(), "the finished dry run stays labelled");

    // A normal run on the same session must really move; this proves the
    // engine restored Idle with `$C` clear instead of leaving the controller
    // in check mode.
    load(&engine, "G21 G90\nG0 X3\n");
    assert!(
        !engine.check_run(),
        "a new program clears the dry-run label"
    );
    engine
        .send(Command::StartProgram { from_line: None })
        .expect("start a normal program");
    wait_for_event(&engine, Duration::from_secs(10), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    assert!(
        wait_until(
            || (engine.snapshot().wpos[Axis::X] - 3.0).abs() < 1.0e-6,
            Duration::from_secs(5)
        ),
        "a normal run after a dry run did not move: {:?}",
        engine.snapshot().wpos
    );
    engine.shutdown();
}

#[test]
fn normal_start_still_moves() {
    let (engine, _session) = connect_simulator();
    load(&engine, "G21 G90\nG0 X4\n");
    assert!(!engine.check_run());
    engine
        .send(Command::StartProgram { from_line: None })
        .expect("start program");
    wait_for_event(&engine, Duration::from_secs(10), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    assert!(
        wait_until(
            || (engine.snapshot().wpos[Axis::X] - 4.0).abs() < 1.0e-6,
            Duration::from_secs(5)
        ),
        "normal StartProgram did not move: {:?}",
        engine.snapshot().wpos
    );
    assert!(!engine.check_run());
    engine.shutdown();
}

#[test]
fn stopping_a_check_run_returns_the_simulator_to_idle() {
    let (engine, _session) = connect_simulator();
    let initial = engine.snapshot().wpos;
    load(&engine, &long_program(20_000));

    engine
        .send(Command::StartCheckRun)
        .expect("start check run");
    wait_for_event(&engine, Duration::from_secs(5), |event| {
        matches!(event, EngineEvent::CheckStarted { .. })
    });
    engine
        .send(Command::PauseProgram)
        .expect("pause the dry run");
    wait_for_event(&engine, Duration::from_secs(5), |event| {
        matches!(event, EngineEvent::JobPaused { .. })
    });
    engine
        .send(Command::StopProgram { reset: false })
        .expect("stop the dry run");
    let aborted = wait_for_event(&engine, Duration::from_secs(5), |event| {
        matches!(event, EngineEvent::JobAborted { .. })
    });
    assert!(matches!(
        aborted,
        EngineEvent::JobAborted {
            reason: JobAbortReason::OperatorStop,
            ..
        }
    ));
    wait_for_idle(&engine);

    assert_eq!(engine.snapshot().job.state, JobState::Aborted);
    assert!(engine.check_run(), "a stopped dry run stays labelled");
    let after = engine.snapshot().wpos;
    assert!(
        (after[Axis::X] - initial[Axis::X]).abs() < 1.0e-6,
        "stop during check moved an axis: before {initial:?}, after {after:?}"
    );

    // Recoverable: the next MDI line must move.
    engine
        .send(Command::Mdi("G0 X2".into()))
        .expect("MDI after a stopped dry run");
    assert!(
        wait_until(
            || (engine.snapshot().wpos[Axis::X] - 2.0).abs() < 1.0e-6,
            Duration::from_secs(5)
        ),
        "the simulator did not move after a stopped dry run: {:?}",
        engine.snapshot().wpos
    );
    engine.shutdown();
}

#[test]
fn alarm_during_check_never_resumes_and_is_recoverable() {
    let (engine, _session) = connect_simulator();
    let initial = engine.snapshot().wpos;
    load(&engine, &long_program(20_000));

    engine
        .send(Command::StartCheckRun)
        .expect("start check run");
    wait_for_event(&engine, Duration::from_secs(5), |event| {
        matches!(event, EngineEvent::CheckStarted { .. })
    });
    engine
        .send(Command::PauseProgram)
        .expect("pause the dry run");
    wait_for_event(&engine, Duration::from_secs(5), |event| {
        matches!(event, EngineEvent::JobPaused { .. })
    });

    engine
        .send(Command::Realtime(RealtimeCommand::SoftEStop))
        .expect("soft E-stop during the dry run");
    let aborted = wait_for_event(&engine, Duration::from_secs(5), |event| {
        matches!(event, EngineEvent::JobAborted { .. })
    });
    assert!(matches!(
        aborted,
        EngineEvent::JobAborted {
            reason: JobAbortReason::EmergencyStop,
            ..
        }
    ));
    assert!(
        matches!(
            engine.snapshot().job.state,
            JobState::Aborted | JobState::Interrupted
        ),
        "unexpected job state: {:?}",
        engine.snapshot().job.state
    );
    assert_eq!(engine.snapshot().state, MachineState::Alarm);
    let after = engine.snapshot().wpos;
    assert!(
        (after[Axis::X] - initial[Axis::X]).abs() < 1.0e-6,
        "the E-stop path moved an axis: before {initial:?}, after {after:?}"
    );

    // Unlocking is the operator's explicit recovery. The engine must then
    // clear the check flag it entered; nothing resumes the program.
    engine
        .send(Command::Unlock)
        .expect("unlock after the E-stop");
    wait_for_idle(&engine);
    assert!(
        matches!(
            engine.snapshot().job.state,
            JobState::Aborted | JobState::Interrupted
        ),
        "unexpected job state after recovery: {:?}",
        engine.snapshot().job.state
    );
    assert!(engine.check_run(), "the interrupted run is still a dry run");

    engine
        .send(Command::Mdi("G0 X2".into()))
        .expect("MDI after the alarm recovered");
    assert!(
        wait_until(
            || (engine.snapshot().wpos[Axis::X] - 2.0).abs() < 1.0e-6,
            Duration::from_secs(5)
        ),
        "the simulator did not move after alarm recovery: {:?}",
        engine.snapshot().wpos
    );
    engine.shutdown();
}

#[test]
fn check_run_is_refused_without_a_program_or_in_alarm() {
    let (engine, _session) = connect_simulator();
    let error = engine
        .send(Command::StartCheckRun)
        .expect_err("no program is loaded");
    assert!(matches!(error, EngineError::NoProgram), "{error:?}");

    load(&engine, "G0 X1\nM2\n");
    engine
        .send(Command::Realtime(RealtimeCommand::SoftEStop))
        .expect("soft E-stop");
    assert!(wait_until(
        || engine.snapshot().state == MachineState::Alarm,
        Duration::from_secs(2)
    ));
    let error = engine
        .send(Command::StartCheckRun)
        .expect_err("alarm blocks a check run");
    assert!(
        matches!(
            error,
            EngineError::MotionBlocked {
                state: MachineState::Alarm
            }
        ),
        "{error:?}"
    );
    engine.shutdown();
}

#[test]
fn check_run_is_refused_without_a_link() {
    let engine = Engine::spawn(fast_config()).expect("spawn engine");
    let error = engine
        .send(Command::StartCheckRun)
        .expect_err("no link is connected");
    assert!(matches!(error, EngineError::NotConnected), "{error:?}");
    engine.shutdown();
}
