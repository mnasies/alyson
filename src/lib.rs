pub mod db;
pub mod network;
pub mod types;
pub mod ui;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 20;

/// Errors serialized and transmitted over the network between peers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum WireError {
    #[error("invalid name")]
    InvalidName,

    #[error("handshake failed: {0}")]
    HandshakeFailed(String),

    #[error("disconnected")]
    Disconnected,

    #[error("client registration timeout")]
    ClientRegistrationTimeout,

    #[error("payload too large")]
    PayloadTooLarge,

    #[error("client not found")]
    ClientNotFound,

    #[error("room not found")]
    RoomNotFound,

    #[error("not a room member")]
    NotRoomMember,

    #[error("serialization failed")]
    SerializationFailed,

    #[error("internal server error")]
    InternalServerError,

    #[error(" server failure: {0}")]
    DatabaseFailure(String),
}

/// Node-internal operational and runtime errors (never transmitted raw across the wire).
#[derive(Debug, Clone, Error)]
pub enum AlysonError {
    #[error("io error: {0}")]
    Io(Arc<std::io::Error>),

    #[error("TCP connection failed: {0}")]
    TcpConnectionFailed(Arc<std::io::Error>),

    #[error("database error: {0}")]
    Database(String),

    #[error("serialization failed: {0}")]
    Serialization(String),

    #[error("channel failure: {0}")]
    ChannelFailure(String),

    #[error("port not available")]
    PortNotAvailable,

    #[error("wire protocol error: {0}")]
    Wire(#[from] WireError),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<std::io::Error> for AlysonError {
    fn from(e: std::io::Error) -> Self {
        AlysonError::Io(Arc::new(e))
    }
}
