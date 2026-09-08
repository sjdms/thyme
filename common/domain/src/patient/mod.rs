use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

// -- Clinical history ------------------------------------------------
//
// The types below back `server::db::history_repository::HistoryRepository`
// and are the "patient's history" that gets fed to the AI as context
// when processing a dictated transcript (see `server::ai::prompt`). Each
// one is a thin domain-level read model over one of the tables from
// `migrations/0013_create_clinical_history_tables.sql` — one table per
// concept, matching the philosophy `Patient`/`PatientSearchResult`
// already set (identifiers/contacts split out of `patients` itself)
// rather than a single generic "events" table that would lose typed
// columns per kind of entry.
//
// `source` distinguishes an entry a clinician typed directly
// (`"manual"`) from one that started as an AI suggestion a clinician
// then accepted (`"ai_suggested"`) — kept on the row itself (not just on
// the now-resolved suggestion) so the provenance survives even after
// `ai_suggested_actions` history is pruned, per
// THYME_DATABASE_SCHEMA.md's general "don't lose provenance" stance on
// clinical data.

/// A free-text clinical note on a patient's chart.
#[derive(Debug, Clone)]
pub struct ClinicalNoteEntry {
    pub id: Uuid,
    pub note: String,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

/// A diagnosis on a patient's chart, with an optional ICD-10 `code`.
///
/// `status`/`notes` back `ThymeAction::UpdateDiagnosis`; `HistoryRepository::fetch`
/// never returns a diagnosis that's been soft-deleted via
/// `ThymeAction::DeleteDiagnosis` (`deleted_at IS NOT NULL`), so this
/// type doesn't carry `deleted_at`/`deleted_reason` — a row that made
/// it into a `DiagnosisEntry` is, by construction, still current.
#[derive(Debug, Clone)]
pub struct DiagnosisEntry {
    pub id: Uuid,
    pub diagnosis: String,
    pub code: Option<String>,
    /// `"active"` | `"resolved"` | `"ruled_out"`, or any other short
    /// label a clinician/the model uses — see
    /// `ThymeAction::UpdateDiagnosis`.
    pub status: String,
    pub notes: Option<String>,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

/// A scheduled follow-up, `weeks` after `created_at`, due on `due_date`.
#[derive(Debug, Clone)]
pub struct FollowUpEntry {
    pub id: Uuid,
    pub weeks: i32,
    pub due_date: NaiveDate,
    pub status: String,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

/// A known allergy/adverse reaction on a patient's chart.
#[derive(Debug, Clone)]
pub struct AllergyEntry {
    pub id: Uuid,
    pub allergen: String,
    pub reaction: Option<String>,
    pub severity: Option<String>,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

/// A medication a patient is (or was) taking.
///
/// `instructions`/`discontinue_reason` back `ThymeAction::UpdateMedication`
/// and `ThymeAction::DiscontinueMedication` respectively.
#[derive(Debug, Clone)]
pub struct MedicationEntry {
    pub id: Uuid,
    pub name: String,
    pub dosage: Option<String>,
    pub frequency: Option<String>,
    /// Free-text patient-facing instructions ("with food", "at bedtime
    /// in the affected eye only") distinct from `dosage`/`frequency`
    /// themselves.
    pub instructions: Option<String>,
    /// `"active"` | `"discontinued"`.
    pub status: String,
    /// Why this medication was discontinued, if stated — only ever set
    /// once `status == "discontinued"`.
    pub discontinue_reason: Option<String>,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

/// A lab/diagnostic test ordered for a patient.
#[derive(Debug, Clone)]
pub struct LabOrderEntry {
    pub id: Uuid,
    pub test_name: String,
    pub reason: Option<String>,
    pub status: String,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

/// A specialty referral requested for a patient.
#[derive(Debug, Clone)]
pub struct ReferralEntry {
    pub id: Uuid,
    pub specialty: String,
    pub reason: Option<String>,
    pub status: String,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

/// A single eye's refraction/prescription reading
/// (`ThymeAction::AddRefraction`). `sphere`/`cylinder`/`add` are kept as
/// `String` rather than a numeric type so a signed diopter value (e.g.
/// `"-2.25"`, `"+1.00"`) round-trips exactly as dictated, the same
/// reasoning `dosage`/`frequency` on `MedicationEntry` are strings
/// rather than parsed/structured values.
#[derive(Debug, Clone)]
pub struct RefractionEntry {
    pub id: Uuid,
    /// `"OD"` (right eye), `"OS"` (left eye), or `"OU"` (both).
    pub eye: String,
    pub sphere: String,
    pub cylinder: Option<String>,
    pub axis: Option<i32>,
    pub add: Option<String>,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

/// The full bundle of a patient's clinical history that gets handed to
/// the AI as context (`server::ai::prompt::build_prompt`) and to any
/// future "patient chart" UI. Each list is already ordered newest-first
/// and bounded (see `HistoryRepository::fetch`) — this is meant to be a
/// clinically-useful recent history, not an unbounded audit log.
#[derive(Debug, Clone, Default)]
pub struct PatientHistory {
    pub clinical_notes: Vec<ClinicalNoteEntry>,
    pub diagnoses: Vec<DiagnosisEntry>,
    pub follow_ups: Vec<FollowUpEntry>,
    pub allergies: Vec<AllergyEntry>,
    pub medications: Vec<MedicationEntry>,
    pub lab_orders: Vec<LabOrderEntry>,
    pub referrals: Vec<ReferralEntry>,
    pub refractions: Vec<RefractionEntry>,
}

impl PatientHistory {
    /// `true` when every list is empty — lets a prompt builder say "no
    /// prior history on file" plainly instead of printing seven empty
    /// section headers.
    pub fn is_empty(&self) -> bool {
        self.clinical_notes.is_empty()
            && self.diagnoses.is_empty()
            && self.follow_ups.is_empty()
            && self.allergies.is_empty()
            && self.medications.is_empty()
            && self.lab_orders.is_empty()
            && self.referrals.is_empty()
            && self.refractions.is_empty()
    }
}

/// The clinical/business representation of a patient, as understood by the
/// server. This is intentionally decoupled from whatever shape gets sent
/// over the wire (see `packets::patient::PatientRecord`) so the transport
/// format can stay narrow and purpose-specific without constraining the
/// domain model.
#[derive(Debug, Clone)]
pub struct Patient {
    pub id: Uuid,
    pub status: String,
    pub legal_name: String,
    pub preferred_name: Option<String>,
    pub birth_date: Option<NaiveDate>,
    pub sex: Option<String>,
}

/// A single row produced by a patient search. Carries just enough
/// identifying information for a receptionist to recognize the right
/// patient and disambiguate near-duplicates, per the "Patient search"
/// section of the UI spec.
#[derive(Debug, Clone)]
pub struct PatientSearchResult {
    pub id: Uuid,
    pub legal_name: String,
    pub preferred_name: Option<String>,
    pub birth_date: Option<NaiveDate>,
    pub sex: Option<String>,
    pub mrn: Option<String>,
    pub phone: Option<String>,
}

/// A patient a receptionist has registered for a doctor to see —
/// the "incoming patients" queue on the doctor's dashboard
/// (THYME_UI_SPEC.md §40 "Dashboard"). Carries enough of the patient's
/// identity to render a queue row without a second lookup, the same way
/// `PatientSearchResult` does for search rows.
#[derive(Debug, Clone)]
pub struct PatientQueueEntry {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub legal_name: String,
    pub preferred_name: Option<String>,
    pub birth_date: Option<NaiveDate>,
    pub mrn: Option<String>,
    /// `"waiting"`, `"with_doctor"`, or `"done"` — see
    /// `db::queue_repository::PatientQueueRepository`.
    pub status: String,
    pub registered_by: String,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}