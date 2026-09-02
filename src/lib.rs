pub mod client;
pub mod network;
pub mod ui;

#[derive(Debug)]
pub enum WireError {
    InvalidNameError,
}
