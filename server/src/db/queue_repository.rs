use crate::error::{Result, ServerError};
use domain::patient::PatientQueueEntry;
use sqlx::{AssertSqlSafe, Pool, Row};
use std::io;
use uuid::Uuid;

/// Data access for the "incoming patients" queue (THYME_UI_SPEC.md §40
/// "Dashboard"). Kept separate from `PatientRepository` for the same
/// reason `UserRepository` is separate from it — different lifecycle,
/// even though it reads through to the same `patients` table.
pub struct QueueRepository {
    pool: Pool<sqlx::Postgres>,
}

/// A row's worth of columns shared by every query in this file — pulled
/// out so `register`, `advance_status`, and `list_active` all build a
/// `PatientQueueEntry` the same way instead of drifting apart.
const ENTRY_COLUMNS: &str = r#"
    q.id,
    q.patient_id,
    p.legal_name,
    p.preferred_name,
    p.birth_date,
    (
        SELECT value FROM patient_identifiers pi
        WHERE pi.patient_id = p.id AND pi.identifier_type = 'MRN'
        ORDER BY pi.is_primary DESC, pi.created_at ASC
        LIMIT 1
    ) AS mrn,
    q.status,
    q.registered_by,
    q.note,
    q.created_at
"#;

impl QueueRepository {
    pub fn new(pool: Pool<sqlx::Postgres>) -> Self {
        Self { pool }
    }

    /// Registers a patient into the queue with `status = 'waiting'`.
    /// Does not check for an existing open entry for the same patient —
    /// a receptionist re-registering someone (e.g. after they stepped
    /// out) is expected to just add a fresh row rather than being
    /// blocked by a stale one.
    pub async fn register(&self, patient_id: Uuid, registered_by: &str, note: Option<&str>) -> Result<PatientQueueEntry> {
        let row = sqlx::query(
            r#"
            INSERT INTO patient_queue (patient_id, status, registered_by, note)
            VALUES ($1, 'waiting', $2, $3)
            RETURNING id
            "#,
        )
        .bind(patient_id)
        .bind(registered_by)
        .bind(note)
        .fetch_one(&self.pool)
        .await?;

        let id: Uuid = row.try_get("id")?;
        self.fetch_entry(id).await
    }

    /// Moves a queue entry to `status` (`"waiting"`, `"with_doctor"`, or
    /// `"done"`). Validation of the value itself is the caller's job
    /// (see `ThymeServer::validate_queue_status`) — this just persists
    /// whatever it's given.
    pub async fn advance_status(&self, queue_id: Uuid, status: &str) -> Result<PatientQueueEntry> {
        let result = sqlx::query(
            r#"
            UPDATE patient_queue
            SET status = $1, updated_at = now()
            WHERE id = $2
            "#,
        )
        .bind(status)
        .bind(queue_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ServerError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "queue entry not found",
            )));
        }

        self.fetch_entry(queue_id).await
    }

    /// Every queue entry not yet `"done"`, oldest first — the doctor
    /// dashboard's "incoming patients" list. `"done"` entries are
    /// excluded rather than kept-but-hidden so this stays a small,
    /// same-day working set instead of growing without bound.
    pub async fn list_active(&self) -> Result<Vec<PatientQueueEntry>> {
        let query = format!(
            r#"
            SELECT {ENTRY_COLUMNS}
            FROM patient_queue q
            JOIN patients p ON p.id = q.patient_id
            WHERE q.status <> 'done'
            ORDER BY q.created_at ASC
            "#
        );

        let rows = sqlx::query(AssertSqlSafe(query)).fetch_all(&self.pool).await?;
        rows.into_iter().map(Self::row_to_entry).collect()
    }

    /// `true` if `patient_id` currently has a `"with_doctor"` entry in
    /// the queue — the server-side half of "voice recording should only
    /// work when there is a patient at the doctor's office"
    /// (`packets::ai::ProcessTranscriptionPacket`'s doc comment). The
    /// client-side gate on the Ctrl+R shortcut itself
    /// (`ui::controllers::doctor`) is only a convenience; this is the
    /// check that's actually trusted, the same way patient search/
    /// creation aren't trusted client-side either.
    pub async fn is_with_doctor(&self, patient_id: Uuid) -> Result<bool> {
        let row = sqlx::query(
            r#"SELECT EXISTS (
                   SELECT 1 FROM patient_queue WHERE patient_id = $1 AND status = 'with_doctor'
               ) AS present"#,
        )
        .bind(patient_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.try_get("present")?)
    }

    async fn fetch_entry(&self, queue_id: Uuid) -> Result<PatientQueueEntry> {
        let query = format!(
            r#"
            SELECT {ENTRY_COLUMNS}
            FROM patient_queue q
            JOIN patients p ON p.id = q.patient_id
            WHERE q.id = $1
            "#
        );

        let row = sqlx::query(AssertSqlSafe(query)).bind(queue_id).fetch_one(&self.pool).await?;
        Self::row_to_entry(row)
    }

    fn row_to_entry(row: sqlx::postgres::PgRow) -> Result<PatientQueueEntry> {
        Ok(PatientQueueEntry {
            id: row.try_get("id")?,
            patient_id: row.try_get("patient_id")?,
            legal_name: row.try_get("legal_name")?,
            preferred_name: row.try_get("preferred_name")?,
            birth_date: row.try_get("birth_date")?,
            mrn: row.try_get("mrn")?,
            status: row.try_get("status")?,
            registered_by: row.try_get("registered_by")?,
            note: row.try_get("note")?,
            created_at: row.try_get("created_at")?,
        })
    }
}