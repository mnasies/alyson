pub mod client;
pub mod network;
pub mod ui;

use std::sync::Arc;

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 20;

#[derive(Debug, Clone)]
pub enum WireError {
    Io(Arc<std::io::Error>),
    InvalidName,
    HandshakeFailed(String),
    TcpConnectionFailed(Arc<std::io::Error>),
    Disconnected,
    PortNotAvailable,
    ClientRegistrationTimeout,
    SerializationFailed,
    PayloadTooLarge,
    ChannelNotFound,
}

impl From<std::io::Error> for WireError {
    fn from(e: std::io::Error) -> Self {
        WireError::Io(Arc::new(e))
    }
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            WireError::Io(e) => write!(f, "IO error: {e}"),
            WireError::InvalidName => write!(f, "invalid name"),
            WireError::HandshakeFailed(s) => write!(f, "handshake failed: {s}"),
            WireError::Disconnected => write!(f, "client disconnected"),
            WireError::PortNotAvailable => write!(f, "port not available"),
            WireError::TcpConnectionFailed(e) => write!(f, "TCP connection failed: {e}"),
            WireError::ChannelNotFound => write!(f, "channel not found"),
            WireError::PayloadTooLarge => write!(f, "payload too large"),
            WireError::SerializationFailed => write!(f, "serialization failed"),
            WireError::ClientRegistrationTimeout => write!(f, "client registration timeout"),
        }
    }
}

impl std::error::Error for WireError {}
