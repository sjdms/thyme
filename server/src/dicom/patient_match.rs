//! Resolving "who this DICOM instance belongs to" against Thyme's own
//! patient roster.
//!
//! A device only ever tells a receiving SCP who a study's patient is
//! by whatever was typed into that device's own patient-entry screen
//! (or, on a worklist-driven setup, whatever a receptionist typed into
//! *this clinic's* system when scheduling the exam) — DICOM's Patient
//! ID (0010,0020) attribute. This module's only job is turning that
//! free-text value into a Thyme `patient_id`, or admitting it
//! couldn't.

use uuid::Uuid;

use crate::db::PatientRepository;
use crate::error::Result;

/// Attempts to resolve `source_patient_id` (the DICOM Patient ID
/// attribute exactly as received) to a Thyme patient, by treating it
/// as this clinic's MRN.
///
/// This is deliberately the *only* matching strategy today — no
/// fallback to matching by name/DOB, which would risk silently filing
/// a device reading against the wrong same-named patient with no
/// clinician in the loop to catch it. A clinic wires a device's own
/// patient-lookup workflow (whether that's manual entry at the device
/// or a DICOM worklist query, neither of which this receiver
/// implements yet) to send the same MRN Thyme already assigned that
/// patient; if a device instead sends its own internal exam/patient
/// number, or nothing at all, this correctly returns `Ok(None)` rather
/// than guessing, and the study lands `"unmatched"` for a clinician to
/// resolve by hand (see `db::DeviceRepository`/`device_studies.status`).
///
/// Returns `Ok(None)` both for "nothing sent" and for "sent something
/// that matches no one" — the caller doesn't need to distinguish those
/// two cases, only whether it got a usable id back.
pub async fn resolve_patient(
    patient_repo: &PatientRepository,
    source_patient_id: Option<&str>,
) -> Result<Option<Uuid>> {
    let Some(id) = source_patient_id else {
        return Ok(None);
    };

    patient_repo.find_by_mrn(id).await
}
