//! Bridges the receptionist dashboard window's callbacks/properties to
//! `services::patient_service` and `services::queue_service`, plus the
//! settings/sign-out overlays shared with `controllers::doctor`
//! (`services::settings_service`).
//!
//! As with `controllers::login`, all background work reports back to the
//! UI exclusively through `slint::Weak::upgrade_in_event_loop`.

use std::sync::{Arc, Mutex};

use slint::ComponentHandle;

use crate::services::connection::{Connection, NewPatientInput};
use crate::services::patient_service::{to_patient_rows, PatientSearchUpdate, PatientService};
use crate::services::queue_service::{self, QueueService, QueueUpdate};
use crate::services::settings_service::Settings;
use crate::{ReceptionistWindow, ThymeChipTone};

/// Owns the receptionist dashboard's patient-search state and queue
/// service for as long as the window is open.
pub struct ReceptionistController {
    patient_service: Arc<PatientService>,
    queue_service: Arc<QueueService>,
    /// The most recent query this controller actually sent to the
    /// server, so a response that arrives after the user has typed
    /// something else can be discarded instead of clobbering newer
    /// results — see `packets::patient::PatientSearchResponse`.
    last_sent_query: Mutex<String>,
}

/// Wires the receptionist window's callbacks to `patient_service`/
/// `queue_service` and starts listening for both update streams, plus
/// the settings/sign-out overlays (`services::settings_service`).
pub fn install(window: &ReceptionistWindow, connection: Connection) {
    let patient_service = Arc::new(PatientService::new(connection.clone()));
    let queue_service = Arc::new(QueueService::new(connection));
    let controller = Arc::new(ReceptionistController {
        patient_service,
        queue_service: queue_service.clone(),
        last_sent_query: Mutex::new(String::new()),
    });

    // Subscribed first, before any other setup, to minimize the window
    // between the server's connect-time `PatientQueueSnapshot` push and
    // this listener actually being registered — see
    // `controllers::doctor::install`'s identical comment.
    controller.clone().listen_queue(window.as_weak());
    // Same reasoning for the patient-search/file stream: this has to be
    // subscribed before the default-list search fires below (or any
    // other search), or that search's response has nowhere to land.
    controller.clone().listen(window.as_weak());

    // Loads whatever this terminal last saved so the settings panel
    // (and the sign-out-confirmation check below) reflect it
    // immediately, without waiting for the user to open Settings first.
    apply_settings_to_window(window, &Settings::load());

    let search_controller = controller.clone();
    let search_weak = window.as_weak();
    window.on_search_edited(move |query| {
        search_controller.clone().handle_search_edited(search_weak.clone(), query.to_string());
    });

    // Load the default "everyone" list immediately, rather than
    // leaving the Patients tab blank until the receptionist types
    // something — mirrors whatever's already in `search-query` (empty
    // on a fresh window) through the exact same path a real search
    // uses.
    controller.clone().handle_search_edited(window.as_weak(), window.get_search_query().to_string());

    let select_weak = window.as_weak();
    window.on_patient_selected(move |patient_id| {
        if let Some(window) = select_weak.upgrade() {
            window.set_selected_patient_id(patient_id);
            // Selecting a different patient (or discarding the current
            // selection entirely — see `PatientSearchView`'s toggle-off
            // click) drops everything that was staged for whichever
            // patient was selected before: the queue note typed into
            // the "Send to doctor" panel, and any registration error
            // still being shown from a previous attempt. Without this,
            // switching (or discarding) selection left the old note/
            // error sitting there, wired to whatever patient happens to
            // be selected next rather than the one the receptionist
            // actually typed it for.
            window.set_queue_note("".into());
            window.set_queue_register_error_text("".into());
            window.set_queue_register_busy(false);
        }
    });

    // Per THYME_UI_SPEC.md §8 "Patient search safety", `PatientSearchView`
    // only ever fires this once a search has already come back with no
    // plausible match — this handler just opens the form and seeds the
    // legal name field with whatever the user was searching for, since
    // that's the best guess at the name they're about to register.
    let create_open_weak = window.as_weak();
    window.on_create_patient_requested(move |query| {
        if let Some(window) = create_open_weak.upgrade() {
            window.set_show_create_form(true);
            window.set_create_error_text("".into());
            window.set_create_busy(false);
            window.set_create_legal_name(query);
            window.set_create_preferred_name("".into());
            window.set_create_birth_date("".into());
            window.set_create_sex("".into());
            window.set_create_phone("".into());
        }
    });

    let create_cancel_weak = window.as_weak();
    window.on_create_cancelled(move || {
        if let Some(window) = create_cancel_weak.upgrade() {
            window.set_show_create_form(false);
            window.set_create_error_text("".into());
        }
    });

    let create_submit_controller = controller.clone();
    let create_submit_weak = window.as_weak();
    window.on_create_submitted(move || {
        create_submit_controller.clone().handle_create_submitted(create_submit_weak.clone());
    });

    // "Send to doctor" — registers the selected patient into the
    // "incoming patients" queue (THYME_UI_SPEC.md §40 "Dashboard").
    let register_controller = controller.clone();
    let register_weak = window.as_weak();
    window.on_queue_register_requested(move |patient_id, note| {
        register_controller.clone().handle_queue_register_requested(
            register_weak.clone(),
            patient_id.to_string(),
            note.to_string(),
        );
    });

    let settings_open_weak = window.as_weak();
    window.on_settings_requested(move || {
        if let Some(window) = settings_open_weak.upgrade() {
            // Re-read from disk (not just the last-applied in-memory
            // values) in case it was edited some other way since this
            // window opened.
            apply_settings_to_window(&window, &Settings::load());
            window.set_settings_error_text("".into());
            window.set_show_settings(true);
        }
    });

    let settings_save_weak = window.as_weak();
    window.on_settings_saved(move || {
        let Some(window) = settings_save_weak.upgrade() else { return };
        let settings = settings_from_window(&window);
        match settings.save() {
            Ok(()) => {
                window.set_settings_error_text("".into());
                window.set_show_settings(false);
            }
            Err(e) => {
                window.set_settings_error_text(format!("Could not save settings: {e}").into());
            }
        }
    });

    let settings_cancel_weak = window.as_weak();
    window.on_settings_cancelled(move || {
        if let Some(window) = settings_cancel_weak.upgrade() {
            window.set_show_settings(false);
            window.set_settings_error_text("".into());
        }
    });

    // THYME_UI_SPEC.md doesn't prescribe this specific confirmation,
    // but §52 "Dialog rules" and the "confirm before signing out"
    // setting itself (design/settings-panel.slint) call for a focused,
    // single-purpose prompt before an action that ends the session —
    // skipped entirely when the setting is off, same as before.
    let sign_out_weak = window.as_weak();
    window.on_sign_out(move || {
        let Some(window) = sign_out_weak.upgrade() else { return };
        if window.get_settings_confirm_sign_out() {
            window.set_show_sign_out_confirm(true);
        } else {
            // There's no server-side logout packet yet; closing the
            // (only remaining) window is enough to end the session
            // and, since it's the last visible window, ends the event
            // loop.
            let _ = window.hide();
        }
    });

    let sign_out_confirm_weak = window.as_weak();
    window.on_sign_out_confirmed(move || {
        if let Some(window) = sign_out_confirm_weak.upgrade() {
            window.set_show_sign_out_confirm(false);
            let _ = window.hide();
        }
    });

    let sign_out_cancel_weak = window.as_weak();
    window.on_sign_out_cancelled(move || {
        if let Some(window) = sign_out_cancel_weak.upgrade() {
            window.set_show_sign_out_confirm(false);
        }
    });
}

/// Applies a loaded `Settings` to the window's bound settings-panel
/// properties. Shared by the initial load and the "reload on open" step
/// in `on_settings_requested` so the two never drift apart.
fn apply_settings_to_window(window: &ReceptionistWindow, settings: &Settings) {
    window.set_settings_mic_gain(settings.mic_gain);
    window.set_settings_confirm_sign_out(settings.confirm_before_sign_out);
}

/// Reads the settings-panel fields back off the window — they're bound
/// `in-out` and edited directly by `ThymeSettingsPanel`, same pattern
/// `handle_create_submitted` uses below for the create-patient form.
///
/// `server_address`/`computer_name` aren't editable from this panel
/// (see `design/settings-panel.slint`), so they're carried over as-is
/// from whatever's currently on disk rather than round-tripped through
/// the window.
fn settings_from_window(window: &ReceptionistWindow) -> Settings {
    Settings {
        mic_gain: window.get_settings_mic_gain(),
        confirm_before_sign_out: window.get_settings_confirm_sign_out(),
        ..Settings::load()
    }
}

impl ReceptionistController {
    /// Fires a search for whatever's in the box — including an empty
    /// box. An empty query used to be handled entirely locally (clear
    /// the results, never touch the server), which is exactly why the
    /// list defaulted to blank instead of showing the patient roster;
    /// now it's sent through like any other query, and
    /// `PatientRepository::search` treats an empty query as "list
    /// everyone" server-side.
    fn handle_search_edited(self: Arc<Self>, weak: slint::Weak<ReceptionistWindow>, query: String) {
        let trimmed = query.trim().to_string();

        *self.last_sent_query.lock().unwrap() = trimmed.clone();

        if let Some(window) = weak.upgrade() {
            window.set_search_busy(true);
            window.set_search_error_text("".into());
        }

        if self.patient_service.search(trimmed).is_err() {
            if let Some(window) = weak.upgrade() {
                window.set_search_busy(false);
                window.set_search_error_text("Not connected to the server.".into());
                window.set_connection_status_text("Offline — could not reach the server".into());
                window.set_connection_status_tone(ThymeChipTone::Danger);
            }
        }
    }

    /// Reads the create-form fields straight off the window (they're
    /// bound `in-out` and edited directly by `PatientCreateForm`) and
    /// submits them. `PatientCreateForm.can-submit` already keeps the
    /// button disabled until a legal name is present, but this is
    /// re-checked here too since callbacks can in principle still fire
    /// (e.g. pressing Enter in a field) while that's not satisfied.
    fn handle_create_submitted(self: Arc<Self>, weak: slint::Weak<ReceptionistWindow>) {
        let Some(window) = weak.upgrade() else { return };

        let legal_name = window.get_create_legal_name().trim().to_string();
        if legal_name.is_empty() {
            window.set_create_error_text("A legal name is required.".into());
            return;
        }

        let non_empty = |s: slint::SharedString| {
            let trimmed = s.trim().to_string();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        };

        let input = NewPatientInput {
            legal_name,
            preferred_name: non_empty(window.get_create_preferred_name()),
            birth_date: non_empty(window.get_create_birth_date()),
            sex: non_empty(window.get_create_sex()),
            phone: non_empty(window.get_create_phone()),
        };

        window.set_create_busy(true);
        window.set_create_error_text("".into());

        if self.patient_service.create(input).is_err() {
            window.set_create_busy(false);
            window.set_create_error_text("Not connected to the server.".into());
            window.set_connection_status_text("Offline — could not reach the server".into());
            window.set_connection_status_tone(ThymeChipTone::Danger);
        }
    }

    /// Registers `patient_id` into the "incoming patients" queue. An
    /// empty (or whitespace-only) note is sent as `None`, matching how
    /// `handle_create_submitted` treats the create-patient form's
    /// optional fields.
    fn handle_queue_register_requested(
        self: Arc<Self>,
        weak: slint::Weak<ReceptionistWindow>,
        patient_id: String,
        note: String,
    ) {
        let Some(window) = weak.upgrade() else { return };

        let trimmed_note = note.trim();
        let note = if trimmed_note.is_empty() { None } else { Some(trimmed_note.to_string()) };

        window.set_queue_register_busy(true);
        window.set_queue_register_error_text("".into());

        if self.queue_service.register(patient_id, note).is_err() {
            window.set_queue_register_busy(false);
            window.set_queue_register_error_text("Not connected to the server.".into());
            window.set_connection_status_text("Offline — could not reach the server".into());
            window.set_connection_status_tone(ThymeChipTone::Danger);
        }
    }

    /// Spawns a task that forwards `patient_service`'s update stream to
    /// the UI for as long as the connection stays alive.
    fn listen(self: Arc<Self>, weak: slint::Weak<ReceptionistWindow>) {
        let mut updates = self.patient_service.subscribe();

        tokio::spawn(async move {
            loop {
                match updates.recv().await {
                    Ok(update) => {
                        let controller = self.clone();
                        let _ = weak.upgrade_in_event_loop(move |window| controller.apply_update(&window, update));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    /// Spawns a task that forwards `queue_service`'s update stream to
    /// the UI for as long as the connection stays alive. Kept separate
    /// from `listen` (rather than merged into one task) so the two
    /// event kinds — patient search and the queue — stay independent,
    /// the same way `services::connection::ServerEvent` keeps their
    /// variants independent.
    fn listen_queue(self: Arc<Self>, weak: slint::Weak<ReceptionistWindow>) {
        let mut updates = self.queue_service.subscribe();

        tokio::spawn(async move {
            loop {
                match updates.recv().await {
                    Ok(QueueUpdate::Snapshot(entries)) => {
                        let _ = weak.upgrade_in_event_loop(move |window| {
                            // A snapshot is pushed after *any* queue
                            // change, not just this window's own
                            // register calls (see
                            // `packets::queue::PatientQueueSnapshot`'s
                            // doc comment) — only clear the pending
                            // register state (and the note field) when
                            // a registration from this window was
                            // actually in flight, so an unrelated
                            // change (e.g. the doctor advancing someone
                            // else's status) doesn't wipe out a note
                            // being typed for a different patient.
                            let was_registering = window.get_queue_register_busy();
                            window.set_queue_entries(queue_service::to_queue_rows(&entries));
                            if was_registering {
                                window.set_queue_register_busy(false);
                                window.set_queue_register_error_text("".into());
                                window.set_queue_note("".into());
                            }
                        });
                    }
                    Ok(QueueUpdate::RegisterFailed(message)) => {
                        let _ = weak.upgrade_in_event_loop(move |window| {
                            window.set_queue_register_busy(false);
                            window.set_queue_register_error_text(message.into());
                        });
                    }
                    Ok(QueueUpdate::AdvanceFailed(_)) => {
                        // The receptionist client never sends
                        // `PatientQueueAdvanceRequest` — only
                        // `controllers::doctor` does — so this variant
                        // can't be produced here. Ignored defensively
                        // rather than matched away entirely, so a
                        // future receptionist-side advance feature
                        // doesn't silently fail to show its errors.
                    }
                    Ok(QueueUpdate::Disconnected) => {
                        let _ = weak.upgrade_in_event_loop(move |window| {
                            window.set_connection_status_text("Offline — could not reach the server".into());
                            window.set_connection_status_tone(ThymeChipTone::Danger);
                        });
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    fn apply_update(&self, window: &ReceptionistWindow, update: PatientSearchUpdate) {
        match update {
            PatientSearchUpdate::Results { query, results } => {
                if *self.last_sent_query.lock().unwrap() != query {
                    // A newer search has since been fired; this response
                    // is stale — discard it rather than clobbering
                    // whatever the user is now looking at.
                    return;
                }
                window.set_search_busy(false);
                window.set_search_error_text("".into());
                window.set_search_results(to_patient_rows(&results));
            }
            PatientSearchUpdate::Failed { query, message } => {
                if *self.last_sent_query.lock().unwrap() != query {
                    return;
                }
                window.set_search_busy(false);
                window.set_search_error_text(message.into());
            }
            PatientSearchUpdate::Created { patient } => {
                window.set_create_busy(false);
                window.set_show_create_form(false);
                window.set_create_error_text("".into());
                window.set_create_legal_name("".into());
                window.set_create_preferred_name("".into());
                window.set_create_birth_date("".into());
                window.set_create_sex("".into());
                window.set_create_phone("".into());

                // Per THYME_CLINICAL_WORKFLOWS.md's "New patient" step,
                // the newly created patient should become the thing the
                // user is now looking at, the same way an existing match
                // would after being selected from search results.
                window.set_search_query(patient.legal_name.clone().into());
                window.set_selected_patient_id(patient.patient_id.clone().into());
                window.set_search_results(to_patient_rows(std::slice::from_ref(&patient)));
            }
            PatientSearchUpdate::CreateFailed { message } => {
                window.set_create_busy(false);
                window.set_create_error_text(message.into());
            }
            PatientSearchUpdate::FileReady { .. } | PatientSearchUpdate::FileFailed { .. } => {
                // The receptionist client never sends
                // `PatientFileRequest` — only `controllers::doctor`'s
                // "Patients" tab does — so these variants can't be
                // produced here. Ignored defensively rather than
                // matched away entirely, same as
                // `controllers::doctor::listen_patients`'s handling of
                // `Created`/`CreateFailed`.
            }
            PatientSearchUpdate::Disconnected => {
                window.set_search_busy(false);
                window.set_connection_status_text("Offline — could not reach the server".into());
                window.set_connection_status_tone(ThymeChipTone::Danger);
            }
        }
    }
}