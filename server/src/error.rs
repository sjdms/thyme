use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("packet error: {0}")]
    Packet(#[from] packet_datatypes::error::PacketError),

    #[error("database migration error: {0}")]
    DatabaseMigration(#[from] sqlx::migrate::MigrateError),

    #[error("database generic error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("http error calling AI provider: {0}")]
    Http(#[from] reqwest::Error),

    #[error("failed to parse AI provider response as JSON: {0}")]
    AiResponseParse(#[from] serde_json::Error),

    #[error("AI error: {0}")]
    Ai(String),

    #[error("DICOM error: {0}")]
    Dicom(String),
}

pub type Result<T> = std::result::Result<T, ServerError>;