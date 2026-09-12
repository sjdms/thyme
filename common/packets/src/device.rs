//! Packets for the DICOM ingestion pipeline (`server::dicom`/
//! `server::db::device_repository`) — a doctor client viewing OCT
//! images and reviewing device-reported refraction readings for one
//! patient.
//!
//! Flow, end to end:
//!   1. An OCT topographer or auto-refractor/keratometer pushes a study
//!      directly to the server over DICOM (`server::dicom`); nothing in
//!      this file is involved in that leg at all — a client never talks
//!      DICOM itself, and never initiates anything on the device side.
//!   2. Once that study is matched to a patient
//!      (`server::dicom::patient_match`), its OCT images and any staged
//!      refraction suggestions are queryable by a doctor client viewing
//!      that patient's file: send `DevicePatientItemsRequest`, get back
//!      `DevicePatientItemsResponse`.
//!   3. A refraction suggestion is reviewed exactly like an AI one
//!      (`packets::ai`) — `ApplyDeviceSuggestionRequest`/
//!      `RejectDeviceSuggestionRequest` — except there is no
//!      "needs_clarification" state and no auto-apply override: every
//!      device-reported reading always waits for an explicit accept
//!      (see `db::DeviceRepository::record_suggested_action`'s doc
//!      comment for why).
//!   4. An OCT image is never staged for review — it has no clinical
//!      interpretation to accept or reject, just pixel data — so a
//!      client fetches its bytes directly with `OctImageDataRequest`
//!      once it knows the image exists from `DevicePatientItemsResponse`.

use macros::{PacketField, packet};

/// One OCT image, as it travels over the wire — the transport
/// projection of one `db::device_repository::OctImageRecord`, minus
/// `file_path` (a server-local detail a client has no use for; it
/// fetches the actual bytes via `OctImageDataRequest` instead).
#[derive(Debug, Clone, PacketField)]
pub struct OctImageRecord {
    pub image_id: String,
    /// `"OD"` | `"OS"` | `""` (empty when the device didn't report
    /// laterality).
    pub eye: String,
    pub rows: i32,
    pub columns: i32,
    pub number_of_frames: i32,
    /// ISO 8601 (RFC 3339), empty if the instance had no usable
    /// Acquisition Date/Time — see
    /// `server::dicom::util::date_time_value`.
    pub acquisition_datetime: String,
}

/// One staged refraction reading, as it travels over the wire — the
/// device-sourced counterpart of `packets::ai::SuggestedActionRecord`.
/// Only ever `"pending"`: a device suggestion has no
/// `"needs_clarification"` state (there is nothing to ask a device)
/// and, unlike an AI suggestion, no auto-applied types either, so this
/// carries no `status` field at all — every record in a
/// `DevicePatientItemsResponse` is awaiting review by construction.
///
/// `payload_json` is the action's full JSON form, exactly as
/// `packets::ai::SuggestedActionRecord::payload_json` documents —
/// carried opaquely for the same reason: a client that only displays
/// suggestions never needs to parse it.
#[derive(Debug, Clone, PacketField)]
pub struct DeviceSuggestedActionRecord {
    pub suggestion_id: String,
    pub action_type: String,
    pub description: String,
    pub payload_json: String,
}

/// Sent by a doctor client viewing a patient's file, to load that
/// patient's device-originated data — OCT images and any refraction
/// readings still awaiting review.
#[packet(id = 0xD01, serverbound)]
pub struct DevicePatientItemsRequest {
    pub patient_id: String,
}

/// Successful response to `DevicePatientItemsRequest`. `oct_images` is
/// every image on file for this patient (newest first); `pending_suggestions`
/// is every still-unreviewed refraction reading (oldest first, so a
/// client reviewing them in order sees earlier exams before later
/// ones).
#[packet(id = 0xD02, clientbound)]
pub struct DevicePatientItemsResponse {
    pub patient_id: String,
    pub oct_images: Vec<OctImageRecord>,
    pub pending_suggestions: Vec<DeviceSuggestedActionRecord>,
}

/// Sent instead of `DevicePatientItemsResponse` when this patient's
/// device data couldn't be loaded (unknown patient, backend failure).
#[packet(id = 0xD03, clientbound)]
pub struct DevicePatientItemsError {
    pub patient_id: String,
    pub message: String,
}

/// Sent by a doctor client to accept one staged refraction reading,
/// writing it to the patient's chart
/// (`server::db::history_repository`) exactly as the device reported
/// it — same "no edit, only accept/reject" stance as
/// `packets::ai::ApplySuggestedActionRequest`.
#[packet(id = 0xD04, serverbound)]
pub struct ApplyDeviceSuggestionRequest {
    pub suggestion_id: String,
}

/// Successful response to `ApplyDeviceSuggestionRequest`. `summary` is
/// a human-readable confirmation (e.g. "Add refraction: OD -2.25")
/// suitable to show directly to the user.
#[packet(id = 0xD05, clientbound)]
pub struct ApplyDeviceSuggestionResponse {
    pub suggestion_id: String,
    pub summary: String,
}

/// Sent instead of `ApplyDeviceSuggestionResponse` when the suggestion
/// couldn't be applied (already resolved, unknown id, backend
/// failure).
#[packet(id = 0xD06, clientbound)]
pub struct ApplyDeviceSuggestionError {
    pub suggestion_id: String,
    pub message: String,
}

/// Sent by a doctor client to dismiss a staged refraction reading
/// without applying it — e.g. a repeat exam superseding an earlier
/// one, or a reading the clinician judges unreliable. Still recorded
/// (`status = "rejected"`) rather than deleted, mirroring
/// `packets::ai::RejectSuggestedActionRequest`.
#[packet(id = 0xD07, serverbound)]
pub struct RejectDeviceSuggestionRequest {
    pub suggestion_id: String,
}

#[packet(id = 0xD08, clientbound)]
pub struct RejectDeviceSuggestionResponse {
    pub suggestion_id: String,
}

/// Sent instead of `RejectDeviceSuggestionResponse` when the
/// suggestion couldn't be dismissed (already resolved, unknown id,
/// backend failure).
#[packet(id = 0xD09, clientbound)]
pub struct RejectDeviceSuggestionError {
    pub suggestion_id: String,
    pub message: String,
}

/// Sent by a doctor client to fetch one OCT image's actual pixel data,
/// once `DevicePatientItemsResponse` has told it the image exists.
#[packet(id = 0xD0A, serverbound)]
pub struct OctImageDataRequest {
    pub image_id: String,
}

/// Successful response to `OctImageDataRequest`. `pixel_data` is the
/// exact bytes `server::dicom::oct` wrote to disk — native,
/// uncompressed pixel data in the layout `rows`/`columns`/
/// `number_of_frames` describe (see that module's doc comment on why
/// it's never a compressed/encapsulated format).
///
/// Sent as one `Vec<u8>` field rather than chunked: fine for a single
/// 2D OCT slice, but this protocol's generic `Vec<T>` wire format
/// encodes a byte array one byte at a time
/// (`packet_datatypes::PacketField for Vec<T>`), with no bulk-copy
/// fast path — a full multi-frame OCT volume is the point at which
/// that stops being fine, and this would want a dedicated
/// length-prefixed byte-blob encoding (or a chunked transfer) instead.
/// Left as-is for now rather than speculatively building that before
/// it's known to matter.
#[packet(id = 0xD0B, clientbound)]
pub struct OctImageDataResponse {
    pub image_id: String,
    pub eye: String,
    pub rows: i32,
    pub columns: i32,
    pub number_of_frames: i32,
    pub pixel_data: Vec<u8>,
}

/// Sent instead of `OctImageDataResponse` when the image's bytes
/// couldn't be loaded (unknown id, the file missing from disk, a read
/// failure).
#[packet(id = 0xD0C, clientbound)]
pub struct OctImageDataError {
    pub image_id: String,
    pub message: String,
}
