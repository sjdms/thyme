use std::future::Future;
use std::pin::Pin;

use reqwest::Client;
use serde_json::json;

use crate::ai::provider::AiProvider;
use crate::error::{Result, ServerError};

/// Default endpoint when `LOCAL_AI_URL` isn't set — Ollama's default
/// listen address. Kept configurable via env because "local" covers
/// anything from Ollama on the same machine to an LM Studio / vLLM
/// instance elsewhere on the clinic's own network — this provider only
/// commits to Ollama's `/api/generate` request/response shape, not to
/// where it's running.
const DEFAULT_LOCAL_AI_URL: &str = "http://localhost:11434";

/// Default model when `LOCAL_AI_MODEL` isn't set. Chosen as a small,
/// widely-available model suitable for a clinic's own hardware rather
/// than for output quality — a deployment that cares about quality is
/// expected to set `LOCAL_AI_MODEL` to whatever they've actually pulled.
const DEFAULT_LOCAL_AI_MODEL: &str = "llama3.1";

/// Calls a local (on-prem / offline) model server, so a clinic that
/// can't or won't send transcripts over the internet still gets the
/// same suggestion pipeline as `GeminiProvider`. Speaks Ollama's
/// `/api/generate` HTTP API, which is also what most local-inference
/// front ends (LM Studio's Ollama-compatibility mode, etc.) implement,
/// so this one provider covers more than just Ollama itself.
pub struct LocalProvider {
    base_url: String,
    model: String,
    http_client: Client,
}

impl LocalProvider {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            model: model.into(),
            http_client: Client::new(),
        }
    }

    pub fn default_base_url() -> &'static str {
        DEFAULT_LOCAL_AI_URL
    }

    pub fn default_model() -> &'static str {
        DEFAULT_LOCAL_AI_MODEL
    }

    async fn generate_impl(&self, prompt: &str) -> Result<String> {
        let url = format!("{}/api/generate", self.base_url.trim_end_matches('/'));

        // `format: "json"` is Ollama's equivalent of Gemini's
        // `responseMimeType: application/json` above — a hint, not a
        // guarantee, which is exactly why
        // `ai::parse_actions_from_response` still tolerates code-fenced
        // or slightly-off responses regardless of provider.
        let body = json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            "format": "json",
            "options": { "temperature": 0.1 },
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                ServerError::Ai(format!(
                    "could not reach local AI server at {url}: {e}"
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ServerError::Ai(format!(
                "local AI server returned {status}: {text}"
            )));
        }

        let json_response: serde_json::Value = response.json().await?;

        json_response["response"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| {
                ServerError::Ai(format!(
                    "unexpected local AI response shape: {json_response}"
                ))
            })
    }
}

impl AiProvider for LocalProvider {
    fn generate<'a>(&'a self, prompt: &'a str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(self.generate_impl(prompt))
    }

    fn name(&self) -> &'static str {
        "local"
    }

    fn model(&self) -> &str {
        &self.model
    }
}