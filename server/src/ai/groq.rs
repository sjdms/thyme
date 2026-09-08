use std::future::Future;
use std::pin::Pin;

use reqwest::Client;
use serde_json::json;

use crate::ai::provider::AiProvider;
use crate::error::{Result, ServerError};

/// Default model when `GEMINI_MODEL` isn't set. "Flash" specifically
/// (over "Pro") because this call sits in the hot path of a doctor
/// waiting on their transcription panel — see
/// `controllers::doctor::DoctorController::stop_recording_now` on the
/// client side, whose transcript-then-suggestions round trip a
/// clinician is actively watching. Kept configurable via env rather
/// than hardcoded so a Gemini model rename doesn't require a rebuild.
const DEFAULT_GROQ_MODEL: &str = "llama-3.1-8b-instant";

const GEMINI_API_BASE: &str = "https://api.groq.com/openai/v1";

/// Calls Google's Gemini API over the internet. Requires a
/// `GEMINI_API_KEY` (see `AiProvider::from_env` in `super`); there is no
/// sensible fallback for a missing key, since this provider is nothing
/// but an HTTP client for that API.
pub struct GroqProvider {
    api_key: String,
    model: String,
    http_client: Client,
}

impl GroqProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            http_client: Client::new(),
        }
    }

    pub fn default_model() -> &'static str {
        DEFAULT_GROQ_MODEL
    }

    async fn generate_impl(&self, prompt: &str) -> Result<String> {
        let url = format!(
            "{GEMINI_API_BASE}/responses"
        );

        let body = json!({
            "model": self.model,
            "input": prompt,
            "stream": false,
        
        });

        let response = self
            .http_client
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ServerError::Ai(format!(
                "Groq API returned {status}: {text}"
            )));
        }

        let json_response: serde_json::Value = response.json().await?;

        json_response["response"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| {
                ServerError::Ai(format!(
                    "unexpected Groq AI response shape: {json_response}"
                ))
            })
    }
}

impl AiProvider for GroqProvider {
    fn generate<'a>(&'a self, prompt: &'a str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(self.generate_impl(prompt))
    }

    fn name(&self) -> &'static str {
        "groq"
    }

    fn model(&self) -> &str {
        &self.model
    }
}