//! Ingests one Autorefraction Measurements Storage instance — a
//! refraction reading pushed by an auto-refractor/keratometer — into
//! staged `ThymeAction::AddRefraction` suggestions, one per eye that
//! was actually measured.
//!
//! Staged, not filed directly: same reasoning as
//! `db::DeviceRepository::record_suggested_action`'s doc comment —
//! this receiver's patient-matching is a free-text identifier lookup,
//! not a clinician looking at the right chart, so every device-sourced
//! reading waits for an explicit accept via
//! `HistoryRepository::apply_action` before it's chart fact.

use dicom_object::InMemDicomObject;
use tokio::sync::broadcast;

use domain::action::ThymeAction;

use crate::db::{DeviceRepository, NewDeviceStudy, PatientRepository};
use crate::error::{Result, ServerError};

use super::{patient_match, tags, util};

/// Decodes one Autorefraction Measurements instance and stages an
/// `AddRefraction` suggestion for each populated eye. `calling_ae_title`
/// is passed through for the same reason as in `super::oct::ingest`;
/// `device_items_changed` likewise — see that function's doc comment.
pub async fn ingest(
    obj: &InMemDicomObject,
    calling_ae_title: &str,
    patient_repo: &PatientRepository,
    device_repo: &DeviceRepository,
    device_items_changed: &broadcast::Sender<String>,
) -> Result<()> {
    let sop_instance_uid = util::str_value(obj, tags::SOP_INSTANCE_UID)
        .ok_or_else(|| ServerError::Dicom("refraction instance has no SOP Instance UID".to_string()))?;
    let sop_class_uid = util::str_value(obj, tags::SOP_CLASS_UID)
        .ok_or_else(|| ServerError::Dicom("refraction instance has no SOP Class UID".to_string()))?;

    let readings = [
        ("OD", tags::AUTOREFRACTION_RIGHT_EYE_SEQUENCE),
        ("OS", tags::AUTOREFRACTION_LEFT_EYE_SEQUENCE),
    ]
    .into_iter()
    .filter_map(|(eye, seq_tag)| eye_reading(obj, eye, seq_tag))
    .collect::<Vec<_>>();

    if readings.is_empty() {
        // Both eye sequences absent or unparsable — nothing usable in
        // this instance. Not treated as an error: a device sending an
        // otherwise-empty measurement (an aborted exam, say) shouldn't
        // make the whole association fail; there's simply nothing to
        // stage for it.
        return Ok(());
    }

    let study = device_repo
        .record_study(NewDeviceStudy {
            source_patient_id: util::str_value(obj, tags::PATIENT_ID).as_deref(),
            source_patient_name: util::str_value(obj, tags::PATIENT_NAME).as_deref(),
            device_ae_title: calling_ae_title,
            modality: util::str_value(obj, tags::MODALITY).as_deref(),
            sop_class_uid: &sop_class_uid,
            sop_instance_uid: &sop_instance_uid,
            study_instance_uid: util::str_value(obj, tags::STUDY_INSTANCE_UID).as_deref(),
        })
        .await?;

    // Resent instance — see `oct::ingest`'s identical check.
    if study.status != "unmatched" {
        return Ok(());
    }

    let source_patient_id = util::str_value(obj, tags::PATIENT_ID);
    let Some(patient_id) = patient_match::resolve_patient(patient_repo, source_patient_id.as_deref()).await?
    else {
        return Ok(());
    };

    for action in &readings {
        device_repo.record_suggested_action(study.id, patient_id, action).await?;
    }

    device_repo.mark_matched(study.id, patient_id, "pending_review").await?;
    // Best-effort — see `oct::ingest`'s identical call for why a
    // failed send here is fine to ignore.
    let _ = device_items_changed.send(patient_id.to_string());

    Ok(())
}

/// Reads one eye's `AUTOREFRACTION_..._EYE_SEQUENCE` item into an
/// `AddRefraction` action, if that eye was actually measured. Per
/// `tags::SPHERE_POWER`'s doc comment, Sphere Power is the one
/// attribute that module makes non-optional when the sequence item is
/// present at all — everything else genuinely is optional on the
/// device's part, so a missing Cylinder Sequence means no astigmatism
/// was measured for that eye, not a parse failure.
fn eye_reading(obj: &InMemDicomObject, eye: &str, seq_tag: dicom_core::Tag) -> Option<ThymeAction> {
    let item = util::sequence_item(obj, seq_tag)?;
    let sphere = util::f64_value(item, tags::SPHERE_POWER)?;

    let (cylinder, axis) = util::sequence_item(item, tags::CYLINDER_SEQUENCE)
        .map(|cyl_item| {
            (
                util::f64_value(cyl_item, tags::CYLINDER_POWER).map(format_diopters),
                util::f64_value(cyl_item, tags::CYLINDER_AXIS).map(|a| a.round() as i32),
            )
        })
        .unwrap_or((None, None));

    Some(ThymeAction::AddRefraction {
        eye: eye.to_string(),
        sphere: format_diopters(sphere),
        cylinder,
        axis,
        // Add power (near addition) has no attribute in the
        // Autorefraction Measurements module — it's a manifest/dispensed
        // value, not something an auto-refractor measures.
        add: None,
        clarification: None,
    })
}

/// Formats a diopter value the way `RefractionEntry::sphere`/`cylinder`
/// expect (see their doc comment): sign always shown, two decimal
/// places, matching how a clinician dictating "minus two twenty five"
/// or "plus one" would have it transcribed.
fn format_diopters(value: f64) -> String {
    format!("{}{:.2}", if value >= 0.0 { "+" } else { "-" }, value.abs())
}
