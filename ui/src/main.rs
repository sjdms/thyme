mod error;
mod stt;
mod services;
mod controllers;

slint::include_modules!();

use std::sync::Arc;

use slint::ComponentHandle;

use crate::services::auth_service::AuthService;
use crate::services::settings_service::Settings;

fn main() -> error::Result<()> {
    // The Slint event loop owns this (the main) thread. Tokio's worker
    // pool runs independently in the background; `rt.enter()` makes
    // `tokio::spawn` usable from here — and from inside Slint
    // callbacks, which also run on this thread — without blocking this
    // thread inside `block_on`. Every background task talks back to the
    // UI exclusively through `slint::Weak::upgrade_in_event_loop` (see
    // `controllers::login`, `controllers::receptionist`).
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();

    // Server address and this terminal's display name are per-machine
    // settings (services::settings_service) editable from either
    // dashboard's Settings panel — loaded here so a value saved last
    // session is what this connection attempt actually uses.
    let settings = Settings::load();

    let login = LoginWindow::new()?;
    let auth_service = Arc::new(AuthService::new(settings.server_address, settings.computer_name));
    controllers::login::LoginController::install(&login, auth_service);

    login.show()?;
    slint::run_event_loop()?;

    Ok(())
}