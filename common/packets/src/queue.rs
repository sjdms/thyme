use macros::packet;
use packet_datatypes::{PacketField, PacketReader, PacketWriter, error::Result};

/// A single "incoming patients" queue row as it travels over the wire.
/// Transport-layer projection of `domain::patient::PatientQueueEntry`,
/// matching the pattern `PatientRecord` sets for search results.
#[derive(Debug, Clone)]
pub struct QueueEntryRecord {
    pub queue_id: String,
    pub patient_id: String,
    pub legal_name: String,
    pub preferred_name: Option<String>,
    pub birth_date: Option<String>,
    pub mrn: Option<String>,
    /// `"waiting"`, `"with_doctor"`, or `"done"`.
    pub status: String,
    pub registered_by: String,
    pub note: Option<String>,
    /// RFC 3339; display-only on the client.
    pub created_at: String,
}

impl PacketField for QueueEntryRecord {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self> {
        Ok(Self {
            queue_id: PacketField::decode(reader)?,
            patient_id: PacketField::decode(reader)?,
            legal_name: PacketField::decode(reader)?,
            preferred_name: PacketField::decode(reader)?,
            birth_date: PacketField::decode(reader)?,
            mrn: PacketField::decode(reader)?,
            status: PacketField::decode(reader)?,
            registered_by: PacketField::decode(reader)?,
            note: PacketField::decode(reader)?,
            created_at: PacketField::decode(reader)?,
        })
    }

    fn encode(&self, writer: &mut PacketWriter) -> Result<()> {
        self.queue_id.encode(writer)?;
        self.patient_id.encode(writer)?;
        self.legal_name.encode(writer)?;
        self.preferred_name.encode(writer)?;
        self.birth_date.encode(writer)?;
        self.mrn.encode(writer)?;
        self.status.encode(writer)?;
        self.registered_by.encode(writer)?;
        self.note.encode(writer)?;
        self.created_at.encode(writer)?;
        Ok(())
    }
}

impl From<domain::patient::PatientQueueEntry> for QueueEntryRecord {
    fn from(value: domain::patient::PatientQueueEntry) -> Self {
        Self {
            queue_id: value.id.to_string(),
            patient_id: value.patient_id.to_string(),
            legal_name: value.legal_name,
            preferred_name: value.preferred_name,
            birth_date: value.birth_date.map(|date| date.to_string()),
            mrn: value.mrn,
            status: value.status,
            registered_by: value.registered_by,
            note: value.note,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

/// Sent by a receptionist client to register an already-identified
/// patient (found or just created through the normal search/create
/// flow — this packet doesn't itself search or create anything) into
/// the "incoming patients" queue for a doctor to see.
#[packet(id = 0x20, serverbound)]
pub struct PatientQueueRegisterRequest {
    pub patient_id: String,
    pub note: Option<String>,
}

/// Sent instead of a `PatientQueueSnapshot` update to the requester when
/// registration itself failed (bad `patient_id`, backend failure).
/// Everyone's queue view — including the requester's — still gets
/// updated through the normal `PatientQueueSnapshot` broadcast on
/// success, the same way patient creation confirms through
/// `PatientSearchUpdate::Created` rather than a dedicated ack.
#[packet(id = 0x21, clientbound)]
pub struct PatientQueueRegisterError {
    pub message: String,
}

/// Sent by a doctor client to move a queue entry to its next status
/// (`"waiting"` -> `"with_doctor"` -> `"done"`).
#[packet(id = 0x22, serverbound)]
pub struct PatientQueueAdvanceRequest {
    pub queue_id: String,
    pub status: String,
}

#[packet(id = 0x23, clientbound)]
pub struct PatientQueueAdvanceError {
    pub message: String,
}

/// The full current queue (every entry not yet `"done"`), pushed to
/// every connected client once right after authentication and again on
/// every change. Sent as a full snapshot rather than incremental deltas
/// — the queue is always small (same-day walk-ups only), so this stays
/// simple and self-healing the way `PatientSearchResponse` does for
/// search results, rather than requiring clients to reconcile a stream
/// of adds/removes.
#[packet(id = 0x24, clientbound)]
pub struct PatientQueueSnapshot {
    pub entries: Vec<QueueEntryRecord>,
}
