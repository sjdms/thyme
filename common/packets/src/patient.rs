use macros::{PacketField, packet};

/// A single patient search result as it travels over the wire.
///
/// This is a transport-layer projection of a patient, not the domain
/// model itself: it intentionally carries only what the receptionist
/// search screen needs (see `THYME_UI_SPEC.md`, "Patient search"), not the
/// full clinical chart. A deeper patient view should be requested through
/// additional, purpose-specific packets rather than growing this one.
#[derive(Debug, Clone, PacketField)]
pub struct PatientRecord {
    pub patient_id: String,
    pub legal_name: String,
    pub preferred_name: Option<String>,
    pub birth_date: Option<String>,
    pub sex: Option<String>,
    pub mrn: Option<String>,
    pub phone: Option<String>,
}

/// `packets` already depends on `domain`, so the wire<->domain conversion
/// lives here rather than scattered across callers.
impl From<domain::patient::PatientSearchResult> for PatientRecord {
    fn from(value: domain::patient::PatientSearchResult) -> Self {
        Self {
            patient_id: value.id.to_string(),
            legal_name: value.legal_name,
            preferred_name: value.preferred_name,
            birth_date: value.birth_date.map(|date| date.to_string()),
            sex: value.sex,
            mrn: value.mrn,
            phone: value.phone,
        }
    }
}

/// Sent by an authenticated client to search for patients by name, MRN,
/// phone number or date of birth. Matching strategy is entirely
/// server-side, per the "Patient search" section of the DB schema.
#[packet(id = 0x10, serverbound)]
pub struct PatientSearchRequest {
    pub query: String,
}

/// Successful response carrying a bounded page of matches. `query` is
/// echoed back so a UI that has since moved on to a newer search can
/// safely discard a stale response instead of applying it.
#[packet(id = 0x11, clientbound)]
pub struct PatientSearchResponse {
    pub query: String,
    pub results: Vec<PatientRecord>,
}

/// Sent instead of `PatientSearchResponse` when the search could not be
/// completed (permission denied, backend failure, etc).
#[packet(id = 0x12, clientbound)]
pub struct PatientSearchError {
    pub query: String,
    pub message: String,
}

/// Sent by an authenticated client to register a new patient. Per
/// THYME_UI_SPEC.md §8 "Patient search safety", this is only meant to be
/// reachable after a search has already come back with no plausible
/// match — that gating happens client-side (see `PatientSearchView`),
/// not here; the server still re-validates the fields it's given.
#[packet(id = 0x13, serverbound)]
pub struct PatientCreateRequest {
    pub legal_name: String,
    pub preferred_name: Option<String>,
    /// ISO `YYYY-MM-DD`; validated server-side.
    pub birth_date: Option<String>,
    pub sex: Option<String>,
    pub phone: Option<String>,
}

/// Successful response carrying the newly created patient, including the
/// MRN the server assigned to it.
#[packet(id = 0x14, clientbound)]
pub struct PatientCreateResponse {
    pub patient: PatientRecord,
}

/// Sent instead of `PatientCreateResponse` when the patient could not be
/// created (validation failure, backend failure, etc). `message` is
/// suitable to show directly to the user.
#[packet(id = 0x15, clientbound)]
pub struct PatientCreateError {
    pub message: String,
}

// -- Patient file (clinical history) ----------------------------------
//
// Wire projections of `domain::patient::PatientHistory` and its entry
// types — the doctor-facing "Patients" tab's file view
// (`PatientFileRequest`/`PatientFileResponse` below), which reuses the
// exact same aggregate `server::db::history_repository::HistoryRepository::fetch`
// already builds for the AI pipeline. As with `PatientRecord`, dates
// travel as plain strings (`created_at` as RFC 3339, `due_date` as
// `YYYY-MM-DD`) — display-only on the client, never parsed back.

#[derive(Debug, Clone, PacketField)]
pub struct ClinicalNoteRecord {
    pub id: String,
    pub note: String,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: String,
}

impl From<domain::patient::ClinicalNoteEntry> for ClinicalNoteRecord {
    fn from(value: domain::patient::ClinicalNoteEntry) -> Self {
        Self {
            id: value.id.to_string(),
            note: value.note,
            authored_by: value.authored_by,
            source: value.source,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, PacketField)]
pub struct DiagnosisRecord {
    pub id: String,
    pub diagnosis: String,
    pub code: Option<String>,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: String,
}

impl From<domain::patient::DiagnosisEntry> for DiagnosisRecord {
    fn from(value: domain::patient::DiagnosisEntry) -> Self {
        Self {
            id: value.id.to_string(),
            diagnosis: value.diagnosis,
            code: value.code,
            authored_by: value.authored_by,
            source: value.source,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, PacketField)]
pub struct FollowUpRecord {
    pub id: String,
    pub weeks: i32,
    pub due_date: String,
    pub status: String,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: String,
}

impl From<domain::patient::FollowUpEntry> for FollowUpRecord {
    fn from(value: domain::patient::FollowUpEntry) -> Self {
        Self {
            id: value.id.to_string(),
            weeks: value.weeks,
            due_date: value.due_date.to_string(),
            status: value.status,
            authored_by: value.authored_by,
            source: value.source,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, PacketField)]
pub struct AllergyRecord {
    pub id: String,
    pub allergen: String,
    pub reaction: Option<String>,
    pub severity: Option<String>,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: String,
}

impl From<domain::patient::AllergyEntry> for AllergyRecord {
    fn from(value: domain::patient::AllergyEntry) -> Self {
        Self {
            id: value.id.to_string(),
            allergen: value.allergen,
            reaction: value.reaction,
            severity: value.severity,
            authored_by: value.authored_by,
            source: value.source,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, PacketField)]
pub struct MedicationRecord {
    pub id: String,
    pub name: String,
    pub dosage: Option<String>,
    pub frequency: Option<String>,
    pub status: String,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: String,
}

impl From<domain::patient::MedicationEntry> for MedicationRecord {
    fn from(value: domain::patient::MedicationEntry) -> Self {
        Self {
            id: value.id.to_string(),
            name: value.name,
            dosage: value.dosage,
            frequency: value.frequency,
            status: value.status,
            authored_by: value.authored_by,
            source: value.source,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, PacketField)]
pub struct LabOrderRecord {
    pub id: String,
    pub test_name: String,
    pub reason: Option<String>,
    pub status: String,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: String,
}

impl From<domain::patient::LabOrderEntry> for LabOrderRecord {
    fn from(value: domain::patient::LabOrderEntry) -> Self {
        Self {
            id: value.id.to_string(),
            test_name: value.test_name,
            reason: value.reason,
            status: value.status,
            authored_by: value.authored_by,
            source: value.source,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, PacketField)]
pub struct ReferralRecord {
    pub id: String,
    pub specialty: String,
    pub reason: Option<String>,
    pub status: String,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: String,
}

impl From<domain::patient::ReferralEntry> for ReferralRecord {
    fn from(value: domain::patient::ReferralEntry) -> Self {
        Self {
            id: value.id.to_string(),
            specialty: value.specialty,
            reason: value.reason,
            status: value.status,
            authored_by: value.authored_by,
            source: value.source,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

/// The full bundle of a patient's clinical history as it travels over
/// the wire — transport projection of `domain::patient::PatientHistory`.
#[derive(Debug, Clone, PacketField)]
pub struct PatientHistoryRecord {
    pub clinical_notes: Vec<ClinicalNoteRecord>,
    pub diagnoses: Vec<DiagnosisRecord>,
    pub follow_ups: Vec<FollowUpRecord>,
    pub allergies: Vec<AllergyRecord>,
    pub medications: Vec<MedicationRecord>,
    pub lab_orders: Vec<LabOrderRecord>,
    pub referrals: Vec<ReferralRecord>,
}

impl From<domain::patient::PatientHistory> for PatientHistoryRecord {
    fn from(value: domain::patient::PatientHistory) -> Self {
        Self {
            clinical_notes: value.clinical_notes.into_iter().map(Into::into).collect(),
            diagnoses: value.diagnoses.into_iter().map(Into::into).collect(),
            follow_ups: value.follow_ups.into_iter().map(Into::into).collect(),
            allergies: value.allergies.into_iter().map(Into::into).collect(),
            medications: value.medications.into_iter().map(Into::into).collect(),
            lab_orders: value.lab_orders.into_iter().map(Into::into).collect(),
            referrals: value.referrals.into_iter().map(Into::into).collect(),
        }
    }
}

/// Sent by an authenticated client — today only the doctor dashboard's
/// "Patients" tab does this — to open a patient's full file: identity
/// plus every clinical history category on record, regardless of
/// whether that patient is currently in the queue or in the room. This
/// is deliberately a separate request from `PatientSearchRequest`
/// (which only ever returns the narrow `PatientRecord` search-result
/// shape) per that packet's own doc comment about a deeper patient view
/// needing its own purpose-specific packet.
#[packet(id = 0x16, serverbound)]
pub struct PatientFileRequest {
    pub patient_id: String,
}

/// Successful response to `PatientFileRequest`.
#[packet(id = 0x17, clientbound)]
pub struct PatientFileResponse {
    pub patient: PatientRecord,
    pub history: PatientHistoryRecord,
}

/// Sent instead of `PatientFileResponse` when the file couldn't be
/// opened (unknown patient, backend failure). `patient_id` is echoed
/// back so a client that has since opened a different patient can
/// discard a stale error.
#[packet(id = 0x18, clientbound)]
pub struct PatientFileError {
    pub patient_id: String,
    pub message: String,
}