mod gemini;
mod groq;
mod local;
pub mod prompt;
mod provider;

pub use gemini::GeminiProvider;
pub use local::LocalProvider;
pub use provider::AiProvider;

use domain::action::{ClarificationAnswerType, ThymeAction};
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

    /// Resolves a clinician's raw answer to a `Clarification` down to
    /// the exact value `ThymeAction::resolve_clarification` should fill
    /// the blocked field with (see `prompt::build_clarification_prompt`
    /// for why this goes through the model at all rather than just
    /// using `raw_answer` verbatim).
    ///
    /// This is the one place in the pipeline where trusting the model's
    /// output at face value would be a real problem rather than just an
    /// imperfect suggestion: a `SingleChoice` clarification exists
    /// specifically to key off of an exact, known-good value (a
    /// `medication_id`-bearing name, say), and `resolve_clarification`
    /// downstream writes whatever string comes back here straight onto
    /// the chart. So — unlike `process_transcription`, which lets a
    /// malformed individual action get silently dropped
    /// (`parse_actions_from_response`) because the worst case is just a
    /// missed suggestion — a malformed or off-menu answer here is
    /// rejected outright as an error rather than passed through:
    ///
    ///   - For `ClarificationAnswerType::SingleChoice`, the resolved
    ///     answer must match one of `Clarification::choices` exactly
    ///     (byte-for-byte, after trimming). Anything else — the model
    ///     paraphrasing, inventing an option that wasn't offered, or
    ///     legitimately reporting `null` because the doctor's answer
    ///     didn't match any choice — is an error, never silently
    ///     coerced to "closest choice" or accepted verbatim.
    ///   - For `ClarificationAnswerType::Text`, the only requirement is
    ///     that the model actually returned some non-empty text rather
    ///     than `null`.
    pub async fn process_clarification_answer(
        &self,
        patient: &Patient,
        history: &PatientHistory,
        record: &crate::db::AiSuggestedAction,
        action: &ThymeAction,
        raw_answer: &str,
    ) -> Result<String> {
        let clarification = action.clarification().ok_or_else(|| {
            ServerError::Ai(format!(
                "suggestion {} has no clarification to resolve",
                record.id
            ))
        })?;

        let prompt = prompt::build_clarification_prompt(patient, history, record, action, raw_answer);
        let raw_response = self.provider.generate(&prompt).await?;
        let resolved = parse_clarification_answer(&raw_response)?;

        let resolved = match resolved {
            Some(answer) if !answer.trim().is_empty() => answer.trim().to_string(),
            _ => {
                return Err(ServerError::Ai(format!(
                    "AI could not resolve the doctor's answer to clarification for suggestion {} \
                     (raw answer: {raw_answer:?})",
                    record.id
                )));
            }
        };

        match clarification.answer_type {
            ClarificationAnswerType::SingleChoice => {
                if !clarification
                    .choices
                    .iter()
                    .any(|choice| choice == &resolved)
                {
                    return Err(ServerError::Ai(format!(
                        "AI's resolved answer {resolved:?} for suggestion {} did not exactly \
                         match any of the offered choices {:?}",
                        record.id, clarification.choices
                    )));
                }
            }
            ClarificationAnswerType::Text => {
                // Any non-empty text is acceptable — already enforced
                // above.
            }
        }

        Ok(resolved)
    }
}

/// Response envelope this pipeline asks the model for when resolving a
/// clarification — see `prompt::build_clarification_prompt`'s "Required
/// output format" section: `{"answer": ... }`, where the value is
/// either a string or JSON `null` (the model's own signal that it
/// couldn't confidently resolve the doctor's answer — see hard rules 3
/// and 4 there).
#[derive(Debug, Deserialize)]
struct ClarificationAnswerEnvelope {
    answer: Option<String>,
}

/// Parses a model's raw text response to a clarification-resolution
/// prompt. Tolerant of the same code-fence wrapping
/// `parse_actions_from_response` guards against, for the same reason —
/// models add one despite being told not to.
///
/// Returns `Ok(None)` for an explicit `{"answer": null}` (the model
/// declining to resolve, per hard rules 3/4 in
/// `prompt::build_clarification_prompt`) as a normal outcome, not a
/// parse failure. Only genuinely malformed JSON is an `Err`.
fn parse_clarification_answer(raw: &str) -> Result<Option<String>> {
    let cleaned = strip_code_fence(raw.trim());

    let envelope: ClarificationAnswerEnvelope = serde_json::from_str(cleaned).map_err(|e| {
        ServerError::Ai(format!(
            "AI clarification response was not valid JSON matching {{\"answer\": ...}} ({e}): {cleaned}"
        ))
    })?;

    Ok(envelope.answer)
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