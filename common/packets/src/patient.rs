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

/// A single ended-encounter record — the transport projection of
/// `domain::patient::VisitSummaryEntry`. See that type's doc comment
/// for why this is its own record rather than folded into
/// `ClinicalNoteRecord`, even though the shape is identical.
#[derive(Debug, Clone, PacketField)]
pub struct VisitSummaryRecord {
    pub id: String,
    pub summary: String,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: String,
}

impl From<domain::patient::VisitSummaryEntry> for VisitSummaryRecord {
    fn from(value: domain::patient::VisitSummaryEntry) -> Self {
        Self {
            id: value.id.to_string(),
            summary: value.summary,
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

#[derive(Debug, Clone, PacketField)]
pub struct RefractionRecord {
    pub id: String,
    /// `"OD"` (right eye), `"OS"` (left eye), or `"OU"` (both).
    pub eye: String,
    pub sphere: String,
    pub cylinder: Option<String>,
    pub axis: Option<i32>,
    pub add: Option<String>,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: String,
}

impl From<domain::patient::RefractionEntry> for RefractionRecord {
    fn from(value: domain::patient::RefractionEntry) -> Self {
        Self {
            id: value.id.to_string(),
            eye: value.eye,
            sphere: value.sphere,
            cylinder: value.cylinder,
            axis: value.axis,
            add: value.add,
            authored_by: value.authored_by,
            source: value.source,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

/// A formal glasses/spectacle prescription — the transport projection
/// of `domain::patient::GlassesPrescriptionEntry`. See that type's doc
/// comment for how this differs from `RefractionRecord`.
#[derive(Debug, Clone, PacketField)]
pub struct GlassesPrescriptionRecord {
    pub id: String,
    pub od_sphere: Option<String>,
    pub od_cylinder: Option<String>,
    pub od_axis: Option<i32>,
    pub od_add: Option<String>,
    pub os_sphere: Option<String>,
    pub os_cylinder: Option<String>,
    pub os_axis: Option<i32>,
    pub os_add: Option<String>,
    pub pd: Option<String>,
    pub notes: Option<String>,
    /// `"active"` | `"voided"`.
    pub status: String,
    /// `YYYY-MM-DD`, empty if none on file.
    pub expires_on: String,
    pub voided_reason: Option<String>,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: String,
}

impl From<domain::patient::GlassesPrescriptionEntry> for GlassesPrescriptionRecord {
    fn from(value: domain::patient::GlassesPrescriptionEntry) -> Self {
        Self {
            id: value.id.to_string(),
            od_sphere: value.od_sphere,
            od_cylinder: value.od_cylinder,
            od_axis: value.od_axis,
            od_add: value.od_add,
            os_sphere: value.os_sphere,
            os_cylinder: value.os_cylinder,
            os_axis: value.os_axis,
            os_add: value.os_add,
            pd: value.pd,
            notes: value.notes,
            status: value.status,
            expires_on: value.expires_on.map(|d| d.to_string()).unwrap_or_default(),
            voided_reason: value.voided_reason,
            authored_by: value.authored_by,
            source: value.source,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

/// A formal medication prescription/Rx — the transport projection of
/// `domain::patient::MedicationPrescriptionEntry`. See that type's doc
/// comment for how this differs from `MedicationRecord`.
#[derive(Debug, Clone, PacketField)]
pub struct MedicationPrescriptionRecord {
    pub id: String,
    pub medication_name: String,
    pub strength: Option<String>,
    pub form: Option<String>,
    pub sig: String,
    pub quantity: Option<String>,
    pub refills: Option<i32>,
    pub generic_allowed: bool,
    pub notes: Option<String>,
    /// `"active"` | `"voided"`.
    pub status: String,
    /// `YYYY-MM-DD`, empty if none on file.
    pub expires_on: String,
    pub voided_reason: Option<String>,
    pub authored_by: Option<String>,
    pub source: String,
    pub created_at: String,
}

impl From<domain::patient::MedicationPrescriptionEntry> for MedicationPrescriptionRecord {
    fn from(value: domain::patient::MedicationPrescriptionEntry) -> Self {
        Self {
            id: value.id.to_string(),
            medication_name: value.medication_name,
            strength: value.strength,
            form: value.form,
            sig: value.sig,
            quantity: value.quantity,
            refills: value.refills,
            generic_allowed: value.generic_allowed,
            notes: value.notes,
            status: value.status,
            expires_on: value.expires_on.map(|d| d.to_string()).unwrap_or_default(),
            voided_reason: value.voided_reason,
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
    pub refractions: Vec<RefractionRecord>,
    pub visit_summaries: Vec<VisitSummaryRecord>,
    pub glasses_prescriptions: Vec<GlassesPrescriptionRecord>,
    pub medication_prescriptions: Vec<MedicationPrescriptionRecord>,
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
            refractions: value.refractions.into_iter().map(Into::into).collect(),
            visit_summaries: value.visit_summaries.into_iter().map(Into::into).collect(),
            glasses_prescriptions: value.glasses_prescriptions.into_iter().map(Into::into).collect(),
            medication_prescriptions: value.medication_prescriptions.into_iter().map(Into::into).collect(),
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