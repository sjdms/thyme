use thiserror::Error;

#[derive(Debug, Error)]
pub enum PacketError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, PacketError>;
