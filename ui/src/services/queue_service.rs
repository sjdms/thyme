//! Patient-queue ("incoming patients") business logic on top of
//! `services::connection`, plus the shared Rust<->Slint row conversion
//! both `controllers::doctor` (actionable dashboard) and
//! `controllers::receptionist` (registration + read-only queue view)
//! build on — mirrors `services::patient_service`'s shape.

use packets::queue::QueueEntryRecord;
use slint::{ModelRc, VecModel};
use tokio::sync::broadcast;

use crate::error::Result;
use crate::services::connection::{Connection, ServerEvent};
use crate::QueueEntryRow;

/// A queue-relevant update, filtered out of the connection's general
/// `ServerEvent` stream so controllers don't need to know about event
/// kinds they don't care about.
#[derive(Debug, Clone)]
pub enum QueueUpdate {
    Snapshot(Vec<QueueEntryRecord>),
    RegisterFailed(String),
    AdvanceFailed(String),
    Disconnected,
}

pub struct QueueService {
    connection: Connection,
}

impl QueueService {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    /// Registers `patient_id` into the queue. The result arrives later
    /// as a `QueueUpdate` on `subscribe()` — a `Snapshot` on success
    /// (per `packets::queue::PatientQueueRegisterError`'s doc comment,
    /// there's no dedicated ack), or `RegisterFailed` otherwise.
    pub fn register(&self, patient_id: impl Into<String>, note: Option<String>) -> Result<()> {
        self.connection.register_patient(patient_id.into(), note)
    }

    /// Moves a queue entry to its next status. See `register` — the
    /// result arrives as a `QueueUpdate::Snapshot` or `AdvanceFailed`.
    pub fn advance(&self, queue_id: impl Into<String>, status: impl Into<String>) -> Result<()> {
        self.connection.advance_queue_status(queue_id.into(), status.into())
    }

    /// Subscribes to queue updates. Spawns a small adapter task that
    /// filters the connection's general event stream down to just the
    /// variants this service cares about.
    pub fn subscribe(&self) -> broadcast::Receiver<QueueUpdate> {
        let mut source = self.connection.subscribe();
        let (tx, rx) = broadcast::channel(32);

        tokio::spawn(async move {
            loop {
                match source.recv().await {
                    Ok(ServerEvent::QueueSnapshot { entries }) => {
                        let _ = tx.send(QueueUpdate::Snapshot(entries));
                    }
                    Ok(ServerEvent::QueueRegisterFailed { message }) => {
                        let _ = tx.send(QueueUpdate::RegisterFailed(message));
                    }
                    Ok(ServerEvent::QueueAdvanceFailed { message }) => {
                        let _ = tx.send(QueueUpdate::AdvanceFailed(message));
                    }
                    Ok(ServerEvent::Disconnected { .. }) => {
                        let _ = tx.send(QueueUpdate::Disconnected);
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

/// Builds the dashboard/queue-tab row model from a queue snapshot.
/// Shared by `controllers::doctor` and `controllers::receptionist` so
/// the two windows always describe the same entry the same way.
pub fn to_queue_rows(entries: &[QueueEntryRecord]) -> ModelRc<QueueEntryRow> {
    let rows: Vec<QueueEntryRow> = entries.iter().map(to_queue_row).collect();
    ModelRc::new(VecModel::from(rows))
}

fn to_queue_row(entry: &QueueEntryRecord) -> QueueEntryRow {
    QueueEntryRow {
        queue_id: entry.queue_id.clone().into(),
        patient_id: entry.patient_id.clone().into(),
        patient_name: entry
            .preferred_name
            .clone()
            .unwrap_or_else(|| entry.legal_name.clone())
            .into(),
        detail_line: format_detail_line(entry).into(),
        status: entry.status.clone().into(),
        status_label: status_label(&entry.status).into(),
        note: entry.note.clone().unwrap_or_default().into(),
        registered_by: entry.registered_by.clone().into(),
        time_label: format_time_label(&entry.created_at).into(),
    }
}

fn format_detail_line(entry: &QueueEntryRecord) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(dob) = &entry.birth_date {
        parts.push(dob.clone());
    }
    if let Some(mrn) = &entry.mrn {
        parts.push(format!("MRN {mrn}"));
    }
    parts.join(" • ")
}

fn status_label(status: &str) -> String {
    match status {
        "waiting" => "Waiting".to_string(),
        "with_doctor" => "With doctor".to_string(),
        "done" => "Done".to_string(),
        other => other.to_string(),
    }
}

/// `created_at` is RFC 3339 (see `QueueEntryRecord`); this renders it
/// as a local `HH:MM` for the dashboard/queue list, falling back to
/// the raw string if it doesn't parse rather than hiding the field.
fn format_time_label(created_at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(created_at)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_else(|_| created_at.to_string())
}