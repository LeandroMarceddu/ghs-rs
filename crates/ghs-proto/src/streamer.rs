//! Sans-IO character-counting streamer.
//!
//! grblHAL reads commands into a fixed serial receive buffer and acknowledges
//! one line at a time with `ok` or `error:<n>`. The hard safety invariant is:
//!
//! > the bytes of every line sent but not yet acknowledged, including the
//! > terminating `\n`, must never exceed the controller's receive buffer size.
//!
//! The reference FlexiHAL build reports 1024 bytes (its `64` is the planner
//! block buffer, not the serial RX buffer), so
//! [`StreamerConfig::rx_buffer_size`] must be set from `$I+` as soon as the
//! controller is identified. A 64-byte buffer is still exercised as a
//! deliberately conservative stress case.
//!
//! `Bf:` (planner blocks free, serial RX bytes free) can only *reduce* the
//! budget. It never lets the streamer exceed the character-count invariant.

use std::collections::VecDeque;

/// Default receive buffer size used before the controller reports its own.
///
/// This is the legacy grbl default; always replace it with the value from the
/// `$I` `OPT` line when it is available.
pub const DEFAULT_RX_BUFFER_SIZE: usize = 128;

/// Default maximum length of one queued line, in bytes.
pub const DEFAULT_MAX_LINE_LEN: usize = 4096;

/// Maximum number of lines held in the pending queue.
///
/// Bounds memory if a job runner ignores backpressure; the engine should feed
/// lines incrementally instead of relying on this cap.
pub const MAX_QUEUED_LINES: usize = 4096;

/// Streamer configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamerConfig {
    /// Controller serial receive buffer size in bytes.
    pub rx_buffer_size: usize,
    /// Maximum accepted length of one queued line, excluding the terminator.
    pub max_line_len: usize,
    /// Use `Bf:` serial free space to reduce the send budget.
    pub use_bf_topup: bool,
}

impl Default for StreamerConfig {
    fn default() -> Self {
        StreamerConfig {
            rx_buffer_size: DEFAULT_RX_BUFFER_SIZE,
            max_line_len: DEFAULT_MAX_LINE_LEN,
            use_bf_topup: true,
        }
    }
}

/// Why a line could not be queued, or why a buffer size was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StreamerError {
    /// The line contains a control or non-ASCII character.
    #[error("line contains an invalid character 0x{byte:02X} at byte offset {index}")]
    InvalidCharacter {
        /// Byte offset of the offending character.
        index: usize,
        /// Offending byte value.
        byte: u8,
    },
    /// The line plus its terminator can never fit in the receive buffer.
    #[error("line is {len} bytes but the {limit}-byte receive buffer can never hold it")]
    LineTooLong {
        /// Line length in bytes, excluding the terminator.
        len: usize,
        /// Receive buffer size.
        limit: usize,
    },
    /// The line exceeds the configured per-line maximum.
    #[error("line is {len} bytes; the configured maximum is {limit}")]
    ExceedsMaxLineLen {
        /// Line length in bytes.
        len: usize,
        /// Configured maximum.
        limit: usize,
    },
    /// The pending queue is full.
    #[error("stream queue is full at {0} lines")]
    QueueFull(usize),
    /// A receive buffer size of zero was requested.
    #[error("receive buffer size must be greater than zero")]
    ZeroBufferSize,
}

/// A line acknowledged by the controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AckedLine {
    /// Line text, without the terminator.
    pub line: String,
    /// Bytes freed in the controller buffer, including the terminator.
    pub bytes: usize,
    /// `true` for `ok`, `false` for `error:<n>`.
    pub ok: bool,
}

#[derive(Debug, Clone)]
struct InFlight {
    line: String,
    bytes: usize,
}

/// Sans-IO character-counting streamer.
///
/// Feed it complete G-code or `$` command lines with [`Streamer::enqueue`],
/// write [`Streamer::poll_output`] to the transport, and pass controller
/// acknowledgements to [`Streamer::on_ok`] / [`Streamer::on_error`]. Feed `Bf:`
/// values to [`Streamer::on_bf`].
///
/// `poll_output` returns complete bytes (each line followed by `\n`) so the
/// caller never has to reproduce the accounting. Write the whole buffer; if a
/// write is partial, call [`Streamer::clear`] after the link is re-established.
#[derive(Debug, Clone)]
pub struct Streamer {
    config: StreamerConfig,
    queue: VecDeque<String>,
    in_flight: VecDeque<InFlight>,
    sent_unacked: usize,
    bf_credit: Option<usize>,
    planner_free: Option<u32>,
    paused: bool,
    halted: bool,
}

impl Default for Streamer {
    fn default() -> Self {
        Streamer::new(StreamerConfig::default())
    }
}

impl Streamer {
    /// Create a streamer with the given configuration.
    #[must_use]
    pub fn new(config: StreamerConfig) -> Self {
        Streamer {
            config,
            queue: VecDeque::new(),
            in_flight: VecDeque::new(),
            sent_unacked: 0,
            bf_credit: None,
            planner_free: None,
            paused: false,
            halted: false,
        }
    }

    /// Current configuration.
    #[must_use]
    pub const fn config(&self) -> &StreamerConfig {
        &self.config
    }

    /// Queue one line for sending.
    ///
    /// The line must not contain a terminator. While the streamer is halted
    /// after an `error:`, queuing an empty line or a `$` command releases the
    /// halt, because those clear grblHAL's sticky error state.
    ///
    /// # Errors
    ///
    /// Returns [`StreamerError`] for invalid characters, a line that can never
    /// fit the controller buffer, or a full queue.
    pub fn enqueue(&mut self, line: &str) -> Result<(), StreamerError> {
        for (index, byte) in line.bytes().enumerate() {
            if byte != b'\t' && (byte < 0x20 || byte == 0x7F || byte >= 0x80) {
                return Err(StreamerError::InvalidCharacter { index, byte });
            }
        }
        if line.len() > self.config.max_line_len {
            return Err(StreamerError::ExceedsMaxLineLen {
                len: line.len(),
                limit: self.config.max_line_len,
            });
        }
        if line.len() + 1 > self.config.rx_buffer_size {
            return Err(StreamerError::LineTooLong {
                len: line.len(),
                limit: self.config.rx_buffer_size,
            });
        }
        if self.queue.len() >= MAX_QUEUED_LINES {
            return Err(StreamerError::QueueFull(MAX_QUEUED_LINES));
        }
        if self.halted && (line.trim().is_empty() || line.trim_start().starts_with('$')) {
            self.halted = false;
        }
        self.queue.push_back(line.to_string());
        Ok(())
    }

    /// Emit every line that currently fits the budget.
    ///
    /// Returns complete bytes, each line followed by `\n`. Real-time bytes are
    /// out of band and are never included here.
    #[must_use]
    pub fn poll_output(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.paused || self.halted {
            return out;
        }
        while let Some(front) = self.queue.front() {
            let bytes = front.len() + 1;
            if bytes > self.send_budget() {
                break;
            }
            let Some(line) = self.queue.pop_front() else {
                break;
            };
            self.sent_unacked = self.sent_unacked.saturating_add(bytes);
            if let Some(credit) = self.bf_credit.as_mut() {
                *credit = credit.saturating_sub(bytes);
            }
            out.extend_from_slice(line.as_bytes());
            out.push(b'\n');
            self.in_flight.push_back(InFlight { line, bytes });
        }
        out
    }

    /// Bytes that may still be sent before the next acknowledgement.
    #[must_use]
    pub fn available_bytes(&self) -> usize {
        self.send_budget()
    }

    fn send_budget(&self) -> usize {
        let char_budget = self.config.rx_buffer_size.saturating_sub(self.sent_unacked);
        if self.config.use_bf_topup {
            match self.bf_credit {
                Some(credit) => char_budget.min(credit),
                None => char_budget,
            }
        } else {
            char_budget
        }
    }

    /// Acknowledge one line with `ok`.
    ///
    /// Returns `None` when no line is in flight, which means the stream lost
    /// synchronization with the controller.
    pub fn on_ok(&mut self) -> Option<AckedLine> {
        self.acknowledge(true)
    }

    /// Acknowledge one line with `error:<n>` and halt output.
    ///
    /// grblHAL keeps the error active for subsequent G-code until a reset, an
    /// empty line or a `$` command clears it. Output stays halted until such a
    /// line is queued or [`Streamer::release_halt`] is called.
    ///
    /// Returns `None` when no line is in flight.
    pub fn on_error(&mut self) -> Option<AckedLine> {
        let acked = self.acknowledge(false);
        if acked.is_some() {
            self.halted = true;
        }
        acked
    }

    fn acknowledge(&mut self, ok: bool) -> Option<AckedLine> {
        let entry = self.in_flight.pop_front()?;
        self.sent_unacked = self.sent_unacked.saturating_sub(entry.bytes);
        if let Some(credit) = self.bf_credit.as_mut() {
            *credit = credit
                .saturating_add(entry.bytes)
                .min(self.config.rx_buffer_size);
        }
        Some(AckedLine {
            line: entry.line,
            bytes: entry.bytes,
            ok,
        })
    }

    /// Record a `Bf:<planner_free>,<rx_free>` report.
    ///
    /// `rx_free` is clamped to the configured buffer size. The character-count
    /// invariant still caps every send, so `Bf:` can only reduce the budget.
    pub fn on_bf(&mut self, planner_free: u32, rx_free: u32) {
        self.planner_free = Some(planner_free);
        if self.config.use_bf_topup {
            self.bf_credit = Some((rx_free as usize).min(self.config.rx_buffer_size));
        }
    }

    /// Free planner blocks from the most recent `Bf:` report.
    #[must_use]
    pub const fn planner_free(&self) -> Option<u32> {
        self.planner_free
    }

    /// Stop emitting queued lines. Acknowledgements are still processed.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume emitting queued lines.
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Returns `true` while output is paused.
    #[must_use]
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    /// Returns `true` while output is halted after an error.
    #[must_use]
    pub const fn is_halted(&self) -> bool {
        self.halted
    }

    /// Clear the error halt without queuing a clearing line.
    pub fn release_halt(&mut self) {
        self.halted = false;
    }

    /// Drop all queued and in-flight lines and reset protocol accounting.
    ///
    /// Only call this after a soft reset, a disconnect or when the controller's
    /// receive buffer is known to be empty; otherwise the character-count
    /// invariant no longer reflects the controller's real buffer.
    ///
    /// Returns the number of dropped lines.
    pub fn clear(&mut self) -> usize {
        let dropped = self.queue.len() + self.in_flight.len();
        self.queue.clear();
        self.in_flight.clear();
        self.sent_unacked = 0;
        self.bf_credit = None;
        self.halted = false;
        dropped
    }

    /// Number of lines sent but not yet acknowledged.
    #[must_use]
    pub fn in_flight_lines(&self) -> usize {
        self.in_flight.len()
    }

    /// Number of lines waiting to be sent.
    #[must_use]
    pub fn queued_lines(&self) -> usize {
        self.queue.len()
    }

    /// Bytes sent but not yet acknowledged, terminators included.
    #[must_use]
    pub const fn in_flight_bytes(&self) -> usize {
        self.sent_unacked
    }

    /// Bytes of queued lines, terminators included.
    #[must_use]
    pub fn queued_bytes(&self) -> usize {
        self.queue.iter().map(|line| line.len() + 1).sum()
    }

    /// Returns `true` when nothing is queued or in flight.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.queue.is_empty() && self.in_flight.is_empty()
    }

    /// Replace the receive buffer size, for example from `$I+`.
    ///
    /// Queued lines that can no longer fit are dropped and counted in the
    /// returned value. In-flight lines cannot be recalled; sending stays
    /// blocked until they drain.
    ///
    /// # Errors
    ///
    /// Returns [`StreamerError::ZeroBufferSize`] when `size` is zero.
    pub fn set_rx_buffer_size(&mut self, size: usize) -> Result<usize, StreamerError> {
        if size == 0 {
            return Err(StreamerError::ZeroBufferSize);
        }
        self.config.rx_buffer_size = size;
        let before = self.queue.len();
        self.queue.retain(|line| line.len() < size);
        let dropped = before - self.queue.len();
        if dropped > 0 {
            tracing::warn!(
                dropped,
                rx_buffer_size = size,
                "dropped queued lines that cannot fit the receive buffer"
            );
        }
        Ok(dropped)
    }

    /// Enable or disable use of `Bf:` Rx free space as an additional budget.
    pub fn set_use_bf_topup(&mut self, enabled: bool) {
        self.config.use_bf_topup = enabled;
        if !enabled {
            self.bf_credit = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn streamer(rx: usize) -> Streamer {
        Streamer::new(StreamerConfig {
            rx_buffer_size: rx,
            max_line_len: 512,
            use_bf_topup: true,
        })
    }

    fn lines(bytes: &[u8]) -> Vec<&[u8]> {
        bytes
            .split_inclusive(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .collect()
    }

    #[test]
    fn reference_64_byte_buffer_is_never_exceeded() {
        let mut streamer = streamer(64);
        let line = "G1 X0 Y0 Z0 F100"; // 17 bytes + newline
        for _ in 0..4 {
            streamer.enqueue(line).expect("fits");
        }
        let out = streamer.poll_output();
        assert_eq!(lines(&out).len(), 3);
        assert_eq!(out.len(), 51);
        assert!(out.len() <= 64);
        assert_eq!(streamer.in_flight_lines(), 3);
        assert_eq!(streamer.queued_lines(), 1);

        assert!(streamer.on_ok().is_some());
        let out = streamer.poll_output();
        assert_eq!(out.len(), 17);
        assert_eq!(streamer.in_flight_bytes(), 51);
    }

    #[test]
    fn a_line_larger_than_the_buffer_is_rejected() {
        let mut streamer = streamer(16);
        assert_eq!(
            streamer.enqueue("0123456789abcdef"),
            Err(StreamerError::LineTooLong { len: 16, limit: 16 })
        );
        assert!(streamer.enqueue("0123456789abcde").is_ok());
        assert_eq!(streamer.queued_lines(), 1);
    }

    #[test]
    fn empty_lines_are_legal_and_acknowledged() {
        let mut streamer = streamer(8);
        streamer.enqueue("").expect("empty line");
        let out = streamer.poll_output();
        assert_eq!(out, b"\n");
        let acked = streamer.on_ok().expect("ack");
        assert_eq!(acked.bytes, 1);
        assert_eq!(acked.line, "");
        assert!(streamer.is_idle());
    }

    #[test]
    fn partial_acks_release_one_line_at_a_time() {
        let mut streamer = streamer(32);
        streamer.enqueue("G0 X0").expect("fits"); // 6 bytes
        streamer.enqueue("G0 X1").expect("fits");
        streamer.enqueue("G0 X2").expect("fits");
        streamer.enqueue("G0 X3").expect("fits");
        streamer.enqueue("G0 X4").expect("fits");
        streamer.enqueue("G0 X5").expect("fits");
        let out = streamer.poll_output();
        assert_eq!(out.len(), 30);
        assert_eq!(streamer.in_flight_lines(), 5);
        assert_eq!(streamer.queued_lines(), 1);

        streamer.on_ok();
        assert_eq!(streamer.poll_output(), b"G0 X5\n");
        assert_eq!(streamer.in_flight_bytes(), 30);
        assert_eq!(streamer.in_flight_lines(), 5);
    }

    #[test]
    fn bf_can_only_reduce_the_budget() {
        let mut streamer = streamer(128);
        streamer.on_bf(15, 10);
        assert_eq!(streamer.available_bytes(), 10);
        streamer.enqueue("G1 X10 Y10 F500").expect("fits");
        assert!(streamer.poll_output().is_empty());

        streamer.on_bf(15, 128);
        assert_eq!(streamer.available_bytes(), 128);
        let out = streamer.poll_output();
        assert_eq!(out.len(), 16);

        // A generous Bf report cannot exceed the 128-byte character budget.
        streamer.on_bf(15, 128);
        let out = streamer.poll_output();
        assert!(out.is_empty());
        assert_eq!(streamer.available_bytes(), 128 - 16);
    }

    #[test]
    fn bf_topup_can_be_disabled() {
        let mut streamer = streamer(64);
        streamer.set_use_bf_topup(false);
        streamer.on_bf(0, 0);
        streamer.enqueue("G0 X0").expect("fits");
        assert_eq!(streamer.poll_output(), b"G0 X0\n");
    }

    #[test]
    fn pause_stops_output_and_resume_continues() {
        let mut streamer = streamer(64);
        streamer.enqueue("G0 X0").expect("fits");
        streamer.pause();
        assert!(streamer.poll_output().is_empty());
        assert!(streamer.is_paused());
        streamer.resume();
        assert!(!streamer.is_paused());
        assert_eq!(streamer.poll_output(), b"G0 X0\n");
    }

    #[test]
    fn error_halts_until_a_clearing_line_is_queued() {
        let mut streamer = streamer(64);
        streamer.enqueue("G0 X0").expect("fits");
        streamer.enqueue("G0 X1").expect("fits");
        let out = streamer.poll_output();
        assert_eq!(out.len(), 12);

        let acked = streamer.on_error().expect("ack");
        assert!(!acked.ok);
        assert!(streamer.is_halted());
        assert!(streamer.poll_output().is_empty());

        streamer.enqueue("$X").expect("clears the error");
        assert!(!streamer.is_halted());
        assert_eq!(streamer.poll_output(), b"$X\n");
    }

    #[test]
    fn blank_line_releases_the_error_halt() {
        let mut streamer = streamer(64);
        streamer.enqueue("G0 X0").expect("fits");
        let _ = streamer.poll_output();
        streamer.on_error();
        assert!(streamer.is_halted());
        streamer.enqueue("").expect("blank line");
        assert!(!streamer.is_halted());
        assert_eq!(streamer.poll_output(), b"\n");
    }

    #[test]
    fn clear_drops_everything() {
        let mut streamer = streamer(64);
        streamer.enqueue("G0 X0").expect("fits");
        streamer.enqueue("G0 X1").expect("fits");
        let _ = streamer.poll_output();
        assert_eq!(streamer.clear(), 2);
        assert!(streamer.is_idle());
        assert_eq!(streamer.in_flight_bytes(), 0);
        assert!(!streamer.is_halted());
        streamer.enqueue("G0 X2").expect("fits");
        assert_eq!(streamer.poll_output(), b"G0 X2\n");
    }

    #[test]
    fn shrinking_the_buffer_drops_queued_lines() {
        let mut streamer = streamer(128);
        streamer.enqueue(&"G1 X".repeat(20)).expect("fits 128");
        streamer.enqueue("G0 X0").expect("fits");
        assert_eq!(streamer.set_rx_buffer_size(64), Ok(1));
        assert_eq!(streamer.queued_lines(), 1);
        assert_eq!(
            streamer.set_rx_buffer_size(0),
            Err(StreamerError::ZeroBufferSize)
        );
        assert_eq!(streamer.queued_bytes(), 6);
    }

    #[test]
    fn enqueue_rejects_injection_and_bad_characters() {
        let mut streamer = streamer(64);
        assert!(matches!(
            streamer.enqueue("G0 X0\nM3"),
            Err(StreamerError::InvalidCharacter { .. })
        ));
        assert!(matches!(
            streamer.enqueue("G0 X0\rM3"),
            Err(StreamerError::InvalidCharacter { .. })
        ));
        assert!(matches!(
            streamer.enqueue("G0 X0\0"),
            Err(StreamerError::InvalidCharacter { .. })
        ));
        assert!(streamer.enqueue("caf\u{e9}").is_err());
        assert!(streamer.enqueue("G0 X\tY0").is_ok());
    }

    #[test]
    fn queue_capacity_is_bounded() {
        let mut streamer = streamer(64);
        streamer.enqueue("").expect("fits");
        for _ in 1..MAX_QUEUED_LINES {
            streamer.enqueue("").expect("fits");
        }
        assert_eq!(
            streamer.enqueue(""),
            Err(StreamerError::QueueFull(MAX_QUEUED_LINES))
        );
    }

    #[test]
    fn unsolicited_ack_is_ignored() {
        let mut streamer = streamer(64);
        assert!(streamer.on_ok().is_none());
        assert!(streamer.on_error().is_none());
        assert!(!streamer.is_halted());
    }

    #[derive(Debug, Clone)]
    enum Op {
        Enqueue(String),
        Ok,
        Error,
        Bf(u32, u32),
        Pause,
        Resume,
        Clear,
        Poll,
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            prop::collection::vec(prop::char::range('a', 'z'), 0..80)
                .prop_map(|chars| Op::Enqueue(chars.into_iter().collect())),
            Just(Op::Ok),
            Just(Op::Error),
            (0u32..2048, 0u32..512).prop_map(|(planner, rx)| Op::Bf(planner, rx)),
            Just(Op::Pause),
            Just(Op::Resume),
            Just(Op::Clear),
            Just(Op::Poll),
        ]
    }

    proptest! {
        #[test]
        fn character_counting_invariant_holds(
            rx in 8usize..=256,
            ops in prop::collection::vec(op_strategy(), 0..300)
        ) {
            let mut streamer = Streamer::new(StreamerConfig {
                rx_buffer_size: rx,
                max_line_len: 512,
                use_bf_topup: true,
            });
            let mut unacked: VecDeque<usize> = VecDeque::new();

            for op in ops {
                match op {
                    Op::Enqueue(text) => {
                        match streamer.enqueue(&text) {
                            Ok(()) => {}
                            Err(StreamerError::LineTooLong { len, limit }) => {
                                prop_assert_eq!(limit, rx);
                                prop_assert!(len + 1 > rx);
                            }
                            Err(other) => prop_assert!(false, "unexpected enqueue error: {other:?}"),
                        }
                    }
                    Op::Ok => {
                        if streamer.on_ok().is_some() {
                            unacked.pop_front();
                        } else {
                            prop_assert!(unacked.is_empty());
                        }
                    }
                    Op::Error => {
                        if streamer.on_error().is_some() {
                            unacked.pop_front();
                        }
                    }
                    Op::Bf(planner, free) => streamer.on_bf(planner, free),
                    Op::Pause => streamer.pause(),
                    Op::Resume => streamer.resume(),
                    Op::Clear => {
                        streamer.clear();
                        unacked.clear();
                    }
                    Op::Poll => {
                        let out = streamer.poll_output();
                        let before: usize = unacked.iter().sum();
                        prop_assert!(
                            before + out.len() <= rx,
                            "{} unacked + {} emitted exceeds {}",
                            before,
                            out.len(),
                            rx
                        );
                        if streamer.is_paused() {
                            prop_assert!(out.is_empty());
                        }
                        for line in out.split_inclusive(|byte| *byte == b'\n') {
                            unacked.push_back(line.len());
                        }
                    }
                }

                prop_assert_eq!(streamer.in_flight_lines(), unacked.len());
                let expected: usize = unacked.iter().sum();
                prop_assert_eq!(streamer.in_flight_bytes(), expected);
                prop_assert!(streamer.in_flight_bytes() <= rx);
            }
        }
    }
}
