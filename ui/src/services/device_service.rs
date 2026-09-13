//! Device-originated (DICOM) data business logic on top of
//! `services::connection` — OCT imaging and staged refraction
//! suggestions for one patient at a time. Mirrors `services::ai_service`'s
//! shape; the "accept/reject" half of a device suggestion's lifecycle is
//! close enough to an AI suggestion's that it reuses the same
//! `SuggestionRow`/`ThymeSuggestionCard` Slint types (see
//! `to_device_suggestion_rows` below) rather than inventing a parallel
//! one.

use std::fs;
use std::path::PathBuf;

use packets::device::{DeviceSuggestedActionRecord, OctImageRecord};
use slint::{ModelRc, VecModel};
use tokio::sync::broadcast;

use crate::error::Result;
use crate::services::connection::{Connection, ServerEvent};
use crate::{OctImageRow, SuggestionRow};

/// A device-pipeline-relevant update, filtered out of the connection's
/// general `ServerEvent` stream — mirrors `services::ai_service::AiUpdate`.
#[derive(Debug, Clone)]
pub enum DeviceUpdate {
    /// One patient's device-originated data — every OCT image on file
    /// plus any refraction reading still awaiting review.
    ItemsReady {
        patient_id: String,
        oct_images: Vec<OctImageRecord>,
        pending_suggestions: Vec<DeviceSuggestedActionRecord>,
    },
    ItemsFailed { patient_id: String, message: String },
    SuggestionApplied { suggestion_id: String, summary: String },
    SuggestionApplyFailed { suggestion_id: String, message: String },
    SuggestionRejected { suggestion_id: String },
    SuggestionRejectFailed { suggestion_id: String, message: String },
    ImageDataReady {
        image_id: String,
        eye: String,
        rows: i32,
        columns: i32,
        number_of_frames: i32,
        pixel_data: Vec<u8>,
    },
    ImageDataFailed { image_id: String, message: String },
    Disconnected,
}

pub struct DeviceService {
    connection: Connection,
}

impl DeviceService {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    /// Loads one patient's device-originated data. See
    /// `Connection::request_device_items`.
    pub fn request_patient_items(&self, patient_id: impl Into<String>) -> Result<()> {
        self.connection.request_device_items(patient_id)
    }

    /// Accepts one staged refraction reading. See
    /// `Connection::apply_device_suggestion`.
    pub fn apply_suggestion(&self, suggestion_id: impl Into<String>) -> Result<()> {
        self.connection.apply_device_suggestion(suggestion_id)
    }

    /// Dismisses one staged refraction reading. See
    /// `Connection::reject_device_suggestion`.
    pub fn reject_suggestion(&self, suggestion_id: impl Into<String>) -> Result<()> {
        self.connection.reject_device_suggestion(suggestion_id)
    }

    /// Fetches one OCT image's pixel bytes. See
    /// `Connection::request_oct_image_data`.
    pub fn request_image_data(&self, image_id: impl Into<String>) -> Result<()> {
        self.connection.request_oct_image_data(image_id)
    }

    /// Subscribes to device-pipeline updates. Spawns a small adapter
    /// task that filters the connection's general event stream down to
    /// just the variants this service cares about — mirrors
    /// `services::ai_service::AiService::subscribe`.
    pub fn subscribe(&self) -> broadcast::Receiver<DeviceUpdate> {
        let mut source = self.connection.subscribe();
        let (tx, rx) = broadcast::channel(32);

        tokio::spawn(async move {
            loop {
                match source.recv().await {
                    Ok(ServerEvent::DeviceItemsReady { patient_id, oct_images, pending_suggestions }) => {
                        let _ = tx.send(DeviceUpdate::ItemsReady { patient_id, oct_images, pending_suggestions });
                    }
                    Ok(ServerEvent::DeviceItemsFailed { patient_id, message }) => {
                        let _ = tx.send(DeviceUpdate::ItemsFailed { patient_id, message });
                    }
                    Ok(ServerEvent::DeviceSuggestionApplied { suggestion_id, summary }) => {
                        let _ = tx.send(DeviceUpdate::SuggestionApplied { suggestion_id, summary });
                    }
                    Ok(ServerEvent::DeviceSuggestionApplyFailed { suggestion_id, message }) => {
                        let _ = tx.send(DeviceUpdate::SuggestionApplyFailed { suggestion_id, message });
                    }
                    Ok(ServerEvent::DeviceSuggestionRejected { suggestion_id }) => {
                        let _ = tx.send(DeviceUpdate::SuggestionRejected { suggestion_id });
                    }
                    Ok(ServerEvent::DeviceSuggestionRejectFailed { suggestion_id, message }) => {
                        let _ = tx.send(DeviceUpdate::SuggestionRejectFailed { suggestion_id, message });
                    }
                    Ok(ServerEvent::OctImageDataReady { image_id, eye, rows, columns, number_of_frames, pixel_data }) => {
                        let _ = tx.send(DeviceUpdate::ImageDataReady {
                            image_id,
                            eye,
                            rows,
                            columns,
                            number_of_frames,
                            pixel_data,
                        });
                    }
                    Ok(ServerEvent::OctImageDataFailed { image_id, message }) => {
                        let _ = tx.send(DeviceUpdate::ImageDataFailed { image_id, message });
                    }
                    Ok(ServerEvent::Disconnected { .. }) => {
                        let _ = tx.send(DeviceUpdate::Disconnected);
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

/// Writes one OCT image's raw pixel bytes to disk, alongside this
/// terminal's other local files (`services::settings_service::Settings`'s
/// `HOME/Thyme/...` convention), and returns the path they were saved
/// to. There's no image-decoding pipeline in this client yet (see
/// `packets::device::OctImageDataResponse`'s doc comment on why the
/// bytes are native/uncompressed rather than a displayable format) —
/// saving to disk for a clinician to open in an external DICOM/image
/// viewer is what's built for now, in place of rendering the scan
/// inline.
pub fn save_oct_image(image_id: &str, pixel_data: &[u8]) -> std::io::Result<PathBuf> {
    let dir = oct_image_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{image_id}.raw"));
    fs::write(&path, pixel_data)?;
    Ok(path)
}

fn oct_image_dir() -> PathBuf {
    let base = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("Thyme").join("oct-images")
}

/// Builds the OCT imaging section's row model from a patient's images —
/// shared the same way `services::ai_service::to_suggestion_rows` is.
pub fn to_oct_image_rows(images: &[OctImageRecord]) -> ModelRc<OctImageRow> {
    let rows: Vec<OctImageRow> = images
        .iter()
        .map(|record| OctImageRow {
            image_id: record.image_id.clone().into(),
            eye: eye_label(&record.eye).into(),
            dimensions: format_dimensions(record.rows, record.columns, record.number_of_frames).into(),
            acquired: format_acquired(&record.acquisition_datetime).into(),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// Builds the "device readings awaiting review" section's row model —
/// reuses `SuggestionRow`/`ThymeSuggestionCard` (see this file's header
/// comment) with the clarification-only fields always empty, since a
/// device suggestion never has a `Clarification` (there's nothing to
/// ask a device) and is always `"pending"` by construction — every
/// record `DevicePatientItemsResponse` sends is one still awaiting
/// review.
pub fn to_device_suggestion_rows(suggestions: &[DeviceSuggestedActionRecord]) -> ModelRc<SuggestionRow> {
    let rows: Vec<SuggestionRow> = suggestions
        .iter()
        .map(|record| SuggestionRow {
            suggestion_id: record.suggestion_id.clone().into(),
            action_type: record.action_type.clone().into(),
            description: record.description.clone().into(),
            status: "pending".into(),
            status_label: "Needs review".into(),
            clarification_message: "".into(),
            clarification_answer_type: "".into(),
            clarification_choices: ModelRc::new(VecModel::from(Vec::<slint::SharedString>::new())),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

fn eye_label(eye: &str) -> String {
    match eye {
        "OD" => "OD — right eye".to_string(),
        "OS" => "OS — left eye".to_string(),
        "OU" => "OU — both eyes".to_string(),
        "" => "Eye not recorded".to_string(),
        other => other.to_string(),
    }
}

fn format_dimensions(rows: i32, columns: i32, number_of_frames: i32) -> String {
    if rows <= 0 || columns <= 0 {
        return "Dimensions not recorded".to_string();
    }
    if number_of_frames > 1 {
        format!("{rows} × {columns} ({number_of_frames} frames)")
    } else {
        format!("{rows} × {columns}")
    }
}

/// Just the date portion of the RFC 3339 acquisition timestamp — mirrors
/// `services::patient_service::format_meta`'s reasoning for chart entries:
/// the exact time isn't meaningful to a clinician skimming a list of
/// scans.
fn format_acquired(acquisition_datetime: &str) -> String {
    if acquisition_datetime.is_empty() {
        return "Acquisition date not recorded".to_string();
    }
    acquisition_datetime.split('T').next().unwrap_or(acquisition_datetime).to_string()
}
