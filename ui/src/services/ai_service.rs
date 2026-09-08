//! AI transcription-processing business logic on top of
//! `services::connection`, plus the shared Rust<->Slint suggestion-row
//! conversion `controllers::doctor` builds on — mirrors
//! `services::queue_service`'s shape.

use packets::ai::{AppliedActionResult, SuggestedActionRecord};
use slint::{ModelRc, VecModel};
use tokio::sync::broadcast;

use crate::error::Result;
use crate::services::connection::{Connection, ServerEvent};
use crate::SuggestionRow;

/// An AI-pipeline-relevant update, filtered out of the connection's
/// general `ServerEvent` stream so `controllers::doctor` doesn't need
/// to know about event kinds it doesn't care about — mirrors
/// `services::queue_service::QueueUpdate`.
#[derive(Debug, Clone)]
pub enum AiUpdate {
    /// Every action the AI proposed from one transcript (possibly
    /// empty — see `domain::action::ThymeAction::NoActionNeeded`).
    SuggestionsReady {
        transcription_id: String,
        patient_id: String,
        suggestions: Vec<SuggestedActionRecord>,
    },
    ProcessingFailed(String),
    SuggestionApplied { suggestion_id: String, summary: String },
    SuggestionApplyFailed { suggestion_id: String, message: String },
    SuggestionRejected { suggestion_id: String },
    SuggestionRejectFailed { suggestion_id: String, message: String },
    ClarificationResolved { suggestion_id: String, description: String, status: String },
    ClarificationResolveFailed { suggestion_id: String, message: String },
    AllApplied { transcription_id: String, results: Vec<AppliedActionResult> },
    AllApplyFailed { transcription_id: String, message: String },
    Disconnected,
}

pub struct AiService {
    connection: Connection,
}

impl AiService {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    /// Sends a finished transcript for AI processing. See
    /// `Connection::process_transcription`.
    pub fn process_transcription(
        &self,
        patient_id: impl Into<String>,
        transcript: impl Into<String>,
    ) -> Result<()> {
        self.connection.process_transcription(patient_id, transcript)
    }

    /// Accepts one still-pending suggestion. See
    /// `Connection::apply_suggestion`.
    pub fn apply_suggestion(&self, suggestion_id: impl Into<String>) -> Result<()> {
        self.connection.apply_suggestion(suggestion_id)
    }

    /// Dismisses one still-pending suggestion. See
    /// `Connection::reject_suggestion`.
    pub fn reject_suggestion(&self, suggestion_id: impl Into<String>) -> Result<()> {
        self.connection.reject_suggestion(suggestion_id)
    }

    /// Answers a still-`"needs_clarification"` suggestion. See
    /// `Connection::resolve_clarification`.
    pub fn resolve_clarification(
        &self,
        suggestion_id: impl Into<String>,
        answer: impl Into<String>,
    ) -> Result<()> {
        self.connection.resolve_clarification(suggestion_id, answer)
    }

    /// Accepts every still-pending suggestion from one transcript. See
    /// `Connection::apply_all_suggestions`.
    pub fn apply_all_suggestions(&self, transcription_id: impl Into<String>) -> Result<()> {
        self.connection.apply_all_suggestions(transcription_id)
    }

    /// Subscribes to AI-pipeline updates. Spawns a small adapter task
    /// that filters the connection's general event stream down to just
    /// the variants this service cares about — mirrors
    /// `services::queue_service::QueueService::subscribe`.
    pub fn subscribe(&self) -> broadcast::Receiver<AiUpdate> {
        let mut source = self.connection.subscribe();
        let (tx, rx) = broadcast::channel(32);

        tokio::spawn(async move {
            loop {
                match source.recv().await {
                    Ok(ServerEvent::AiSuggestionsReady { transcription_id, patient_id, suggestions }) => {
                        let _ = tx.send(AiUpdate::SuggestionsReady { transcription_id, patient_id, suggestions });
                    }
                    Ok(ServerEvent::AiProcessingFailed { message }) => {
                        let _ = tx.send(AiUpdate::ProcessingFailed(message));
                    }
                    Ok(ServerEvent::SuggestionApplied { suggestion_id, summary }) => {
                        let _ = tx.send(AiUpdate::SuggestionApplied { suggestion_id, summary });
                    }
                    Ok(ServerEvent::SuggestionApplyFailed { suggestion_id, message }) => {
                        let _ = tx.send(AiUpdate::SuggestionApplyFailed { suggestion_id, message });
                    }
                    Ok(ServerEvent::SuggestionRejected { suggestion_id }) => {
                        let _ = tx.send(AiUpdate::SuggestionRejected { suggestion_id });
                    }
                    Ok(ServerEvent::SuggestionRejectFailed { suggestion_id, message }) => {
                        let _ = tx.send(AiUpdate::SuggestionRejectFailed { suggestion_id, message });
                    }
                    Ok(ServerEvent::ClarificationResolved { suggestion_id, description, status }) => {
                        let _ = tx.send(AiUpdate::ClarificationResolved { suggestion_id, description, status });
                    }
                    Ok(ServerEvent::ClarificationResolveFailed { suggestion_id, message }) => {
                        let _ = tx.send(AiUpdate::ClarificationResolveFailed { suggestion_id, message });
                    }
                    Ok(ServerEvent::AllSuggestionsApplied { transcription_id, results }) => {
                        let _ = tx.send(AiUpdate::AllApplied { transcription_id, results });
                    }
                    Ok(ServerEvent::AllSuggestionsApplyFailed { transcription_id, message }) => {
                        let _ = tx.send(AiUpdate::AllApplyFailed { transcription_id, message });
                    }
                    Ok(ServerEvent::Disconnected { .. }) => {
                        let _ = tx.send(AiUpdate::Disconnected);
                        break;
                    }
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        rx
    }
}

/// Builds the suggestion-panel row model from a list of suggestions.
/// Shared the same way `services::queue_service::to_queue_rows` is.
pub fn to_suggestion_rows(suggestions: &[SuggestedActionRecord]) -> ModelRc<SuggestionRow> {
    let rows: Vec<SuggestionRow> = suggestions.iter().map(to_suggestion_row).collect();
    ModelRc::new(VecModel::from(rows))
}

fn to_suggestion_row(record: &SuggestedActionRecord) -> SuggestionRow {
    SuggestionRow {
        suggestion_id: record.suggestion_id.clone().into(),
        action_type: record.action_type.clone().into(),
        description: record.description.clone().into(),
        status: record.status.clone().into(),
        status_label: status_label(&record.status).into(),
        clarification_message: record.clarification_message.clone().into(),
        clarification_answer_type: record.clarification_answer_type.clone().into(),
        clarification_choices: ModelRc::new(VecModel::from(
            record
                .clarification_choices
                .iter()
                .map(|c| c.clone().into())
                .collect::<Vec<slint::SharedString>>(),
        )),
    }
}

fn status_label(status: &str) -> String {
    match status {
        "pending" => "Needs review".to_string(),
        "needs_clarification" => "Needs clarification".to_string(),
        "applied" => "Added to chart".to_string(),
        "rejected" => "Dismissed".to_string(),
        other => other.to_string(),
    }
}
