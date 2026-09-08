use thiserror::Error;
use tokio::sync::broadcast;

pub type Result<T> = std::result::Result<T, ClientError>;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("packet error: {0}")]
    Packet(#[from] packet_datatypes::error::PacketError),

    #[error("ui error: {0}")]
    Ui(#[from] slint::PlatformError),

    #[error("audio error: {0}")]
    Audio(#[from] cpal::Error),

    #[error("audio (hound) error: {0}")]
    AudioHound(#[from] hound::Error),

    #[error("http error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("uncategorized error: {0}")]
    Generic(String),

    
    #[error("recv error: {0}")]
    Recv(#[from] broadcast::error::RecvError),

    #[error("not connected to the server")]
    Disconnected,
}