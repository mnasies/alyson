pub mod client;
pub mod network;
pub mod ui;

#[derive(Debug)]
pub enum WireError {
    Io(std::io::Error),
    InvalidName,
    HandshakeFailed(String),
    TcpConnectionFailed(std::io::Error),
    Disconnected,
    PortNotAvailable,
    ClientRegistrationTimeout,
}

impl From<std::io::Error> for WireError {
    fn from(e: std::io::Error) -> Self {
        WireError::Io(e)
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
            WireError::ClientRegistrationTimeout => write!(f, "client registration timeout"),
        }
    }
}

impl std::error::Error for WireError {}
