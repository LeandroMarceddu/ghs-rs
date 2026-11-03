//! End-to-end job-runner tests over the scripted controller.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::RecvTimeoutError;
use ghs_engine::{
    Command, ConnectTarget, Engine, EngineConfig, EngineError, EngineEvent, EngineHandle,
    JobAbortReason, JobPauseReason, JobState, JogCommand, MachineSnapshot, MachineState, Position,
    RealtimeCommand, SystemCommand,
};
use support::{
    Control, IDLE_STATUS, ScriptedTransport, standard_script, standard_script_with_opt,
    standard_script_with_options, wait_until,
};

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

fn connected_engine(script: impl FnOnce(&Control)) -> (EngineHandle, Control) {
    connected_engine_with(config(), script)
}

fn connected_engine_with(
    config: EngineConfig,
    script: impl FnOnce(&Control),
) -> (EngineHandle, Control) {
    let (transport, control) = ScriptedTransport::new();
    script(&control);
    let handle = Engine::spawn(config).expect("spawn engine");
    handle
        .connect(ConnectTarget::Transport(Box::new(transport)))
        .expect("connect transport");
    assert!(
        wait_until(|| handle.is_ready(), Duration::from_secs(2)),
        "engine did not become ready"
    );
    (handle, control)
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

fn wait_for_snapshot(
    handle: &EngineHandle,
    predicate: impl Fn(&MachineSnapshot) -> bool,
) -> Arc<MachineSnapshot> {
    assert!(
        wait_until(|| predicate(&handle.snapshot()), Duration::from_secs(2)),
        "snapshot condition was not met: {:?}",
        handle.snapshot().job
    );
    handle.snapshot()
}

fn queue_program(control: &Control, lines: &[&str]) {
    for line in lines {
        control.expect_line(line, &["ok"]);
    }
}

fn sent_program_lines(control: &Control, lines: &[&str]) -> Vec<String> {
    control
        .sent_lines()
        .into_iter()
        .filter(|sent| lines.contains(&sent.as_str()))
        .collect()
}

fn assert_program_order(control: &Control, lines: &[&str]) {
    let expected: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();
    assert_eq!(sent_program_lines(control, lines), expected);
}

fn load(handle: &EngineHandle, source: &str) {
    handle
        .send(Command::LoadProgram {
            name: "test.nc".into(),
            source: source.into(),
        })
        .expect("load program");
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

#[test]
fn load_rejects_invalid_programs_and_accepts_valid_ones() {
    let (handle, _control) = connected_engine(standard_script);

    let error = handle
        .send(Command::LoadProgram {
            name: "bad.nc".into(),
            source: "G1 X\n".into(),
        })
        .expect_err("invalid program");
    assert!(
        matches!(error, EngineError::InvalidProgram(_)),
        "unexpected error: {error:?}"
    );
    assert!(
        !handle
            .snapshot()
            .job
            .name
            .as_deref()
            .is_some_and(|n| n == "bad.nc")
    );

    load(&handle, "G21 G90\nG0 X0 Y0 Z5\nG1 Z-1 F100\nG1 X10\nM2\n");
    let loaded = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::JobLoaded { .. })
    });
    match loaded {
        EngineEvent::JobLoaded {
            name,
            total_lines,
            program,
        } => {
            assert_eq!(name, "test.nc");
            assert_eq!(total_lines, 5);
            assert_eq!(program.lines.len(), 5);
        }
        other => panic!("expected JobLoaded, got {other:?}"),
    }
    let snapshot = handle.snapshot();
    assert_eq!(snapshot.job.name.as_deref(), Some("test.nc"));
    assert_eq!(snapshot.job.total_lines, 5);
    assert_eq!(snapshot.job.state, JobState::Idle);
    handle.shutdown();
}

#[test]
fn streams_a_program_to_completion_in_order() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G21 G90", "G0 X0 Y0 Z5", "G1 Z-1 F100", "G1 X10", "M2"];
    queue_program(&control, &lines);
    load(&handle, &lines.join("\n"));
    let _ = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::JobLoaded { .. })
    });
    start(&handle);

    let started = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::JobStarted { .. })
    });
    assert_eq!(
        started,
        EngineEvent::JobStarted {
            from_line: 1,
            total_lines: 5
        }
    );
    let completed = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    assert!(matches!(completed, EngineEvent::JobCompleted { .. }));

    let snapshot = wait_for_snapshot(&handle, |snapshot| {
        snapshot.job.state == JobState::Completed
    });
    assert_eq!(snapshot.job.completed_lines, 5);
    assert_eq!(snapshot.job.total_lines, 5);
    assert_eq!(snapshot.job.progress, 1.0);
    assert_program_order(&control, &lines);
    assert!(
        control.unexpected().is_empty(),
        "{:?}",
        control.unexpected()
    );

    control.expect_line("G0 X9", &["ok"]);
    handle
        .send(Command::Mdi("G0 X9".into()))
        .expect("MDI after a completed job");
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|line| line == "G0 X9"),
            Duration::from_secs(1)
        ),
        "a completed job must not leave the streamer paused"
    );
    handle.shutdown();
}

#[test]
fn pause_and_resume_never_duplicate_or_skip_a_line() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "G0 X3", "G0 X4", "G0 X5", "M2"];
    control.expect_line(lines[0], &[]);
    queue_program(&control, &lines[1..]);
    load(&handle, &lines.join("\n"));
    start(&handle);
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|line| line == lines[0]),
            Duration::from_secs(1)
        ),
        "the first line was not sent"
    );

    handle.send(Command::PauseProgram).expect("pause");
    control.push_line("ok");
    let paused = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::JobPaused { .. })
    });
    assert!(matches!(
        paused,
        EngineEvent::JobPaused {
            reason: JobPauseReason::Operator,
            ..
        }
    ));
    std::thread::sleep(Duration::from_millis(40));
    let before = sent_program_lines(&control, &lines);
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(
        sent_program_lines(&control, &lines),
        before,
        "a paused job must not feed more lines"
    );

    handle.send(Command::ResumeProgram).expect("resume");
    wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::JobResumed)
    });
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    assert_program_order(&control, &lines);
    assert!(control.sent_bytes().contains(&0x82), "pause must feed hold");
    assert!(
        control.sent_bytes().contains(&0x81),
        "resume must cycle start"
    );
    handle.shutdown();
}

#[test]
fn stop_retains_the_program_and_never_resumes() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "G0 X3", "G0 X4", "G0 X5", "G0 X6", "M2"];
    control.expect_line(lines[0], &[]);
    queue_program(&control, &lines[1..]);
    load(&handle, &lines.join("\n"));
    start(&handle);
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|line| line == lines[0]),
            Duration::from_secs(1)
        ),
        "the first line was not sent"
    );

    handle
        .send(Command::StopProgram { reset: false })
        .expect("stop");
    let aborted = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::JobAborted { .. })
    });
    assert!(matches!(
        aborted,
        EngineEvent::JobAborted {
            reason: JobAbortReason::OperatorStop,
            ..
        }
    ));
    std::thread::sleep(Duration::from_millis(80));
    let snapshot = handle.snapshot();
    assert_eq!(snapshot.job.state, JobState::Aborted);
    assert!(snapshot.job.name.is_some(), "the program stays loaded");
    assert!(
        !control.sent_bytes().contains(&0x81),
        "stop must never send cycle start"
    );
    let after_stop = sent_program_lines(&control, &lines);
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(sent_program_lines(&control, &lines), after_stop);
    handle.shutdown();
}

#[test]
fn stop_with_reset_sends_a_soft_reset() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "M2"];
    control.expect_line(lines[0], &[]);
    queue_program(&control, &lines[1..]);
    load(&handle, &lines.join("\n"));
    start(&handle);
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|line| line == lines[0]),
            Duration::from_secs(1)
        ),
        "the first line was not sent"
    );

    handle
        .send(Command::StopProgram { reset: true })
        .expect("stop and reset");
    wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::JobAborted { .. })
    });
    assert!(
        wait_until(
            || control.sent_bytes().contains(&0x18),
            Duration::from_secs(1)
        ),
        "reset stop must send 0x18"
    );
    assert!(!control.sent_bytes().contains(&0x81));
    handle.shutdown();
}

#[test]
fn controller_error_stops_the_job_and_clears_the_sticky_error() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 Z5", "G0 X2", "M2"];
    control.expect_line("G0 X1", &["ok"]);
    control.expect_line("G0 Z5", &["error:20"]);
    control.expect_line("", &["ok"]);
    load(&handle, &lines.join("\n"));
    start(&handle);

    let error = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobError { .. })
    });
    match error {
        EngineEvent::JobError {
            line,
            code,
            message,
        } => {
            assert_eq!(line, 2);
            assert_eq!(code, 20);
            assert!(message.contains("error 20"), "{message}");
        }
        other => panic!("expected JobError, got {other:?}"),
    }
    assert!(
        wait_until(
            || control.sent_lines().iter().any(String::is_empty),
            Duration::from_secs(1)
        ),
        "the engine must clear the sticky error with a blank line"
    );
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        !control.sent_lines().iter().any(|line| line == "G0 X2"),
        "no program line may follow an error"
    );
    assert_eq!(handle.snapshot().job.state, JobState::Aborted);
    assert!(!control.sent_bytes().contains(&0x81));
    handle.shutdown();
}

#[test]
fn m0_pauses_and_resume_continues() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "M0", "G0 X2", "M2"];
    queue_program(&control, &lines);
    load(&handle, &lines.join("\n"));
    start(&handle);

    let paused = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobPaused { .. })
    });
    assert!(matches!(
        paused,
        EngineEvent::JobPaused {
            reason: JobPauseReason::ProgramStop,
            ..
        }
    ));
    assert!(!control.sent_lines().iter().any(|line| line == "G0 X2"));

    handle
        .send(Command::ResumeProgram)
        .expect("resume after M0");
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    assert_program_order(&control, &lines);
    handle.shutdown();
}

#[test]
fn optional_stop_pauses_only_when_enabled() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "M1", "G0 X2", "M2"];
    queue_program(&control, &lines);
    load(&handle, &lines.join("\n"));

    handle
        .send(Command::SetOptionalStop(true))
        .expect("optional stop on");
    assert!(handle.optional_stop());
    assert!(control.sent_bytes().contains(&0x88));
    start(&handle);
    let paused = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobPaused { .. })
    });
    assert!(matches!(
        paused,
        EngineEvent::JobPaused {
            reason: JobPauseReason::OptionalStop,
            ..
        }
    ));
    handle.send(Command::ResumeProgram).expect("resume");
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    handle.shutdown();
}

#[test]
fn single_block_pauses_after_each_acknowledged_line() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "M2"];
    queue_program(&control, &lines);
    load(&handle, &lines.join("\n"));
    handle
        .send(Command::SetSingleBlock(true))
        .expect("single block on");
    assert!(handle.single_block());
    assert!(control.sent_bytes().contains(&0x89));
    start(&handle);

    for expected in 1..=2 {
        let paused = wait_event(&handle, Duration::from_secs(2), |event| {
            matches!(event, EngineEvent::JobPaused { .. })
        });
        assert!(matches!(
            paused,
            EngineEvent::JobPaused {
                reason: JobPauseReason::SingleBlock,
                ..
            }
        ));
        assert_eq!(handle.snapshot().job.completed_lines, expected);
        handle.send(Command::ResumeProgram).expect("resume");
    }
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    assert_program_order(&control, &lines);
    handle.shutdown();
}

#[test]
fn breakpoint_pauses_before_the_marked_line() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "G0 X3", "M2"];
    queue_program(&control, &lines);
    load(&handle, &lines.join("\n"));
    handle
        .send(Command::SetBreakpoint {
            line: 2,
            enabled: true,
        })
        .expect("breakpoint");
    start(&handle);

    let paused = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobPaused { .. })
    });
    assert!(matches!(
        paused,
        EngineEvent::JobPaused {
            reason: JobPauseReason::Breakpoint,
            line: 2
        }
    ));
    assert!(!control.sent_lines().iter().any(|line| line == "G0 X2"));
    handle.send(Command::ResumeProgram).expect("resume");
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    assert_program_order(&control, &lines);
    handle.shutdown();
}

#[test]
fn block_delete_skips_marked_lines() {
    let (handle, control) = connected_engine(standard_script);
    control.expect_line("$B", &["ok"]);
    let lines = ["/G0 X1", "G0 X2", "M2"];
    control.expect_line("G0 X2", &["ok"]);
    control.expect_line("M2", &["ok"]);
    load(&handle, &lines.join("\n"));
    handle
        .send(Command::SetBlockDelete(true))
        .expect("block delete on");
    assert!(handle.block_delete());
    start(&handle);
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    assert!(!control.sent_lines().iter().any(|line| line == "/G0 X1"));
    assert!(control.sent_lines().iter().any(|line| line == "G0 X2"));
    assert_eq!(handle.snapshot().job.completed_lines, 3);
    handle.shutdown();
}

#[test]
fn tool_change_waits_for_ack_and_cycle_start() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "T3 M6", "G0 X2", "M2"];
    let jog_line = "$J=G21 G91 X0.000 Y0.000 Z1.000 F1000.000";
    control.expect_line(lines[0], &["ok"]);
    control.expect_line(lines[1], &["ok"]);
    control.expect_line(jog_line, &["ok"]);
    control.expect_line(lines[2], &["ok"]);
    control.expect_line(lines[3], &["ok"]);
    load(&handle, &lines.join("\n"));
    start(&handle);

    let tool = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::ToolChangeRequired { .. })
    });
    assert_eq!(tool, EngineEvent::ToolChangeRequired { tool: 3, line: 2 });
    assert!(!control.sent_lines().iter().any(|line| line == "G0 X2"));
    assert!(handle.snapshot().job.paused);

    control.set_status("<Tool|MPos:0.000,0.000,0.000|T:3>");
    assert!(wait_until(
        || handle.snapshot().state == MachineState::Tool,
        Duration::from_secs(1)
    ));
    handle
        .send(Command::AcknowledgeToolChange)
        .expect("acknowledge");
    assert!(
        wait_until(
            || control.sent_bytes().contains(&0xA3),
            Duration::from_secs(1)
        ),
        "tool change acknowledgement must send 0xA3"
    );
    assert!(!control.sent_lines().iter().any(|line| line == "G0 X2"));

    let jog = JogCommand::incremental(Position::xyz(0.0, 0.0, 1.0), 1000.0).expect("jog");
    handle
        .send(Command::Jog(jog))
        .expect("jog during tool change");
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|line| line == jog_line),
            Duration::from_secs(1)
        ),
        "jog must be allowed while the tool change is pending"
    );

    control.set_status("<Idle|MPos:0.000,0.000,0.000|T:3>");
    assert!(wait_until(
        || handle.snapshot().state == MachineState::Idle,
        Duration::from_secs(1)
    ));
    handle.send(Command::ResumeProgram).expect("resume");
    assert!(control.sent_bytes().contains(&0x81));
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    assert_program_order(&control, &lines);
    handle.shutdown();
}

#[test]
fn alarm_interrupts_the_job_without_resuming() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "G0 X3", "M2"];
    control.expect_line(lines[0], &[]);
    queue_program(&control, &lines[1..]);
    load(&handle, &lines.join("\n"));
    start(&handle);
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|line| line == lines[0]),
            Duration::from_secs(1)
        ),
        "the first line was not sent"
    );

    control.set_status("<Alarm:1|MPos:0.000,0.000,0.000>");
    let aborted = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobAborted { .. })
    });
    assert!(matches!(
        aborted,
        EngineEvent::JobAborted {
            reason: JobAbortReason::Alarm,
            ..
        }
    ));
    std::thread::sleep(Duration::from_millis(100));
    assert!(!control.sent_bytes().contains(&0x81));
    assert_eq!(handle.snapshot().job.state, JobState::Interrupted);
    assert!(!control.sent_lines().iter().any(|line| line == "G0 X3"));
    handle.shutdown();
}

#[test]
fn door_interrupts_the_job_without_resuming() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "M2"];
    control.expect_line(lines[0], &[]);
    queue_program(&control, &lines[1..]);
    load(&handle, &lines.join("\n"));
    start(&handle);
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|line| line == lines[0]),
            Duration::from_secs(1)
        ),
        "the first line was not sent"
    );

    control.set_status("<Door:1|MPos:0.000,0.000,0.000>");
    let aborted = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobAborted { .. })
    });
    assert!(matches!(
        aborted,
        EngineEvent::JobAborted {
            reason: JobAbortReason::Door,
            ..
        }
    ));
    std::thread::sleep(Duration::from_millis(100));
    assert!(!control.sent_bytes().contains(&0x81));
    assert!(!control.sent_lines().iter().any(|line| line == "G0 X2"));
    handle.shutdown();
}

#[test]
fn disconnect_interrupts_the_job_and_retains_the_program() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "M2"];
    control.expect_line(lines[0], &[]);
    queue_program(&control, &lines[1..]);
    load(&handle, &lines.join("\n"));
    start(&handle);
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|line| line == lines[0]),
            Duration::from_secs(1)
        ),
        "the first line was not sent"
    );

    handle.disconnect();
    let aborted = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobAborted { .. })
    });
    assert!(matches!(
        aborted,
        EngineEvent::JobAborted {
            reason: JobAbortReason::Disconnected,
            ..
        }
    ));
    let snapshot = handle.snapshot();
    assert_eq!(snapshot.job.state, JobState::Interrupted);
    assert_eq!(snapshot.job.name.as_deref(), Some("test.nc"));
    assert!(snapshot.job.line.is_some_and(|line| line >= 1));
    assert!(!control.sent_bytes().contains(&0x81));
    handle.shutdown();
}

#[test]
fn watchdog_pauses_then_interrupts_the_job_without_resuming() {
    let (handle, control) = connected_engine_with(
        EngineConfig {
            watchdog_pause: Duration::from_millis(60),
            watchdog_disconnect: Duration::from_millis(200),
            ..config()
        },
        standard_script,
    );
    let lines = ["G0 X1", "G0 X2", "M2"];
    control.expect_line(lines[0], &[]);
    queue_program(&control, &lines[1..]);
    load(&handle, &lines.join("\n"));
    start(&handle);
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|line| line == lines[0]),
            Duration::from_secs(1)
        ),
        "the first line was not sent"
    );

    control.set_respond_status(false);
    let paused = wait_event(&handle, Duration::from_secs(1), |event| {
        matches!(event, EngineEvent::JobPaused { .. })
    });
    assert!(matches!(
        paused,
        EngineEvent::JobPaused {
            reason: JobPauseReason::Watchdog,
            ..
        }
    ));
    let aborted = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobAborted { .. })
    });
    assert!(matches!(
        aborted,
        EngineEvent::JobAborted {
            reason: JobAbortReason::Disconnected,
            ..
        }
    ));
    assert_eq!(handle.snapshot().job.state, JobState::Interrupted);
    assert!(!control.sent_bytes().contains(&0x81));
    handle.shutdown();
}

#[test]
fn m1_without_optional_stop_does_not_pause() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "M1", "G0 X2", "M2"];
    queue_program(&control, &lines);
    load(&handle, &lines.join("\n"));
    start(&handle);
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    assert_eq!(handle.snapshot().job.state, JobState::Completed);
    assert_program_order(&control, &lines);
    handle.shutdown();
}

#[test]
fn job_streaming_never_exceeds_the_reported_rx_buffer() {
    let (handle, control) = connected_engine_with(config(), |control| {
        standard_script_with_opt(control, "[OPT:V,64,64,3,0]");
    });
    let line = "G1 X1.000 F100";
    let mut source = String::new();
    for _ in 0..20 {
        source.push_str(line);
        source.push('\n');
    }
    source.push_str("M2\n");
    for _ in 0..20 {
        control.expect_line(line, &["ok"]);
    }
    control.expect_line("M2", &["ok"]);
    load(&handle, &source);
    start(&handle);
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });

    for chunk in control.sent() {
        if chunk.contains(&b'\n') {
            assert!(
                chunk.len() <= 64,
                "wrote {} bytes into a 64-byte receive buffer: {chunk:?}",
                chunk.len()
            );
        }
    }
    assert_eq!(
        control
            .sent_lines()
            .iter()
            .filter(|sent| sent.as_str() == line)
            .count(),
        20
    );
    handle.shutdown();
}

#[test]
fn start_from_line_is_explicitly_deferred() {
    let (handle, _control) = connected_engine(standard_script);
    load(&handle, "G0 X1\nG0 X2\nM2\n");
    let error = handle
        .send(Command::StartProgram { from_line: Some(2) })
        .expect_err("start-from-line is deferred");
    assert!(
        matches!(error, EngineError::Unsupported(_)),
        "unexpected error: {error:?}"
    );
    assert_eq!(handle.snapshot().job.state, JobState::Idle);
    handle.shutdown();
}

#[test]
fn manual_realtime_feed_hold_and_cycle_start_drive_the_job() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "M2"];
    control.expect_line(lines[0], &[]);
    queue_program(&control, &lines[1..]);
    load(&handle, &lines.join("\n"));
    start(&handle);
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|line| line == lines[0]),
            Duration::from_secs(1)
        ),
        "the first line was not sent"
    );

    handle
        .send(Command::Realtime(RealtimeCommand::FeedHold))
        .expect("raw feed hold");
    assert!(handle.snapshot().job.paused);
    control.push_line("ok");
    handle
        .send(Command::Realtime(RealtimeCommand::CycleStart))
        .expect("raw cycle start");
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    assert_program_order(&control, &lines);
    handle.shutdown();
}

/// Status used while the scripted controller is in check mode.
const CHECK_STATUS: &str = "<Check|MPos:0.000,0.000,0.000|WPos:0.000,0.000,0.000|Bf:15,1024|FS:0,0|Pn:|WCS:G54|Ov:100,100,100|A:|$C:1>";

fn start_check(handle: &EngineHandle) {
    handle
        .send(Command::StartCheckRun)
        .expect("start check run");
}

#[test]
fn check_run_waits_for_check_status_then_streams_and_returns_idle() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "M2"];
    load(&handle, &lines.join("\n"));

    control.expect_line("$C", &["ok"]);
    for line in lines {
        control.expect_line(line, &["ok"]);
    }
    // The exit `$C` also delivers the Idle status that completes the exit.
    control.expect_line("$C", &["ok", IDLE_STATUS]);
    start_check(&handle);
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|line| line == "$C"),
            Duration::from_secs(1)
        ),
        "the entering $C was not sent"
    );
    assert_eq!(handle.snapshot().job.state, JobState::Idle);
    std::thread::sleep(Duration::from_millis(60));
    assert!(
        !control.sent_lines().iter().any(|line| line == "G0 X1"),
        "no program line may be sent before the controller reports Check"
    );

    control.set_status(CHECK_STATUS);
    let started = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::CheckStarted { .. })
    });
    assert_eq!(started, EngineEvent::CheckStarted { total_lines: 3 });
    assert!(
        wait_until(|| handle.check_run(), Duration::from_secs(1)),
        "the engine must label the run as a dry run"
    );

    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    let completed = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::CheckCompleted { .. })
    });
    assert!(matches!(completed, EngineEvent::CheckCompleted { .. }));

    // Future status polls must not put the controller back in Check.
    control.set_status(support::IDLE_STATUS);
    assert!(
        wait_until(
            || handle.snapshot().state == MachineState::Idle,
            Duration::from_secs(1)
        ),
        "the controller did not return to Idle: {:?}",
        handle.snapshot().state
    );
    assert_eq!(
        control
            .sent_lines()
            .iter()
            .filter(|line| line.as_str() == "$C")
            .count(),
        2,
        "exactly one $C enters and one exits check mode"
    );
    assert_program_order(&control, &lines);
    assert!(!control.sent_bytes().contains(&0x81));
    assert_eq!(handle.snapshot().job.state, JobState::Completed);
    assert!(handle.check_run());
    assert!(
        handle
            .last_status()
            .is_none_or(|status| status.check_mode != Some(true)),
        "check mode must be off after the run"
    );
    handle.shutdown();
}

#[test]
fn check_run_requires_a_loaded_program_and_an_idle_machine() {
    let (handle, control) = connected_engine(standard_script);
    let error = handle
        .send(Command::StartCheckRun)
        .expect_err("no program is loaded");
    assert!(matches!(error, EngineError::NoProgram), "{error:?}");
    assert!(!control.sent_lines().iter().any(|line| line == "$C"));

    load(&handle, "G0 X1\nM2\n");
    control.set_status("<Alarm:1|MPos:0.000,0.000,0.000>");
    assert!(wait_until(
        || handle.snapshot().state == MachineState::Alarm,
        Duration::from_secs(1)
    ));
    let error = handle
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
    assert!(!control.sent_lines().iter().any(|line| line == "$C"));
    handle.shutdown();
}

#[test]
fn check_run_is_refused_while_a_job_is_active() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "M2"];
    control.expect_line(lines[0], &[]);
    load(&handle, &lines.join("\n"));
    start(&handle);
    assert!(
        wait_until(
            || control.sent_lines().iter().any(|line| line == lines[0]),
            Duration::from_secs(1)
        ),
        "the normal job did not start"
    );
    let error = handle
        .send(Command::StartCheckRun)
        .expect_err("a running job blocks a check run");
    assert!(
        matches!(error, EngineError::JobState { .. }),
        "unexpected error: {error:?}"
    );
    assert!(!control.sent_lines().iter().any(|line| line == "$C"));
    handle
        .send(Command::StopProgram { reset: false })
        .expect("stop the normal job");
    handle.shutdown();
}

#[test]
fn stopping_while_entering_check_mode_restores_idle() {
    let (handle, control) = connected_engine(standard_script);
    load(&handle, "G0 X1\nM2\n");

    // Hold the entering `$C` without an acknowledgement so the stop happens
    // while the engine is still waiting for the controller.
    control.expect_line("$C", &[]);
    control.expect_line("$C", &["ok", IDLE_STATUS]);
    start_check(&handle);
    assert!(wait_until(
        || control.sent_lines().iter().any(|line| line == "$C"),
        Duration::from_secs(1)
    ));
    handle
        .send(Command::StopProgram { reset: false })
        .expect("stop before the check transition");
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobAborted { .. })
    });

    // The controller acknowledges the entry and reports Check; the engine
    // must now leave check mode instead of starting the program late.
    control.set_status(CHECK_STATUS);
    control.push_line("ok");
    assert!(wait_until(
        || control
            .sent_lines()
            .iter()
            .filter(|line| line.as_str() == "$C")
            .count()
            == 2,
        Duration::from_secs(2)
    ));
    control.set_status(IDLE_STATUS);
    assert!(
        wait_until(
            || handle.snapshot().state == MachineState::Idle,
            Duration::from_secs(1)
        ),
        "the cancelled entry left the controller in {:?}",
        handle.snapshot().state
    );
    std::thread::sleep(Duration::from_millis(80));
    assert!(
        !control.sent_lines().iter().any(|line| line == "G0 X1"),
        "a cancelled check entry must not stream the program"
    );
    assert_eq!(handle.snapshot().job.state, JobState::Aborted);
    handle.shutdown();
}

#[test]
fn a_controller_error_during_check_still_leaves_check_mode() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "M2"];
    load(&handle, &lines.join("\n"));

    control.expect_line("$C", &["ok"]);
    control.expect_line(lines[0], &["error:20"]);
    // The sticky error is cleared with a blank line before `$C` is sent.
    control.expect_line("", &["ok"]);
    control.expect_line("$C", &["ok", IDLE_STATUS]);
    start_check(&handle);
    assert!(wait_until(
        || control.sent_lines().iter().any(|line| line == "$C"),
        Duration::from_secs(1)
    ));
    control.set_status(CHECK_STATUS);
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::CheckStarted { .. })
    });
    let error = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobError { .. })
    });
    assert!(matches!(error, EngineEvent::JobError { code: 20, .. }));
    assert!(wait_until(
        || control
            .sent_lines()
            .iter()
            .filter(|line| line.as_str() == "$C")
            .count()
            == 2,
        Duration::from_secs(2)
    ));
    control.set_status(IDLE_STATUS);
    assert!(wait_until(
        || handle.snapshot().state == MachineState::Idle,
        Duration::from_secs(1)
    ));
    assert_eq!(handle.snapshot().job.state, JobState::Aborted);
    assert!(!control.sent_lines().iter().any(|line| line == lines[1]));
    assert!(!control.sent_bytes().contains(&0x81));
    handle.shutdown();
}

#[test]
fn operator_owned_check_mode_is_not_toggled_by_the_engine() {
    let (handle, control) = connected_engine_with(config(), |control| {
        standard_script_with_options(
            control,
            "[OPT:VNMSL+2,64,1024,3,0]",
            "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
            CHECK_STATUS,
        );
    });
    let lines = ["G0 X1", "M2"];
    load(&handle, &lines.join("\n"));
    for line in lines {
        control.expect_line(line, &["ok"]);
    }
    start_check(&handle);

    let started = wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::CheckStarted { .. })
    });
    assert_eq!(started, EngineEvent::CheckStarted { total_lines: 2 });
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobCompleted { .. })
    });
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::CheckCompleted { .. })
    });
    assert!(
        !control.sent_lines().iter().any(|line| line == "$C"),
        "the engine must not toggle a check mode the operator enabled"
    );
    assert_eq!(handle.snapshot().state, MachineState::Check);
    assert!(handle.check_run());

    // The operator can still leave the mode they entered.
    control.expect_line("$C", &["ok", IDLE_STATUS]);
    handle
        .send(Command::System(SystemCommand::CheckMode))
        .expect("operator exits check mode");
    control.set_status(IDLE_STATUS);
    assert!(
        wait_until(
            || handle.snapshot().state == MachineState::Idle,
            Duration::from_secs(1)
        ),
        "the operator could not leave check mode"
    );
    handle.shutdown();
}

#[test]
fn stopping_a_check_run_exits_check_mode_once_without_moving() {
    let (handle, control) = connected_engine(standard_script);
    let lines = ["G0 X1", "G0 X2", "M2"];
    load(&handle, &lines.join("\n"));

    control.expect_line("$C", &["ok"]);
    // Hold the first line without an acknowledgement so the job is in flight.
    control.expect_line(lines[0], &[]);
    control.expect_line("$C", &["ok", IDLE_STATUS]);
    start_check(&handle);
    assert!(wait_until(
        || control.sent_lines().iter().any(|line| line == "$C"),
        Duration::from_secs(1)
    ));
    control.set_status(CHECK_STATUS);
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::CheckStarted { .. })
    });

    assert!(wait_until(
        || control.sent_lines().iter().any(|line| line == lines[0]),
        Duration::from_secs(1)
    ));
    handle
        .send(Command::StopProgram { reset: false })
        .expect("stop the check run");
    wait_event(&handle, Duration::from_secs(2), |event| {
        matches!(event, EngineEvent::JobAborted { .. })
    });

    assert!(wait_until(
        || control
            .sent_lines()
            .iter()
            .filter(|line| line.as_str() == "$C")
            .count()
            == 2,
        Duration::from_secs(2)
    ));
    let exit_count = control
        .sent_lines()
        .iter()
        .filter(|line| line.as_str() == "$C")
        .count();
    assert_eq!(exit_count, 2);
    control.set_status(IDLE_STATUS);
    assert!(wait_until(
        || handle.snapshot().state == MachineState::Idle,
        Duration::from_secs(1)
    ));
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(
        control
            .sent_lines()
            .iter()
            .filter(|line| line.as_str() == "$C")
            .count(),
        2,
        "the exit $C must not be repeated"
    );
    assert!(!control.sent_bytes().contains(&0x81));
    assert!(!control.sent_lines().iter().any(|line| line == lines[1]));
    assert_eq!(handle.snapshot().job.state, JobState::Aborted);
    assert!(handle.check_run(), "the stopped run is still a dry run");
    handle.shutdown();
}
