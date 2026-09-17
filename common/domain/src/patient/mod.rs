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

/// The administrative record of one encounter ending
/// (`ThymeAction::EndVisit`) — kept in its own table
/// (`migrations/0018_create_visit_summaries_table.sql`) rather than
/// folded into `clinical_notes`, even though the two are structurally
/// identical, because they answer different questions: a clinical note
/// is *clinical content* someone chose to record mid-visit, while a
/// visit summary is the administrative closing statement for one whole
/// encounter — a chart view (or a future "past visits" list) wants to
/// tell those apart without parsing note text. `summary` is optional
/// (`ThymeAction::EndVisit::summary` is too — see that variant's doc
/// comment) because ending a visit is not conditional on the AI having
/// produced a wrap-up worth keeping.
#[derive(Debug, Clone)]
pub struct VisitSummaryEntry {
    pub id: Uuid,
    pub summary: String,
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

/// A formal spectacle/glasses prescription (`ThymeAction::IssueGlassesPrescription`)
/// — a document meant for the patient to take to an optician, not a raw
/// measurement. Deliberately its own concept rather than a flag on
/// `RefractionEntry`: a refraction is a *reading* taken during an
/// exam (there can be several, e.g. a subjective and objective
/// refraction, or a repeat measurement), while a prescription is a
/// *decision* the doctor makes to issue — possibly based on a refraction
/// reading, possibly overriding one, but not the same record. Both eyes
/// live on one row here (unlike `RefractionEntry`, which is one row per
/// eye per reading) because that's what a real prescription document
/// is: one artifact covering both eyes plus `pd`, since an optician
/// can't actually make a single lens's glasses from half a
/// prescription.
#[derive(Debug, Clone)]
pub struct GlassesPrescriptionEntry {
    pub id: Uuid,
    pub od_sphere: Option<String>,
    pub od_cylinder: Option<String>,
    pub od_axis: Option<i32>,
    pub od_add: Option<String>,
    pub os_sphere: Option<String>,
    pub os_cylinder: Option<String>,
    pub os_axis: Option<i32>,
    pub os_add: Option<String>,
    /// Pupillary distance in millimeters, as dictated (kept as `String`
    /// for the same round-tripping reason as `sphere`/`cylinder`/`add`
    /// above — a value like `"32/30"` for separate near/far PDs isn't
    /// numeric).
    pub pd: Option<String>,
    pub notes: Option<String>,
    /// `"active"` | `"voided"` — see `ThymeAction::VoidGlassesPrescription`.
    /// No separate `"expired"` status: `expires_on` already says
    /// whether a still-`"active"` row is past its date, so a status
    /// value that could drift out of sync with it would only add a
    /// second, potentially-stale source of truth for the same fact.
    pub status: String,
    pub expires_on: Option<NaiveDate>,
    pub voided_reason: Option<String>,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

/// A formal medication prescription/Rx (`ThymeAction::IssueMedicationPrescription`)
/// — an authorization for a pharmacy to dispense, not a note that the
/// patient is taking something. Deliberately separate from
/// `MedicationEntry`: the medication list answers "what is this
/// patient currently on" (relevant for interaction checks, allergy
/// review, a quick chart skim), while this answers "what did this
/// doctor formally prescribe and when" (relevant for refills, pharmacy
/// callbacks, and prescribing history) — the two can and do diverge
/// (an over-the-counter medication a patient reports taking has a
/// `MedicationEntry` with no prescription behind it at all; a
/// prescription can be issued without necessarily also being added to
/// the active list, e.g. a short one-off course). Issuing one of these
/// does not automatically create or update a `MedicationEntry` — a
/// clinician or the AI proposes `AddMedication` separately if the
/// active list should also reflect it.
#[derive(Debug, Clone)]
pub struct MedicationPrescriptionEntry {
    pub id: Uuid,
    pub medication_name: String,
    pub strength: Option<String>,
    pub form: Option<String>,
    /// Patient-facing directions for use (e.g. "1 drop in each eye
    /// twice daily") — required, per `ThymeAction::IssueMedicationPrescription`'s
    /// doc comment: a prescription without directions isn't one.
    pub sig: String,
    pub quantity: Option<String>,
    pub refills: Option<i32>,
    pub generic_allowed: bool,
    pub notes: Option<String>,
    /// `"active"` | `"voided"` — see `GlassesPrescriptionEntry::status`'s
    /// doc comment for why there's no separate `"expired"` value.
    pub status: String,
    pub expires_on: Option<NaiveDate>,
    pub voided_reason: Option<String>,
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
    pub visit_summaries: Vec<VisitSummaryEntry>,
    pub glasses_prescriptions: Vec<GlassesPrescriptionEntry>,
    pub medication_prescriptions: Vec<MedicationPrescriptionEntry>,
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
            && self.visit_summaries.is_empty()
            && self.glasses_prescriptions.is_empty()
            && self.medication_prescriptions.is_empty()
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