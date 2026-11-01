//! Typed engine errors.

use ghs_core::{JobState, MachineState};
use ghs_proto::{EncodeError, StreamerError};
use ghs_serial::TransportError;

/// Errors returned by the engine's public API or surfaced while driving a
/// session.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The engine thread has stopped; no further commands can be accepted.
    #[error("engine thread is not running")]
    EngineStopped,

    /// The bounded command queue is full.
    ///
    /// The caller should retry after draining a reply; the engine never blocks
    /// a UI thread to make room.
    #[error("engine command queue is full")]
    CommandQueueFull,

    /// A synchronous engine request did not receive a reply in time.
    #[error("timed out waiting for the engine to {operation}")]
    Timeout {
        /// Short description of the operation, for example `"connect"`.
        operation: &'static str,
    },

    /// No transport is open.
    #[error("no transport is connected")]
    NotConnected,

    /// A transport is already open.
    #[error("a transport is already connected")]
    AlreadyConnected,

    /// The transport is open but the handshake has not completed.
    #[error("the engine is still handshaking with the controller")]
    NotReady,

    /// Another single-line command or a settings verification is in progress.
    #[error("the engine is busy with another command")]
    Busy,

    /// Motion is refused in the current machine state.
    #[error("motion is blocked while the machine is in {state} state")]
    MotionBlocked {
        /// Current machine state.
        state: MachineState,
    },

    /// The command requires the idle state.
    #[error("command requires the idle state; the machine is in {state} state")]
    NotIdle {
        /// Current machine state.
        state: MachineState,
    },

    /// A pendant or MPG has taken over the input stream.
    #[error("a pendant (MPG) has control of the controller")]
    PendantActive,

    /// The command is malformed or not allowed through this gateway.
    #[error("invalid command: {0}")]
    InvalidCommand(String),

    /// No program is loaded in the job runner.
    #[error("no program is loaded")]
    NoProgram,

    /// The job is in a state that does not allow the requested action.
    #[error("cannot {action} while the job is in the {state:?} state")]
    JobState {
        /// Requested action, for example `"start"`.
        action: &'static str,
        /// Current job state.
        state: JobState,
    },

    /// The program failed validation before it could be streamed.
    #[error("invalid program: {0}")]
    InvalidProgram(String),

    /// The request is understood but not implemented yet.
    #[error("not supported yet: {0}")]
    Unsupported(String),

    /// The engine configuration was rejected.
    #[error("invalid engine configuration: {0}")]
    InvalidConfig(String),

    /// The transport rejected an operation.
    #[error(transparent)]
    Transport(#[from] TransportError),

    /// A command could not be encoded.
    #[error(transparent)]
    Encode(#[from] EncodeError),

    /// The streamer rejected a line.
    #[error(transparent)]
    Streamer(#[from] StreamerError),

    /// The engine thread could not be spawned.
    #[error("failed to spawn the engine thread: {0}")]
    ThreadSpawn(#[source] std::io::Error),
}
