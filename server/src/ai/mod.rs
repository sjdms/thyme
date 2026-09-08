mod gemini;
mod groq;
mod local;
pub mod prompt;
mod provider;

pub use gemini::GeminiProvider;
pub use local::LocalProvider;
pub use provider::AiProvider;

use domain::action::ThymeAction;
use domain::patient::{Patient, PatientHistory};
use serde::Deserialize;

use crate::ai::groq::GroqProvider;
use crate::error::{Result, ServerError};

/// Orchestrates the transcription-processing pipeline on top of
/// whichever `AiProvider` this server was started with. Everything
/// provider-specific (HTTP shape, auth, endpoint) lives behind that
/// trait — this type only knows how to build a prompt
/// (`prompt::build_prompt`) and parse a response
/// (`parse_actions_from_response`), so it works identically regardless
/// of whether the active provider is `GeminiProvider`, `LocalProvider`,
/// or something added later.
pub struct AiProcessor {
    provider: Box<dyn AiProvider>,
}

impl AiProcessor {
    pub fn new(provider: Box<dyn AiProvider>) -> Self {
        Self { provider }
    }

    /// Picks a backend from environment variables:
    ///
    ///   - `AI_PROVIDER` — `"gemini"` (default) or `"local"`.
    ///   - Gemini: `GEMINI_API_KEY` (required), `GEMINI_MODEL`
    ///     (defaults to `GeminiProvider::default_model()`).
    ///   - Local: `LOCAL_AI_URL` (defaults to
    ///     `LocalProvider::default_base_url()`), `LOCAL_AI_MODEL`
    ///     (defaults to `LocalProvider::default_model()`).
    ///
    /// Panics on a missing required setting, same as `STT_API_KEY` and
    /// `DATABASE_URL` already do in `ThymeServer::new` — better to fail
    /// at startup than the first time a doctor tries to record.
    pub fn from_env() -> Self {
        let backend = std::env::var("AI_PROVIDER").unwrap_or_else(|_| "gemini".to_string());

        let provider: Box<dyn AiProvider> = match backend.trim().to_lowercase().as_str() {
            "local" => {
                let base_url = std::env::var("LOCAL_AI_URL")
                    .unwrap_or_else(|_| LocalProvider::default_base_url().to_string());
                let model = std::env::var("LOCAL_AI_MODEL")
                    .unwrap_or_else(|_| LocalProvider::default_model().to_string());
                println!("AI processing: using local model \"{model}\" at {base_url}");
                Box::new(LocalProvider::new(base_url, model))
            }
            "groq" => {
                let api_key = std::env::var("GROQ_API_KEY")
                    .expect("GROQ_API_KEY must be set when AI_PROVIDER=groq");
                let model = std::env::var("GEMINI_MODEL")
                    .unwrap_or_else(|_| GroqProvider::default_model().to_string());

                println!("AI processing: using model \"{model}\"");
                Box::new(GroqProvider::new(api_key, model))
            }
            other => {
                if other != "gemini" {
                    eprintln!(
                        "Unknown AI_PROVIDER \"{other}\"; falling back to gemini. \
                        Set AI_PROVIDER=local to use a local model instead."
                    );
                }
                let api_key = std::env::var("GEMINI_API_KEY")
                    .expect("GEMINI_API_KEY must be set when AI_PROVIDER=gemini (the default)");
                let model = std::env::var("GEMINI_MODEL")
                    .unwrap_or_else(|_| GeminiProvider::default_model().to_string());
                println!("AI processing: using Gemini model \"{model}\"");
                Box::new(GeminiProvider::new(api_key, model))
            }
        };

        Self::new(provider)
    }

    pub fn provider_name(&self) -> &'static str {
        self.provider.name()
    }

    pub fn model_name(&self) -> String {
        self.provider.model().to_string()
    }

    /// Runs the full pipeline for one transcript: builds the prompt
    /// (`prompt::build_prompt`), calls the active provider, and parses
    /// its response into `ThymeAction`s
    /// (`parse_actions_from_response`).
    ///
    /// Returns the parsed actions alongside the provider's raw text
    /// response — the latter is kept only for the audit trail
    /// (`transcriptions.raw_ai_response`; see
    /// `db::ai_repository::AiRepository::record_transcription`) and is
    /// never re-parsed by any other caller.
    pub async fn process_transcription(
        &self,
        patient: &Patient,
        history: &PatientHistory,
        transcript: &str,
    ) -> Result<(Vec<ThymeAction>, String)> {
        let prompt = prompt::build_prompt(patient, history, transcript);
        let raw_response = self.provider.generate(&prompt).await?;
        let actions = parse_actions_from_response(&raw_response)?;
        Ok((actions, raw_response))
    }
}

/// Response envelope this pipeline asks the model for — see
/// `prompt::build_prompt`'s "Required output format" section:
/// `{"actions": [...]}`.
#[derive(Debug, Deserialize)]
struct ActionsEnvelope {
    actions: Vec<serde_json::Value>,
}

/// Parses a model's raw text response into `ThymeAction`s.
///
/// Tolerant of the ways a model can fall slightly short of "ONLY valid
/// JSON" despite being told so directly (hard rule 6 in
/// `prompt::build_prompt`):
///   - a ` ```json ` fenced code block wrapped around the object;
///   - a bare JSON array of actions instead of the `{"actions": [...]}`
///     envelope.
///
/// Each element of the actions array is parsed independently, and an
/// element that doesn't match any `ThymeAction` schema (wrong tag,
/// missing required field, wrong type) is dropped with a server-side
/// log line rather than failing the entire batch — one malformed
/// suggestion among several good ones shouldn't cost the doctor all of
/// them. If literally nothing parses, that is still a valid ("no
/// suggestions") outcome rather than an error: the model isn't required
/// to find something actionable every time, which is exactly the case
/// `ThymeAction::NoActionNeeded` exists to make explicit rather than
/// leaving implicit.
fn parse_actions_from_response(raw: &str) -> Result<Vec<ThymeAction>> {
    let cleaned = strip_code_fence(raw.trim());

    let value: serde_json::Value = serde_json::from_str(cleaned)
        .map_err(|e| ServerError::Ai(format!("AI response was not valid JSON ({e}): {cleaned}")))?;

    let raw_actions: Vec<serde_json::Value> = if let Some(array) = value.as_array() {
        // Fallback shape: a bare array of actions instead of the
        // `{"actions": [...]}` envelope the prompt actually asks for.
        array.clone()
    } else {
        let envelope: ActionsEnvelope = serde_json::from_value(value).map_err(|e| {
            ServerError::Ai(format!(
                "AI response JSON did not match the expected {{\"actions\": [...]}} shape: {e}"
            ))
        })?;
        envelope.actions
    };

    let mut actions = Vec::with_capacity(raw_actions.len());
    for raw_action in raw_actions {
        match serde_json::from_value::<ThymeAction>(raw_action.clone()) {
            Ok(action) => actions.push(action),
            Err(e) => {
                eprintln!("Dropping unparseable AI-suggested action ({e}): {raw_action}");
            }
        }
    }

    Ok(actions)
}

/// Strips a leading/trailing ` ```json ... ``` ` (or plain ` ``` ... ```
/// `) fence, if present. Models frequently add one despite being told
/// not to (hard rule 6 in `prompt::build_prompt`) — cheaper to strip it
/// defensively here than to rely on that instruction never being
/// ignored.
fn strip_code_fence(text: &str) -> &str {
    let text = text.trim();
    let Some(rest) = text.strip_prefix("```") else {
        return text;
    };
    let rest = rest.strip_prefix("json").unwrap_or(rest);
    let rest = rest.trim_start_matches(['\r', '\n']);
    rest.strip_suffix("```").unwrap_or(rest).trim()
}
