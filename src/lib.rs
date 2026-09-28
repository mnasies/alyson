pub mod network;
pub mod types;
pub mod ui;

use std::sync::Arc;
use thiserror::Error;

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 20;

#[derive(Debug, Clone, Error)]
pub enum WireError {
    #[error("io error: {0}")]
    Io(Arc<std::io::Error>),

    #[error("invalid name")]
    InvalidName,

    #[error("handshake failed: {0}")]
    HandshakeFailed(String),

    #[error("TCP connection failed: {0}")]
    TcpConnectionFailed(Arc<std::io::Error>),

    #[error("disconnected")]
    Disconnected,

    #[error("port not available")]
    PortNotAvailable,

    #[error("client registration timeout")]
    ClientRegistrationTimeout,

    #[error("serialization failed")]
    SerializationFailed,

    #[error("payload too large")]
    PayloadTooLarge,

    #[error("channel not found")]
    ChannelNotFound,

    #[error("sender not found")]
    SenderNotFound,

    #[error("receiver not found")]
    ReceiverNotFound,

    #[error("client not found")]
    ClientNotFound,

    #[error("channel failure")]
    ChannelFailure,

    #[error("invalid args")]
    InvalidArgs,
}

impl From<std::io::Error> for WireError {
    fn from(e: std::io::Error) -> Self {
        WireError::Io(Arc::new(e))
    }
}
