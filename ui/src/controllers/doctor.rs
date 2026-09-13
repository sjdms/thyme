//! Bridges the doctor dashboard window's shortcut/property surface to
//! `stt::AudioRecorder`, and wires the "incoming patients" dashboard
//! (`services::queue_service`) plus the global settings/sign-out
//! overlays (`services::settings_service`) shared with
//! `controllers::receptionist`.
//!
//! Unlike `controllers::receptionist`, the recording half of this
//! controller does almost nothing across an `await` point: starting/
//! stopping a `cpal` stream is effectively synchronous, so the shortcut
//! handlers below just call into `AudioRecorder` directly from the
//! Slint callback (which already runs on the UI thread) rather than
//! spawning a task. The one thing that *does* cross threads there is
//! `AudioRecorder`'s own device error callback, which `cpal` invokes
//! from its internal audio thread — that one goes back through
//! `slint::Weak::upgrade_in_event_loop`, same as every other
//! cross-thread UI update in this app (including the queue-snapshot
//! listener below).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use packets::ai::SuggestedActionRecord;

use crate::services::ai_service::{self, AiService, AiUpdate};
use crate::services::connection::Connection;
use crate::services::device_service::{self, DeviceService, DeviceUpdate};
use crate::services::patient_service::{self, PatientSearchUpdate, PatientService};
use crate::services::queue_service::{self, QueueService, QueueUpdate};
use crate::services::settings_service::Settings;
use crate::services::stt_service::STTService;
use crate::stt::{AudioRecorder, STTInterface};
use crate::{DoctorWindow, PatientFileData, SuggestionRow, ThymeChipTone};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The AI review panel's authoritative state — the full list of
/// suggestions from the most recently processed transcript, plus that
/// transcript's id (needed for `ApplyAllSuggestedActionsRequest`).
///
/// Kept behind `Arc<Mutex<_>>` (not `Rc<RefCell<_>>`) specifically so
/// it can be touched both from synchronous Slint-callback closures on
/// the UI thread (`on_suggestion_accepted` and friends, below) and from
/// `slint::Weak::upgrade_in_event_loop` closures — which must be `Send`
/// even though they end up running on the UI thread, since they're
/// scheduled from `listen_ai`'s background task. `Rc<RefCell<_>>`
/// can't cross that boundary; `Arc<Mutex<_>>` can, and the contents
/// (`String`s and small structs) are cheap enough that the extra
/// synchronization costs nothing noticeable.
#[derive(Default)]
struct AiPanelState {
    transcription_id: Option<String>,
    /// The patient this transcript's suggestions belong to (from
    /// `AiUpdate::SuggestionsReady`) — kept alongside `transcription_id`
    /// so that once a suggestion is actually applied (`SuggestionApplied`/
    /// `AllApplied`/an auto-applying `ClarificationResolved`), `listen_ai`
    /// knows whose patient file just changed server-side and needs
    /// re-fetching, without the individual apply/resolve events having
    /// to carry a patient id of their own.
    patient_id: Option<String>,
    suggestions: Vec<SuggestedActionRecord>,
}

impl AiPanelState {
    fn reset(&mut self) {
        self.transcription_id = None;
        self.patient_id = None;
        self.suggestions.clear();
    }

    fn mark_status(&mut self, suggestion_id: &str, status: &str) {
        if let Some(record) = self
            .suggestions
            .iter_mut()
            .find(|r| r.suggestion_id == suggestion_id)
        {
            record.status = status.to_string();
        }
    }

    /// Folds a `ResolveClarificationResponse` into this panel's local
    /// copy of the suggestion it answered: the freshly recomputed
    /// `description` and `status` replace the old ones, and the
    /// `Clarification` fields are cleared — mirroring what
    /// `domain::action::ThymeAction::resolve_clarification` just did to
    /// the action itself server-side.
    fn resolve_clarification(&mut self, suggestion_id: &str, description: String, status: String) {
        if let Some(record) = self
            .suggestions
            .iter_mut()
            .find(|r| r.suggestion_id == suggestion_id)
        {
            record.description = description;
            record.status = status;
            record.clarification_message.clear();
            record.clarification_answer_type.clear();
            record.clarification_choices.clear();
        }
    }
}

/// What a recording, once stopped and transcribed, should be used for —
/// decided at the moment recording *starts* (`begin_recording` vs.
/// `toggle_clarification_recording`) and read back in
/// `stop_recording_now` to route the finished transcript. The main
/// Ctrl+R flow and "answer this clarification by voice" share the same
/// `AudioRecorder`/`STTInterface` pipeline (there's only one
/// microphone), so this is what tells `stop_recording_now` which of
/// `Connection::process_transcription`/`Connection::resolve_clarification`
/// the transcript is actually for.
#[derive(Clone, Debug, PartialEq, Default)]
enum RecordingTarget {
    #[default]
    Transcript,
    Clarification(String),
}

/// The "Patients" tab's staleness-tracking state: the most recent
/// search query and the most recently selected patient id this
/// controller actually sent to the server, so a response that arrives
/// after the user has since typed something else or selected a
/// different patient can be discarded instead of clobbering whatever
/// they're now looking at — the same pattern
/// `controllers::receptionist::ReceptionistController::last_sent_query`
/// uses for its own search box. Kept in its own `Arc<Mutex<_>>` (like
/// `AiPanelState`) rather than a field on `DoctorController` so
/// `listen_patients`'s task never has to carry that controller's
/// non-`Send` `recorder` field — see `listen_queue`'s doc comment for
/// the same reasoning.
#[derive(Default)]
struct PatientFileTrackingState {
    last_query: String,
    last_selected_id: String,
    /// The patient this controller most recently auto-fetched a file
    /// for because they were `"with_doctor"` — the Dashboard tab's
    /// "in the room now" panel, kept separate from `last_selected_id`
    /// (the "Patients" tab's doctor-initiated search) so the two
    /// features' fetches never clobber each other even when they
    /// happen to be the same patient. See `listen_queue`.
    current_patient_id: String,
}

/// Owns the doctor dashboard's recorder and in-session recordings list
/// for as long as the window is open, plus the queue service used by
/// the "incoming patients" dashboard (THYME_UI_SPEC.md §40
/// "Dashboard"). Kept as `Rc` (not `Arc`) because `recorder` holds a
/// `cpal` stream that isn't `Send`; the queue-snapshot listener is
/// spawned as a free task below instead of a method on this struct so
/// it doesn't need this whole controller to be `Send` — see its doc
/// comment.
struct DoctorController {
    /// `None` when no input device was available at window-open time
    /// (see `install`) — the shortcut then just reports an error instead
    /// of silently doing nothing.
    recorder: Option<Rc<RefCell<AudioRecorder>>>,
    transcriber: Option<Arc<tokio::sync::Mutex<STTInterface>>>,
    queue_service: Rc<QueueService>,
    /// Debounces `shortcut_released`: releasing R starts this timer
    /// instead of stopping the recording right away. A re-press before
    /// it fires (`begin_recording`) cancels it and recording continues
    /// as one uninterrupted take — the underlying `cpal` stream was
    /// never actually stopped — instead of being chopped into a burst
    /// of tiny recordings by a flickery key release. See
    /// `DoctorController::RELEASE_DEBOUNCE`.
    pending_stop: RefCell<Option<slint::Timer>>,
    /// Set once a transcription finishes successfully, to flip the
    /// transcription panel from "done" back to "idle" a little while
    /// later (see `DoctorController::TRANSCRIPT_DISMISS_DELAY`).
    /// Replaced (dropping the old timer) whenever a new recording is
    /// sent off for transcription, so an in-flight take never gets its
    /// panel yanked away by a stale timer from an earlier one.
    recording_generation: Arc<AtomicUsize>,
    /// Kept (not just belt-and-suspenders, like it used to be) so
    /// `stop_recording_now` can fire `ProcessTranscriptionPacket` the
    /// moment a transcript comes back, without needing a whole
    /// `AiService` in a `Send` context — see that method.
    connection: Connection,
    /// The AI review panel's current suggestions — see `AiPanelState`.
    ai_state: Arc<Mutex<AiPanelState>>,
    /// What the *next* recording to stop should be used for — see
    /// `RecordingTarget`. Set the moment a recording starts
    /// (`begin_recording`/`toggle_clarification_recording`), read (and
    /// reset back to `Transcript`) in `stop_recording_now`.
    recording_target: RefCell<RecordingTarget>,
}

/// Wires the doctor window's shortcut callbacks to a fresh
/// `AudioRecorder`, its dashboard to the live "incoming patients" queue,
/// and its settings/sign-out overlays to `services::settings_service`.
pub fn install(window: &DoctorWindow, connection: Connection) {
    let queue_service = Rc::new(QueueService::new(connection.clone()));

    // Created up front (rather than down with the "Patients" tab
    // wiring below, where it used to live) because `listen_queue` now
    // needs it too — the Dashboard tab's "patient in the room" panel
    // fetches through the exact same `PatientService`/tracking state
    // the "Patients" tab search uses, so the two never talk to the
    // server through subtly different paths.
    let patient_service = Arc::new(PatientService::new(connection.clone()));
    let patient_tracking = Arc::new(Mutex::new(PatientFileTrackingState::default()));
    // The device-ingestion pipeline's (`server::dicom`) counterpart to
    // `patient_service` — created alongside it and shared the same
    // tracking state, since both are keyed off exactly the same two
    // "which patient is this for" contexts (the Dashboard's "in the
    // room now" panel and the "Patients" tab's selection).
    let device_service = Arc::new(DeviceService::new(connection.clone()));

    // Subscribed before `listen_queue` below — and before anything else
    // in `install` — for the same reason `listen_queue` itself wants to
    // go early (see its call site's comment): `listen_queue`'s task can
    // discover a `with_doctor` patient from its very first (possibly
    // replayed) snapshot and immediately call
    // `patient_service.request_file(...)`. `PatientService::subscribe`
    // establishes its underlying `Connection::subscribe` synchronously,
    // in-line, before spawning anything — so as long as this call
    // happens first, on this same thread, the receiver that will carry
    // that request's `PatientFileReady`/`PatientFileFailed` response is
    // guaranteed to already exist by the time `listen_queue`'s spawned
    // task could possibly run and fire that request, no matter how the
    // runtime schedules the two tasks. Calling this any later leaves a
    // real (if narrow) window on a multi-threaded runtime where that
    // very first auto-fetched chart's response arrives with no
    // subscriber and is silently dropped — the dashboard is then stuck
    // on "Loading ... chart…" until the room changes again.
    listen_patients(
        patient_service.clone(),
        device_service.clone(),
        patient_tracking.clone(),
        window.as_weak(),
    );

    // Subscribed here too, for the identical reason `listen_patients`
    // is subscribed above and before `listen_queue` — `listen_queue`'s
    // very first reaction to a `with_doctor` patient also fires
    // `device_service.request_patient_items(...)` (see below), and that
    // response needs a subscriber already in place.
    listen_device(
        device_service.clone(),
        patient_service.clone(),
        patient_tracking.clone(),
        window.as_weak(),
    );

    // Called right after `listen_patients` (see the comment above) —
    // still early, but the exact ordering relative to the rest of
    // `install` no longer matters for the initial `PatientQueueSnapshot`
    // itself the way it once did: the server's connect-time push is now
    // cached on `Connection` (`last_queue_snapshot`) and replayed to
    // whoever calls `QueueService::subscribe`, however late. What *does*
    // still need `listen_patients` to have already run is this
    // function's own reaction to that snapshot — see above.
    listen_queue(
        queue_service.clone(),
        patient_service.clone(),
        device_service.clone(),
        patient_tracking.clone(),
        window.as_weak(),
    );

    let recorder = match AudioRecorder::new() {
        Ok(recorder) => Some(Rc::new(RefCell::new(recorder))),
        Err(e) => {
            window.set_recording_error_text(format!("No microphone available: {e}").into());
            None
        }
    };

    // Loads whatever this terminal last saved so the settings panel
    // (and the sign-out-confirmation check below) reflect it immediately,
    // without waiting for the user to open Settings first — and so the
    // saved gain is actually in effect from the first recording, not
    // just after the settings panel has been opened and saved once.
    let initial_settings = Settings::load();
    if let Some(recorder) = &recorder {
        recorder.borrow().set_gain(initial_settings.mic_gain);
    }

    let Ok(stt_api_key) = STTService::new(connection.clone()).get_stt_api_key() else {
        panic!("Expected STT API key.");
    };

    let controller = Rc::new(DoctorController {
        transcriber: Some(Arc::new(tokio::sync::Mutex::new(STTInterface::new(
            stt_api_key,
        )))),
        recorder,
        queue_service: queue_service.clone(),
        pending_stop: RefCell::new(None),
        recording_generation: Arc::new(AtomicUsize::new(0)),
        connection: connection.clone(),
        ai_state: Arc::new(Mutex::new(AiPanelState::default())),
        recording_target: RefCell::new(RecordingTarget::default()),
    });

    let ai_service = Rc::new(AiService::new(connection.clone()));
    listen_ai(
        ai_service.clone(),
        patient_service.clone(),
        window.as_weak(),
        controller.ai_state.clone(),
    );

    let accept_ai_service = ai_service.clone();
    window.on_suggestion_accepted(move |suggestion_id| {
        let _ = accept_ai_service.apply_suggestion(suggestion_id.to_string());
    });

    let reject_ai_service = ai_service.clone();
    window.on_suggestion_rejected(move |suggestion_id| {
        let _ = reject_ai_service.reject_suggestion(suggestion_id.to_string());
    });

    let approve_all_ai_service = ai_service.clone();
    let approve_all_state = controller.ai_state.clone();
    window.on_approve_all_requested(move || {
        let transcription_id = approve_all_state.lock().unwrap().transcription_id.clone();
        if let Some(transcription_id) = transcription_id {
            let _ = approve_all_ai_service.apply_all_suggestions(transcription_id);
        }
    });

    // A clarification answered by typing/choosing (rather than by
    // voice — that path is `on_clarification_voice_toggled` below,
    // which only gets the answer once the dictated recording has been
    // transcribed) goes straight to the server.
    let answer_ai_service = ai_service.clone();
    window.on_clarification_answer_submitted(move |suggestion_id, answer| {
        let _ =
            answer_ai_service.resolve_clarification(suggestion_id.to_string(), answer.to_string());
    });

    let voice_controller = controller.clone();
    let voice_weak = window.as_weak();
    window.on_clarification_voice_toggled(move |suggestion_id| {
        voice_controller
            .clone()
            .toggle_clarification_recording(voice_weak.clone(), suggestion_id.to_string());
    });

    // The "Patients" tab (sidebar index 1, `PatientFileView`): search
    // for any patient on file — checked in today or not — and open
    // their full chart. `PatientService` is reused verbatim from
    // `controllers::receptionist`'s dashboard search rather than
    // duplicated, so both the receptionist and doctor terminals talk
    // to the server through the exact same request/formatting path.

    let patients_search_service = patient_service.clone();
    let patients_search_tracking = patient_tracking.clone();
    let patients_search_weak = window.as_weak();
    window.on_patients_search_edited(move |query| {
        handle_patients_search_edited(
            &patients_search_service,
            &patients_search_tracking,
            &patients_search_weak,
            query.to_string(),
        );
    });

    let patients_select_service = patient_service.clone();
    let patients_select_device_service = device_service.clone();
    let patients_select_tracking = patient_tracking.clone();
    let patients_select_weak = window.as_weak();
    window.on_patients_patient_selected(move |patient_id| {
        handle_patients_patient_selected(
            &patients_select_service,
            &patients_select_device_service,
            &patients_select_tracking,
            &patients_select_weak,
            patient_id.to_string(),
        );
    });

    // Load the default "everyone" list immediately, rather than
    // leaving the "Patients" tab blank until the doctor types
    // something — mirrors whatever's already in `patients-search-query`
    // (empty on a fresh window) through the exact same path a real
    // search uses.
    handle_patients_search_edited(
        &patient_service,
        &patient_tracking,
        &window.as_weak(),
        window.get_patients_search_query().to_string(),
    );

    // Device-originated (DICOM) actions from either `PatientFileCard`
    // instantiation (see `doctor-window.slint`'s shared
    // `save-oct-image`/`device-suggestion-accepted`/
    // `device-suggestion-rejected` window callbacks) — routed straight
    // to `device_service`; the result comes back through
    // `listen_device`, same request/response split every other
    // service in this controller uses.
    let save_image_service = device_service.clone();
    window.on_save_oct_image(move |image_id| {
        let _ = save_image_service.request_image_data(image_id.to_string());
    });

    let accept_device_service = device_service.clone();
    window.on_device_suggestion_accepted(move |suggestion_id| {
        let _ = accept_device_service.apply_suggestion(suggestion_id.to_string());
    });

    let reject_device_service = device_service.clone();
    window.on_device_suggestion_rejected(move |suggestion_id| {
        let _ = reject_device_service.reject_suggestion(suggestion_id.to_string());
    });

    apply_settings_to_window(window, &initial_settings);

    let press_controller = controller.clone();
    let press_weak = window.as_weak();
    window.on_shortcut_pressed(move || {
        press_controller.clone().begin_recording(press_weak.clone());
    });

    let release_controller = controller.clone();
    let release_weak = window.as_weak();
    window.on_shortcut_released(move || {
        release_controller
            .clone()
            .finish_recording(release_weak.clone());
    });

    // The panel's own "×" — lets someone dismiss a completed
    // transcription immediately instead of waiting out
    // `TRANSCRIPT_DISMISS_DELAY`, or clear a failed one after reading
    // the error.
    let dismiss_weak = window.as_weak();
    let dismiss_ai_state = controller.ai_state.clone();
    window.on_transcription_panel_dismissed(move || {
        if let Some(window) = dismiss_weak.upgrade() {
            window.set_transcription_state("idle".into());
            window.set_transcript_error_text("".into());
            dismiss_ai_state.lock().unwrap().reset();
            window.set_ai_processing(false);
            window.set_ai_error_text("".into());
            window.set_ai_transcription_id("".into());
            window.set_ai_suggestions(ai_service::to_suggestion_rows(&[]));
        }
    });

    // "waiting" -> "with_doctor" -> "done", one step at a time — the
    // target status is already computed Slint-side (see
    // `DoctorWindow.next-status` in doctor-window.slint) and handed to
    // us here.
    let advance_controller = controller.clone();
    let advance_weak = window.as_weak();
    window.on_queue_advance_requested(move |queue_id, status| {
        if advance_controller
            .queue_service
            .advance(queue_id.to_string(), status.to_string())
            .is_err()
        {
            if let Some(window) = advance_weak.upgrade() {
                window.set_queue_error_text("Not connected to the server.".into());
                window.set_connection_status_text("Offline — could not reach the server".into());
                window.set_connection_status_tone(ThymeChipTone::Danger);
            }
        }
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

    let settings_save_controller = controller.clone();
    let settings_save_weak = window.as_weak();
    window.on_settings_saved(move || {
        let Some(window) = settings_save_weak.upgrade() else {
            return;
        };
        let settings = settings_from_window(&window);
        match settings.save() {
            Ok(()) => {
                if let Some(recorder) = &settings_save_controller.recorder {
                    recorder.borrow().set_gain(settings.mic_gain);
                }
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
        let Some(window) = sign_out_weak.upgrade() else {
            return;
        };
        if window.get_settings_confirm_sign_out() {
            window.set_show_sign_out_confirm(true);
        } else {
            // There's no server-side logout packet yet — closing the
            // window is enough to end the session.
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
/// properties. Shared by the initial load and the "reload on open"
/// step in `on_settings_requested` so the two never drift apart.
fn apply_settings_to_window(window: &DoctorWindow, settings: &Settings) {
    window.set_settings_mic_gain(settings.mic_gain);
    window.set_settings_confirm_sign_out(settings.confirm_before_sign_out);
}

/// Reads the settings-panel fields back off the window — they're bound
/// `in-out` and edited directly by `ThymeSettingsPanel`, same pattern
/// `controllers::receptionist::handle_create_submitted` uses for the
/// create-patient form.
///
/// `server_address`/`computer_name` aren't editable from this panel
/// (see `design/settings-panel.slint`), so they're carried over as-is
/// from whatever's currently on disk rather than round-tripped through
/// the window, same as `controllers::receptionist::settings_from_window`.
fn settings_from_window(window: &DoctorWindow) -> Settings {
    Settings {
        mic_gain: window.get_settings_mic_gain(),
        confirm_before_sign_out: window.get_settings_confirm_sign_out(),
        ..Settings::load()
    }
}

/// Forwards `queue_service`'s update stream to the dashboard for as
/// long as the connection stays alive. A free function (not a method on
/// `DoctorController`) so this task's `Send` future never has to carry
/// `DoctorController`'s non-`Send` `recorder` field — it only closes
/// over the `Rc`-free `QueueService`/`Weak<DoctorWindow>` it's handed.
/// What `listen_queue` should do about the Dashboard's "patient in the
/// room" chart panel for this snapshot, decided once up front (see
/// `listen_queue`) rather than re-derived in both places that act on
/// it (the fetch itself, and the loading-state update).
enum CurrentPatientFileChange {
    /// The with-doctor patient is unchanged since the last snapshot —
    /// nothing to do. `Snapshot` fires on every queue change (a new
    /// registration, a *different* patient advancing), not just when
    /// the room's occupant does, so this is the common case.
    Unchanged,
    /// No patient is with the doctor anymore — clear the panel.
    Cleared,
    /// A new patient just became `"with_doctor"` — fetch their file.
    Patient(String),
}

fn listen_queue(
    queue_service: Rc<QueueService>,
    patient_service: Arc<PatientService>,
    device_service: Arc<DeviceService>,
    tracking: Arc<Mutex<PatientFileTrackingState>>,
    weak: slint::Weak<DoctorWindow>,
) {
    let mut updates = queue_service.subscribe();

    tokio::spawn(async move {
        loop {
            match updates.recv().await {
                Ok(QueueUpdate::Snapshot(entries)) => {
                    let waiting_count =
                        entries.iter().filter(|e| e.status == "waiting").count() as i32;

                    // Voice recording is only meaningful (and only
                    // accepted by the server — see
                    // `packets::ai::ProcessTranscriptionPacket`) while
                    // a patient is actually `"with_doctor"` right now,
                    // so a transcript can be paired with their history.
                    // This terminal only ever has one patient in the
                    // room at a time, so the first (oldest) match is
                    // the one that matters.
                    let with_doctor = entries.iter().find(|e| e.status == "with_doctor").cloned();

                    // `entries` is oldest-first (see
                    // `QueueRepository::list_active`), so the last
                    // `"waiting"` entry is the most recently
                    // registered one still waiting — what "Begin
                    // encounter" (`DoctorWindow.latest-waiting-queue-id`)
                    // starts.
                    let latest_waiting = entries
                        .iter()
                        .rev()
                        .find(|e| e.status == "waiting")
                        .map(|e| e.queue_id.clone())
                        .unwrap_or_default();

                    let file_change = {
                        let mut tracked = tracking.lock().unwrap();
                        let new_id = with_doctor
                            .as_ref()
                            .map(|e| e.patient_id.clone())
                            .unwrap_or_default();
                        if tracked.current_patient_id == new_id {
                            CurrentPatientFileChange::Unchanged
                        } else {
                            tracked.current_patient_id = new_id.clone();
                            if new_id.is_empty() {
                                CurrentPatientFileChange::Cleared
                            } else {
                                CurrentPatientFileChange::Patient(new_id)
                            }
                        }
                    };

                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_queue_entries(queue_service::to_queue_rows(&entries));
                        window.set_waiting_count(waiting_count);
                        window.set_queue_error_text("".into());
                        window.set_latest_waiting_queue_id(latest_waiting.into());

                        match with_doctor {
                            Some(entry) => {
                                window.set_has_patient_with_doctor(true);
                                window.set_current_queue_id(entry.queue_id.into());
                                window.set_current_patient_id(entry.patient_id.into());
                                window.set_current_patient_name(
                                    entry.preferred_name.unwrap_or(entry.legal_name).into(),
                                );
                            }
                            None => {
                                window.set_has_patient_with_doctor(false);
                                window.set_current_queue_id("".into());
                                window.set_current_patient_id("".into());
                                window.set_current_patient_name("".into());
                            }
                        }
                    });

                    match file_change {
                        CurrentPatientFileChange::Unchanged => {}
                        CurrentPatientFileChange::Cleared => {
                            let _ = weak.upgrade_in_event_loop(move |window| {
                                window.set_current_patient_file_loading(false);
                                window.set_current_patient_has_file(false);
                                window.set_current_patient_file_error_text("".into());
                            });
                        }
                        CurrentPatientFileChange::Patient(patient_id) => {
                            let _ = weak.upgrade_in_event_loop(move |window| {
                                window.set_current_patient_file_loading(true);
                                window.set_current_patient_has_file(false);
                                window.set_current_patient_file_error_text("".into());
                            });
                            // Device data has no separate loading/error
                            // UI of its own — it fills in once
                            // `listen_device` hears back, or simply
                            // stays empty if nothing's on file yet, so
                            // there's nothing to reset here beyond the
                            // chart-loading state above.
                            let _ = device_service.request_patient_items(patient_id.clone());
                            if patient_service.request_file(patient_id).is_err() {
                                let _ = weak.upgrade_in_event_loop(move |window| {
                                    window.set_current_patient_file_loading(false);
                                    window.set_current_patient_file_error_text(
                                        "Not connected to the server.".into(),
                                    );
                                    window.set_connection_status_text(
                                        "Offline — could not reach the server".into(),
                                    );
                                    window.set_connection_status_tone(ThymeChipTone::Danger);
                                });
                            }
                        }
                    }
                }
                Ok(QueueUpdate::AdvanceFailed(message)) => {
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_queue_error_text(message.into());
                    });
                }
                Ok(QueueUpdate::RegisterFailed(_)) => {
                    // The doctor client never sends
                    // `PatientQueueRegisterRequest` — only
                    // `controllers::receptionist` does — so this
                    // variant can't be produced here. Ignored
                    // defensively rather than matched away entirely,
                    // so a future doctor-side registration feature
                    // doesn't silently fail to show its errors.
                }
                Ok(QueueUpdate::Disconnected) => {
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_connection_status_text(
                            "Offline — could not reach the server".into(),
                        );
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

/// Handles `patients-search-edited`. A free function (not a method on
/// `DoctorController`) for the same `Send`-boundary reason
/// `listen_queue`/`listen_patients` are — it's called synchronously
/// from the Slint callback, but keeping it alongside its counterpart
/// below rather than as a controller method keeps the whole "Patients"
/// tab's plumbing in one place.
/// Fires a search for whatever's in the "Patients" tab's box —
/// including an empty box. An empty query used to be handled entirely
/// locally (clear the results, never touch the server), which is
/// exactly why the list defaulted to blank instead of showing the
/// patient roster; now it's sent through like any other query, and
/// `PatientRepository::search` treats an empty query as "list
/// everyone" server-side — mirrors
/// `controllers::receptionist::ReceptionistController::handle_search_edited`.
fn handle_patients_search_edited(
    patient_service: &Arc<PatientService>,
    tracking: &Arc<Mutex<PatientFileTrackingState>>,
    weak: &slint::Weak<DoctorWindow>,
    query: String,
) {
    let trimmed = query.trim().to_string();

    tracking.lock().unwrap().last_query = trimmed.clone();

    let Some(window) = weak.upgrade() else { return };
    window.set_patients_search_busy(true);
    window.set_patients_search_error_text("".into());

    if patient_service.search(trimmed).is_err() {
        window.set_patients_search_busy(false);
        window.set_patients_search_error_text("Not connected to the server.".into());
        window.set_connection_status_text("Offline — could not reach the server".into());
        window.set_connection_status_tone(ThymeChipTone::Danger);
    }
}

/// Handles `patients-patient-selected` — opens the chosen patient's
/// file. Per `PatientFileView`'s doc comment, this works for *any*
/// patient on file, whether or not they're currently checked in.
fn handle_patients_patient_selected(
    patient_service: &Arc<PatientService>,
    device_service: &Arc<DeviceService>,
    tracking: &Arc<Mutex<PatientFileTrackingState>>,
    weak: &slint::Weak<DoctorWindow>,
    patient_id: String,
) {
    let Some(window) = weak.upgrade() else { return };
    window.set_patients_selected_patient_id(patient_id.clone().into());

    if patient_id.is_empty() {
        tracking.lock().unwrap().last_selected_id = String::new();
        window.set_patients_file_loading(false);
        window.set_patients_has_file(false);
        window.set_patients_file_error_text("".into());
        return;
    }

    tracking.lock().unwrap().last_selected_id = patient_id.clone();
    window.set_patients_file_loading(true);
    window.set_patients_has_file(false);
    window.set_patients_file_error_text("".into());

    // Same "no dedicated loading UI" reasoning as the Dashboard's
    // auto-fetch (see `listen_queue`) — device data just fills in
    // whenever `listen_device` hears back.
    let _ = device_service.request_patient_items(patient_id.clone());

    if patient_service.request_file(patient_id).is_err() {
        window.set_patients_file_loading(false);
        window.set_patients_file_error_text("Not connected to the server.".into());
        window.set_connection_status_text("Offline — could not reach the server".into());
        window.set_connection_status_tone(ThymeChipTone::Danger);
    }
}

/// Forwards `patient_service`'s update stream to the "Patients" tab for
/// as long as the connection stays alive — the `listen_queue` of the
/// patient-file feature. A free function for the same reason
/// `listen_queue`/`listen_ai` are: its `Send` future must never have to
/// carry `DoctorController`'s non-`Send` `recorder` field, so it only
/// closes over the `Send + Sync` pieces it actually needs.
fn listen_patients(
    patient_service: Arc<PatientService>,
    device_service: Arc<DeviceService>,
    tracking: Arc<Mutex<PatientFileTrackingState>>,
    weak: slint::Weak<DoctorWindow>,
) {
    let mut updates = patient_service.subscribe();

    tokio::spawn(async move {
        loop {
            match updates.recv().await {
                Ok(PatientSearchUpdate::Results { query, results }) => {
                    if tracking.lock().unwrap().last_query != query {
                        // A newer search has since been fired; this
                        // response is stale.
                        continue;
                    }
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_patients_search_busy(false);
                        window.set_patients_search_error_text("".into());
                        window.set_patients_search_results(patient_service::to_patient_rows(
                            &results,
                        ));
                    });
                }
                Ok(PatientSearchUpdate::Failed { query, message }) => {
                    if tracking.lock().unwrap().last_query != query {
                        continue;
                    }
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_patients_search_busy(false);
                        window.set_patients_search_error_text(message.into());
                    });
                }
                Ok(PatientSearchUpdate::FileReady { patient, history }) => {
                    // A single `PatientFileResponse` can be relevant to
                    // either or both of this controller's two file
                    // consumers — the "Patients" tab's doctor-initiated
                    // search (`last_selected_id`) and the Dashboard's
                    // automatic "patient in the room" panel
                    // (`current_patient_id`, set by `listen_queue`) —
                    // since both request through the same
                    // `PatientService` and neither response is tagged
                    // with who asked for it. Checked once up front so a
                    // response stale for both (a third patient has
                    // since been selected/roomed) is discarded outright.
                    let (is_selected, is_current) = {
                        let tracked = tracking.lock().unwrap();
                        (
                            tracked.last_selected_id == patient.patient_id,
                            tracked.current_patient_id == patient.patient_id,
                        )
                    };
                    if !is_selected && !is_current {
                        continue;
                    }

                    // `PatientFileData` wraps `ModelRc<HistoryEntryRow>`,
                    // which holds an `Rc` and so isn't `Send` — it can't
                    // be built out here and then captured into this
                    // closure, since `upgrade_in_event_loop` requires a
                    // `Send` closure (the closure itself just gets
                    // handed off to the UI thread's event loop; it
                    // isn't run here). `patient`/`history` are plain
                    // `String`/`Vec<_>` wire records, so those are fine
                    // to capture — the `ModelRc` is only ever
                    // constructed once we're already on the UI thread.

                    let device_service = device_service.clone();
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        let data: PatientFileData =
                            patient_service::to_patient_file_data(&patient, &history);
                        if is_selected {
                            window.set_patients_file_loading(false);
                            window.set_patients_file_error_text("".into());
                            window.set_patients_has_file(true);
                            window.set_patients_file(data.clone());
                        }
                        if is_current {
                            window.set_current_patient_file_loading(false);
                            window.set_current_patient_file_error_text("".into());
                            window.set_current_patient_has_file(true);
                            window.set_current_patient_file(data);
                        }

                        let _ = device_service.request_patient_items(patient.patient_id.clone());
                    });
                }
                Ok(PatientSearchUpdate::FileFailed {
                    patient_id,
                    message,
                }) => {
                    let (is_selected, is_current) = {
                        let tracked = tracking.lock().unwrap();
                        (
                            tracked.last_selected_id == patient_id,
                            tracked.current_patient_id == patient_id,
                        )
                    };
                    if !is_selected && !is_current {
                        continue;
                    }
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        if is_selected {
                            window.set_patients_file_loading(false);
                            window.set_patients_has_file(false);
                            window.set_patients_file_error_text(message.clone().into());
                        }
                        if is_current {
                            window.set_current_patient_file_loading(false);
                            window.set_current_patient_has_file(false);
                            window.set_current_patient_file_error_text(message.into());
                        }
                    });
                }
                Ok(PatientSearchUpdate::Created { .. })
                | Ok(PatientSearchUpdate::CreateFailed { .. }) => {
                    // The doctor client never sends
                    // `PatientCreateRequest` — only
                    // `controllers::receptionist` does — so these
                    // variants can't be produced here. Ignored
                    // defensively rather than matched away entirely,
                    // same as `QueueUpdate::RegisterFailed` in
                    // `listen_queue`.
                }
                Ok(PatientSearchUpdate::Disconnected) => {
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_connection_status_text(
                            "Offline — could not reach the server".into(),
                        );
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

/// Forwards `ai_service`'s update stream to the AI review panel for as
/// long as the connection stays alive — the `listen_queue` of the AI
/// pipeline. A free function for the same reason `listen_queue` is:
/// its `Send` future must never have to carry `DoctorController`'s
/// non-`Send` `recorder` field, so it only closes over the `Rc`-free
/// pieces it actually needs (`Rc<AiService>` is fine here since it's
/// never itself moved into a `Send` context — only the receiver its
/// `subscribe()` call returns is).
fn listen_ai(
    ai_service: Rc<AiService>,
    patient_service: Arc<PatientService>,
    weak: slint::Weak<DoctorWindow>,
    state: Arc<Mutex<AiPanelState>>,
) {
    let mut updates = ai_service.subscribe();

    // Re-requests the current transcript's patient file so the
    // Dashboard's "in the room now" panel (and the "Patients" tab, if
    // it happens to have the same chart open) pick up whatever a just-
    // applied suggestion changed server-side — the response comes back
    // through the exact same `PatientSearchUpdate::FileReady` path
    // `listen_patients` already handles, so this just has to fire the
    // request; it doesn't need to touch the window itself.
    let refresh_patient_file = {
        let patient_service = patient_service.clone();
        let state = state.clone();
        move || {
            let patient_id = state.lock().unwrap().patient_id.clone();
            if let Some(patient_id) = patient_id {
                let _ = patient_service.request_file(patient_id);
            }
        }
    };

    tokio::spawn(async move {
        loop {
            match updates.recv().await {
                Ok(AiUpdate::SuggestionsReady {
                    transcription_id,
                    patient_id,
                    suggestions,
                }) => {
                    {
                        let mut state = state.lock().unwrap();
                        state.transcription_id = Some(transcription_id.clone());
                        state.patient_id = Some(patient_id);
                        state.suggestions = suggestions;
                    }
                    let state = state.clone();
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        let state = state.lock().unwrap();
                        window.set_ai_processing(false);
                        window.set_ai_error_text("".into());
                        window.set_ai_transcription_id(transcription_id.into());
                        window
                            .set_ai_suggestions(ai_service::to_suggestion_rows(&state.suggestions));
                    });
                }
                Ok(AiUpdate::ProcessingFailed(message)) => {
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_ai_processing(false);
                        window.set_ai_error_text(message.into());
                    });
                }
                Ok(AiUpdate::SuggestionApplied {
                    suggestion_id,
                    summary: _,
                }) => {
                    let state = state.clone();
                    {
                        state.lock().unwrap().mark_status(&suggestion_id, "applied");
                    }
                    refresh_patient_file();
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        let state = state.lock().unwrap();
                        window
                            .set_ai_suggestions(ai_service::to_suggestion_rows(&state.suggestions));
                    });
                }
                Ok(AiUpdate::SuggestionApplyFailed {
                    suggestion_id: _,
                    message,
                }) => {
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_ai_error_text(message.into());
                    });
                }
                Ok(AiUpdate::SuggestionRejected { suggestion_id }) => {
                    let state = state.clone();
                    {
                        state
                            .lock()
                            .unwrap()
                            .mark_status(&suggestion_id, "rejected");
                    }
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        let state = state.lock().unwrap();
                        window
                            .set_ai_suggestions(ai_service::to_suggestion_rows(&state.suggestions));
                    });
                }
                Ok(AiUpdate::SuggestionRejectFailed {
                    suggestion_id: _,
                    message,
                }) => {
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_ai_error_text(message.into());
                    });
                }
                Ok(AiUpdate::ClarificationResolved {
                    suggestion_id,
                    description,
                    status,
                }) => {
                    let state = state.clone();
                    // Resolving a clarification can land the action
                    // straight on `"applied"` (`ThymeAction::auto_apply`)
                    // instead of going back to `"pending"` for a normal
                    // accept — that already-applied case still changed
                    // the patient's chart server-side, so it needs the
                    // same refresh a `SuggestionApplied` gets.
                    let just_applied = status == "applied";
                    {
                        state.lock().unwrap().resolve_clarification(
                            &suggestion_id,
                            description,
                            status,
                        );
                    }
                    if just_applied {
                        refresh_patient_file();
                    }
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        let state = state.lock().unwrap();
                        window
                            .set_ai_suggestions(ai_service::to_suggestion_rows(&state.suggestions));
                    });
                }
                Ok(AiUpdate::ClarificationResolveFailed {
                    suggestion_id: _,
                    message,
                }) => {
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_ai_error_text(message.into());
                    });
                }
                Ok(AiUpdate::AllApplied {
                    transcription_id: _,
                    results,
                }) => {
                    let state = state.clone();
                    let any_failed = results.iter().any(|r| !r.success);
                    let any_applied = results.iter().any(|r| r.success);
                    {
                        let mut state = state.lock().unwrap();
                        for result in &results {
                            if result.success {
                                state.mark_status(&result.suggestion_id, "applied");
                            }
                        }
                    }
                    if any_applied {
                        refresh_patient_file();
                    }
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        let state = state.lock().unwrap();
                        window.set_ai_suggestions(ai_service::to_suggestion_rows(&state.suggestions));
                        if any_failed {
                            window.set_ai_error_text(
                                "Some suggestions couldn't be applied. Please try them individually.".into(),
                            );
                        }
                    });
                }
                Ok(AiUpdate::AllApplyFailed {
                    transcription_id: _,
                    message,
                }) => {
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_ai_error_text(message.into());
                    });
                }
                Ok(AiUpdate::Disconnected) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
            refresh_patient_file()
        }
    });
}

/// Forwards `device_service`'s update stream to both `PatientFileCard`
/// panels for as long as the connection stays alive — the `listen_ai`
/// of the device-ingestion pipeline (`server::dicom`). A free function
/// for the same `Send`-boundary reason `listen_queue`/`listen_ai`/
/// `listen_patients` are.
///
/// Unlike `AiPanelState`, there's no local copy of the suggestion list
/// to keep in sync here: `DeviceItemsReady` always carries the full,
/// current set for one patient (mirrors how `PatientFileReady` always
/// carries a full chart, not a delta), so applying/rejecting a
/// suggestion just means removing that one row from whichever of the
/// two window-held `PatientFileData`s has it, then re-requesting both
/// that patient's chart (only if the reading was actually applied) and
/// their device items (either way, so a superseded/resolved reading
/// doesn't linger) — see `resolve_device_suggestion` below.
fn listen_device(
    device_service: Arc<DeviceService>,
    patient_service: Arc<PatientService>,
    tracking: Arc<Mutex<PatientFileTrackingState>>,
    weak: slint::Weak<DoctorWindow>,
) {
    let mut updates = device_service.subscribe();

    tokio::spawn(async move {
        loop {
            match updates.recv().await {
                Ok(DeviceUpdate::ItemsReady {
                    patient_id,
                    oct_images,
                    pending_suggestions,
                }) => {
                    let (is_selected, is_current) = {
                        let tracked = tracking.lock().unwrap();
                        println!(
                            "Device update for patient {patient_id}: last_selected_id={}, current_patient_id={}",
                            tracked.last_selected_id, tracked.current_patient_id
                        );
                        (
                            tracked.last_selected_id == patient_id,
                            tracked.current_patient_id == patient_id,
                        )
                    };
                    if !is_selected && !is_current {
                        println!(
                            "Ignoring device update for patient {patient_id} because it's not selected or current"
                        );
                        // A newer selection/room change has since
                        // superseded this patient — same staleness
                        // check `listen_patients` makes for
                        // `PatientSearchUpdate::FileReady`.
                        continue;
                    }

                    println!(
                        "Updating device items for patient {patient_id}: {} OCT images, {} pending suggestions",
                        oct_images.len(),
                        pending_suggestions.len()
                    );
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        let oct_rows = device_service::to_oct_image_rows(&oct_images);
                        let suggestion_rows =
                            device_service::to_device_suggestion_rows(&pending_suggestions);
                        if is_selected {
                            let mut file = window.get_patients_file();
                            file.oct_images = oct_rows.clone();
                            file.pending_device_suggestions = suggestion_rows.clone();
                            window.set_patients_file(file);
                        }
                        if is_current {
                            let mut file = window.get_current_patient_file();
                            file.oct_images = oct_rows;
                            file.pending_device_suggestions = suggestion_rows;
                            window.set_current_patient_file(file);
                        }
                    });
                }
                Ok(DeviceUpdate::ItemsFailed {
                    patient_id,
                    message,
                }) => {
                    let (is_selected, is_current) = {
                        let tracked = tracking.lock().unwrap();
                        (
                            tracked.last_selected_id == patient_id,
                            tracked.current_patient_id == patient_id,
                        )
                    };
                    if !is_selected && !is_current {
                        continue;
                    }
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_device_status_text(message.into());
                    });
                }
                Ok(DeviceUpdate::SuggestionApplied {
                    suggestion_id,
                    summary: _,
                }) => {
                    resolve_device_suggestion(
                        &weak,
                        &tracking,
                        &patient_service,
                        &device_service,
                        suggestion_id,
                        true,
                    );
                }
                Ok(DeviceUpdate::SuggestionApplyFailed {
                    suggestion_id: _,
                    message,
                }) => {
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_device_status_text(message.into());
                    });
                }
                Ok(DeviceUpdate::SuggestionRejected { suggestion_id }) => {
                    resolve_device_suggestion(
                        &weak,
                        &tracking,
                        &patient_service,
                        &device_service,
                        suggestion_id,
                        false,
                    );
                }
                Ok(DeviceUpdate::SuggestionRejectFailed {
                    suggestion_id: _,
                    message,
                }) => {
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_device_status_text(message.into());
                    });
                }
                Ok(DeviceUpdate::ImageDataReady {
                    image_id,
                    pixel_data,
                    ..
                }) => {
                    // A plain synchronous `fs::write` (see
                    // `device_service::save_oct_image`) rather than
                    // `spawn_blocking` — a single OCT slice's worth of
                    // bytes is small enough that this doesn't
                    // meaningfully stall the task, the same judgment
                    // call `services::settings_service::Settings::save`
                    // already makes for this app's other local writes.
                    let status = match device_service::save_oct_image(&image_id, &pixel_data) {
                        Ok(path) => format!("Saved image to {}", path.display()),
                        Err(e) => format!("Could not save image: {e}"),
                    };
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_oct_status_text(status.into());
                    });
                }
                Ok(DeviceUpdate::ImageDataFailed {
                    image_id: _,
                    message,
                }) => {
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window
                            .set_oct_status_text(format!("Could not load image: {message}").into());
                    });
                }
                Ok(DeviceUpdate::Disconnected) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Removes `suggestion_id` from a `SuggestionRow` list, reporting
/// whether it was actually present — shared by both branches
/// `resolve_device_suggestion` below handles.
fn remove_suggestion_row(
    list: &ModelRc<SuggestionRow>,
    suggestion_id: &str,
) -> (ModelRc<SuggestionRow>, bool) {
    let mut found = false;
    let rows: Vec<SuggestionRow> = list
        .iter()
        .filter(|r| {
            let matches = r.suggestion_id.as_str() == suggestion_id;
            found = found || matches;
            !matches
        })
        .collect();
    (ModelRc::new(VecModel::from(rows)), found)
}

/// Shared handling for `DeviceUpdate::SuggestionApplied`/`SuggestionRejected`
/// — see `listen_device`'s doc comment for the overall approach.
/// `refresh_chart` is `true` only for an applied suggestion (a
/// rejection never touches the patient's chart, so there's nothing new
/// for `patient_service` to fetch); either way, `device_service` is
/// re-queried so the now-resolved reading stops showing as pending.
fn resolve_device_suggestion(
    weak: &slint::Weak<DoctorWindow>,
    tracking: &Arc<Mutex<PatientFileTrackingState>>,
    patient_service: &Arc<PatientService>,
    device_service: &Arc<DeviceService>,
    suggestion_id: String,
    refresh_chart: bool,
) {
    let tracking = tracking.clone();
    let patient_service = patient_service.clone();
    let device_service = device_service.clone();
    let _ = weak.upgrade_in_event_loop(move |window| {
        window.set_device_status_text("".into());

        let mut current = window.get_current_patient_file();
        let (new_current, removed_from_current) =
            remove_suggestion_row(&current.pending_device_suggestions, &suggestion_id);
        current.pending_device_suggestions = new_current;
        window.set_current_patient_file(current);

        let mut selected = window.get_patients_file();
        let (new_selected, removed_from_selected) =
            remove_suggestion_row(&selected.pending_device_suggestions, &suggestion_id);
        selected.pending_device_suggestions = new_selected;
        window.set_patients_file(selected);

        if removed_from_current {
            let id = tracking.lock().unwrap().current_patient_id.clone();
            if !id.is_empty() {
                if refresh_chart {
                    let _ = patient_service.request_file(id.clone());
                }
                let _ = device_service.request_patient_items(id);
            }
        }
        if removed_from_selected {
            let id = tracking.lock().unwrap().last_selected_id.clone();
            if !id.is_empty() {
                if refresh_chart {
                    let _ = patient_service.request_file(id.clone());
                }
                let _ = device_service.request_patient_items(id);
            }
        }
    });
}

impl DoctorController {
    /// How long `finish_recording` waits after a release before it
    /// actually stops the stream. Chosen to comfortably absorb OS
    /// key-repeat jitter and a fumbled quick re-press without making a
    /// deliberate stop feel laggy.
    const RELEASE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);

    /// How long a successful transcription's panel stays up in its
    /// "done" state (showing the transcript) before auto-dismissing
    /// back to "idle". A failed transcription is left up indefinitely
    /// instead — see `stop_recording_now` — since an error shouldn't
    /// disappear before it's been read.
    const TRANSCRIPT_DISMISS_DELAY: std::time::Duration = std::time::Duration::from_secs(4);

    fn begin_recording(self: Rc<Self>, weak: slint::Weak<DoctorWindow>) {
        let Some(window) = weak.upgrade() else { return };

        // A stop is still pending from a very recent release — the
        // stream was never actually stopped, so this press just
        // continues that same recording instead of starting a new one.
        if self.pending_stop.borrow_mut().take().is_some() {
            return;
        }

        // Recording only makes sense — and is only accepted by the
        // server (`packets::ai::ProcessTranscriptionPacket`'s doc
        // comment) — while a patient is actually `"with_doctor"` right
        // now, so the transcript can be paired with their history.
        // `has-patient-with-doctor` is kept current by `listen_queue`
        // from the same queue snapshot the dashboard renders, so this
        // is never stale by more than one queue update.
        if !window.get_has_patient_with_doctor() {
            window.set_recording_error_text(
                "Start this patient's visit before recording a voice note.".into(),
            );
            return;
        }

        let Some(recorder) = &self.recorder else {
            window.set_recording_error_text("No microphone available.".into());
            return;
        };

        if window.get_recording_active() {
            // The OS repeats key-pressed events while a key is held —
            // this is expected and just means "still holding", not a
            // new press. Recording is already running (either this same
            // take, or a clarification voice-answer — see
            // `RecordingTarget`); nothing to do either way.
            return;
        }

        *self.recording_target.borrow_mut() = RecordingTarget::Transcript;

        // `start_recording`'s error callback is invoked from cpal's own
        // audio thread, so it must be `Send + Sync`. `slint::Weak<T>` is
        // `Send` but not `Sync`, so it has to be shuttled through a
        // `Mutex` (which is `Sync` whenever its contents are `Send`)
        // rather than captured directly.
        let error_weak: Arc<Mutex<slint::Weak<DoctorWindow>>> = Arc::new(Mutex::new(weak.clone()));
        let start_result = recorder.borrow_mut().start_recording(move || {
            let weak = error_weak.lock().unwrap().clone();
            let _ = weak.upgrade_in_event_loop(move |window| {
                window.set_recording_active(false);
                window.set_recording_error_text("Recording device error. Please try again.".into());
            });
        });

        match start_result {
            Ok(()) => {
                window.set_recording_active(true);
                window.set_recording_error_text("".into());
            }
            Err(e) => {
                window.set_recording_error_text(format!("Could not start recording: {e}").into());
            }
        }
    }

    /// Handles `shortcut_released`. Doesn't stop the recording itself —
    /// see `RELEASE_DEBOUNCE` — it just schedules `stop_recording_now`
    /// and lets a follow-up `begin_recording` cancel it if R comes back
    /// down in time.
    fn finish_recording(self: Rc<Self>, weak: slint::Weak<DoctorWindow>) {
        let Some(window) = weak.upgrade() else { return };

        if self.recorder.is_none() {
            return;
        }

        if !window.get_recording_active() {
            // R was released without a matching held-and-recording
            // start (e.g. Ctrl/Shift let go first, or an earlier device
            // error already stopped things) — nothing to save.
            return;
        }

        let stop_controller = self.clone();
        let stop_weak = weak.clone();
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::SingleShot,
            Self::RELEASE_DEBOUNCE,
            move || {
                stop_controller
                    .clone()
                    .stop_recording_now(stop_weak.clone());
            },
        );
        *self.pending_stop.borrow_mut() = Some(timer);
    }

    /// Actually stops the stream and persists the take, once
    /// `RELEASE_DEBOUNCE` has elapsed without a re-press cancelling it.
    fn stop_recording_now(self: Rc<Self>, weak: slint::Weak<DoctorWindow>) {
        self.pending_stop.borrow_mut().take();

        let Some(window) = weak.upgrade() else { return };
        let Some(recorder) = &self.recorder else {
            return;
        };

        if !window.get_recording_active() {
            return;
        }

        if matches!(
            *self.recording_target.borrow(),
            RecordingTarget::Clarification(_)
        ) {
            // A clarification voice-answer recording is what's actually
            // running right now (see `RecordingTarget`) — that's
            // stopped via `stop_clarification_recording`, not this one.
            // (Reachable if R is somehow released while answering a
            // clarification by voice; harmless either way.)
            return self.stop_clarification_recording(weak);
        }

        window.set_recording_active(false);

        let audio = match recorder.borrow_mut().stop_recording() {
            Ok(file) => file,
            Err(e) => {
                window.set_recording_error_text(format!("Could not stop recording: {e}").into());
                return;
            }
        };

        let Some(transcriber) = self.transcriber.clone() else {
            return;
        };

        // Increment the generation for this new recording
        let current_gen = self.recording_generation.fetch_add(1, Ordering::SeqCst) + 1;

        window.set_transcript_error_text("".into());
        window.set_transcription_state("transcribing".into());

        // A fresh take supersedes whatever the AI review panel was
        // showing for the previous one — cleared here (not just on
        // dismiss) so a doctor who starts a new recording before
        // dismissing the last one doesn't see stale suggestions sitting
        // under the new transcript once it comes back.
        self.ai_state.lock().unwrap().reset();
        window.set_ai_processing(false);
        window.set_ai_error_text("".into());
        window.set_ai_transcription_id("".into());
        window.set_ai_suggestions(ai_service::to_suggestion_rows(&[]));

        // The patient currently `"with_doctor"` — captured now (not
        // inside the spawned task, which can't touch `window`) so it
        // travels with this specific take even if the queue changes
        // before transcription finishes. `connection` likewise has to
        // be captured here rather than read off `self`, since the
        // spawned task below must stay `Send` and `self` (via
        // `recorder`) isn't.
        let patient_id = window.get_current_patient_id().to_string();
        let connection = self.connection.clone();

        let stt_weak = window.as_weak();
        let gen_arc = self.recording_generation.clone();

        tokio::spawn(async move {
            let result = {
                let guard = transcriber.lock().await;
                guard.transcribe_audio(audio, 3).await
            };

            // Store the boolean before `result` is consumed by the closure below
            let is_ok = result.is_ok();
            let stt_weak_for_ui = stt_weak.clone();

            // Hand the transcript off for AI processing as soon as it's
            // available, rather than waiting on the UI-thread closure
            // below — `connection.process_transcription` just queues a
            // packet write, so there's no reason to delay it behind a
            // round-trip through the event loop. Skipped when the
            // patient id is empty (no one currently `"with_doctor"` at
            // the moment this take started) rather than sent anyway —
            // the server would reject it either way, but there's no
            // point paying for the round trip only to show an error
            // that "start a visit" already covers.
            if let Ok(transcript) = &result {
                if !patient_id.is_empty() {
                    let _ = slint::invoke_from_event_loop({
                        let stt_weak = stt_weak.clone();
                        move || {
                            if let Some(window) = stt_weak.upgrade() {
                                window.set_ai_processing(true);
                            }
                        }
                    });
                    let _ =
                        connection.process_transcription(patient_id.clone(), transcript.clone());
                }
            }

            let _ = slint::invoke_from_event_loop(move || {
                let Some(window) = stt_weak_for_ui.upgrade() else {
                    return;
                };

                match result {
                    Ok(transcript) => {
                        window.set_last_transcript(transcript.into());
                        window.set_transcription_state("done".into());
                    }
                    Err(e) => {
                        window
                            .set_transcript_error_text(format!("Transcription failed: {e}").into());
                        window.set_transcription_state("error".into());
                    }
                }
            });

            // Handle the auto-dismiss asynchronously instead of using slint::Timer
            if is_ok {
                tokio::time::sleep(DoctorController::TRANSCRIPT_DISMISS_DELAY).await;

                // Only dismiss if a new recording hasn't been started since this one finished
                if gen_arc.load(Ordering::SeqCst) == current_gen {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(window) = stt_weak.upgrade() {
                            if window.get_transcription_state() == "done" {
                                // Prevent auto-dismiss if the AI is still processing,
                                // has returned suggestions for review, or threw an error.
                                let is_processing = window.get_ai_processing();
                                let has_suggestions = window.get_ai_suggestions().row_count() > 0;
                                let has_error = !window.get_ai_error_text().is_empty();

                                if !is_processing && !has_suggestions && !has_error {
                                    window.set_transcription_state("idle".into());
                                }
                            }
                        }
                    });
                }
            }
        });
    }

    /// Starts or stops a clarification's voice-answer recording,
    /// toggled by the suggestion card's mic button
    /// (`clarification_voice_toggled`). Shares the same
    /// `AudioRecorder`/`STTInterface` pipeline as the main Ctrl+R flow
    /// (see `RecordingTarget`) — pressed while a *different* recording
    /// (the main transcript, or another suggestion's answer) is already
    /// in progress, this is a no-op rather than stealing the mic out
    /// from under it.
    fn toggle_clarification_recording(
        self: Rc<Self>,
        weak: slint::Weak<DoctorWindow>,
        suggestion_id: String,
    ) {
        let Some(window) = weak.upgrade() else { return };

        let already_recording_this = window.get_recording_active()
            && *self.recording_target.borrow()
                == RecordingTarget::Clarification(suggestion_id.clone());

        if already_recording_this {
            self.stop_clarification_recording(weak);
            return;
        }

        if window.get_recording_active() {
            window.set_recording_error_text(
                "Still recording something else — finish that first.".into(),
            );
            return;
        }

        let Some(recorder) = &self.recorder else {
            window.set_recording_error_text("No microphone available.".into());
            return;
        };

        *self.recording_target.borrow_mut() = RecordingTarget::Clarification(suggestion_id.clone());

        // See `begin_recording`'s matching comment: `start_recording`'s
        // error callback runs on cpal's own audio thread, so the weak
        // handle has to travel through a `Mutex` to stay `Sync`.
        let error_weak: Arc<Mutex<slint::Weak<DoctorWindow>>> = Arc::new(Mutex::new(weak.clone()));
        let start_result = recorder.borrow_mut().start_recording(move || {
            let weak = error_weak.lock().unwrap().clone();
            let _ = weak.upgrade_in_event_loop(move |window| {
                window.set_recording_active(false);
                window.set_recording_clarification_id("".into());
                window.set_recording_error_text("Recording device error. Please try again.".into());
            });
        });

        match start_result {
            Ok(()) => {
                window.set_recording_active(true);
                window.set_recording_error_text("".into());
                window.set_recording_clarification_id(suggestion_id.into());
            }
            Err(e) => {
                *self.recording_target.borrow_mut() = RecordingTarget::Transcript;
                window.set_recording_error_text(format!("Could not start recording: {e}").into());
            }
        }
    }

    /// Stops a clarification voice-answer recording started by
    /// `toggle_clarification_recording`, transcribes it, and sends the
    /// result on as that suggestion's answer
    /// (`Connection::resolve_clarification`) — the clarification-answer
    /// counterpart of `stop_recording_now`.
    fn stop_clarification_recording(self: Rc<Self>, weak: slint::Weak<DoctorWindow>) {
        let Some(window) = weak.upgrade() else { return };
        let Some(recorder) = &self.recorder else {
            return;
        };

        let RecordingTarget::Clarification(suggestion_id) =
            self.recording_target.replace(RecordingTarget::Transcript)
        else {
            return;
        };

        window.set_recording_active(false);
        window.set_recording_clarification_id("".into());

        let audio = match recorder.borrow_mut().stop_recording() {
            Ok(file) => file,
            Err(e) => {
                window.set_recording_error_text(format!("Could not stop recording: {e}").into());
                return;
            }
        };

        let Some(transcriber) = self.transcriber.clone() else {
            return;
        };

        let connection = self.connection.clone();
        let stt_weak = weak.clone();

        tokio::spawn(async move {
            let result = {
                let guard = transcriber.lock().await;
                guard.transcribe_audio(audio, 3).await
            };

            match result {
                Ok(answer) => {
                    let _ = connection.resolve_clarification(suggestion_id, answer);
                }
                Err(e) => {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(window) = stt_weak.upgrade() {
                            window.set_ai_error_text(
                                format!("Could not transcribe answer: {e}").into(),
                            );
                        }
                    });
                }
            }
        });
    }
}
