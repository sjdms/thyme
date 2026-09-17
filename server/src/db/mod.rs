use sqlx::{Pool, postgres::PgPoolOptions};

use crate::error::Result;

pub(crate) async fn initialize_database() -> Result<Pool<sqlx::Postgres>> {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;;

    Ok(pool)
}

mod user_repository;
pub use user_repository::UserRepository;

mod patient_repository;
pub use patient_repository::{NewPatient, PatientRepository};

mod queue_repository;
pub use queue_repository::QueueRepository;

mod history_repository;
pub use history_repository::HistoryRepository;

mod ai_repository;
pub use ai_repository::{AiRepository, AiSuggestedAction};

mod device_repository;
pub use device_repository::{
    DeviceRepository, DeviceStudy, DeviceSuggestedAction, NewDeviceStudy, NewOctImage, OctImageRecord,
};