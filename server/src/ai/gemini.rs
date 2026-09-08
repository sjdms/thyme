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
const DEFAULT_GEMINI_MODEL: &str = "gemini-3.5-flash-lite";

const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

/// Calls Google's Gemini API over the internet. Requires a
/// `GEMINI_API_KEY` (see `AiProvider::from_env` in `super`); there is no
/// sensible fallback for a missing key, since this provider is nothing
/// but an HTTP client for that API.
pub struct GeminiProvider {
    api_key: String,
    model: String,
    http_client: Client,
}

impl GeminiProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            http_client: Client::new(),
        }
    }

    pub fn default_model() -> &'static str {
        DEFAULT_GEMINI_MODEL
    }

    async fn generate_impl(&self, prompt: &str) -> Result<String> {
        let url = format!(
            "{GEMINI_API_BASE}/{}:generateContent?key={}",
            self.model, self.api_key
        );

        // `responseMimeType: application/json` asks Gemini itself to
        // constrain its output to valid JSON — belt-and-suspenders on
        // top of the prompt's own formatting instructions
        // (`ai::prompt::build_prompt`), not a replacement for the
        // tolerant parsing `ai::parse_actions_from_response` still
        // does, since this is a hint the API can still fall short of
        // (e.g. on a model/version that doesn't support it).
        let body = json!({
            "contents": [{
                "parts": [{ "text": prompt }]
            }],
            "generationConfig": {
                "temperature": 0.1,
                "responseMimeType": "application/json",
            }
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ServerError::Ai(format!(
                "Gemini API returned {status}: {text}"
            )));
        }

        let json_response: serde_json::Value = response.json().await?;

        json_response["candidates"]
            .as_array()
            .and_then(|candidates| candidates.first())
            .and_then(|candidate| candidate["content"]["parts"].as_array())
            .and_then(|parts| parts.first())
            .and_then(|part| part["text"].as_str())
            .map(str::to_string)
            .ok_or_else(|| {
                ServerError::Ai(format!(
                    "unexpected Gemini response shape: {json_response}"
                ))
            })
    }
}

impl AiProvider for GeminiProvider {
    fn generate<'a>(&'a self, prompt: &'a str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(self.generate_impl(prompt))
    }

    fn name(&self) -> &'static str {
        "gemini"
    }

    fn model(&self) -> &str {
        &self.model
    }
}