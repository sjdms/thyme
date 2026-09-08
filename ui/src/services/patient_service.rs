//! Patient-search, patient-creation, and patient-file business logic on
//! top of `services::connection`.

use packets::patient::{
    AllergyRecord, ClinicalNoteRecord, DiagnosisRecord, FollowUpRecord, LabOrderRecord,
    MedicationRecord, PatientHistoryRecord, PatientRecord, RefractionRecord, ReferralRecord,
};
use slint::{ModelRc, VecModel};
use tokio::sync::broadcast;

use crate::error::Result;
use crate::services::connection::{Connection, NewPatientInput, ServerEvent};
use crate::{HistoryEntryRow, PatientFileData, PatientRow};

/// A patient-search/creation/file-relevant update, filtered out of the
/// connection's general `ServerEvent` stream so controllers don't need
/// to know about event kinds they don't care about.
#[derive(Debug, Clone)]
pub enum PatientSearchUpdate {
    Results { query: String, results: Vec<PatientRecord> },
    Failed { query: String, message: String },
    Created { patient: PatientRecord },
    CreateFailed { message: String },
    /// A patient's full file (identity + clinical history) has been
    /// opened successfully — see `packets::patient::PatientFileResponse`.
    FileReady { patient: PatientRecord, history: PatientHistoryRecord },
    /// A patient file couldn't be opened; `patient_id` is echoed back
    /// (from `packets::patient::PatientFileError`) so a caller that has
    /// since selected a different patient can discard a stale error.
    FileFailed { patient_id: String, message: String },
    Disconnected,
}

pub struct PatientService {
    connection: Connection,
}

impl PatientService {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    /// Fires a search. The response arrives asynchronously through
    /// `subscribe()`, tagged with this exact query so a caller that has
    /// since typed something else can discard a stale answer (see
    /// `packets::patient::PatientSearchResponse`).
    pub fn search(&self, query: impl Into<String>) -> Result<()> {
        self.connection.search_patients(query)
    }

    /// Registers a new patient. Per THYME_UI_SPEC.md §8, this is only
    /// meant to be reachable after a search has already come back with
    /// no match — the caller (the search view) is responsible for that
    /// gating, not this service.
    pub fn create(&self, input: NewPatientInput) -> Result<()> {
        self.connection.create_patient(input)
    }

    /// Opens a patient's full file. Result arrives the same way
    /// `search`'s does, tagged with the patient's id (see
    /// `PatientSearchUpdate::FileReady`/`FileFailed`).
    pub fn request_file(&self, patient_id: impl Into<String>) -> Result<()> {
        self.connection.request_patient_file(patient_id)
    }

    /// Subscribes to patient-search/creation/file updates. Spawns a
    /// small adapter task that filters the connection's general event
    /// stream down to just the variants this service cares about.
    pub fn subscribe(&self) -> broadcast::Receiver<PatientSearchUpdate> {
        let mut source = self.connection.subscribe();
        let (tx, rx) = broadcast::channel(32);

        tokio::spawn(async move {
            loop {
                match source.recv().await {
                    Ok(ServerEvent::PatientSearchResults { query, results }) => {
                        let _ = tx.send(PatientSearchUpdate::Results { query, results });
                    }
                    Ok(ServerEvent::PatientSearchFailed { query, message }) => {
                        let _ = tx.send(PatientSearchUpdate::Failed { query, message });
                    }
                    Ok(ServerEvent::PatientCreated { patient }) => {
                        let _ = tx.send(PatientSearchUpdate::Created { patient });
                    }
                    Ok(ServerEvent::PatientCreateFailed { message }) => {
                        let _ = tx.send(PatientSearchUpdate::CreateFailed { message });
                    }
                    Ok(ServerEvent::PatientFileReady { patient, history }) => {
                        let _ = tx.send(PatientSearchUpdate::FileReady { patient, history });
                    }
                    Ok(ServerEvent::PatientFileFailed { patient_id, message }) => {
                        let _ = tx.send(PatientSearchUpdate::FileFailed { patient_id, message });
                    }
                    Ok(ServerEvent::Disconnected { .. }) => {
                        let _ = tx.send(PatientSearchUpdate::Disconnected);
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

/// Builds the rows a `PatientResultRow` list needs out of a page of
/// search results — shared by `controllers::receptionist` (the
/// dashboard's patient search) and `controllers::doctor` (the
/// "Patients" tab's file-search side panel), so the two never drift
/// into slightly different row formatting.
pub fn to_patient_rows(results: &[PatientRecord]) -> ModelRc<PatientRow> {
    let rows: Vec<PatientRow> = results
        .iter()
        .map(|record| PatientRow {
            patient_id: record.patient_id.clone().into(),
            legal_name: record
                .preferred_name
                .clone()
                .unwrap_or_else(|| record.legal_name.clone())
                .into(),
            detail_line: format_detail_line(record).into(),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// Builds the compact detail line THYME_UI_SPEC.md §7 recommends for a
/// search result row (DOB, MRN, phone), skipping whatever the record
/// doesn't have rather than showing empty placeholders.
pub fn format_detail_line(record: &PatientRecord) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(dob) = &record.birth_date {
        parts.push(dob.clone());
    }
    if let Some(mrn) = &record.mrn {
        parts.push(format!("MRN {mrn}"));
    }
    if let Some(phone) = &record.phone {
        parts.push(phone.clone());
    }
    if parts.is_empty() { "No additional details on file.".to_string() } else { parts.join(" • ") }
}

/// Builds the `PatientFileData` the doctor-facing "Patients" tab's file
/// panel (`PatientFileView`, in `patient-file-view.slint`) renders —
/// the transport-layer `PatientRecord`/`PatientHistoryRecord` pair
/// turned into display-ready rows, so that `.slint` file stays free of
/// date/locale/provenance-labeling logic, the same "format in Rust,
/// render in Slint" split `to_patient_rows` above already uses.
pub fn to_patient_file_data(patient: &PatientRecord, history: &PatientHistoryRecord) -> PatientFileData {
    PatientFileData {
        patient_id: patient.patient_id.clone().into(),
        legal_name: patient.legal_name.clone().into(),
        preferred_name: patient.preferred_name.clone().unwrap_or_default().into(),
        detail_line: format_detail_line(patient).into(),
        clinical_notes: to_rows(&history.clinical_notes, clinical_note_row),
        diagnoses: to_rows(&history.diagnoses, diagnosis_row),
        follow_ups: to_rows(&history.follow_ups, follow_up_row),
        allergies: to_rows(&history.allergies, allergy_row),
        medications: to_rows(&history.medications, medication_row),
        lab_orders: to_rows(&history.lab_orders, lab_order_row),
        referrals: to_rows(&history.referrals, referral_row),
        refractions: to_rows(&history.refractions, refraction_row),
    }
}

fn to_rows<T>(entries: &[T], format: impl Fn(&T) -> HistoryEntryRow) -> ModelRc<HistoryEntryRow> {
    let rows: Vec<HistoryEntryRow> = entries.iter().map(format).collect();
    ModelRc::new(VecModel::from(rows))
}

/// Shared provenance line for every history category: who authored the
/// entry (falling back to "Unknown" — every category's `authored_by`
/// is optional), where it came from, and when, using just the date
/// portion of `created_at`'s RFC 3339 timestamp since the exact time
/// isn't meaningful to a clinician skimming a chart.
fn format_meta(authored_by: &Option<String>, source: &str, created_at: &str) -> String {
    let who = authored_by.as_deref().unwrap_or("Unknown");
    let date = created_at.split('T').next().unwrap_or(created_at);
    format!("{who} • {source} • {date}")
}

fn clinical_note_row(entry: &ClinicalNoteRecord) -> HistoryEntryRow {
    HistoryEntryRow {
        line: entry.note.clone().into(),
        meta: format_meta(&entry.authored_by, &entry.source, &entry.created_at).into(),
    }
}

fn diagnosis_row(entry: &DiagnosisRecord) -> HistoryEntryRow {
    let line = match &entry.code {
        Some(code) => format!("{} ({code})", entry.diagnosis),
        None => entry.diagnosis.clone(),
    };
    HistoryEntryRow {
        line: line.into(),
        meta: format_meta(&entry.authored_by, &entry.source, &entry.created_at).into(),
    }
}

fn follow_up_row(entry: &FollowUpRecord) -> HistoryEntryRow {
    let line = format!(
        "Follow-up in {} weeks — due {} ({})",
        entry.weeks, entry.due_date, entry.status
    );
    HistoryEntryRow {
        line: line.into(),
        meta: format_meta(&entry.authored_by, &entry.source, &entry.created_at).into(),
    }
}

fn allergy_row(entry: &AllergyRecord) -> HistoryEntryRow {
    let mut line = entry.allergen.clone();
    let mut details: Vec<String> = Vec::new();
    if let Some(reaction) = &entry.reaction {
        details.push(reaction.clone());
    }
    if let Some(severity) = &entry.severity {
        details.push(severity.clone());
    }
    if !details.is_empty() {
        line = format!("{line} ({})", details.join(", "));
    }
    HistoryEntryRow {
        line: line.into(),
        meta: format_meta(&entry.authored_by, &entry.source, &entry.created_at).into(),
    }
}

fn medication_row(entry: &MedicationRecord) -> HistoryEntryRow {
    let mut details: Vec<String> = Vec::new();
    if let Some(dosage) = &entry.dosage {
        details.push(dosage.clone());
    }
    if let Some(frequency) = &entry.frequency {
        details.push(frequency.clone());
    }
    let line = if details.is_empty() {
        format!("{} ({})", entry.name, entry.status)
    } else {
        format!("{} — {} ({})", entry.name, details.join(", "), entry.status)
    };
    HistoryEntryRow {
        line: line.into(),
        meta: format_meta(&entry.authored_by, &entry.source, &entry.created_at).into(),
    }
}

fn lab_order_row(entry: &LabOrderRecord) -> HistoryEntryRow {
    let line = match &entry.reason {
        Some(reason) => format!("{} — {} ({})", entry.test_name, reason, entry.status),
        None => format!("{} ({})", entry.test_name, entry.status),
    };
    HistoryEntryRow {
        line: line.into(),
        meta: format_meta(&entry.authored_by, &entry.source, &entry.created_at).into(),
    }
}

/// Expands the wire `eye` code into the label a clinician skimming a
/// chart actually wants — the raw "OD"/"OS"/"OU" alone reads as jargon
/// to a UI that also renders everything else in plain English.
fn eye_label(eye: &str) -> String {
    match eye {
        "OD" => "OD — right eye".to_string(),
        "OS" => "OS — left eye".to_string(),
        "OU" => "OU — both eyes".to_string(),
        other => other.to_string(),
    }
}

fn refraction_row(entry: &RefractionRecord) -> HistoryEntryRow {
    let mut line = format!("{}: {}", eye_label(&entry.eye), entry.sphere);
    if let Some(cylinder) = &entry.cylinder {
        line.push_str(&format!(" / {cylinder}"));
        if let Some(axis) = entry.axis {
            line.push_str(&format!(" × {axis}°"));
        }
    }
    if let Some(add) = &entry.add {
        line.push_str(&format!("  add {add}"));
    }
    HistoryEntryRow {
        line: line.into(),
        meta: format_meta(&entry.authored_by, &entry.source, &entry.created_at).into(),
    }
}

fn referral_row(entry: &ReferralRecord) -> HistoryEntryRow {
    let line = match &entry.reason {
        Some(reason) => format!("{} — {} ({})", entry.specialty, reason, entry.status),
        None => format!("{} ({})", entry.specialty, entry.status),
    };
    HistoryEntryRow {
        line: line.into(),
        meta: format_meta(&entry.authored_by, &entry.source, &entry.created_at).into(),
    }
}