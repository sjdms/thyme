//! Ingests one Ophthalmic Tomography Image Storage instance — an OCT
//! scan pushed by a topographer — into `device_studies`/`oct_images`.
//!
//! Unlike `super::refraction`, nothing here is staged for clinician
//! acceptance: an image carries no clinical interpretation to review,
//! only pixel data, so once the study is matched to a patient it's
//! filed straight away (`device_studies.status` goes straight to
//! `"applied"`; see the migration's header). What still *does* require
//! a clinician, same as an unmatched refraction study, is a Patient ID
//! this receiver can't resolve — an image filed against the wrong
//! chart is exactly as much a patient-safety problem as a reading
//! filed against the wrong chart.

use std::path::{Path, PathBuf};

use dicom_object::InMemDicomObject;
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::db::{DeviceRepository, NewDeviceStudy, NewOctImage, PatientRepository};
use crate::error::{Result, ServerError};

use super::{patient_match, tags, util};

/// Decodes and files one OCT instance. `store_dir` is where raw pixel
/// data is written (see `image_path` below); `calling_ae_title` is the
/// sending device's negotiated AE title, recorded on the study purely
/// for a clinician's later reference (`server::dicom::association`
/// hands this along from the DICOM UL association, not from anything
/// inside the data set itself, since Calling AE Title is an
/// association-level detail, not a data-set attribute).
///
/// `device_items_changed` is fired once this instance is matched to a
/// patient — see `server::ThymeServer::device_items_changed`'s doc
/// comment for why this exists at all: without it, a client already
/// viewing this patient (most commonly the Dashboard's "in the room
/// now" panel, since that's exactly when a device is likely to be
/// actively scanning them) would have no way to learn this image
/// arrived until something else happened to make it re-fetch.
pub async fn ingest(
    obj: &InMemDicomObject,
    calling_ae_title: &str,
    store_dir: &Path,
    patient_repo: &PatientRepository,
    device_repo: &DeviceRepository,
    device_items_changed: &broadcast::Sender<String>,
) -> Result<()> {
    let sop_instance_uid = util::str_value(obj, tags::SOP_INSTANCE_UID)
        .ok_or_else(|| ServerError::Dicom("OCT instance has no SOP Instance UID".to_string()))?;
    let sop_class_uid = util::str_value(obj, tags::SOP_CLASS_UID)
        .ok_or_else(|| ServerError::Dicom("OCT instance has no SOP Class UID".to_string()))?;

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

    // A resent instance (see `record_study`'s doc comment) already has
    // its image row from the first delivery; re-decoding and
    // re-writing an identical pixel blob to disk under a fresh id would
    // only leave an orphaned duplicate file behind; requires nothing
    // further here.
    if study.status != "unmatched" {
        return Ok(());
    }

    let source_patient_id = util::str_value(obj, tags::PATIENT_ID);
    let Some(patient_id) = patient_match::resolve_patient(patient_repo, source_patient_id.as_deref()).await?
    else {
        // Left `"unmatched"` — see `patient_match::resolve_patient`'s
        // doc comment for why this never guesses instead.
        return Ok(());
    };

    let pixel_data = util::pixel_data(obj)
        .ok_or_else(|| ServerError::Dicom(format!("OCT instance {sop_instance_uid} has no pixel data")))?;

    let file_path = image_path(store_dir, &study.id);
    if let Some(parent) = file_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::File::create(&file_path).await?;
    file.write_all(&pixel_data).await?;

    device_repo
        .insert_oct_image(
            study.id,
            NewOctImage {
                eye: util::str_value(obj, tags::IMAGE_LATERALITY).as_deref(),
                file_path: &file_path.to_string_lossy(),
                rows: util::i32_value(obj, tags::ROWS),
                columns: util::i32_value(obj, tags::COLUMNS),
                number_of_frames: util::i32_value(obj, tags::NUMBER_OF_FRAMES),
                acquisition_datetime: util::date_time_value(obj, tags::ACQUISITION_DATE, tags::ACQUISITION_TIME),
            },
        )
        .await?;

    device_repo.mark_matched(study.id, patient_id, "applied").await?;
    // Best-effort: no subscribers (no client currently connected) is
    // routine, not an error — same reasoning as every other `let _ =
    // ...send(...)` in this codebase (see `ThymeServer::queue_changed`'s
    // usages).
    let _ = device_items_changed.send(patient_id.to_string());

    Ok(())
}

/// Where an OCT instance's raw pixel data is written on disk, keyed by
/// `device_studies.id` rather than the DICOM SOP Instance UID: a UUID
/// is guaranteed filesystem-safe as a filename, where a UID's `.`
/// separators and unbounded length are not guaranteed to be everywhere
/// this might eventually be deployed.
///
/// Format-agnostic on purpose: `PIXEL_DATA` here is whatever bytes were
/// negotiated (this receiver only offers Explicit/Implicit VR Little
/// Endian transfer syntaxes — see
/// `super::association::listen_for_associations` — so this is always
/// native, uncompressed pixel data, never a JPEG/JPEG2000-encapsulated
/// fragment sequence that would need `dicom-pixeldata` to decode). A
/// `.raw` extension avoids asserting a specific pixel layout that
/// `rows`/`columns`/`number_of_frames` (stored alongside it in
/// `oct_images`) are what actually describes.
fn image_path(store_dir: &Path, study_id: &Uuid) -> PathBuf {
    store_dir.join(format!("{study_id}.raw"))
}
