use crate::error::Result;
use crate::services::connection::Connection;

pub struct STTService {
    connection: Connection,
}

impl STTService {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    /// Sends a request to the server to get the STT API key.
    ///
    /// Blocks until the request is answered and the key is recieved.
    pub fn get_stt_api_key(&self) -> Result<String> {
        self.connection.request_stt_api_key()?;
        loop {
            match self.connection.subscribe().blocking_recv()? {
                super::connection::ServerEvent::STTKeyRecieved { key } => return Ok(key),
                super::connection::ServerEvent::Disconnected { _reason } => {
                    return Err(crate::error::ClientError::Generic("Client disconnected.".into()))
                }
                _ => {
                    continue;
                }
            }
        }
    }
}
