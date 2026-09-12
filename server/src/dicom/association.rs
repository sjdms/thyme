//! A minimal DICOM Storage SCP (Service Class Provider): listens for
//! incoming associations, negotiates the SOP classes in
//! `super::sop_class`, and for each C-STORE it receives, decodes the
//! data set and hands it to `super::oct` or `super::refraction`
//! depending on which SOP Class was affected.
//!
//! This runs as an ordinary tokio task on the same runtime as
//! `ThymeServer` (see `main.rs`) — `dicom-ul`'s async association API
//! (`establish_async`/`receive`/`send` on tokio I/O) is what makes that
//! possible without a second, separate thread pool just for this.
//!
//! A note on how this was written: `dicom-ul`/`dicom-object`'s DIMSE
//! plumbing (assembling P-DATA-TF fragments into a Command set,
//! reading `CommandField`/`CommandDataSetType`, writing a C-STORE-RSP
//! back) is exactly what a tool like `dicom-storescp` does internally,
//! but it isn't exposed as a single ready-made "receive one C-STORE"
//! call — a caller is expected to drive the PDU loop itself. This
//! module does that by hand, built and cited against the crate's
//! documented types (`Pdu`, `PDataValue`, `ServerAssociationOptions`)
//! rather than against a compiler, per this task's constraints. The
//! spots most likely to need a small adjustment once this actually
//! builds are flagged inline: exactly which accessor exposes the
//! calling AE title on an established `ServerAssociation`, and the
//! precise name of the "write a data set back out" method on
//! `InMemDicomObject` (`write_dataset_with_ts`, assumed here as the
//! `read_dataset_with_ts` counterpart the crate's docs describe).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use dicom_core::{DataElement, PrimitiveValue, VR};
use dicom_encoding::TransferSyntaxIndex;
use dicom_object::InMemDicomObject;
use dicom_transfer_syntax_registry::entries::{EXPLICIT_VR_LITTLE_ENDIAN, IMPLICIT_VR_LITTLE_ENDIAN};
use dicom_ul::pdu::{PDataValue, PDataValueType};
use dicom_ul::{Pdu, association::server::ServerAssociationOptions};
use tokio::net::{TcpListener, TcpStream};

use crate::db::{DeviceRepository, PatientRepository};
use crate::error::{Result, ServerError};

use super::{oct, refraction, sop_class, tags};

/// Runs the DICOM receiver. One instance owns one listening socket;
/// `run` never returns under normal operation, the same as
/// `ThymeServer::run`.
pub struct DicomScp {
    listen_addr: SocketAddr,
    ae_title: String,
    store_dir: PathBuf,
    patient_repo: PatientRepository,
    device_repo: DeviceRepository,
}

impl DicomScp {
    pub fn new(
        listen_addr: SocketAddr,
        ae_title: String,
        store_dir: PathBuf,
        patient_repo: PatientRepository,
        device_repo: DeviceRepository,
    ) -> Self {
        Self { listen_addr, ae_title, store_dir, patient_repo, device_repo }
    }

    /// Builds a `DicomScp` from environment configuration:
    ///
    /// - `DICOM_LISTEN_ADDR` (default `0.0.0.0:11112`, the IANA-
    ///   registered default DICOM port)
    /// - `DICOM_AE_TITLE` (default `THYME-SCP`) — this node's own
    ///   Called AE Title; every device pushing to this receiver needs
    ///   to be configured to call it by this name.
    /// - `DICOM_STORAGE_DIR` (default `./dicom-storage`) — where OCT
    ///   pixel data is written; see `oct::image_path`.
    pub fn from_env(patient_repo: PatientRepository, device_repo: DeviceRepository) -> Self {
        let listen_addr = std::env::var("DICOM_LISTEN_ADDR")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| "0.0.0.0:11112".parse().unwrap());
        let ae_title = std::env::var("DICOM_AE_TITLE").unwrap_or_else(|_| "THYME-SCP".to_string());
        let store_dir = std::env::var("DICOM_STORAGE_DIR").unwrap_or_else(|_| "./dicom-storage".to_string()).into();

        Self::new(listen_addr, ae_title, store_dir, patient_repo, device_repo)
    }

    pub async fn run(self: Arc<Self>) -> Result<()> {
        let listener = TcpListener::bind(self.listen_addr).await?;

        loop {
            let (socket, peer) = listener.accept().await?;
            let this = Arc::clone(&self);

            tokio::spawn(async move {
                if let Err(e) = this.handle_association(socket).await {
                    eprintln!("DICOM association from {peer} ended: {e}");
                }
            });
        }
    }

    /// Negotiates one association and services C-STORE requests on it
    /// until the peer releases or aborts.
    async fn handle_association(&self, socket: TcpStream) -> Result<()> {
        let mut options = ServerAssociationOptions::new()
            .ae_title(&self.ae_title)
            .with_abstract_syntax(sop_class::VERIFICATION)
            .with_transfer_syntax(IMPLICIT_VR_LITTLE_ENDIAN.uid())
            .with_transfer_syntax(EXPLICIT_VR_LITTLE_ENDIAN.uid());

        for sop_class_uid in sop_class::SUPPORTED_STORAGE_SOP_CLASSES {
            options = options.with_abstract_syntax(*sop_class_uid);
        }

        let mut association = options
            .establish_async(socket)
            .await
            .map_err(|e| ServerError::Dicom(format!("association negotiation failed: {e}")))?;

        // ASSUMPTION (see module doc comment): the established
        // association exposes the peer's Calling AE Title under this
        // name. It's recorded purely as an audit/provenance label
        // (`device_studies.device_ae_title`) — nothing in ingestion
        // logic depends on its exact value, so a wrong accessor name
        // here is a one-line fix, not a design problem.
        let calling_ae_title = association.called_ae_title().to_string();

        loop {
            match association.receive().await {
                Ok(Pdu::PData { data }) => {
                    if let Err(e) = self.handle_p_data(&mut association, &calling_ae_title, data).await {
                        eprintln!("error handling C-STORE on association with {calling_ae_title}: {e}");
                    }
                }
                Ok(Pdu::ReleaseRQ) => {
                    let _ = association.send(&Pdu::ReleaseRP).await;
                    break;
                }
                Ok(Pdu::AbortRQ { .. }) | Err(_) => break,
                // A-ASSOCIATE PDUs can't appear mid-association;
                // anything else received here is protocol noise this
                // SCP has no reply for.
                Ok(_) => {}
            }
        }

        Ok(())
    }

    /// Reassembles one DIMSE message (a Command set, optionally
    /// followed by a Data Set) out of the P-DATA-TF PDU(s) carrying it,
    /// and — if it's a C-STORE-RQ — decodes and ingests the instance,
    /// then answers with a C-STORE-RSP.
    ///
    /// Real associations negotiate exactly one presentation context per
    /// abstract syntax offered, so every PDV in a well-formed exchange
    /// carries the same `presentation_context_id`; this reads it off
    /// the first fragment and assumes the rest agree, rather than
    /// tracking multiple in-flight contexts at once — this SCP has no
    /// use for more than one C-STORE happening concurrently on a
    /// single association anyway (devices in the wild send one
    /// instance per association, or one at a time within it).
    async fn handle_p_data(
        &self,
        association: &mut dicom_ul::association::server::AsyncServerAssociation<TcpStream>,
        calling_ae_title: &str,
        mut data: Vec<PDataValue>,
    ) -> Result<()> {
        let Some(first) = data.first() else { return Ok(()) };
        let presentation_context_id = first.presentation_context_id;

        // -- Command set --------------------------------------------
        let mut command_bytes = Vec::new();
        while let Some(pos) = data.iter().position(|v| v.value_type == PDataValueType::Command) {
            let value = data.remove(pos);
            let is_last = value.is_last;
            command_bytes.extend_from_slice(&value.data);
            if is_last {
                break;
            }
            data = self.receive_more(association, presentation_context_id, PDataValueType::Command).await?;
        }

        let command = InMemDicomObject::read_dataset_with_ts(command_bytes.as_slice(), &IMPLICIT_VR_LITTLE_ENDIAN.erased())
            .map_err(|e| ServerError::Dicom(format!("failed to decode DIMSE command set: {e}")))?;

        let command_field = command
            .element(tags::COMMAND_FIELD)
            .ok()
            .and_then(|e| e.to_int::<i32>().ok())
            .unwrap_or_default() as u16;

        if command_field != tags::COMMAND_FIELD_C_STORE_RQ {
            // Only C-STORE is served; anything else (a stray C-ECHO on
            // a Storage-only presentation context, say) is answered
            // with nothing rather than guessing at a response shape
            // this SCP doesn't otherwise implement.
            return Ok(());
        }

        let message_id = command.element(tags::MESSAGE_ID).ok().and_then(|e| e.to_int().ok()).unwrap_or(1);
        let affected_sop_class_uid = command
            .element(tags::AFFECTED_SOP_CLASS_UID)
            .ok()
            .and_then(|e| e.to_str().ok())
            .map(|s| s.trim().to_string())
            .ok_or_else(|| ServerError::Dicom("C-STORE-RQ missing Affected SOP Class UID".to_string()))?;
        let affected_sop_instance_uid = command
            .element(tags::AFFECTED_SOP_INSTANCE_UID)
            .ok()
            .and_then(|e| e.to_str().ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let has_dataset = command
            .element(tags::COMMAND_DATA_SET_TYPE)
            .ok()
            .and_then(|e| e.to_int().ok())
            .map(|v: u16| v as u16 != tags::COMMAND_DATA_SET_TYPE_NONE)
            .unwrap_or(false);

        // -- Data set (the actual DICOM instance being stored) -------
        if has_dataset {
            let mut dataset_bytes = Vec::new();
            loop {
                let pos = data.iter().position(|v| v.value_type == PDataValueType::Data);
                let Some(pos) = pos else {
                    data = self.receive_more(association, presentation_context_id, PDataValueType::Data).await?;
                    continue;
                };
                let value = data.remove(pos);
                let is_last = value.is_last;
                dataset_bytes.extend_from_slice(&value.data);
                if is_last {
                    break;
                }
            }

            // The transfer syntax actually negotiated for this
            // presentation context, not necessarily Implicit VR LE —
            // only the *command* set is always Implicit VR LE (PS3.7
            // 9.1). ASSUMPTION (see module doc comment): a negotiated
            // association exposes its accepted presentation contexts
            // (id -> transfer syntax) under this name/shape.
            let transfer_syntax_uid = association
                .presentation_contexts()
                .iter()
                .find(|pc| pc.id == presentation_context_id)
                .map(|pc| pc.transfer_syntax.as_str())
                .unwrap_or_else(|| IMPLICIT_VR_LITTLE_ENDIAN.uid());

            let transfer_syntax = dicom_transfer_syntax_registry::TransferSyntaxRegistry
                .get(transfer_syntax_uid)
                .ok_or_else(|| ServerError::Dicom(format!("unsupported transfer syntax {transfer_syntax_uid}")))?;

            let object = InMemDicomObject::read_dataset_with_ts(dataset_bytes.as_slice(), transfer_syntax)
                .map_err(|e| ServerError::Dicom(format!("failed to decode DICOM data set: {e}")))?;

            self.ingest(&object, calling_ae_title).await?;
        }

        self.send_c_store_response(
            association,
            presentation_context_id,
            message_id,
            &affected_sop_class_uid,
            &affected_sop_instance_uid,
        )
        .await
    }

    /// Dispatches a decoded instance to the right ingestion handler by
    /// SOP Class UID (`super::sop_class::SopClassKind`).
    async fn ingest(&self, object: &InMemDicomObject, calling_ae_title: &str) -> Result<()> {
        let sop_class_uid = super::util::str_value(object, tags::SOP_CLASS_UID).unwrap_or_default();

        match sop_class::SopClassKind::from_uid(&sop_class_uid) {
            Some(sop_class::SopClassKind::OphthalmicTomographyImage) => {
                oct::ingest(object, calling_ae_title, &self.store_dir, &self.patient_repo, &self.device_repo).await
            }
            Some(sop_class::SopClassKind::AutorefractionMeasurements) => {
                refraction::ingest(object, calling_ae_title, &self.patient_repo, &self.device_repo).await
            }
            None => {
                // Negotiation only ever offers the SOP Classes in
                // `SUPPORTED_STORAGE_SOP_CLASSES`, so a well-behaved
                // SCU can't actually reach this; a promiscuous or
                // misbehaving one might. Logged and otherwise ignored
                // rather than failing the whole association over one
                // instance this receiver was never going to know what
                // to do with.
                eprintln!("received instance of unhandled SOP Class {sop_class_uid}; ignoring");
                Ok(())
            }
        }
    }

    /// Waits for the next `PData` PDU and returns its values, for when
    /// the current message's Command or Data fragments span more than
    /// one PDU (true for anything larger than the negotiated maximum
    /// PDU length — routine for an OCT volume's pixel data).
    async fn receive_more(
        &self,
        association: &mut dicom_ul::association::server::AsyncServerAssociation<TcpStream>,
        expected_presentation_context_id: u8,
        expected_type: PDataValueType,
    ) -> Result<Vec<PDataValue>> {
        match association.receive().await {
            Ok(Pdu::PData { data }) => Ok(data),
            Ok(_) => Err(ServerError::Dicom(format!(
                "expected continuation P-DATA-TF ({expected_type:?} on presentation context \
                 {expected_presentation_context_id}) but got a different PDU"
            ))),
            Err(e) => Err(ServerError::Dicom(format!("association read error: {e}"))),
        }
    }

    /// Builds and sends the C-STORE-RSP command set answering one
    /// C-STORE-RQ with a plain Success status. This receiver has no
    /// failure status it distinguishes at the DIMSE level — an
    /// instance this SCP can't make sense of (an unmatched patient, a
    /// missing attribute) is still accepted at the network layer and
    /// handled by staging it for review (`db::DeviceRepository`) rather
    /// than rejected outright, so the sending device doesn't spuriously
    /// retry or alarm over something a clinician needs to resolve, not
    /// the device.
    async fn send_c_store_response(
        &self,
        association: &mut dicom_ul::association::server::AsyncServerAssociation<TcpStream>,
        presentation_context_id: u8,
        message_id: i32,
        affected_sop_class_uid: &str,
        affected_sop_instance_uid: &str,
    ) -> Result<()> {
        let mut response = InMemDicomObject::new_empty();
        response.put(DataElement::new(
            tags::COMMAND_FIELD,
            VR::US,
            PrimitiveValue::from(tags::COMMAND_FIELD_C_STORE_RSP as u16),
        ));
        response.put(DataElement::new(
            tags::MESSAGE_ID_BEING_RESPONDED_TO,
            VR::US,
            PrimitiveValue::from(message_id as u16),
        ));
        response.put(DataElement::new(
            tags::AFFECTED_SOP_CLASS_UID,
            VR::UI,
            PrimitiveValue::from(affected_sop_class_uid),
        ));
        response.put(DataElement::new(
            tags::AFFECTED_SOP_INSTANCE_UID,
            VR::UI,
            PrimitiveValue::from(affected_sop_instance_uid),
        ));
        response.put(DataElement::new(
            tags::COMMAND_DATA_SET_TYPE,
            VR::US,
            PrimitiveValue::from(tags::COMMAND_DATA_SET_TYPE_NONE),
        ));
        response.put(DataElement::new(tags::STATUS, VR::US, PrimitiveValue::from(tags::STATUS_SUCCESS)));

        let mut buffer = Vec::new();
        // ASSUMPTION (see module doc comment): `write_dataset_with_ts`
        // is `read_dataset_with_ts`'s write-side counterpart.
        response
            .write_dataset_with_ts(&mut buffer, &IMPLICIT_VR_LITTLE_ENDIAN.erased())
            .map_err(|e| ServerError::Dicom(format!("failed to encode C-STORE-RSP: {e}")))?;

        association
            .send(&Pdu::PData {
                data: vec![PDataValue {
                    presentation_context_id,
                    value_type: PDataValueType::Command,
                    is_last: true,
                    data: buffer,
                }],
            })
            .await
            .map_err(|e| ServerError::Dicom(format!("failed to send C-STORE-RSP: {e}")))
    }
}
