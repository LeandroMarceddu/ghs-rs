//! End-to-end engine test over the in-process simulator.
//!
//! The unit/integration suite uses a scripted transport. This test proves the
//! real handshake and telemetry path against the simulator that ships with the
//! app's demo mode.

use std::thread;
use std::time::{Duration, Instant};

use ghs_demo::{SimProfile, start};
use ghs_engine::{
    Axes, Axis, Command, ConnectTarget, Engine, EngineConfig, EngineError, EngineEvent,
    EngineHandle, JobState, JogCommand, JogMode, Position, SystemCommand,
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

#[test]
fn connects_to_the_simulator_and_runs_mdi() {
    let (endpoint, _session) = start(SimProfile::FlexiHal).expect("start simulator");
    let engine = Engine::spawn(fast_config()).expect("spawn engine");

    engine
        .connect(ConnectTarget::Transport(Box::new(endpoint)))
        .expect("connect");

    let deadline = Instant::now() + Duration::from_secs(8);
    while !engine.is_ready() && Instant::now() < deadline {
        drain_events(&engine);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(engine.is_ready(), "engine did not become ready");

    let info = engine.controller_info().expect("controller info");
    assert_eq!(info.axes.to_string(), "XYZ");
    assert_eq!(info.rx_buffer_size, Some(1024));
    assert_eq!(info.planner_blocks, Some(64));
    assert!(
        !engine.setting_descriptions().is_empty(),
        "setting metadata must be published at the end of discovery"
    );
    assert!(
        !engine.setting_groups().is_empty(),
        "setting groups must be published at the end of discovery"
    );

    engine
        .send(Command::Mdi("G0 X1".into()))
        .expect("accept MDI");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut moved = false;
    while Instant::now() < deadline {
        drain_events(&engine);
        let snapshot = engine.snapshot();
        if (snapshot.wpos[Axis::X] - 1.0).abs() < 1.0e-6 {
            moved = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        moved,
        "simulator position did not update: {:?}",
        engine.snapshot().wpos
    );

    let axes = [Axis::X].into_iter().collect::<Axes>();
    let mut distance = Position::new(axes);
    distance.set(Axis::X, 1.0);
    let jog = JogCommand::new(axes, distance, 1000.0, JogMode::Incremental).expect("jog");
    engine.send(Command::Jog(jog)).expect("accept jog");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut jogged = false;
    while Instant::now() < deadline {
        drain_events(&engine);
        let snapshot = engine.snapshot();
        if (snapshot.wpos[Axis::X] - 2.0).abs() < 1.0e-6 {
            jogged = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        jogged,
        "jog did not move the simulator: {:?}",
        engine.snapshot().wpos
    );

    engine.disconnect();
    engine.shutdown();
}

#[test]
fn reads_writes_and_verifies_startup_lines_on_the_simulator() {
    let (engine, _session) = connect_simulator(fast_config());

    engine
        .send(Command::System(SystemCommand::StartupLines))
        .expect("read startup lines");
    assert!(
        wait_until(|| engine.startup_lines().len() == 2, Duration::from_secs(2)),
        "the simulator's startup lines were not collected: {:?}",
        engine.startup_lines()
    );
    assert_eq!(
        engine
            .startup_lines()
            .iter()
            .map(|line| line.index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );

    engine
        .send(Command::System(SystemCommand::WriteStartupLine {
            index: 0,
            line: "G21 G90".into(),
        }))
        .expect("write $N0");
    assert!(
        !engine
            .startup_lines()
            .iter()
            .any(|line| line.line == "G21 G90"),
        "a write must not update the store before the controller's read-back"
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match engine.send(Command::System(SystemCommand::ReadStartupLine(0))) {
            Ok(()) => break,
            Err(EngineError::Busy) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("read $N0: {error}"),
        }
    }
    assert!(
        wait_until(
            || engine
                .startup_lines()
                .iter()
                .any(|line| line.index == 0 && line.line == "G21 G90"),
            Duration::from_secs(2)
        ),
        "the startup-line read-back was not stored: {:?}",
        engine.startup_lines()
    );
    engine.shutdown();
}

fn drain_events(engine: &ghs_engine::EngineHandle) {
    while engine.events().try_recv().is_ok() {}
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

fn connect_simulator(config: EngineConfig) -> (EngineHandle, ghs_demo::SimSession) {
    let (endpoint, session) = start(SimProfile::FlexiHal).expect("start simulator");
    let engine = Engine::spawn(config).expect("spawn engine");
    engine
        .connect(ConnectTarget::Transport(Box::new(endpoint)))
        .expect("connect");
    let deadline = Instant::now() + Duration::from_secs(8);
    while !engine.is_ready() && Instant::now() < deadline {
        drain_events(&engine);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(engine.is_ready(), "engine did not become ready");
    (engine, session)
}

#[test]
fn runs_a_program_to_completion_on_the_simulator() {
    let (engine, _session) = connect_simulator(fast_config());
    engine
        .send(Command::LoadProgram {
            name: "demo.nc".into(),
            source: "G21 G90\nG0 X1 Y1\nG1 Z-1 F100\nM2\n".into(),
        })
        .expect("load program");
    let loaded = wait_for_event(&engine, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobLoaded { .. })
    });
    assert!(matches!(
        loaded,
        EngineEvent::JobLoaded { total_lines: 4, .. }
    ));
    engine
        .send(Command::StartProgram { from_line: None })
        .expect("start program");
    wait_for_event(&engine, Duration::from_secs(10), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });

    let snapshot = engine.snapshot();
    assert_eq!(snapshot.job.state, JobState::Completed);
    assert_eq!(snapshot.job.completed_lines, 4);
    assert_eq!(snapshot.job.progress, 1.0);
    engine.shutdown();
}

#[test]
fn completes_a_tool_change_on_the_simulator() {
    let (engine, _session) = connect_simulator(fast_config());
    engine
        .send(Command::LoadProgram {
            name: "tool.nc".into(),
            source: "T2 M6\nG0 X2\nM2\n".into(),
        })
        .expect("load program");
    engine
        .send(Command::StartProgram { from_line: None })
        .expect("start program");
    let tool = wait_for_event(&engine, Duration::from_secs(5), |event| {
        matches!(event, EngineEvent::ToolChangeRequired { .. })
    });
    assert_eq!(tool, EngineEvent::ToolChangeRequired { tool: 2, line: 1 });
    assert!(wait_until(
        || engine.snapshot().state == ghs_engine::MachineState::Tool,
        Duration::from_secs(2)
    ));
    engine
        .send(Command::AcknowledgeToolChange)
        .expect("acknowledge tool change");
    engine
        .send(Command::ResumeProgram)
        .expect("cycle start after the tool change");
    wait_for_event(&engine, Duration::from_secs(10), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    assert_eq!(engine.snapshot().job.state, JobState::Completed);
    engine.shutdown();
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
