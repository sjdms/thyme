use crate::error::Result;
use chrono::NaiveDate;
use domain::patient::{Patient, PatientSearchResult};
use sqlx::{Pool, Row};

/// Data access for patients. Kept separate from `UserRepository` because
/// the two model very different lifecycles (identity/auth vs. clinical
/// records) even though both currently live in the same database.
pub struct PatientRepository {
    pool: Pool<sqlx::Postgres>,
}

/// Validated input for `PatientRepository::create`. Building/validating
/// this is `ThymeServer::validate_new_patient`'s job (server.rs); this
/// repository only ever sees already-normalized data.
pub struct NewPatient {
    pub legal_name: String,
    pub preferred_name: Option<String>,
    pub birth_date: Option<NaiveDate>,
    pub sex: Option<String>,
    pub phone: Option<String>,
}

impl PatientRepository {
    pub fn new(pool: Pool<sqlx::Postgres>) -> Self {
        Self { pool }
    }

    /// Search across legal name, preferred name, any identifier (MRN,
    /// etc.) and any contact value (phone, email), per "Patient search"
    /// in the DB schema. Results are capped so the search list stays
    /// scannable rather than exhaustive.
    ///
    /// Matching combines two strategies:
    ///   - a case-insensitive substring match (`ILIKE`), so a partial
    ///     name or a partially remembered phone number still finds the
    ///     patient; and
    ///   - `pg_trgm` trigram similarity (the `%` operator and
    ///     `similarity()`) against legal/preferred name, so a spelling
    ///     mishap (e.g. "Jhon Smith") still surfaces "John Smith" — see
    ///     THYME_DATABASE_SCHEMA.md §36 "Patient search", which
    ///     recommends trigram search for exactly this, and
    ///     THYME_CLINICAL_WORKFLOWS.md's "Duplicate warning", which
    ///     expects fuzzy matches to be shown (never treated as proof of
    ///     identity — that stays a UI-level judgment call, not something
    ///     this query enforces).
    ///
    /// Exact/substring hits are ranked ahead of purely-fuzzy ones via
    /// `match_score`.
    ///
    /// An empty (or all-whitespace) `query` isn't treated as "no
    /// matches" — both the receptionist and doctor "Patients" search
    /// should show the patient roster by default rather than a blank
    /// list, so this falls through to `list_all` instead.
    pub async fn search(&self, query: &str) -> Result<Vec<PatientSearchResult>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.list_all().await;
        }
        let term = format!("%{}%", trimmed);

        let rows = sqlx::query(
            r#"
            SELECT
                p.id,
                p.legal_name,
                p.preferred_name,
                p.birth_date,
                p.sex,
                (
                    SELECT value FROM patient_identifiers pi
                    WHERE pi.patient_id = p.id AND pi.identifier_type = 'MRN'
                    ORDER BY pi.is_primary DESC, pi.created_at ASC
                    LIMIT 1
                ) AS mrn,
                (
                    SELECT value FROM patient_contacts pc
                    WHERE pc.patient_id = p.id AND pc.type = 'phone'
                    ORDER BY pc.preferred DESC, pc.created_at ASC
                    LIMIT 1
                ) AS phone,
                GREATEST(
                    similarity(p.legal_name, $2),
                    similarity(coalesce(p.preferred_name, ''), $2)
                ) AS match_score
            FROM patients p
            WHERE
                p.status <> 'merged'
                AND (
                    p.legal_name ILIKE $1
                    OR p.preferred_name ILIKE $1
                    OR p.legal_name % $2
                    OR p.preferred_name % $2
                    OR EXISTS (
                        SELECT 1 FROM patient_identifiers pi
                        WHERE pi.patient_id = p.id AND pi.value ILIKE $1
                    )
                    OR EXISTS (
                        SELECT 1 FROM patient_contacts pc
                        WHERE pc.patient_id = p.id AND pc.value ILIKE $1
                    )
                )
            ORDER BY match_score DESC, p.legal_name
            LIMIT 25
            "#,
        )
        .bind(&term)
        .bind(trimmed)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            results.push(PatientSearchResult {
                id: row.try_get("id")?,
                legal_name: row.try_get("legal_name")?,
                preferred_name: row.try_get("preferred_name")?,
                birth_date: row.try_get("birth_date")?,
                sex: row.try_get("sex")?,
                mrn: row.try_get("mrn")?,
                phone: row.try_get("phone")?,
            });
        }

        Ok(results)
    }

    /// Backs `search`'s empty-query case: every non-merged patient,
    /// alphabetically by legal name, capped the same way a real search
    /// is so the default list stays scannable. There's no query here to
    /// rank by `similarity()` against, so this skips `match_score`
    /// entirely and just orders by name — `search`'s scored query is
    /// only used once there's an actual term to score.
    async fn list_all(&self) -> Result<Vec<PatientSearchResult>> {
        let rows = sqlx::query(
            r#"
            SELECT
                p.id,
                p.legal_name,
                p.preferred_name,
                p.birth_date,
                p.sex,
                (
                    SELECT value FROM patient_identifiers pi
                    WHERE pi.patient_id = p.id AND pi.identifier_type = 'MRN'
                    ORDER BY pi.is_primary DESC, pi.created_at ASC
                    LIMIT 1
                ) AS mrn,
                (
                    SELECT value FROM patient_contacts pc
                    WHERE pc.patient_id = p.id AND pc.type = 'phone'
                    ORDER BY pc.preferred DESC, pc.created_at ASC
                    LIMIT 1
                ) AS phone
            FROM patients p
            WHERE p.status <> 'merged'
            ORDER BY p.legal_name
            LIMIT 25
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            results.push(PatientSearchResult {
                id: row.try_get("id")?,
                legal_name: row.try_get("legal_name")?,
                preferred_name: row.try_get("preferred_name")?,
                birth_date: row.try_get("birth_date")?,
                sex: row.try_get("sex")?,
                mrn: row.try_get("mrn")?,
                phone: row.try_get("phone")?,
            });
        }

        Ok(results)
    }

    /// Registers a new patient: the `patients` row itself, a generated
    /// primary MRN identifier, and an optional phone contact, all in one
    /// transaction so a failure partway through never leaves a patient
    /// without an MRN. Per THYME_CLINICAL_WORKFLOWS.md's "New patient"
    /// step, the caller is responsible for having already ruled out an
    /// existing match — this method just persists what it's given.
    pub async fn create(&self, new_patient: NewPatient) -> Result<PatientSearchResult> {
        let mut tx = self.pool.begin().await?;

        let patient_row = sqlx::query(
            r#"
            INSERT INTO patients (legal_name, preferred_name, birth_date, sex)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(&new_patient.legal_name)
        .bind(&new_patient.preferred_name)
        .bind(new_patient.birth_date)
        .bind(&new_patient.sex)
        .fetch_one(&mut *tx)
        .await?;

        let patient_id: uuid::Uuid = patient_row.try_get("id")?;

        let mrn = Self::generate_mrn();
        sqlx::query(
            r#"
            INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
            VALUES ($1, 'thyme-mrn', $2, 'MRN', TRUE)
            "#,
        )
        .bind(patient_id)
        .bind(&mrn)
        .execute(&mut *tx)
        .await?;

        if let Some(phone) = &new_patient.phone {
            sqlx::query(
                r#"
                INSERT INTO patient_contacts (patient_id, type, value, preferred)
                VALUES ($1, 'phone', $2, TRUE)
                "#,
            )
            .bind(patient_id)
            .bind(phone)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(PatientSearchResult {
            id: patient_id,
            legal_name: new_patient.legal_name,
            preferred_name: new_patient.preferred_name,
            birth_date: new_patient.birth_date,
            sex: new_patient.sex,
            mrn: Some(mrn),
            phone: new_patient.phone,
        })
    }

    /// Fetches one patient's full domain record by id — used by the AI
    /// pipeline (`server::ai::prompt::build_prompt`, via
    /// `ThymeServer::process_transcription`) to give the model the
    /// patient's identity alongside their history. Unlike `search`,
    /// this returns the full clinical `Patient` record rather than the
    /// deliberately narrower `PatientSearchResult`.
    pub async fn get_by_id(&self, patient_id: uuid::Uuid) -> Result<Option<Patient>> {
        let row = sqlx::query(
            r#"SELECT id, status, legal_name, preferred_name, birth_date, sex
               FROM patients WHERE id = $1"#,
        )
        .bind(patient_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| {
            Ok(Patient {
                id: row.try_get("id")?,
                status: row.try_get("status")?,
                legal_name: row.try_get("legal_name")?,
                preferred_name: row.try_get("preferred_name")?,
                birth_date: row.try_get("birth_date")?,
                sex: row.try_get("sex")?,
            })
        })
        .transpose()
    }

    /// Fetches one patient's identity plus their MRN/phone by id — the
    /// doctor-facing "Patients" tab's file view (`ThymeServer`'s
    /// `PatientFileRequest` handling) uses this to render the file's
    /// header the same way a search result row does, just narrowed to
    /// one already-known id instead of a free-text match. Unlike
    /// `get_by_id`, this returns the same `PatientSearchResult` shape
    /// `search` does (identifiers/contacts included) rather than the
    /// bare `Patient` record.
    pub async fn get_full_by_id(&self, patient_id: uuid::Uuid) -> Result<Option<PatientSearchResult>> {
        let row = sqlx::query(
            r#"
            SELECT
                p.id,
                p.legal_name,
                p.preferred_name,
                p.birth_date,
                p.sex,
                (
                    SELECT value FROM patient_identifiers pi
                    WHERE pi.patient_id = p.id AND pi.identifier_type = 'MRN'
                    ORDER BY pi.is_primary DESC, pi.created_at ASC
                    LIMIT 1
                ) AS mrn,
                (
                    SELECT value FROM patient_contacts pc
                    WHERE pc.patient_id = p.id AND pc.type = 'phone'
                    ORDER BY pc.preferred DESC, pc.created_at ASC
                    LIMIT 1
                ) AS phone
            FROM patients p
            WHERE p.id = $1 AND p.status <> 'merged'
            "#,
        )
        .bind(patient_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| {
            Ok(PatientSearchResult {
                id: row.try_get("id")?,
                legal_name: row.try_get("legal_name")?,
                preferred_name: row.try_get("preferred_name")?,
                birth_date: row.try_get("birth_date")?,
                sex: row.try_get("sex")?,
                mrn: row.try_get("mrn")?,
                phone: row.try_get("phone")?,
            })
        })
        .transpose()
    }

    /// Generates an 8-digit MRN in the same shape as the seeded demo
    /// patients (see `migrations/0006_add_demo_patients.sql`). Not
    /// checked for global uniqueness today — a future migration adding a
    /// uniqueness constraint on `patient_identifiers (system, value)`
    /// where `system = 'thyme-mrn'` would be the right place to close
    /// that gap.
    fn generate_mrn() -> String {
        let n: u32 = rand::random::<u32>() % 100_000_000;
        format!("{n:08}")
    }
}