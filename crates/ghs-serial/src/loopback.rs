//! In-process byte-level loopback for tests and simulator demo mode.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::TransportError;
use crate::transport::{Transport, TransportEvent};

/// Creates two connected in-process transport endpoints.
///
/// Bytes sent on one endpoint are delivered as [`TransportEvent::Bytes`] on the
/// other endpoint's [`Transport::events`] receiver. Closing or dropping one
/// endpoint publishes [`TransportEvent::Closed`] to both its peer and its own
/// receiver. No operating-system resources are used.
#[must_use]
pub fn loopback_pair() -> (LoopbackTransport, LoopbackTransport) {
    let (sender_a, receiver_a) = unbounded();
    let (sender_b, receiver_b) = unbounded();

    let state_a = Arc::new(EndpointState::default());
    let state_b = Arc::new(EndpointState::default());

    let endpoint_a = LoopbackTransport {
        inbound: receiver_a,
        outbound: Some(sender_b.clone()),
        self_tx: Some(sender_a.clone()),
        state: Arc::clone(&state_a),
        peer_state: Arc::clone(&state_b),
    };
    let endpoint_b = LoopbackTransport {
        inbound: receiver_b,
        outbound: Some(sender_a),
        self_tx: Some(sender_b),
        state: state_b,
        peer_state: state_a,
    };
    (endpoint_a, endpoint_b)
}

#[derive(Debug, Default)]
struct EndpointState {
    closed: AtomicBool,
}

/// One end of an in-process loopback pair.
///
/// See [`loopback_pair`]. The endpoint sends complete byte buffers; it does not
/// simulate baud rate, buffering, or protocol behaviour.
pub struct LoopbackTransport {
    inbound: Receiver<TransportEvent>,
    outbound: Option<Sender<TransportEvent>>,
    self_tx: Option<Sender<TransportEvent>>,
    state: Arc<EndpointState>,
    peer_state: Arc<EndpointState>,
}

impl LoopbackTransport {
    fn is_locally_closed(&self) -> bool {
        self.state.closed.load(Ordering::Acquire)
    }

    fn notify_peer(&mut self) {
        if self.peer_state.closed.load(Ordering::Acquire) {
            return;
        }
        if let Some(sender) = self.outbound.take() {
            let _ = sender.send(TransportEvent::Closed);
        }
    }

    fn notify_self(&mut self) {
        if let Some(sender) = self.self_tx.take() {
            let _ = sender.send(TransportEvent::Closed);
        }
    }
}

impl Transport for LoopbackTransport {
    fn send(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        if self.is_locally_closed() || self.peer_state.closed.load(Ordering::Acquire) {
            return Err(TransportError::Closed);
        }
        if bytes.is_empty() {
            return Ok(());
        }

        let Some(sender) = self.outbound.as_ref() else {
            return Err(TransportError::Closed);
        };
        match sender.send(TransportEvent::Bytes(bytes.to_vec())) {
            Ok(()) => Ok(()),
            Err(_) => {
                self.peer_state.closed.store(true, Ordering::Release);
                Err(TransportError::Closed)
            }
        }
    }

    fn events(&self) -> &Receiver<TransportEvent> {
        &self.inbound
    }

    fn close(&mut self) {
        if self.state.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.notify_self();
        self.notify_peer();
    }

    fn is_open(&self) -> bool {
        !self.is_locally_closed() && !self.peer_state.closed.load(Ordering::Acquire)
    }
}

impl Drop for LoopbackTransport {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crossbeam_channel::RecvTimeoutError;

    use super::*;

    fn recv(transport: &LoopbackTransport) -> TransportEvent {
        transport
            .events()
            .recv_timeout(Duration::from_secs(1))
            .expect("an event")
    }

    #[test]
    fn roundtrip_delivers_bytes() {
        let (mut sender, receiver) = loopback_pair();
        assert!(sender.is_open());
        assert!(receiver.is_open());

        sender.send(b"G0 X10\r\n").expect("send");
        assert_eq!(
            recv(&receiver),
            TransportEvent::Bytes(b"G0 X10\r\n".to_vec())
        );
    }

    #[test]
    fn traffic_is_bidirectional() {
        let (mut left, mut right) = loopback_pair();

        left.send(b"ping").expect("left send");
        assert_eq!(recv(&right), TransportEvent::Bytes(b"ping".to_vec()));

        right.send(b"pong").expect("right send");
        assert_eq!(recv(&left), TransportEvent::Bytes(b"pong".to_vec()));
    }

    #[test]
    fn close_propagates_to_the_peer() {
        let (mut left, right) = loopback_pair();
        left.close();

        assert!(!left.is_open());
        assert!(!right.is_open());
        assert_eq!(recv(&left), TransportEvent::Closed);
        assert_eq!(recv(&right), TransportEvent::Closed);
        assert!(matches!(left.send(b"x"), Err(TransportError::Closed)));
    }

    #[test]
    fn dropping_an_endpoint_closes_the_peer() {
        let (left, mut right) = loopback_pair();
        drop(left);

        assert!(!right.is_open());
        assert_eq!(recv(&right), TransportEvent::Closed);
        assert!(matches!(right.send(b"x"), Err(TransportError::Closed)));
    }

    #[test]
    fn empty_send_produces_no_event() {
        let (mut left, right) = loopback_pair();
        left.send(b"").expect("empty send");
        assert!(matches!(
            right.events().recv_timeout(Duration::from_millis(30)),
            Err(RecvTimeoutError::Timeout)
        ));
    }

    #[test]
    fn events_receive_timeout_is_reported() {
        let (_left, right) = loopback_pair();
        let started = Instant::now();
        let result = right.events().recv_timeout(Duration::from_millis(40));
        assert!(matches!(result, Err(RecvTimeoutError::Timeout)));
        assert!(started.elapsed() >= Duration::from_millis(35));
    }

    #[test]
    fn events_work_with_select() {
        let (mut left, right) = loopback_pair();
        left.send(b"x").expect("send");

        let events = right.events();
        crossbeam_channel::select! {
            recv(events) -> event => {
                assert_eq!(event.expect("event"), TransportEvent::Bytes(b"x".to_vec()));
            }
        }
    }

    #[test]
    fn select_can_send_in_the_handler() {
        let (mut left, mut right) = loopback_pair();
        right.send(b"in").expect("seed the left inbox");

        crossbeam_channel::select! {
            recv(left.events()) -> event => {
                assert_eq!(event.expect("event"), TransportEvent::Bytes(b"in".to_vec()));
                left.send(b"out").expect("send from the handler");
            }
        }

        assert_eq!(recv(&right), TransportEvent::Bytes(b"out".to_vec()));
    }

    #[test]
    fn boxed_transport_can_select_and_send() {
        let (left, mut right) = loopback_pair();
        right.send(b"in").expect("seed the inbox");
        let mut transport: Box<dyn Transport> = Box::new(left);

        crossbeam_channel::select! {
            recv(transport.events()) -> event => {
                assert_eq!(event.expect("event"), TransportEvent::Bytes(b"in".to_vec()));
                transport.send(b"out").expect("send from the handler");
            }
        }

        assert_eq!(recv(&right), TransportEvent::Bytes(b"out".to_vec()));
    }

    #[test]
    fn close_is_idempotent() {
        let (mut left, right) = loopback_pair();
        left.close();
        left.close();
        assert_eq!(recv(&left), TransportEvent::Closed);
        assert_eq!(recv(&right), TransportEvent::Closed);
        // A second close must not panic or resurrect the endpoint.
        let (mut left, _right) = loopback_pair();
        left.close();
        left.close();
        assert!(!left.is_open());
    }
}
