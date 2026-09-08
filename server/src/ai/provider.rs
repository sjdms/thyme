use std::future::Future;
use std::pin::Pin;

use crate::error::Result;

/// A source of text completions for the transcription-processing
/// pipeline (`super::AiProcessor`). This is the one seam between "we
/// have a prompt and need a model to answer it" and "which model, and
/// how do we reach it" — everything above this trait (prompt
/// construction, response parsing into `ThymeAction`s, persistence)
/// is provider-agnostic, and everything below it is free to be a cloud
/// API, a local inference server, or (in tests) a canned string.
///
/// Implementations are expected to return the model's raw text output
/// with no post-processing — stripping code fences, extracting JSON,
/// etc. is `super::parse_actions_from_response`'s job, not this trait's,
/// so that logic isn't duplicated per provider.
///
/// Written as a hand-rolled `Pin<Box<dyn Future>>` return (the same
/// desugaring the `async-trait` crate generates) rather than a native
/// `async fn` in the trait, specifically so `AiProcessor` can hold this
/// as `Box<dyn AiProvider>` and pick a backend at startup based on
/// `AI_PROVIDER` — native `async fn`s in traits aren't object-safe.
pub trait AiProvider: Send + Sync {
    /// Sends `prompt` to the model and returns its raw text response.
    fn generate<'a>(&'a self, prompt: &'a str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;

    /// A short label for this provider, used only for audit logging
    /// (`transcriptions.ai_provider` — see
    /// `db::ai_repository::AiRepository::record_transcription`), not
    /// for any behavioral branching.
    fn name(&self) -> &'static str;

    /// The specific model in use, similarly logging-only
    /// (`transcriptions.ai_model`).
    fn model(&self) -> &str;
}