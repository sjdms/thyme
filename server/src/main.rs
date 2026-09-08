use crate::error::Result;
use crate::server::ThymeServer;

mod db;
mod error;
mod server;
mod ai;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let db_pool = db::initialize_database().await?;
    ThymeServer::run(ThymeServer::new("0.0.0.0:9086".parse().unwrap(), db_pool).into()).await
}
