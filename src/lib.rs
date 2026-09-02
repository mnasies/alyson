pub mod client;
pub mod network;

#[derive(Debug)]
pub enum WireError {
    InvalidNameError,
}
