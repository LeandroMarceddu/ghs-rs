//! In-process grblHAL simulator bridge for demo mode and integration tests.
//!
//! The simulator itself is sans-IO. This crate owns the thread that feeds it
//! from one end of a [`ghs_serial::loopback_pair`] and writes its output to the
//! other. The engine connects to the returned transport exactly as it would to
//! a serial port, so there is no separate demo code path in the protocol or
//! session layers.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, unbounded};
use ghs_serial::{LoopbackTransport, Transport, TransportEvent, loopback_pair};
pub use ghs_sim::SimProfile;
use ghs_sim::Simulator;
use thiserror::Error;

/// How often simulated time advances while the bridge is idle.
const TICK_INTERVAL: Duration = Duration::from_millis(10);

/// Errors starting or stopping the demo bridge.
#[derive(Debug, Error)]
pub enum DemoError {
    /// The simulator thread could not be started.
    #[error("failed to start the simulator thread: {0}")]
    Thread(#[from] std::io::Error),
}

/// A running in-process simulator.
///
/// Dropping the session stops and joins the thread. The engine end of the
/// loopback is returned separately by [`start`].
pub struct SimSession {
    stop: Arc<AtomicBool>,
    stop_tx: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl SimSession {
    /// Stops the simulator and joins its thread. Idempotent.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.stop_tx.send(());
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("simulator bridge thread panicked");
        }
    }
}

impl Drop for SimSession {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Starts a simulator and returns the engine-side loopback endpoint.
///
/// The simulator sends its banner immediately. The caller owns the returned
/// transport and may drop it at any time; the simulator then sees `Closed` and
/// exits.
///
/// # Errors
///
/// Returns [`DemoError::Thread`] when the thread cannot be started.
pub fn start(profile: SimProfile) -> Result<(LoopbackTransport, SimSession), DemoError> {
    let (engine_end, sim_end) = loopback_pair();
    let stop = Arc::new(AtomicBool::new(false));
    let (stop_tx, stop_rx) = unbounded();

    let thread_stop = Arc::clone(&stop);
    let thread = thread::Builder::new()
        .name("ghs-demo-sim".into())
        .spawn(move || run(profile, sim_end, stop_rx, thread_stop))?;

    Ok((
        engine_end,
        SimSession {
            stop,
            stop_tx,
            thread: Some(thread),
        },
    ))
}

fn run(
    profile: SimProfile,
    mut transport: LoopbackTransport,
    stop_rx: Receiver<()>,
    stop: Arc<AtomicBool>,
) {
    let mut simulator = Simulator::new(profile);
    if transport.send(&simulator.banner()).is_err() {
        return;
    }

    let ticks = crossbeam_channel::tick(TICK_INTERVAL);
    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }

        crossbeam_channel::select! {
            recv(transport.events()) -> event => match event {
                Ok(TransportEvent::Bytes(bytes)) => {
                    let output = simulator.feed(&bytes);
                    if !output.is_empty() && transport.send(&output).is_err() {
                        break;
                    }
                }
                Ok(TransportEvent::Error(message)) => {
                    tracing::debug!(%message, "simulator transport error");
                    break;
                }
                Ok(TransportEvent::Closed) | Err(_) => break,
            },
            recv(stop_rx) -> _ => break,
            recv(ticks) -> _ => {
                let output = simulator.tick(TICK_INTERVAL);
                if !output.is_empty() && transport.send(&output).is_err() {
                    break;
                }
            }
        }
    }

    transport.close();
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ghs_proto::{Response, ResponseParser, parse_controller_info};
    use ghs_serial::Transport;
    use ghs_sim::SimProfile;

    use super::*;

    fn recv_until_ok(transport: &impl Transport) -> Vec<Response> {
        let mut parser = ResponseParser::new();
        let mut responses = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            let Ok(event) = transport.events().recv_timeout(Duration::from_millis(100)) else {
                continue;
            };
            match event {
                TransportEvent::Bytes(bytes) => {
                    responses.extend(parser.push(&bytes));
                    if responses.iter().any(Response::is_ok) {
                        return responses;
                    }
                }
                TransportEvent::Error(message) => panic!("simulator error: {message}"),
                TransportEvent::Closed => break,
            }
        }
        responses
    }

    #[test]
    fn starts_a_flexihal_controller() {
        let (mut engine, mut session) = start(SimProfile::FlexiHal).expect("start simulator");

        let banner = engine
            .events()
            .recv_timeout(Duration::from_secs(1))
            .expect("banner");
        assert!(matches!(banner, TransportEvent::Bytes(bytes) if bytes.starts_with(b"GrblHAL")));

        engine.send(b"$I+\r\n").expect("send $I+");
        let responses = recv_until_ok(&engine);
        let lines: Vec<String> = responses
            .into_iter()
            .filter_map(|response| match response {
                Response::Feedback(line) => Some(line),
                _ => None,
            })
            .collect();
        let info = parse_controller_info(&lines);
        assert_eq!(info.axes.to_string(), "XYZ");
        assert_eq!(info.rx_buffer_size, Some(1024));
        assert_eq!(info.planner_blocks, Some(64));

        session.stop();
    }

    #[test]
    fn reports_status_and_stops_cleanly() {
        let (mut engine, mut session) = start(SimProfile::FlexiHal).expect("start simulator");
        let _ = engine
            .events()
            .recv_timeout(Duration::from_secs(1))
            .expect("banner");

        engine.send(b"?").expect("status");
        let mut parser = ResponseParser::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut saw_status = false;
        while std::time::Instant::now() < deadline && !saw_status {
            let Ok(event) = engine.events().recv_timeout(Duration::from_millis(100)) else {
                continue;
            };
            if let TransportEvent::Bytes(bytes) = event {
                saw_status = parser
                    .push(&bytes)
                    .iter()
                    .any(|response| matches!(response, Response::Status(_)));
            }
        }
        assert!(saw_status, "no status report");

        session.stop();
        session.stop();
        assert!(!engine.is_open());
    }
}
