use chrono::{Duration, Months, NaiveDate, Utc};
use domain::action::ThymeAction;
use domain::patient::{
    AllergyEntry, ClinicalNoteEntry, DiagnosisEntry, FollowUpEntry, GlassesPrescriptionEntry,
    LabOrderEntry, MedicationEntry, MedicationPrescriptionEntry, PatientHistory, RefractionEntry,
    ReferralEntry, VisitSummaryEntry,
};
use sqlx::{Pool, Row};
use uuid::Uuid;

use crate::error::Result;

/// Data access for a patient's clinical history — the tables from
/// `migrations/0013_create_clinical_history_tables.sql`, one query per
/// table plus the two aggregate operations everything else actually
/// calls:
///
///   - `fetch`, which bundles every table into one
///     `domain::patient::PatientHistory` for
///     `server::ai::prompt::build_prompt` to use as context; and
///   - `apply_action`, which is the *only* place a `ThymeAction`
///     actually gets written to a patient's chart — called from
///     `ThymeServer`'s `ApplySuggestedActionRequest` handling once a
///     clinician has explicitly accepted a suggestion (see
///     `db::ai_repository::AiRepository`), never automatically.
///
/// Kept separate from `AiRepository` the same way `PatientRepository`
/// is separate from `QueueRepository` — this owns the *destination*
/// tables (a patient's actual chart), `AiRepository` owns the
/// *staging* tables (`transcriptions`, `ai_suggested_actions`) a
/// suggestion lives in until this repository is asked to apply it.
pub struct HistoryRepository {
    pool: Pool<sqlx::Postgres>,
}

/// How many rows of each history category `fetch` pulls back. Bounded
/// so the AI prompt built from this stays a clinically-useful recent
/// history rather than growing without bound over a patient's lifetime
/// — see `domain::patient::PatientHistory`'s doc comment.
const HISTORY_LIMIT: i64 = 20;

/// Default validity window for a glasses prescription when
/// `ThymeAction::IssueGlassesPrescription::expires_in_months` wasn't
/// stated — 24 months is a common upper bound across jurisdictions for
/// an adult spectacle prescription; a clinic with a different local
/// requirement can still have the doctor state an explicit duration.
const GLASSES_PRESCRIPTION_DEFAULT_VALIDITY_MONTHS: i32 = 24;

/// Default validity window for a medication prescription when
/// `ThymeAction::IssueMedicationPrescription::expires_in_months` wasn't
/// stated — 12 months is a common default for a non-controlled
/// medication; a controlled substance or unusual case is exactly when
/// the doctor is expected to state a duration explicitly instead of
/// relying on this default.
const MEDICATION_PRESCRIPTION_DEFAULT_VALIDITY_MONTHS: i32 = 12;

/// `months` from today, or `None` if `months` is negative/absurdly
/// large (never expected from a real transcript, but `expires_in_months`
/// is model-provided, so this stays defensive rather than panicking on
/// a malformed value) — a prescription simply gets no expiration date
/// in that case rather than the whole action failing.
fn expires_on_from_months(months: i32) -> Option<NaiveDate> {
    let months = u32::try_from(months).ok()?;
    Utc::now().date_naive().checked_add_months(Months::new(months))
}

impl HistoryRepository {
    pub fn new(pool: Pool<sqlx::Postgres>) -> Self {
        Self { pool }
    }

    /// Fetches the most recent `HISTORY_LIMIT` rows of every history
    /// category for `patient_id`, newest first.
    pub async fn fetch(&self, patient_id: Uuid) -> Result<PatientHistory> {
        let clinical_notes = sqlx::query(
            r#"SELECT id, note, authored_by, source, created_at FROM clinical_notes
               WHERE patient_id = $1 ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(patient_id)
        .bind(HISTORY_LIMIT)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(ClinicalNoteEntry {
                id: row.try_get("id")?,
                note: row.try_get("note")?,
                authored_by: row.try_get("authored_by")?,
                source: row.try_get("source")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let diagnoses = sqlx::query(
            r#"SELECT id, diagnosis, code, status, notes, authored_by, source, created_at FROM diagnoses
               WHERE patient_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(patient_id)
        .bind(HISTORY_LIMIT)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(DiagnosisEntry {
                id: row.try_get("id")?,
                diagnosis: row.try_get("diagnosis")?,
                code: row.try_get("code")?,
                status: row.try_get("status")?,
                notes: row.try_get("notes")?,
                authored_by: row.try_get("authored_by")?,
                source: row.try_get("source")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let follow_ups = sqlx::query(
            r#"SELECT id, weeks, due_date, status, authored_by, source, created_at FROM follow_ups
               WHERE patient_id = $1 ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(patient_id)
        .bind(HISTORY_LIMIT)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(FollowUpEntry {
                id: row.try_get("id")?,
                weeks: row.try_get("weeks")?,
                due_date: row.try_get("due_date")?,
                status: row.try_get("status")?,
                authored_by: row.try_get("authored_by")?,
                source: row.try_get("source")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let allergies = sqlx::query(
            r#"SELECT id, allergen, reaction, severity, authored_by, source, created_at FROM allergies
               WHERE patient_id = $1 ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(patient_id)
        .bind(HISTORY_LIMIT)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(AllergyEntry {
                id: row.try_get("id")?,
                allergen: row.try_get("allergen")?,
                reaction: row.try_get("reaction")?,
                severity: row.try_get("severity")?,
                authored_by: row.try_get("authored_by")?,
                source: row.try_get("source")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let medications = sqlx::query(
            r#"SELECT id, name, dosage, frequency, instructions, status, discontinue_reason, authored_by, source, created_at FROM medications
               WHERE patient_id = $1 ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(patient_id)
        .bind(HISTORY_LIMIT)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(MedicationEntry {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                dosage: row.try_get("dosage")?,
                frequency: row.try_get("frequency")?,
                instructions: row.try_get("instructions")?,
                status: row.try_get("status")?,
                discontinue_reason: row.try_get("discontinue_reason")?,
                authored_by: row.try_get("authored_by")?,
                source: row.try_get("source")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let lab_orders = sqlx::query(
            r#"SELECT id, test_name, reason, status, authored_by, source, created_at FROM lab_orders
               WHERE patient_id = $1 ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(patient_id)
        .bind(HISTORY_LIMIT)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(LabOrderEntry {
                id: row.try_get("id")?,
                test_name: row.try_get("test_name")?,
                reason: row.try_get("reason")?,
                status: row.try_get("status")?,
                authored_by: row.try_get("authored_by")?,
                source: row.try_get("source")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let referrals = sqlx::query(
            r#"SELECT id, specialty, reason, status, authored_by, source, created_at FROM referrals
               WHERE patient_id = $1 ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(patient_id)
        .bind(HISTORY_LIMIT)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(ReferralEntry {
                id: row.try_get("id")?,
                specialty: row.try_get("specialty")?,
                reason: row.try_get("reason")?,
                status: row.try_get("status")?,
                authored_by: row.try_get("authored_by")?,
                source: row.try_get("source")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let refractions = sqlx::query(
            r#"SELECT id, eye, sphere, cylinder, axis, add, authored_by, source, created_at FROM refractions
               WHERE patient_id = $1 ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(patient_id)
        .bind(HISTORY_LIMIT)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(RefractionEntry {
                id: row.try_get("id")?,
                eye: row.try_get("eye")?,
                sphere: row.try_get("sphere")?,
                cylinder: row.try_get("cylinder")?,
                axis: row.try_get("axis")?,
                add: row.try_get("add")?,
                authored_by: row.try_get("authored_by")?,
                source: row.try_get("source")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let visit_summaries = sqlx::query(
            r#"SELECT id, summary, authored_by, source, created_at FROM visit_summaries
               WHERE patient_id = $1 ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(patient_id)
        .bind(HISTORY_LIMIT)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(VisitSummaryEntry {
                id: row.try_get("id")?,
                summary: row.try_get("summary")?,
                authored_by: row.try_get("authored_by")?,
                source: row.try_get("source")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let glasses_prescriptions = sqlx::query(
            r#"SELECT id, od_sphere, od_cylinder, od_axis, od_add, os_sphere, os_cylinder, os_axis, os_add,
                      pd, notes, status, expires_on, voided_reason, authored_by, source, created_at
               FROM glasses_prescriptions
               WHERE patient_id = $1 ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(patient_id)
        .bind(HISTORY_LIMIT)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(GlassesPrescriptionEntry {
                id: row.try_get("id")?,
                od_sphere: row.try_get("od_sphere")?,
                od_cylinder: row.try_get("od_cylinder")?,
                od_axis: row.try_get("od_axis")?,
                od_add: row.try_get("od_add")?,
                os_sphere: row.try_get("os_sphere")?,
                os_cylinder: row.try_get("os_cylinder")?,
                os_axis: row.try_get("os_axis")?,
                os_add: row.try_get("os_add")?,
                pd: row.try_get("pd")?,
                notes: row.try_get("notes")?,
                status: row.try_get("status")?,
                expires_on: row.try_get("expires_on")?,
                voided_reason: row.try_get("voided_reason")?,
                authored_by: row.try_get("authored_by")?,
                source: row.try_get("source")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let medication_prescriptions = sqlx::query(
            r#"SELECT id, medication_name, strength, form, sig, quantity, refills, generic_allowed,
                      notes, status, expires_on, voided_reason, authored_by, source, created_at
               FROM medication_prescriptions
               WHERE patient_id = $1 ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(patient_id)
        .bind(HISTORY_LIMIT)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(MedicationPrescriptionEntry {
                id: row.try_get("id")?,
                medication_name: row.try_get("medication_name")?,
                strength: row.try_get("strength")?,
                form: row.try_get("form")?,
                sig: row.try_get("sig")?,
                quantity: row.try_get("quantity")?,
                refills: row.try_get("refills")?,
                generic_allowed: row.try_get("generic_allowed")?,
                notes: row.try_get("notes")?,
                status: row.try_get("status")?,
                expires_on: row.try_get("expires_on")?,
                voided_reason: row.try_get("voided_reason")?,
                authored_by: row.try_get("authored_by")?,
                source: row.try_get("source")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        Ok(PatientHistory {
            clinical_notes,
            diagnoses,
            follow_ups,
            allergies,
            medications,
            lab_orders,
            referrals,
            refractions,
            visit_summaries,
            glasses_prescriptions,
            medication_prescriptions,
        })
    }

    /// Writes `action` to the appropriate history table for
    /// `patient_id`, tagging the new row with `source` (`"manual"` for
    /// a directly-entered action, `"ai_suggested"` for one applied from
    /// `ai_suggested_actions` — see `db::ai_repository`) and
    /// `authored_by`. Returns a human-readable summary of what was
    /// written, suitable to send straight back as
    /// `packets::ai::ApplySuggestedActionResponse::summary`.
    ///
    /// `ThymeAction::NoActionNeeded` writes nothing — there is no chart
    /// entry for "nothing happened"; its summary is just an
    /// acknowledgement. `ThymeAction::EndVisit` writes at most a
    /// `visit_summaries` row (see that arm below) and nothing else —
    /// ending the visit itself is handled one layer up, by whoever
    /// calls this method (see `ThymeServer::apply_action_with_side_effects`).
    pub async fn apply_action(
        &self,
        patient_id: Uuid,
        authored_by: Option<&str>,
        source: &str,
        action: &ThymeAction,
    ) -> Result<String> {
        match action {
            ThymeAction::AddClinicalNote { note, .. } => {
                sqlx::query(
                    r#"INSERT INTO clinical_notes (patient_id, note, authored_by, source)
                       VALUES ($1, $2, $3, $4)"#,
                )
                .bind(patient_id)
                .bind(note)
                .bind(authored_by)
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::AddDiagnosis { diagnosis, code, .. } => {
                sqlx::query(
                    r#"INSERT INTO diagnoses (patient_id, diagnosis, code, authored_by, source)
                       VALUES ($1, $2, $3, $4, $5)"#,
                )
                .bind(patient_id)
                .bind(diagnosis)
                .bind(code)
                .bind(authored_by)
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::ScheduleFollowUp { weeks, .. } => {
                let due_date = (Utc::now() + Duration::weeks(*weeks as i64)).date_naive();
                sqlx::query(
                    r#"INSERT INTO follow_ups (patient_id, weeks, due_date, authored_by, source)
                       VALUES ($1, $2, $3, $4, $5)"#,
                )
                .bind(patient_id)
                .bind(*weeks as i32)
                .bind(due_date)
                .bind(authored_by)
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::AddAllergy { allergen, reaction, severity, .. } => {
                sqlx::query(
                    r#"INSERT INTO allergies (patient_id, allergen, reaction, severity, authored_by, source)
                       VALUES ($1, $2, $3, $4, $5, $6)"#,
                )
                .bind(patient_id)
                .bind(allergen)
                .bind(reaction)
                .bind(severity)
                .bind(authored_by)
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::AddMedication { name, dosage, frequency, .. } => {
                sqlx::query(
                    r#"INSERT INTO medications (patient_id, name, dosage, frequency, authored_by, source)
                       VALUES ($1, $2, $3, $4, $5, $6)"#,
                )
                .bind(patient_id)
                .bind(name)
                .bind(dosage)
                .bind(frequency)
                .bind(authored_by)
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::OrderLabTest { test_name, reason, .. } => {
                sqlx::query(
                    r#"INSERT INTO lab_orders (patient_id, test_name, reason, authored_by, source)
                       VALUES ($1, $2, $3, $4, $5)"#,
                )
                .bind(patient_id)
                .bind(test_name)
                .bind(reason)
                .bind(authored_by)
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::RequestReferral { specialty, reason, .. } => {
                sqlx::query(
                    r#"INSERT INTO referrals (patient_id, specialty, reason, authored_by, source)
                       VALUES ($1, $2, $3, $4, $5)"#,
                )
                .bind(patient_id)
                .bind(specialty)
                .bind(reason)
                .bind(authored_by)
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::NoActionNeeded { .. } => {
                // Nothing to persist — see doc comment above.
            }
            ThymeAction::AddRefraction { eye, sphere, cylinder, axis, add, .. } => {
                sqlx::query(
                    r#"INSERT INTO refractions (patient_id, eye, sphere, cylinder, axis, add, authored_by, source)
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
                )
                .bind(patient_id)
                .bind(eye)
                .bind(sphere)
                .bind(cylinder)
                .bind(axis)
                .bind(add)
                .bind(authored_by)
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::UpdateMedication { medication_id, dosage, frequency, instructions, .. } => {
                sqlx::query(
                    r#"UPDATE medications
                       SET dosage = COALESCE($3, dosage),
                           frequency = COALESCE($4, frequency),
                           instructions = COALESCE($5, instructions)
                       WHERE id = $1 AND patient_id = $2"#,
                )
                .bind(medication_id)
                .bind(patient_id)
                .bind(dosage)
                .bind(frequency)
                .bind(instructions)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::DiscontinueMedication { medication_id, reason, .. } => {
                sqlx::query(
                    r#"UPDATE medications
                       SET status = 'discontinued',
                           discontinue_reason = COALESCE($3, discontinue_reason)
                       WHERE id = $1 AND patient_id = $2"#,
                )
                .bind(medication_id)
                .bind(patient_id)
                .bind(reason)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::UpdateDiagnosis { diagnosis_id, status: new_status, notes, .. } => {
                sqlx::query(
                    r#"UPDATE diagnoses
                       SET status = $3,
                           notes = COALESCE($4, notes)
                       WHERE id = $1 AND patient_id = $2 AND deleted_at IS NULL"#,
                )
                .bind(diagnosis_id)
                .bind(patient_id)
                .bind(new_status)
                .bind(notes)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::DeleteDiagnosis { diagnosis_id, reason, .. } => {
                sqlx::query(
                    r#"UPDATE diagnoses
                       SET deleted_at = now(),
                           deleted_reason = COALESCE($3, deleted_reason)
                       WHERE id = $1 AND patient_id = $2 AND deleted_at IS NULL"#,
                )
                .bind(diagnosis_id)
                .bind(patient_id)
                .bind(reason)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::EndVisit { summary, .. } => {
                // No chart entry when the model didn't produce a
                // wrap-up worth keeping — same reasoning as
                // `NoActionNeeded` above, just conditional on one field
                // instead of the whole variant. Ending the visit itself
                // (the queue-status side of this action) is not this
                // repository's job — see
                // `ThymeServer::apply_action_with_side_effects`, which
                // wraps every call to this method and is where that
                // happens, since it needs `QueueRepository`, which
                // `HistoryRepository` deliberately doesn't have (see
                // this struct's doc comment).
                if let Some(summary) = summary {
                    sqlx::query(
                        r#"INSERT INTO visit_summaries (patient_id, summary, authored_by, source)
                           VALUES ($1, $2, $3, $4)"#,
                    )
                    .bind(patient_id)
                    .bind(summary)
                    .bind(authored_by)
                    .bind(source)
                    .execute(&self.pool)
                    .await?;
                }
            }
            ThymeAction::IssueGlassesPrescription {
                od_sphere,
                od_cylinder,
                od_axis,
                od_add,
                os_sphere,
                os_cylinder,
                os_axis,
                os_add,
                pd,
                expires_in_months,
                notes,
                ..
            } => {
                let expires_on = expires_on_from_months(
                    expires_in_months.unwrap_or(GLASSES_PRESCRIPTION_DEFAULT_VALIDITY_MONTHS),
                );
                sqlx::query(
                    r#"INSERT INTO glasses_prescriptions
                        (patient_id, od_sphere, od_cylinder, od_axis, od_add,
                         os_sphere, os_cylinder, os_axis, os_add, pd, notes,
                         expires_on, authored_by, source)
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"#,
                )
                .bind(patient_id)
                .bind(od_sphere)
                .bind(od_cylinder)
                .bind(od_axis)
                .bind(od_add)
                .bind(os_sphere)
                .bind(os_cylinder)
                .bind(os_axis)
                .bind(os_add)
                .bind(pd)
                .bind(notes)
                .bind(expires_on)
                .bind(authored_by)
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::VoidGlassesPrescription { prescription_id, reason, .. } => {
                sqlx::query(
                    r#"UPDATE glasses_prescriptions
                       SET status = 'voided',
                           voided_reason = COALESCE($3, voided_reason)
                       WHERE id = $1 AND patient_id = $2 AND status = 'active'"#,
                )
                .bind(prescription_id)
                .bind(patient_id)
                .bind(reason)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::IssueMedicationPrescription {
                medication_name,
                strength,
                form,
                sig,
                quantity,
                refills,
                generic_allowed,
                expires_in_months,
                notes,
                ..
            } => {
                let expires_on = expires_on_from_months(
                    expires_in_months.unwrap_or(MEDICATION_PRESCRIPTION_DEFAULT_VALIDITY_MONTHS),
                );
                sqlx::query(
                    r#"INSERT INTO medication_prescriptions
                        (patient_id, medication_name, strength, form, sig, quantity,
                         refills, generic_allowed, notes, expires_on, authored_by, source)
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
                )
                .bind(patient_id)
                .bind(medication_name)
                .bind(strength)
                .bind(form)
                .bind(sig)
                .bind(quantity)
                .bind(refills)
                .bind(generic_allowed)
                .bind(notes)
                .bind(expires_on)
                .bind(authored_by)
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            ThymeAction::VoidMedicationPrescription { prescription_id, reason, .. } => {
                sqlx::query(
                    r#"UPDATE medication_prescriptions
                       SET status = 'voided',
                           voided_reason = COALESCE($3, voided_reason)
                       WHERE id = $1 AND patient_id = $2 AND status = 'active'"#,
                )
                .bind(prescription_id)
                .bind(patient_id)
                .bind(reason)
                .execute(&self.pool)
                .await?;
            }
        }

        Ok(action.describe())
    }
}