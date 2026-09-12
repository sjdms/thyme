use chrono::{DateTime, Utc};
use domain::action::ThymeAction;
use sqlx::{Pool, Row};
use uuid::Uuid;

use crate::error::{Result, ServerError};

/// One row of `device_studies`
/// (`migrations/0017_create_device_studies_tables.sql`) — one DICOM
/// instance received from a device, whether or not it's been matched
/// to a patient yet. See that migration's header for how this fits
/// alongside the AI pipeline's staging tables.
#[derive(Debug, Clone)]
pub struct DeviceStudy {
    pub id: Uuid,
    pub patient_id: Option<Uuid>,
    pub source_patient_id: Option<String>,
    pub source_patient_name: Option<String>,
    pub device_ae_title: String,
    pub modality: Option<String>,
    pub sop_class_uid: String,
    pub sop_instance_uid: String,
    pub study_instance_uid: Option<String>,
    /// `"unmatched"` | `"pending_review"` | `"applied"` | `"rejected"`.
    pub status: String,
    pub received_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// One row of `oct_images`, joined back to its study's `patient_id` —
/// what a client showing a patient's imaging needs, and what
/// `server::dicom::oct` wrote when the image was received.
#[derive(Debug, Clone)]
pub struct OctImageRecord {
    pub id: Uuid,
    pub study_id: Uuid,
    pub eye: Option<String>,
    pub file_path: String,
    pub rows: Option<i32>,
    pub columns: Option<i32>,
    pub number_of_frames: Option<i32>,
    pub acquisition_datetime: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Everything needed to create a `device_studies` row — one per
/// incoming C-STORE, before it's known whether the sender's Patient ID
/// resolves to anyone (see `server::dicom::patient_match`).
pub struct NewDeviceStudy<'a> {
    pub source_patient_id: Option<&'a str>,
    pub source_patient_name: Option<&'a str>,
    pub device_ae_title: &'a str,
    pub modality: Option<&'a str>,
    pub sop_class_uid: &'a str,
    pub sop_instance_uid: &'a str,
    pub study_instance_uid: Option<&'a str>,
}

/// One row of `oct_images`. Pixel data itself lives on disk
/// (`server::dicom::oct` picks the path); this is only ever the
/// metadata pointing at it.
pub struct NewOctImage<'a> {
    pub eye: Option<&'a str>,
    pub file_path: &'a str,
    pub rows: Option<i32>,
    pub columns: Option<i32>,
    pub number_of_frames: Option<i32>,
    pub acquisition_datetime: Option<DateTime<Utc>>,
}

/// One row of `device_suggested_actions` — the device-sourced
/// equivalent of `db::ai_repository::AiSuggestedAction`, minus the
/// transcript-specific fields that table carries and this one has no
/// use for.
#[derive(Debug, Clone)]
pub struct DeviceSuggestedAction {
    pub id: Uuid,
    pub study_id: Uuid,
    pub patient_id: Uuid,
    pub action_type: String,
    pub payload_json: serde_json::Value,
    pub description: String,
    /// `"pending"` | `"applied"` | `"rejected"`.
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

impl DeviceSuggestedAction {
    /// Mirrors `AiSuggestedAction::to_action` — re-parses `payload_json`
    /// back into the `ThymeAction` it was stored from.
    pub fn to_action(&self) -> Result<ThymeAction> {
        serde_json::from_value(self.payload_json.clone()).map_err(|e| {
            ServerError::Dicom(format!(
                "stored device suggestion {} no longer matches a known action shape: {e}",
                self.id
            ))
        })
    }
}

/// Data access for `server::dicom`'s two staging tables —
/// `device_studies`/`oct_images`/`device_suggested_actions`
/// (`migrations/0017_create_device_studies_tables.sql`). Kept separate
/// from `AiRepository` even though the `device_suggested_actions` ->
/// `HistoryRepository::apply_action` flow otherwise mirrors
/// `ai_suggested_actions` -> `apply_action` exactly: a device reading
/// has no transcript and no AI provider behind it, and forcing it
/// through `AiRepository`'s NOT NULL `transcription_id` would
/// misrepresent where the data came from (see the migration's header).
pub struct DeviceRepository {
    pool: Pool<sqlx::Postgres>,
}

impl DeviceRepository {
    pub fn new(pool: Pool<sqlx::Postgres>) -> Self {
        Self { pool }
    }

    /// Records one incoming DICOM instance. Called once per C-STORE,
    /// before patient matching — `patient_id` starts `NULL` and
    /// `status` starts `"unmatched"`; `mark_matched`/`mark_unresolvable`
    /// below move it on from there.
    ///
    /// A device that resends the same SOP Instance UID (a retried
    /// C-STORE after a dropped association, say) hits
    /// `device_studies_sop_instance_unique` — treated as "already
    /// recorded" rather than an error, so `server::dicom::association`
    /// can still answer the resend with a normal C-STORE-RSP Success
    /// instead of failing the whole association over a duplicate.
    pub async fn record_study(&self, new_study: NewDeviceStudy<'_>) -> Result<DeviceStudy> {
        let row = sqlx::query(
            r#"
            INSERT INTO device_studies
                (source_patient_id, source_patient_name, device_ae_title, modality,
                 sop_class_uid, sop_instance_uid, study_instance_uid)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (sop_instance_uid) DO UPDATE
                -- No real column changes on a resend; this no-op SET is
                -- just what lets RETURNING hand back the existing row
                -- (a plain DO NOTHING wouldn't return anything).
                SET sop_instance_uid = EXCLUDED.sop_instance_uid
            RETURNING id, patient_id, source_patient_id, source_patient_name, device_ae_title,
                      modality, sop_class_uid, sop_instance_uid, study_instance_uid, status,
                      received_at, resolved_at
            "#,
        )
        .bind(new_study.source_patient_id)
        .bind(new_study.source_patient_name)
        .bind(new_study.device_ae_title)
        .bind(new_study.modality)
        .bind(new_study.sop_class_uid)
        .bind(new_study.sop_instance_uid)
        .bind(new_study.study_instance_uid)
        .fetch_one(&self.pool)
        .await?;

        Self::row_to_study(row)
    }

    /// Links a study to the patient it was matched to
    /// (`server::dicom::patient_match`) and moves its status on to
    /// `new_status` (`"pending_review"` when a refraction suggestion is
    /// about to be attached, `"applied"` for an OCT-only study with
    /// nothing further to review — see this repository's doc comment).
    pub async fn mark_matched(&self, study_id: Uuid, patient_id: Uuid, new_status: &str) -> Result<()> {
        let resolved_at = (new_status != "pending_review").then(Utc::now);

        sqlx::query(
            r#"UPDATE device_studies SET patient_id = $1, status = $2, resolved_at = $3 WHERE id = $4"#,
        )
        .bind(patient_id)
        .bind(new_status)
        .bind(resolved_at)
        .bind(study_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Records an OCT image against an already-matched study.
    pub async fn insert_oct_image(&self, study_id: Uuid, image: NewOctImage<'_>) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO oct_images
                (study_id, eye, file_path, rows, columns, number_of_frames, acquisition_datetime)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#,
        )
        .bind(study_id)
        .bind(image.eye)
        .bind(image.file_path)
        .bind(image.rows)
        .bind(image.columns)
        .bind(image.number_of_frames)
        .bind(image.acquisition_datetime)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.try_get("id")?)
    }

    /// Stages one `ThymeAction` extracted from a refraction study
    /// (`server::dicom::refraction`) — always `"pending"`, unlike
    /// `AiRepository::record_suggested_action`, which lets a narrow set
    /// of low-risk action types through immediately per
    /// `ThymeAction::auto_apply`. A device reading gets no such
    /// override: which *patient* it belongs to was decided by matching
    /// a free-text identifier the device sent (`patient_match`), which
    /// is a weaker guarantee than a clinician actively looking at that
    /// patient's own chart while dictating, so every device-sourced
    /// reading always waits for an explicit accept.
    pub async fn record_suggested_action(
        &self,
        study_id: Uuid,
        patient_id: Uuid,
        action: &ThymeAction,
    ) -> Result<DeviceSuggestedAction> {
        let payload_json = serde_json::to_value(action)?;
        let description = action.describe();
        let action_type = action.action_type();

        let row = sqlx::query(
            r#"
            INSERT INTO device_suggested_actions
                (study_id, patient_id, action_type, payload_json, description)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, created_at
            "#,
        )
        .bind(study_id)
        .bind(patient_id)
        .bind(action_type)
        .bind(&payload_json)
        .bind(&description)
        .fetch_one(&self.pool)
        .await?;

        Ok(DeviceSuggestedAction {
            id: row.try_get("id")?,
            study_id,
            patient_id,
            action_type: action_type.to_string(),
            payload_json,
            description,
            status: "pending".to_string(),
            created_at: row.try_get("created_at")?,
            resolved_at: None,
        })
    }

    /// Fetches a suggestion by id, but only if it's still `"pending"` —
    /// mirrors `AiRepository::get_pending`'s narrowing, for the same
    /// reason: a suggestion already resolved (accepted or rejected)
    /// can never be resolved a second time.
    pub async fn get_pending(&self, suggestion_id: Uuid) -> Result<Option<DeviceSuggestedAction>> {
        let row = sqlx::query(
            r#"SELECT id, study_id, patient_id, action_type, payload_json, description, status, created_at, resolved_at
               FROM device_suggested_actions
               WHERE id = $1 AND status = 'pending'"#,
        )
        .bind(suggestion_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::row_to_suggested_action).transpose()
    }

    /// Every still-`"pending"` suggestion for one patient, oldest
    /// first — used alongside `list_oct_images_for_patient` to answer
    /// `packets::device::DevicePatientItemsRequest`.
    pub async fn list_pending_for_patient(&self, patient_id: Uuid) -> Result<Vec<DeviceSuggestedAction>> {
        let rows = sqlx::query(
            r#"SELECT id, study_id, patient_id, action_type, payload_json, description, status, created_at, resolved_at
               FROM device_suggested_actions
               WHERE patient_id = $1 AND status = 'pending'
               ORDER BY created_at ASC"#,
        )
        .bind(patient_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Self::row_to_suggested_action).collect()
    }

    /// Marks a suggestion resolved (`"applied"` or `"rejected"`).
    /// Actually applying the underlying `ThymeAction`
    /// (`HistoryRepository::apply_action`, with `source = "device"`) is
    /// always the caller's job, done separately — mirrors
    /// `AiRepository::resolve` exactly.
    pub async fn resolve(&self, suggestion_id: Uuid, status: &str) -> Result<()> {
        sqlx::query(r#"UPDATE device_suggested_actions SET status = $1, resolved_at = now() WHERE id = $2"#)
            .bind(status)
            .bind(suggestion_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Every OCT image filed for one patient, newest first — the other
    /// half of what answers `packets::device::DevicePatientItemsRequest`
    /// alongside `list_pending_for_patient`. Joins through
    /// `device_studies` since `oct_images` itself only carries a
    /// `study_id`, not a `patient_id` directly (see the migration).
    pub async fn list_oct_images_for_patient(&self, patient_id: Uuid) -> Result<Vec<OctImageRecord>> {
        let rows = sqlx::query(
            r#"SELECT oi.id, oi.study_id, oi.eye, oi.file_path, oi.rows, oi.columns,
                      oi.number_of_frames, oi.acquisition_datetime, oi.created_at
               FROM oct_images oi
               JOIN device_studies ds ON ds.id = oi.study_id
               WHERE ds.patient_id = $1
               ORDER BY oi.created_at DESC"#,
        )
        .bind(patient_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Self::row_to_oct_image).collect()
    }

    /// Fetches one OCT image's metadata by id — the lookup behind
    /// `packets::device::OctImageDataRequest`, whose handler
    /// (`server::server::ThymeServer::read_oct_image`) reads the actual
    /// pixel bytes from `file_path` afterwards.
    pub async fn get_oct_image(&self, image_id: Uuid) -> Result<Option<OctImageRecord>> {
        let row = sqlx::query(
            r#"SELECT id, study_id, eye, file_path, rows, columns, number_of_frames,
                      acquisition_datetime, created_at
               FROM oct_images WHERE id = $1"#,
        )
        .bind(image_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::row_to_oct_image).transpose()
    }

    fn row_to_study(row: sqlx::postgres::PgRow) -> Result<DeviceStudy> {
        Ok(DeviceStudy {
            id: row.try_get("id")?,
            patient_id: row.try_get("patient_id")?,
            source_patient_id: row.try_get("source_patient_id")?,
            source_patient_name: row.try_get("source_patient_name")?,
            device_ae_title: row.try_get("device_ae_title")?,
            modality: row.try_get("modality")?,
            sop_class_uid: row.try_get("sop_class_uid")?,
            sop_instance_uid: row.try_get("sop_instance_uid")?,
            study_instance_uid: row.try_get("study_instance_uid")?,
            status: row.try_get("status")?,
            received_at: row.try_get("received_at")?,
            resolved_at: row.try_get("resolved_at")?,
        })
    }

    fn row_to_suggested_action(row: sqlx::postgres::PgRow) -> Result<DeviceSuggestedAction> {
        Ok(DeviceSuggestedAction {
            id: row.try_get("id")?,
            study_id: row.try_get("study_id")?,
            patient_id: row.try_get("patient_id")?,
            action_type: row.try_get("action_type")?,
            payload_json: row.try_get("payload_json")?,
            description: row.try_get("description")?,
            status: row.try_get("status")?,
            created_at: row.try_get("created_at")?,
            resolved_at: row.try_get("resolved_at")?,
        })
    }

    fn row_to_oct_image(row: sqlx::postgres::PgRow) -> Result<OctImageRecord> {
        Ok(OctImageRecord {
            id: row.try_get("id")?,
            study_id: row.try_get("study_id")?,
            eye: row.try_get("eye")?,
            file_path: row.try_get("file_path")?,
            rows: row.try_get("rows")?,
            columns: row.try_get("columns")?,
            number_of_frames: row.try_get("number_of_frames")?,
            acquisition_datetime: row.try_get("acquisition_datetime")?,
            created_at: row.try_get("created_at")?,
        })
    }
}
