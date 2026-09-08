//! Bridges the standalone login window's callbacks/properties to
//! `services::auth_service`, and — on success — hands the authenticated
//! `Connection` off to a freshly created `ReceptionistWindow`, then
//! closes the login window.
//!
//! The login window is standalone: it is not a "screen" inside a shared
//! app shell. On successful authentication this controller opens the
//! role-appropriate dashboard window and hides the login window rather
//! than switching some internal state — see `main.rs`.
//!
//! Every `tokio::spawn`ed task here talks back to the UI exclusively
//! through `slint::Weak::upgrade_in_event_loop`, so property writes
//! triggered by background async work always happen on Slint's own event
//! loop rather than from a Tokio worker thread.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::error::Result;
use crate::services::auth_service::{AuthAttempt, AuthService};
use crate::services::connection::{AvailableUser, PendingConnection};
use crate::{DoctorWindow, LoginUserOption, LoginWindow, ReceptionistWindow, ThymeChipTone};

/// Role name that routes an authenticated user to `DoctorWindow` instead
/// of the default `ReceptionistWindow` — see
/// `server/migrations/0002_create_roles.sql` and
/// THYME_UI_SPEC.md §62 "Role-specific UI". Any other role (including
/// `administrator`, which has no dedicated dashboard yet) falls back to
/// the receptionist dashboard.
const DOCTOR_ROLE: &str = "doctor";

/// Whichever dashboard window ended up open for this session. Kept alive
/// here for the remaining lifetime of the process, same as the old
/// `RECEPTIONIST_WINDOW` thread-local this replaces.
#[allow(dead_code)]
enum OpenDashboard {

    Receptionist(ReceptionistWindow),
    Doctor(DoctorWindow),
}

thread_local! {
    static OPEN_DASHBOARD: RefCell<Option<OpenDashboard>> = RefCell::new(None);
}

pub struct LoginController {
    auth_service: Arc<AuthService>,
    pending: Mutex<Option<PendingConnection>>,
}

impl LoginController {
    /// Wires the login window's callbacks and kicks off the initial
    /// connection attempt.
    pub fn install(window: &LoginWindow, auth_service: Arc<AuthService>) {
        let controller = Arc::new(Self {
            auth_service,
            pending: Mutex::new(None),
        });

        controller.clone().begin_connect(window.as_weak());

        let login_controller = controller.clone();
        let login_weak = window.as_weak();
        window.on_login_requested(move |username, password| {
            login_controller.clone().begin_login(login_weak.clone(), username.to_string(), password.to_string());
        });

        let retry_controller = controller;
        let retry_weak = window.as_weak();
        window.on_retry_connection(move || {
            retry_controller.clone().begin_connect(retry_weak.clone());
        });
    }

    fn begin_connect(self: Arc<Self>, weak: slint::Weak<LoginWindow>) {
        if let Some(window) = weak.upgrade() {
            window.set_login_busy(true);
            window.set_login_error_text("".into());
            window.set_login_connection_status_text("Connecting…".into());
            window.set_login_connection_status_tone(ThymeChipTone::Neutral);
        }

        tokio::spawn(async move {
            let result = self.auth_service.connect().await;
            let controller = self.clone();
            let _ = weak.upgrade_in_event_loop(move |window| controller.apply_auth_attempt(&window, result));
        });
    }

    fn begin_login(self: Arc<Self>, weak: slint::Weak<LoginWindow>, username: String, password: String) {
        let Some(session) = self.pending.lock().unwrap().take() else {
            // Nothing to submit against yet (still connecting, or a
            // prior attempt failed outright) — ask the user to wait
            // rather than doing nothing silently.
            if let Some(window) = weak.upgrade() {
                window.set_login_error_text("Still connecting to the server. Please wait.".into());
            }
            return;
        };

        if let Some(window) = weak.upgrade() {
            window.set_login_busy(true);
            window.set_login_error_text("".into());
        }

        tokio::spawn(async move {
            let result = self.auth_service.login(session, username, password).await;
            let controller = self.clone();
            let _ = weak.upgrade_in_event_loop(move |window| controller.apply_auth_attempt(&window, result));
        });
    }

    fn apply_auth_attempt(&self, window: &LoginWindow, result: Result<AuthAttempt>) {
        window.set_login_busy(false);

        match result {
            Ok(AuthAttempt::AwaitingCredentials { session, available_users }) => {
                window.set_login_connection_status_text("Connected".into());
                window.set_login_connection_status_tone(ThymeChipTone::Success);
                window.set_login_error_text("".into());
                window.set_login_available_users(to_login_options(&available_users));
                *self.pending.lock().unwrap() = Some(session);
            }
            Ok(AuthAttempt::Rejected { session, available_users, message }) => {
                window.set_login_error_text(message.into());
                window.set_login_available_users(to_login_options(&available_users));
                *self.pending.lock().unwrap() = Some(session);
            }
            Ok(AuthAttempt::Authenticated { connection, roles }) => {
                *self.pending.lock().unwrap() = None;

                let display_name = window
                    .get_login_available_users()
                    .iter()
                    .find(|u| u.username == window.get_login_selected_username())
                    .map(|u| u.display_name.clone())
                    .unwrap_or_else(|| window.get_login_selected_username());

                // Per THYME_UI_SPEC.md §62 "Role-specific UI", which
                // dashboard opens is decided from the authenticated
                // user's server-issued roles, not any client-side
                // assumption — see `AuthAttempt::Authenticated`'s doc
                // comment in `services::auth_service`.
                let is_doctor = roles.iter().any(|r| r == DOCTOR_ROLE);

                let opened = if is_doctor {
                    Self::open_doctor(&window, display_name, connection)
                } else {
                    Self::open_receptionist(&window, display_name, connection)
                };

                let Some(dashboard) = opened else { return };

                // The login window is standalone: once the dashboard is
                // open, close it rather than leaving it around as a
                // hidden background screen.
                let _ = window.hide();

                OPEN_DASHBOARD.with(|w| {
                    *w.borrow_mut() = Some(dashboard);
                });
            }
            Err(e) => {
                window.set_login_connection_status_text("Offline — could not reach the server".into());
                window.set_login_connection_status_tone(ThymeChipTone::Danger);
                window.set_login_error_text(e.to_string().into());
            }
        }
    }

    fn open_receptionist(
        login_window: &LoginWindow,
        display_name: slint::SharedString,
        connection: crate::services::connection::Connection,
    ) -> Option<OpenDashboard> {
        let receptionist = match ReceptionistWindow::new() {
            Ok(receptionist) => receptionist,
            Err(e) => {
                login_window.set_login_error_text(format!("Could not open the dashboard: {e}").into());
                return None;
            }
        };

        receptionist.set_user_display_name(display_name);
        receptionist.set_connection_status_text("Connected".into());
        receptionist.set_connection_status_tone(ThymeChipTone::Success);
        crate::controllers::receptionist::install(&receptionist, connection);

        if let Err(e) = receptionist.show() {
            login_window.set_login_error_text(format!("Could not open the dashboard: {e}").into());
            return None;
        }
        // THYME_UI_SPEC.md §2 calls out "large-screen desktop workflows"
        // as the primary target — the dashboard is meant to fill the
        // screen rather than open as a small window the user has to
        // resize/maximize themselves every time.
        receptionist.window().set_fullscreen(true);

        Some(OpenDashboard::Receptionist(receptionist))
    }

    fn open_doctor(
        login_window: &LoginWindow,
        display_name: slint::SharedString,
        connection: crate::services::connection::Connection,
    ) -> Option<OpenDashboard> {
        let doctor = match DoctorWindow::new() {
            Ok(doctor) => doctor,
            Err(e) => {
                login_window.set_login_error_text(format!("Could not open the dashboard: {e}").into());
                return None;
            }
        };

        doctor.set_user_display_name(display_name);
        doctor.set_connection_status_text("Connected".into());
        doctor.set_connection_status_tone(ThymeChipTone::Success);
        crate::controllers::doctor::install(&doctor, connection);

        if let Err(e) = doctor.show() {
            login_window.set_login_error_text(format!("Could not open the dashboard: {e}").into());
            return None;
        }

        Some(OpenDashboard::Doctor(doctor))
    }
}

fn to_login_options(users: &[AvailableUser]) -> ModelRc<LoginUserOption> {
    let items: Vec<LoginUserOption> = users
        .iter()
        .map(|user| LoginUserOption { username: user.username.clone().into(), display_name: user.display_name.clone().into() })
        .collect();
    ModelRc::new(VecModel::from(items))
}