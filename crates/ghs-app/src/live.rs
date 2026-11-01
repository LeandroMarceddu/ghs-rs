//! Live engine link for demo mode.
//!
//! One engine thread owns the transport. This module owns the engine handle,
//! keeps the simulator session alive, drains events without blocking, and maps
//! telemetry into [`AppState`]. It never writes controller bytes: every action
//! is a typed [`Command`] through the engine's safety gateway.

use std::sync::Arc;

use ghs_demo::{SimProfile, SimSession};
use ghs_engine::{
    Command, ConnectTarget, ControllerInfo, Engine, EngineConfig, EngineError, EngineEvent,
    EngineHandle, LogLevel, MachineSnapshot, StatusReport,
};

use crate::state::{AppState, ConnectionState, ConsoleLevel, Units};

/// One live engine plus the in-process simulator feeding it.
pub struct LiveLink {
    engine: EngineHandle,
    sim: Option<SimSession>,
    last_info: Option<Arc<ControllerInfo>>,
    last_snapshot: Option<Arc<MachineSnapshot>>,
    last_status: Option<Arc<StatusReport>>,
}

impl LiveLink {
    /// Spawn an engine and connect it to a fresh FlexiHal demo simulator.
    pub fn start_demo() -> Result<Self, String> {
        let engine = Engine::spawn(EngineConfig::default()).map_err(|error| error.to_string())?;
        let mut link = Self {
            engine,
            sim: None,
            last_info: None,
            last_snapshot: None,
            last_status: None,
        };
        link.connect_demo()?;
        Ok(link)
    }

    /// Start a fresh simulator and reconnect the existing engine to it.
    ///
    /// The previous loopback endpoint is closed by the engine during
    /// disconnect, so demo reconnect always creates a new simulator session.
    pub fn connect_demo(&mut self) -> Result<(), String> {
        if let Some(mut sim) = self.sim.take() {
            sim.stop();
        }
        self.drain_events();
        let (endpoint, sim) = ghs_demo::start(SimProfile::FlexiHal).map_err(|e| e.to_string())?;
        self.sim = Some(sim);
        self.reset_cache();
        if let Err(error) = self
            .engine
            .connect(ConnectTarget::Transport(Box::new(endpoint)))
        {
            if let Some(mut sim) = self.sim.take() {
                sim.stop();
            }
            return Err(error.to_string());
        }
        Ok(())
    }

    /// Discard events left in the queue by the previous session.
    ///
    /// The engine event receiver survives disconnect and reconnect; without
    /// this, a late "Disconnected" log from the old session can appear in the
    /// fresh session's console.
    fn drain_events(&self) {
        while self.engine.events().try_recv().is_ok() {}
    }

    /// Close the transport. The engine thread stays alive for a reconnect.
    pub fn disconnect(&self) {
        self.engine.disconnect();
    }

    /// Send one typed command through the engine gateway.
    ///
    /// # Errors
    ///
    /// Returns the gateway or transport error unchanged.
    pub fn send(&self, command: Command) -> Result<(), EngineError> {
        self.engine.send(command)
    }

    /// Drain events, refresh connection state, and apply new telemetry.
    ///
    /// Returns `true` when the view model changed and the shell should redraw.
    pub fn poll(&mut self, state: &mut AppState) -> bool {
        let mut changed = false;
        let mut events = Vec::new();
        while let Ok(event) = self.engine.events().try_recv() {
            events.push(event);
        }
        for event in events {
            changed |= self.apply_event(state, event);
        }

        let connected = self.engine.is_connected();
        let ready = self.engine.is_ready();
        if ready && state.connection != ConnectionState::Ready {
            state.connection = ConnectionState::Ready;
            changed = true;
        } else if connected
            && !ready
            && !matches!(
                state.connection,
                ConnectionState::Connecting | ConnectionState::Ready
            )
        {
            state.connection = ConnectionState::Connecting;
            changed = true;
        } else if !connected
            && matches!(
                state.connection,
                ConnectionState::Connecting | ConnectionState::Ready
            )
        {
            state.connection = ConnectionState::Disconnected;
            changed = true;
        }

        if let Some(info) = self.engine.controller_info()
            && self
                .last_info
                .as_ref()
                .is_none_or(|last| !Arc::ptr_eq(last, &info))
        {
            state.apply_controller_info(&info);
            self.last_info = Some(info);
            changed = true;
        }

        if let Some(parser) = self.engine.parser_state()
            && let Some(units) = parser.units
        {
            let display = Units::from_core(units);
            if state.profile.capabilities.units != display {
                state.set_units(units);
                changed = true;
            }
        }

        if ready {
            let snapshot = self.engine.snapshot();
            let status = self.engine.last_status();
            let snapshot_changed = self
                .last_snapshot
                .as_ref()
                .is_none_or(|last| !Arc::ptr_eq(last, &snapshot));
            let status_changed = option_arc_changed(&self.last_status, &status);
            if snapshot_changed || status_changed {
                state.apply_snapshot(&snapshot, status.as_deref());
                self.last_snapshot = Some(snapshot);
                self.last_status = status;
                changed = true;
            }
        }

        changed
    }

    fn apply_event(&mut self, state: &mut AppState, event: EngineEvent) -> bool {
        match event {
            EngineEvent::Connected => {
                state.connection = ConnectionState::Ready;
                state.push_console(
                    ConsoleLevel::Info,
                    "Controller connected; live telemetry is flowing",
                );
                true
            }
            EngineEvent::Disconnected => {
                state.connection = ConnectionState::Disconnected;
                state.push_console(
                    ConsoleLevel::Warn,
                    "Disconnected; the last telemetry is stale",
                );
                true
            }
            EngineEvent::ConnectFailed(reason) => {
                state.connection = ConnectionState::Failed(reason.clone());
                state.push_console(ConsoleLevel::Error, format!("Connect failed: {reason}"));
                true
            }
            EngineEvent::Log { level, text } => {
                if discovery_noise(&text) {
                    return false;
                }
                state.push_console(log_level(level), text);
                true
            }
            EngineEvent::Alarm(code) => {
                state.machine_state = "Alarm";
                state.push_console(ConsoleLevel::Error, format!("ALARM:{code}"));
                true
            }
            EngineEvent::Error(code) => {
                state.push_console(ConsoleLevel::Error, format!("error:{code}"));
                true
            }
            EngineEvent::Banner(_) => {
                // The engine already logged the banner as an Info line.
                false
            }
            EngineEvent::ControllerInfo(info) => {
                if self
                    .last_info
                    .as_ref()
                    .is_some_and(|last| Arc::ptr_eq(last, &info))
                {
                    return false;
                }
                state.apply_controller_info(&info);
                self.last_info = Some(info);
                true
            }
            EngineEvent::Settings(_)
            | EngineEvent::CodeTables { .. }
            | EngineEvent::Probe(_)
            | EngineEvent::SettingWritten { .. } => false,
        }
    }

    fn reset_cache(&mut self) {
        self.last_info = None;
        self.last_snapshot = None;
        self.last_status = None;
    }
}

fn option_arc_changed<T>(last: &Option<Arc<T>>, current: &Option<Arc<T>>) -> bool {
    match (last, current) {
        (None, None) => false,
        (Some(last), Some(current)) => !Arc::ptr_eq(last, current),
        _ => true,
    }
}

fn log_level(level: LogLevel) -> ConsoleLevel {
    match level {
        LogLevel::Info => ConsoleLevel::Info,
        LogLevel::Command => ConsoleLevel::Command,
        LogLevel::Ok => ConsoleLevel::Ok,
        LogLevel::Warn => ConsoleLevel::Warn,
        LogLevel::Error => ConsoleLevel::Error,
    }
}

/// Drop the per-setting `$110=...` dump from discovery.
///
/// A controller can return hundreds of settings during the handshake. They are
/// real controller output and remain in the engine's settings snapshot, but
/// they would otherwise evict the banner, identity and every user command from
/// the bounded 200-line console before the first screen is drawn.
fn discovery_noise(text: &str) -> bool {
    text.strip_prefix('$')
        .is_some_and(|rest| rest.starts_with(|ch: char| ch.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use ghs_engine::{Axis, MachineState};

    use super::*;

    fn wait_until(mut predicate: impl FnMut() -> bool, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if predicate() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        predicate()
    }

    #[test]
    fn demo_link_connects_sends_mdi_and_reconnects() {
        let mut link = LiveLink::start_demo().expect("start demo engine");
        assert!(
            wait_until(|| link.engine.is_ready(), Duration::from_secs(5)),
            "demo handshake did not complete"
        );
        assert!(
            wait_until(
                || link.engine.snapshot().state == MachineState::Idle,
                Duration::from_secs(2)
            ),
            "demo controller did not report Idle"
        );

        link.send(Command::Mdi("G0 X1 Y2".into()))
            .expect("MDI send");
        assert!(
            wait_until(
                || {
                    let position = link.engine.snapshot().mpos;
                    (position.get(Axis::X) - 1.0).abs() < 0.001
                        && (position.get(Axis::Y) - 2.0).abs() < 0.001
                },
                Duration::from_secs(3)
            ),
            "snapshot position did not follow the MDI command"
        );

        link.disconnect();
        assert!(
            wait_until(|| !link.engine.is_connected(), Duration::from_secs(2)),
            "disconnect did not close the link"
        );

        link.connect_demo().expect("demo reconnect");
        assert!(
            wait_until(|| link.engine.is_ready(), Duration::from_secs(5)),
            "reconnect handshake did not complete"
        );
        link.disconnect();
    }
}
