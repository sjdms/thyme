use chrono::{DateTime, Utc};
use domain::action::ThymeAction;
use sqlx::{Pool, Row};
use uuid::Uuid;

use crate::error::{Result, ServerError};

/// One row of `ai_suggested_actions`
/// (`migrations/0014_create_ai_tables.sql`). This is the *staging* side
/// of the AI pipeline — see `HistoryRepository`'s doc comment for how
/// the two repositories divide the work: this is a proposal that may or
/// may not ever become a chart entry, not the chart entry itself.
#[derive(Debug, Clone)]
pub struct AiSuggestedAction {
    pub id: Uuid,
    pub transcription_id: Uuid,
    pub patient_id: Uuid,
    pub action_type: String,
    pub payload_json: serde_json::Value,
    pub description: String,
    /// `"pending"` | `"applied"` | `"rejected"`.
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    /// The original dictated transcript this suggestion was proposed
    /// from (`transcriptions.transcript`, joined in on
    /// `transcription_id`). Needed by
    /// `ai::prompt::build_clarification_prompt` so a clarification
    /// answer can be interpreted against the same context the original
    /// suggestion was.
    pub transcript: String,
}

impl AiSuggestedAction {
    /// Re-parses `payload_json` back into the typed action it was
    /// stored from. The only way this can fail is if `ThymeAction`'s
    /// shape changed between when this row was written and now (a
    /// variant renamed or removed) — treated as a normal `Result` error
    /// rather than a panic, so a stale suggestion just fails to apply
    /// instead of taking the connection down.
    pub fn to_action(&self) -> Result<ThymeAction> {
        serde_json::from_value(self.payload_json.clone()).map_err(|e| {
            ServerError::Ai(format!(
                "stored suggestion {} no longer matches a known action shape: {e}",
                self.id
            ))
        })
    }
}

/// Data access for the AI pipeline's staging tables — `transcriptions`
/// and `ai_suggested_actions` (`migrations/0014_create_ai_tables.sql`).
///
/// Kept separate from `HistoryRepository` the same way `QueueRepository`
/// is separate from `PatientRepository`: this repository owns
/// everything up to "the model proposed X"; `HistoryRepository::apply_action`
/// is the *only* place X actually lands on a patient's chart, whether
/// that happens through an explicit clinician review
/// (`ThymeServer::apply_suggestion`/`apply_all_pending`) or through the
/// narrow `ThymeAction::auto_apply` override
/// (`ThymeServer::process_transcription`).
pub struct AiRepository {
    pool: Pool<sqlx::Postgres>,
}

impl AiRepository {
    pub fn new(pool: Pool<sqlx::Postgres>) -> Self {
        Self { pool }
    }

    /// Records one transcript plus everything the active `AiProvider`
    /// said about it, for the audit trail
    /// (`transcriptions.raw_ai_response`) — independent of how many (if
    /// any) actions were actually parsed out of that response. Returns
    /// the new row's id, which every `ai_suggested_actions` row from
    /// this transcript is linked to.
    pub async fn record_transcription(
        &self,
        patient_id: Uuid,
        transcript: &str,
        ai_provider: &str,
        ai_model: &str,
        raw_ai_response: &str,
    ) -> Result<Uuid> {
        let row = sqlx::query(
            r#"INSERT INTO transcriptions (patient_id, transcript, ai_provider, ai_model, raw_ai_response)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING id"#,
        )
        .bind(patient_id)
        .bind(transcript)
        .bind(ai_provider)
        .bind(ai_model)
        .bind(raw_ai_response)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.try_get("id")?)
    }

    /// Records one action the model proposed from `transcription_id`.
    /// `status` is decided by the caller
    /// (`ThymeServer::process_transcription`, from
    /// `ThymeAction::auto_apply`) — `"pending"` for anything that needs
    /// a clinician's explicit review, or already `"applied"` for the
    /// handful of low-risk action types this feature's AI-policy
    /// override lets through immediately. This method only ever
    /// persists the row; it never itself decides or applies anything.
    pub async fn record_suggested_action(
        &self,
        transcription_id: Uuid,
        patient_id: Uuid,
        action: &ThymeAction,
        status: &str,
        transcript: &str,
    ) -> Result<AiSuggestedAction> {
        let payload_json = serde_json::to_value(action)?;
        let description = action.describe();
        let action_type = action.action_type();
        let resolved_at = (status != "pending").then(Utc::now);

        let row = sqlx::query(
            r#"INSERT INTO ai_suggested_actions
                   (transcription_id, patient_id, action_type, payload_json, description, status, resolved_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING id, created_at"#,
        )
        .bind(transcription_id)
        .bind(patient_id)
        .bind(action_type)
        .bind(&payload_json)
        .bind(&description)
        .bind(status)
        .bind(resolved_at)
        .fetch_one(&self.pool)
        .await?;

        Ok(AiSuggestedAction {
            id: row.try_get("id")?,
            transcription_id,
            patient_id,
            action_type: action_type.to_string(),
            payload_json,
            description,
            status: status.to_string(),
            created_at: row.try_get("created_at")?,
            resolved_at,
            transcript: transcript.to_string(),
        })
    }

    /// Fetches a suggestion by id, but only if it's still `"pending"` —
    /// the only state `apply_suggestion`/`reject_suggestion` handling
    /// (see `server::server::ThymeServer`) is ever allowed to act on,
    /// so a suggestion already resolved (by a clinician or by
    /// auto-apply) can never be resolved a second time.
    pub async fn get_pending(&self, suggestion_id: Uuid) -> Result<Option<AiSuggestedAction>> {
        let row = sqlx::query(
            r#"SELECT a.id, a.transcription_id, a.patient_id, a.action_type, a.payload_json, a.description, a.status, a.created_at, a.resolved_at, t.transcript
               FROM ai_suggested_actions a
               JOIN transcriptions t ON t.id = a.transcription_id
               WHERE a.id = $1 AND a.status = 'pending'"#,
        )
        .bind(suggestion_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::row_to_action).transpose()
    }

    /// Fetches a suggestion by id, but only if it's still
    /// `"needs_clarification"` — the only state
    /// `ThymeServer::resolve_clarification` is ever allowed to act on,
    /// mirroring `get_pending`'s narrowing for `apply_suggestion`/
    /// `reject_suggestion`. A suggestion whose `Clarification` has
    /// already been answered (or that was never asking for one) can
    /// never be "resolved" a second time through this path.
    pub async fn get_needing_clarification(&self, suggestion_id: Uuid) -> Result<Option<AiSuggestedAction>> {
        let row = sqlx::query(
            r#"SELECT a.id, a.transcription_id, a.patient_id, a.action_type, a.payload_json, a.description, a.status, a.created_at, a.resolved_at, t.transcript
               FROM ai_suggested_actions a
               JOIN transcriptions t ON t.id = a.transcription_id
               WHERE a.id = $1 AND a.status = 'needs_clarification'"#,
        )
        .bind(suggestion_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::row_to_action).transpose()
    }

    /// Every still-`"pending"` suggestion from one transcript, oldest
    /// first — backs the "Approve all" bulk-review action
    /// (`packets::ai::ApplyAllSuggestedActionsRequest`).
    pub async fn list_pending_for_transcription(
        &self,
        transcription_id: Uuid,
    ) -> Result<Vec<AiSuggestedAction>> {
        let rows = sqlx::query(
            r#"SELECT a.id, a.transcription_id, a.patient_id, a.action_type, a.payload_json, a.description, a.status, a.created_at, a.resolved_at, t.transcript
               FROM ai_suggested_actions a
               JOIN transcriptions t ON t.id = a.transcription_id
               WHERE a.transcription_id = $1 AND a.status = 'pending'
               ORDER BY a.created_at ASC"#,
        )
        .bind(transcription_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Self::row_to_action).collect()
    }

    /// Folds a clinician's answer into a suggestion that was
    /// `"needs_clarification"`: overwrites its stored `payload_json`
    /// and `description` with the freshly-resolved action's (see
    /// `domain::action::ThymeAction::resolve_clarification`), and moves
    /// `status` on to `new_status` — ordinarily `"pending"`, same as an
    /// action that never needed to ask anything, or `"applied"` in the
    /// rare case the now-fully-specified action turned out to be one of
    /// the auto-applied types after all (see
    /// `server::server::ThymeServer::resolve_clarification`, the only
    /// caller, for how that's decided). Always transitions away from
    /// `"needs_clarification"` — never call this on a suggestion that
    /// wasn't fetched via `get_needing_clarification` first.
    pub async fn resolve_clarification(
        &self,
        suggestion_id: Uuid,
        payload_json: &serde_json::Value,
        description: &str,
        new_status: &str,
    ) -> Result<()> {
        let resolved_at = (new_status != "pending").then(Utc::now);

        sqlx::query(
            r#"UPDATE ai_suggested_actions
               SET payload_json = $1, description = $2, status = $3, resolved_at = $4
               WHERE id = $5"#,
        )
        .bind(payload_json)
        .bind(description)
        .bind(new_status)
        .bind(resolved_at)
        .bind(suggestion_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Marks a suggestion resolved (`"applied"` or `"rejected"`),
    /// stamping `resolved_at`. Actually applying the underlying
    /// `ThymeAction` to the patient's chart
    /// (`HistoryRepository::apply_action`) is always the caller's job,
    /// done separately — this method only ever changes
    /// `ai_suggested_actions.status` itself.
    pub async fn resolve(&self, suggestion_id: Uuid, status: &str) -> Result<()> {
        sqlx::query(r#"UPDATE ai_suggested_actions SET status = $1, resolved_at = now() WHERE id = $2"#)
            .bind(status)
            .bind(suggestion_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    fn row_to_action(row: sqlx::postgres::PgRow) -> Result<AiSuggestedAction> {
        Ok(AiSuggestedAction {
            id: row.try_get("id")?,
            transcription_id: row.try_get("transcription_id")?,
            patient_id: row.try_get("patient_id")?,
            action_type: row.try_get("action_type")?,
            payload_json: row.try_get("payload_json")?,
            description: row.try_get("description")?,
            status: row.try_get("status")?,
            created_at: row.try_get("created_at")?,
            resolved_at: row.try_get("resolved_at")?,
            transcript: row.try_get("transcript")?,
        })
    }
}