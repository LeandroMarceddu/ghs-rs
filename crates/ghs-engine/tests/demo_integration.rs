//! End-to-end engine test over the in-process simulator.
//!
//! The unit/integration suite uses a scripted transport. This test proves the
//! real handshake and telemetry path against the simulator that ships with the
//! app's demo mode.

use std::thread;
use std::time::{Duration, Instant};

use ghs_demo::{SimProfile, start};
use ghs_engine::{Axis, Command, ConnectTarget, Engine, EngineConfig};

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

    engine.disconnect();
    engine.shutdown();
}

fn drain_events(engine: &ghs_engine::EngineHandle) {
    while engine.events().try_recv().is_ok() {}
}
