use std::sync::Arc;

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
    ));
    tokio::spawn(async move {
        if let Err(e) = dicom_scp.run().await {
            eprintln!("DICOM receiver stopped: {e}");
        }
    });

    ThymeServer::run(ThymeServer::new("0.0.0.0:9086".parse().unwrap(), db_pool).into()).await
}