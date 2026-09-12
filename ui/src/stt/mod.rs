mod record;
use async_recursion::async_recursion;
pub use record::AudioRecorder;
use reqwest::Client;
use serde_json::Value;

use crate::error::Result;

pub struct STTInterface {
    api_key: String,
    http_client: Client,
}

const DEEPGRAM_POST_URL: &str = "https://api.deepgram.com/v1/listen?model=nova-3&smart_format=true&language=en";

impl STTInterface {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            http_client: Client::new(),
        }
    }

    #[async_recursion]
    pub async fn transcribe_audio(&self, audio: Vec<u8>, tries: usize) -> Result<String> {

        let response = self
            .http_client
            .post(DEEPGRAM_POST_URL)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Token {}", self.api_key),
            )
            .header(reqwest::header::CONTENT_TYPE, "audio/wav")
            .body(audio.clone())
            .send()
            .await?;

        if response.status() != reqwest::StatusCode::OK {
            if tries > 0 {
                return self.transcribe_audio(audio, tries - 1).await;
            }
            
            return Err(crate::error::ClientError::Generic(format!(
                "Deepgram API error: {}", response.json().await.unwrap_or_else(|_| Value::String("Failed to parse error response".to_string())
            ))));
        }
        
        let json_response: Value = response.json().await?;

        // The doctor window's transcription side panel needs a proper
        // `Err` (not a panic) on an unexpected response shape — e.g. an
        // auth failure or rate-limit response from Deepgram won't have
        // this `results.channels[0].alternatives[0].transcript` shape,
        // and panicking here would abort the `tokio::spawn`ed task
        // silently, leaving the panel stuck on "Transcribing…" forever
        // instead of showing an error.
        json_response["results"]["channels"]
            .as_array()
            .and_then(|channels| channels.first())
            .and_then(|channel| channel["alternatives"].as_array())
            .and_then(|alternatives| alternatives.first())
            .and_then(|alternative| alternative["transcript"].as_str())
            .map(str::to_string)
            .ok_or_else(|| {
                crate::error::ClientError::Generic(format!(
                    "unexpected transcription response: {json_response}"
                ))
            })
    }
}
