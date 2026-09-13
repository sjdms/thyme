use std::sync::Arc;

use tokio::sync::broadcast;

use crate::db::{DeviceRepository, PatientRepository};
use crate::dicom::DicomScp;
use crate::error::Result;
use crate::server::ThymeServer;

mod db;
mod dicom;
mod error;
mod server;
mod ai;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let db_pool = db::initialize_database().await?;

    // Shared between `DicomScp` and `ThymeServer` so a study landing on
    // a patient a connected client is already viewing (most commonly
    // the Dashboard's "in the room now" panel) shows up without that
    // client having to do anything to notice — see
    // `ThymeServer::device_items_changed`'s doc comment for the full
    // reasoning. Created here, not inside either constructor, since
    // it's the one piece of state genuinely shared between two
    // otherwise-independent listeners.
    let (device_items_changed, _) = broadcast::channel(16);

    // The DICOM receiver (`dicom::DicomScp`) runs alongside the main
    // Thyme protocol server on its own listener/port — DICOM's own
    // upper-layer association protocol (PS3.8) is a distinct wire
    // protocol from `packets`/`PacketReader`, so it can't share
    // `ThymeServer`'s listener the way, say, two `packets` message
    // types share one connection. It gets its own `PatientRepository`/
    // `DeviceRepository` pair over a cloned pool rather than reaching
    // into `ThymeServer`'s — cheap (each repository is just a pool
    // handle) and keeps the two listeners' lifecycles independent.
    let dicom_scp = Arc::new(DicomScp::from_env(
        PatientRepository::new(db_pool.clone()),
        DeviceRepository::new(db_pool.clone()),
        device_items_changed.clone(),
    ));
    tokio::spawn(async move {
        if let Err(e) = dicom_scp.run().await {
            eprintln!("DICOM receiver stopped: {e}");
        }
    });

    let thyme_server = ThymeServer::with_device_items_changed(
        "0.0.0.0:9086".parse().unwrap(),
        db_pool,
        device_items_changed,
    );
    ThymeServer::run(thyme_server.into()).await
}
