use auth::EncryptedStream;
use packet_datatypes::{Packet, PacketInfo, PacketReader};
use packets::{
    ai::{
        AiProcessingError, AiSuggestionsResponse, AppliedActionResult,
        ApplyAllSuggestedActionsError, ApplyAllSuggestedActionsRequest,
        ApplyAllSuggestedActionsResponse, ApplySuggestedActionError, ApplySuggestedActionRequest,
        ApplySuggestedActionResponse, ProcessTranscriptionPacket, RejectSuggestedActionError,
        RejectSuggestedActionRequest, RejectSuggestedActionResponse, ResolveClarificationError,
        ResolveClarificationRequest, ResolveClarificationResponse, SuggestedActionRecord,
    },
    auth::{
        AuthChallenge, AuthResponse, AuthStatus, RequestSTTAPIKeyPacket, STTAPIKeyResponsePacket,
    },
    device::{
        ApplyDeviceSuggestionError, ApplyDeviceSuggestionRequest, ApplyDeviceSuggestionResponse,
        DevicePatientItemsError, DevicePatientItemsRequest, DevicePatientItemsResponse,
        DeviceSuggestedActionRecord, OctImageDataError, OctImageDataRequest, OctImageDataResponse,
        OctImageRecord as OctImageRecordPacket, RejectDeviceSuggestionError,
        RejectDeviceSuggestionRequest, RejectDeviceSuggestionResponse,
    },
    hello::{Hello, HelloOperationMode},
    keepalive::{PING_PACKET, Pong},
    patient::{
        PatientCreateError, PatientCreateRequest, PatientCreateResponse, PatientFileError,
        PatientFileRequest, PatientFileResponse, PatientSearchError, PatientSearchRequest,
        PatientSearchResponse,
    },
    queue::{
        PatientQueueAdvanceError, PatientQueueAdvanceRequest, PatientQueueRegisterError,
        PatientQueueRegisterRequest, PatientQueueSnapshot,
    },
    stream::FramedStream,
};
use sqlx::Pool;
use std::{collections::HashMap, io, net::SocketAddr, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{RwLock, broadcast},
    time::{Duration, interval},
};
use uuid::Uuid;

use crate::{
    ai::AiProcessor,
    db::{
        AiRepository, DeviceRepository, HistoryRepository, NewPatient, OctImageRecord,
        PatientRepository, QueueRepository, UserRepository,
    },
    error::Result,
};

/// Statuses a queue entry can be advanced through, in order. Shared
/// between validation and the doctor-facing "advance" action so both
/// stay in sync with the same list rather than duplicating it.
const QUEUE_STATUSES: [&str; 3] = ["waiting", "with_doctor", "done"];

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ClientMode {
    Receptionist,
    Doctor,
}

impl From<HelloOperationMode> for ClientMode {
    fn from(mode: HelloOperationMode) -> Self {
        match mode {
            HelloOperationMode::Receptionist => ClientMode::Receptionist,
            HelloOperationMode::Doctor => ClientMode::Doctor,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientSession;

pub struct ThymeServer {
    pub listen_addr: SocketAddr,
    pub active_clients: RwLock<HashMap<SocketAddr, ClientSession>>,

    pub user_repo: UserRepository,
    pub patient_repo: PatientRepository,
    pub queue_repo: QueueRepository,
    pub history_repo: HistoryRepository,
    pub ai_repo: AiRepository,
    /// Data access for device-originated (DICOM) studies and staged
    /// refraction suggestions — see `crate::dicom`. Backs the
    /// `DevicePatientItemsRequest`/`ApplyDeviceSuggestionRequest`/
    /// `RejectDeviceSuggestionRequest`/`OctImageDataRequest` handlers
    /// below, the same way `ai_repo` backs the AI-suggestion review
    /// packets.
    pub device_repo: DeviceRepository,
    ai_processor: AiProcessor,

    stt_api_key: String,

    /// Fires whenever the patient queue changes (a registration or a
    /// status advance). Every connected client's task holds its own
    /// `Receiver` and, on a tick, re-reads `queue_repo.list_active()`
    /// and pushes a fresh `PatientQueueSnapshot` — see
    /// `handle_connection`. Carrying only a change signal rather than
    /// the snapshot itself keeps every client's view built from one
    /// source of truth (the database) instead of racing to keep N
    /// in-memory copies consistent.
    queue_changed: broadcast::Sender<()>,

    /// The device-ingestion pipeline's (`crate::dicom`) counterpart to
    /// `queue_changed` — fires with a patient id whenever a DICOM study
    /// lands on that patient's chart (see `dicom::oct`/`dicom::refraction`).
    /// Needed for a reason `queue_changed` isn't: `crate::dicom::DicomScp`
    /// runs as its own listener on its own port (see `main.rs`),
    /// entirely outside the request/response loop below, so without
    /// this there would be no way for a client already viewing a
    /// patient (most commonly the Dashboard's "in the room now" panel —
    /// exactly when a device is likely to be actively scanning them) to
    /// ever learn new data arrived for them, short of switching away
    /// and back. Carries a patient id rather than `()` since — unlike
    /// the queue, which is one global list every client shows in full —
    /// device data is per patient, and the server has no notion of
    /// "which patient is this client currently looking at" to decide
    /// who needs a push; every client's task gets every tick and
    /// reapplies the exact same is-this-relevant-to-me check
    /// `DevicePatientItemsRequest`'s handler already needs regardless
    /// (`list_device_items` takes a `patient_id`; the id here just
    /// tells a client which patient to ask about, the same as if it had
    /// decided to ask on its own).
    device_items_changed: broadcast::Sender<String>,
}

impl ThymeServer {
    pub fn new(listen_addr: SocketAddr, db_pool: Pool<sqlx::Postgres>) -> Self {
        Self::with_device_items_changed(listen_addr, db_pool, broadcast::channel(16).0)
    }

    /// Like `new`, but sharing an existing `device_items_changed`
    /// sender rather than creating one of its own — what `main.rs`
    /// calls, so the same sender can also be handed to
    /// `dicom::DicomScp::from_env`. `new` above stays the plain,
    /// no-DICOM-awareness constructor other tests/tools can keep using
    /// without needing to know this channel exists.
    pub fn with_device_items_changed(
        listen_addr: SocketAddr,
        db_pool: Pool<sqlx::Postgres>,
        device_items_changed: broadcast::Sender<String>,
    ) -> Self {
        let (queue_changed, _) = broadcast::channel(16);

        Self {
            listen_addr,
            active_clients: RwLock::new(HashMap::new()),

            user_repo: UserRepository::new(db_pool.clone()),
            patient_repo: PatientRepository::new(db_pool.clone()),
            queue_repo: QueueRepository::new(db_pool.clone()),
            history_repo: HistoryRepository::new(db_pool.clone()),
            ai_repo: AiRepository::new(db_pool.clone()),
            device_repo: DeviceRepository::new(db_pool),
            ai_processor: AiProcessor::from_env(),
            queue_changed,
            device_items_changed,

            stt_api_key: std::env::var("STT_API_KEY").expect("STT_API_KEY must be set"),
        }
    }

    pub async fn run(self: Arc<Self>) -> Result<()> {
        let listener = TcpListener::bind(self.listen_addr).await?;
        loop {
            let (stream, peer) = listener.accept().await?;
            let engine = self.clone();
            tokio::spawn(async move {
                if let Err(e) = engine.handle_connection(stream, peer).await {
                    eprintln!("connection from {peer} ended: {e}");
                }
            });
        }
    }

    async fn handle_connection(&self, client_tcp: TcpStream, peer: SocketAddr) -> Result<()> {
        client_tcp.set_nodelay(true)?;

        let mut unencrypted_client = FramedStream::new(client_tcp);

        let hello_packet = unencrypted_client.read_packet::<Hello>().await?;
        println!(
            "{} connected and identified as: {} operating in mode {:?}",
            peer, hello_packet.computer_name, hello_packet.mode
        );

        let mut auth_success = false;
        let mut final_auth_data = None;

        for attempt in 1..=3 {
            let challenge_string = format!("challenge-{}", rand::random::<u64>());
            unencrypted_client
                .write_packet(&AuthChallenge {
                    challenge_string: challenge_string.clone(),
                    available_users: self
                        .user_repo
                        .list_active()
                        .await?
                        .into_iter()
                        .map(|user| (user.username, user.display_name))
                        .collect(),
                })
                .await?;

            let auth_response = unencrypted_client.read_packet::<AuthResponse>().await?;

            if let Some(user) = self
                .verify_auth(&auth_response.username, &auth_response.password_hash)
                .await
            {
                let roles = user.roles.iter().map(|role| role.name.clone()).collect();
                unencrypted_client
                    .write_packet(&AuthStatus {
                        success: true,
                        roles,
                    })
                    .await?;
                auth_success = true;
                final_auth_data = Some((challenge_string, auth_response, user.display_name));
                break;
            } else {
                unencrypted_client
                    .write_packet(&AuthStatus {
                        success: false,
                        roles: Vec::new(),
                    })
                    .await?;
                println!("{} failed authentication (attempt {}/3)", peer, attempt);

                if attempt == 3 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "Max authentication attempts reached",
                    )
                    .into());
                }
            }
        }

        if !auth_success {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Auth failed").into());
        }

        println!("{} successfully authenticated.", peer);

        let raw_tcp = unencrypted_client.into_inner();
        let (challenge, auth_data, display_name) = final_auth_data.unwrap();

        let encrypted_stream = EncryptedStream::new(
            raw_tcp,
            &challenge,
            &auth_data.username,
            &auth_data.password_hash,
        );

        let mut client = FramedStream::new(encrypted_stream);

        self.active_clients.write().await.insert(
            peer,
            ClientSession {
                // computer_name: hello_packet.computer_name,
                // mode: hello_packet.mode.into(),
            },
        );

        // Every client — receptionist or doctor — gets the queue's
        // current state as soon as it authenticates, the same way it
        // gets `available_users` before authenticating; without this a
        // doctor who signs in after patients are already waiting would
        // see an empty dashboard until the next unrelated change.
        match self.build_queue_snapshot().await {
            Ok(snapshot) => client.write_packet(&snapshot).await?,
            Err(e) => eprintln!("{peer} initial queue snapshot failed: {e}"),
        }
        let mut queue_rx = self.queue_changed.subscribe();
        let mut device_items_rx = self.device_items_changed.subscribe();

        let mut ping_interval = interval(Duration::from_secs(5));
        ping_interval.tick().await;
        let mut waiting_for_pong = false;

        let result: Result<()> = loop {
            tokio::select! {
                _ = ping_interval.tick() => {
                    if waiting_for_pong && false { // FIXME: temporarily disabled because the client doesn't respond to pings yet
                        break Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "Client failed to respond to Ping within 5 seconds"
                        ).into());
                    }

                    client.write_packet(&PING_PACKET).await?;
                    waiting_for_pong = true;
                }

                queue_notice = queue_rx.recv() => {
                    match queue_notice {
                        Ok(()) => match self.build_queue_snapshot().await {
                            Ok(snapshot) => client.write_packet(&snapshot).await?,
                            Err(e) => eprintln!("{peer} queue snapshot failed: {e}"),
                        },
                        // A burst of changes coalesces into one refetch
                        // instead of one packet per missed notification —
                        // the snapshot is always the full current state
                        // anyway, so there's nothing to catch up on.
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => {}
                    }
                }

                device_notice = device_items_rx.recv() => {
                    match device_notice {
                        Ok(patient_id) => match Uuid::parse_str(&patient_id) {
                            Ok(patient_id) => match self.list_device_items(patient_id).await {
                                Ok((oct_images, pending_suggestions)) => {
                                    client.write_packet(&DevicePatientItemsResponse {
                                        patient_id: patient_id.to_string(),
                                        oct_images,
                                        pending_suggestions,
                                    }).await?
                                }
                                // Unlike the on-demand handler below,
                                // there's no request to answer with a
                                // `DevicePatientItemsError` for — a
                                // failed unsolicited refresh just means
                                // this client's view is a beat behind
                                // until the next change, not that
                                // anything the client did failed.
                                Err(e) => eprintln!("{peer} device items push failed: {e}"),
                            },
                            // Every id this ever carries came from
                            // `dicom::oct`/`dicom::refraction` calling
                            // `Uuid::to_string()` on a real
                            // `patients.id`, so this is unreachable in
                            // practice — guarded rather than unwrapped
                            // only because nothing enforces that at the
                            // type level across the two crates.
                            Err(_) => {}
                        },
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => {}
                    }
                }

                packet_res = client.read_any_packet() => {
                    let (id, payload) = match packet_res {
                        Ok(p) => p,
                        Err(e) => break Err(crate::error::ServerError::Packet(e)),
                    };

                    match id {
                        Pong::ID => {
                            waiting_for_pong = false;
                        },
                        PatientSearchRequest::ID => {
                            let request = match PatientSearchRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };
                            let query = request.query;

                            // NOTE: this only checks that the connection completed the
                            // handshake as a Receptionist/Doctor terminal, not the
                            // authenticated user's actual permissions. Real permission
                            // checks (per `role_permissions`) belong here once the
                            // session is tied back to a resolved `User`.
                            match self.patient_repo.search(&query).await {
                                Ok(matches) => {
                                    let results = matches.into_iter().map(Into::into).collect();
                                    client.write_packet(&PatientSearchResponse { query, results }).await?;
                                }
                                Err(e) => {
                                    eprintln!("{peer} patient search failed: {e}");
                                    client.write_packet(&PatientSearchError {
                                        query,
                                        message: "Search failed. Please try again.".to_string(),
                                    }).await?;
                                }
                            }
                        },
                        PatientCreateRequest::ID => {
                            let request = match PatientCreateRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };

                            match Self::validate_new_patient(request) {
                                Ok(new_patient) => match self.patient_repo.create(new_patient).await {
                                    Ok(patient) => {
                                        client.write_packet(&PatientCreateResponse { patient: patient.into() }).await?;
                                    }
                                    Err(e) => {
                                        eprintln!("{peer} patient creation failed: {e}");
                                        client.write_packet(&PatientCreateError {
                                            message: "Could not save the new patient. Please try again.".to_string(),
                                        }).await?;
                                    }
                                },
                                Err(message) => {
                                    client.write_packet(&PatientCreateError { message }).await?;
                                }
                            }
                        },
                        PatientFileRequest::ID => {
                            let request = match PatientFileRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };
                            let patient_id_str = request.patient_id;

                            // NOTE: same caveat as patient search/create
                            // above — this only checks that the
                            // connection completed the handshake, not
                            // that this particular user (today, only
                            // the doctor dashboard sends this) is
                            // allowed to view this patient's file.
                            match Uuid::parse_str(&patient_id_str) {
                                Ok(patient_id) => {
                                    let patient = match self.patient_repo.get_full_by_id(patient_id).await {
                                        Ok(Some(patient)) => patient,
                                        Ok(None) => {
                                            client.write_packet(&PatientFileError {
                                                patient_id: patient_id_str,
                                                message: "Unknown patient.".to_string(),
                                            }).await?;
                                            continue;
                                        }
                                        Err(e) => {
                                            eprintln!("{peer} patient file lookup failed: {e}");
                                            client.write_packet(&PatientFileError {
                                                patient_id: patient_id_str,
                                                message: "Could not open the patient's file. Please try again.".to_string(),
                                            }).await?;
                                            continue;
                                        }
                                    };

                                    match self.history_repo.fetch(patient_id).await {
                                        Ok(history) => {
                                            client.write_packet(&PatientFileResponse {
                                                patient: patient.into(),
                                                history: history.into(),
                                            }).await?;
                                        }
                                        Err(e) => {
                                            eprintln!("{peer} patient history fetch failed: {e}");
                                            client.write_packet(&PatientFileError {
                                                patient_id: patient_id_str,
                                                message: "Could not open the patient's file. Please try again.".to_string(),
                                            }).await?;
                                        }
                                    }
                                }
                                Err(_) => {
                                    client.write_packet(&PatientFileError {
                                        patient_id: patient_id_str,
                                        message: "Invalid patient id.".to_string(),
                                    }).await?;
                                }
                            }
                        },
                        PatientQueueRegisterRequest::ID => {
                            let request = match PatientQueueRegisterRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };

                            // NOTE: same caveat as patient search/create
                            // above — this only checks that the
                            // connection completed the handshake, not
                            // that this particular user is allowed to
                            // register patients.
                            match Uuid::parse_str(&request.patient_id) {
                                Ok(patient_id) => {
                                    let note = request.note.as_deref().map(str::trim).filter(|n| !n.is_empty());
                                    match self.queue_repo.register(patient_id, &display_name, note).await {
                                        Ok(_) => {
                                            // The requester's own view
                                            // updates through the same
                                            // broadcast as everyone
                                            // else's — see `queue_changed`
                                            // — rather than a dedicated ack.
                                            let _ = self.queue_changed.send(());
                                        }
                                        Err(e) => {
                                            eprintln!("{peer} queue registration failed: {e}");
                                            client.write_packet(&PatientQueueRegisterError {
                                                message: "Could not add the patient to the queue. Please try again.".to_string(),
                                            }).await?;
                                        }
                                    }
                                }
                                Err(_) => {
                                    client.write_packet(&PatientQueueRegisterError {
                                        message: "Unknown patient.".to_string(),
                                    }).await?;
                                }
                            }
                        },
                        PatientQueueAdvanceRequest::ID => {
                            let request = match PatientQueueAdvanceRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };

                            // NOTE: same caveat as above — this doesn't
                            // yet check that the connection is a doctor
                            // terminal (`ClientMode::Doctor`), only that
                            // it's an authenticated one.
                            let parsed = Uuid::parse_str(&request.queue_id)
                                .map_err(|_| "Unknown queue entry.".to_string())
                                .and_then(|id| {
                                    if QUEUE_STATUSES.contains(&request.status.as_str()) {
                                        Ok((id, request.status))
                                    } else {
                                        Err("Not a valid queue status.".to_string())
                                    }
                                });

                            // Only one patient can be `"with_doctor"`
                            // at a time — a doctor's dashboard only
                            // shows/records against a single "in the
                            // room now" patient (see
                            // `QueueRepository::is_with_doctor`'s doc
                            // comment), so letting a second entry into
                            // that status would silently orphan
                            // whichever one the UI stops tracking. This
                            // is the trusted check (the client is
                            // expected to warn the doctor and let them
                            // end the current encounter first — see
                            // `DoctorWindow.show-discard-confirm` — but
                            // that's only ever a convenience; this is
                            // what's actually enforced).
                            let parsed = match parsed {
                                Ok((queue_id, status)) if status == "with_doctor" => {
                                    match self.queue_repo.current_with_doctor().await {
                                        Ok(Some(existing)) if existing.id != queue_id => Err(format!(
                                            "{} is currently with the doctor. End that encounter before starting a new one.",
                                            existing.preferred_name.unwrap_or(existing.legal_name)
                                        )),
                                        Ok(_) => Ok((queue_id, status)),
                                        Err(e) => {
                                            eprintln!("{peer} queue with_doctor check failed: {e}");
                                            Err("Could not update the queue. Please try again.".to_string())
                                        }
                                    }
                                }
                                other => other,
                            };

                            match parsed {
                                Ok((queue_id, status)) => match self.queue_repo.advance_status(queue_id, &status).await {
                                    Ok(_) => {
                                        let _ = self.queue_changed.send(());
                                    }
                                    Err(e) => {
                                        eprintln!("{peer} queue advance failed: {e}");
                                        client.write_packet(&PatientQueueAdvanceError {
                                            message: "Could not update the queue. Please try again.".to_string(),
                                        }).await?;
                                    }
                                },
                                Err(message) => {
                                    client.write_packet(&PatientQueueAdvanceError { message }).await?;
                                }
                            }
                        },
                        RequestSTTAPIKeyPacket::ID => {
                            client.write_packet(&STTAPIKeyResponsePacket {
                                key: self.stt_api_key.clone()
                            }).await?;
                        }
                        ProcessTranscriptionPacket::ID => {
                            let request = match ProcessTranscriptionPacket::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };

                            // NOTE: same caveat as patient search/create
                            // above — this only checks that the
                            // connection completed the handshake, not
                            // that this particular user is allowed to
                            // record a voice note. The one check that
                            // *is* enforced here (never trusting the
                            // client's own Ctrl+R gating) is that the
                            // named patient is actually "with_doctor"
                            // right now — see `process_transcription`.
                            match Uuid::parse_str(&request.patient_id) {
                                Ok(patient_id) => {
                                    match self.process_transcription(patient_id, request.transcript, &display_name).await {
                                        Ok(response) => client.write_packet(&response).await?,
                                        Err(message) => client.write_packet(&AiProcessingError { message }).await?,
                                    }
                                }
                                Err(_) => {
                                    client.write_packet(&AiProcessingError {
                                        message: "Unknown patient.".to_string(),
                                    }).await?;
                                }
                            }
                        },
                        ApplySuggestedActionRequest::ID => {
                            let request = match ApplySuggestedActionRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };

                            match Uuid::parse_str(&request.suggestion_id) {
                                Ok(suggestion_id) => match self.apply_suggestion(suggestion_id, &display_name).await {
                                    Ok(summary) => client.write_packet(&ApplySuggestedActionResponse {
                                        suggestion_id: request.suggestion_id,
                                        summary,
                                    }).await?,
                                    Err(message) => client.write_packet(&ApplySuggestedActionError {
                                        suggestion_id: request.suggestion_id,
                                        message,
                                    }).await?,
                                },
                                Err(_) => {
                                    client.write_packet(&ApplySuggestedActionError {
                                        suggestion_id: request.suggestion_id,
                                        message: "Unknown suggestion.".to_string(),
                                    }).await?;
                                }
                            }
                        },
                        RejectSuggestedActionRequest::ID => {
                            let request = match RejectSuggestedActionRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };

                            match Uuid::parse_str(&request.suggestion_id) {
                                Ok(suggestion_id) => match self.reject_suggestion(suggestion_id).await {
                                    Ok(()) => client.write_packet(&RejectSuggestedActionResponse {
                                        suggestion_id: request.suggestion_id,
                                    }).await?,
                                    Err(message) => client.write_packet(&RejectSuggestedActionError {
                                        suggestion_id: request.suggestion_id,
                                        message,
                                    }).await?,
                                },
                                Err(_) => {
                                    client.write_packet(&RejectSuggestedActionError {
                                        suggestion_id: request.suggestion_id,
                                        message: "Unknown suggestion.".to_string(),
                                    }).await?;
                                }
                            }
                        },
                        ResolveClarificationRequest::ID => {
                            let request = match ResolveClarificationRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };

                            match Uuid::parse_str(&request.suggestion_id) {
                                Ok(suggestion_id) => match self.resolve_clarification(suggestion_id, &request.answer).await {
                                    Ok((description, status)) => client.write_packet(&ResolveClarificationResponse {
                                        suggestion_id: request.suggestion_id,
                                        description,
                                        status,
                                    }).await?,
                                    Err(message) => client.write_packet(&ResolveClarificationError {
                                        suggestion_id: request.suggestion_id,
                                        message,
                                    }).await?,
                                },
                                Err(_) => {
                                    client.write_packet(&ResolveClarificationError {
                                        suggestion_id: request.suggestion_id,
                                        message: "Unknown suggestion.".to_string(),
                                    }).await?;
                                }
                            }
                        },
                        ApplyAllSuggestedActionsRequest::ID => {
                            let request = match ApplyAllSuggestedActionsRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };

                            match Uuid::parse_str(&request.transcription_id) {
                                Ok(transcription_id) => match self.apply_all_pending(transcription_id, &display_name).await {
                                    Ok(results) => client.write_packet(&ApplyAllSuggestedActionsResponse {
                                        transcription_id: request.transcription_id,
                                        results,
                                    }).await?,
                                    Err(message) => client.write_packet(&ApplyAllSuggestedActionsError {
                                        transcription_id: request.transcription_id,
                                        message,
                                    }).await?,
                                },
                                Err(_) => {
                                    client.write_packet(&ApplyAllSuggestedActionsError {
                                        transcription_id: request.transcription_id,
                                        message: "Unknown transcript.".to_string(),
                                    }).await?;
                                }
                            }
                        },
                        DevicePatientItemsRequest::ID => {
                            let request = match DevicePatientItemsRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };

                            match Uuid::parse_str(&request.patient_id) {
                                Ok(patient_id) => match self.list_device_items(patient_id).await {
                                    Ok((oct_images, pending_suggestions)) => client.write_packet(&DevicePatientItemsResponse {
                                        patient_id: request.patient_id,
                                        oct_images,
                                        pending_suggestions,
                                    }).await?,
                                    Err(message) => client.write_packet(&DevicePatientItemsError {
                                        patient_id: request.patient_id,
                                        message,
                                    }).await?,
                                },
                                Err(_) => {
                                    client.write_packet(&DevicePatientItemsError {
                                        patient_id: request.patient_id,
                                        message: "Unknown patient.".to_string(),
                                    }).await?;
                                }
                            }
                        },
                        ApplyDeviceSuggestionRequest::ID => {
                            let request = match ApplyDeviceSuggestionRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };

                            match Uuid::parse_str(&request.suggestion_id) {
                                Ok(suggestion_id) => match self.apply_device_suggestion(suggestion_id, &display_name).await {
                                    Ok(summary) => client.write_packet(&ApplyDeviceSuggestionResponse {
                                        suggestion_id: request.suggestion_id,
                                        summary,
                                    }).await?,
                                    Err(message) => client.write_packet(&ApplyDeviceSuggestionError {
                                        suggestion_id: request.suggestion_id,
                                        message,
                                    }).await?,
                                },
                                Err(_) => {
                                    client.write_packet(&ApplyDeviceSuggestionError {
                                        suggestion_id: request.suggestion_id,
                                        message: "Unknown suggestion.".to_string(),
                                    }).await?;
                                }
                            }
                        },
                        RejectDeviceSuggestionRequest::ID => {
                            let request = match RejectDeviceSuggestionRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };

                            match Uuid::parse_str(&request.suggestion_id) {
                                Ok(suggestion_id) => match self.reject_device_suggestion(suggestion_id).await {
                                    Ok(()) => client.write_packet(&RejectDeviceSuggestionResponse {
                                        suggestion_id: request.suggestion_id,
                                    }).await?,
                                    Err(message) => client.write_packet(&RejectDeviceSuggestionError {
                                        suggestion_id: request.suggestion_id,
                                        message,
                                    }).await?,
                                },
                                Err(_) => {
                                    client.write_packet(&RejectDeviceSuggestionError {
                                        suggestion_id: request.suggestion_id,
                                        message: "Unknown suggestion.".to_string(),
                                    }).await?;
                                }
                            }
                        },
                        OctImageDataRequest::ID => {
                            let request = match OctImageDataRequest::decode(&mut PacketReader::new(&payload)) {
                                Ok(request) => request,
                                Err(e) => break Err(crate::error::ServerError::Packet(e)),
                            };

                            match Uuid::parse_str(&request.image_id) {
                                Ok(image_id) => match self.read_oct_image(image_id).await {
                                    Ok(response) => client.write_packet(&response).await?,
                                    Err(message) => client.write_packet(&OctImageDataError {
                                        image_id: request.image_id,
                                        message,
                                    }).await?,
                                },
                                Err(_) => {
                                    client.write_packet(&OctImageDataError {
                                        image_id: request.image_id,
                                        message: "Unknown image.".to_string(),
                                    }).await?;
                                }
                            }
                        },
                        _ => {
                            println!("Received unknown packet ID {} with payload: {:?}", id, payload);
                        }
                    }
                }
            }
        };

        self.active_clients.write().await.remove(&peer);
        println!("{} disconnected and removed from active tracking.", peer);

        result
    }

    /// Basic input validation/normalization for patient registration.
    /// Returns `Err(message)` — suitable to send straight back as a
    /// `PatientCreateError` — rather than a `ServerError`, since these
    /// are client input problems, not server failures.
    ///
    /// NOTE: like patient search above, this doesn't yet check that the
    /// connection's authenticated user actually has permission to
    /// register patients — the session isn't tied back to a resolved
    /// `User` yet. Real permission checks belong here once it is.
    fn validate_new_patient(
        request: PatientCreateRequest,
    ) -> std::result::Result<NewPatient, String> {
        let legal_name = request.legal_name.trim().to_string();
        if legal_name.is_empty() {
            return Err("A legal name is required.".to_string());
        }

        let preferred_name = request
            .preferred_name
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty());

        let birth_date = match request.birth_date.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(raw) => match chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
                Ok(date) => Some(date),
                Err(_) => return Err("Date of birth must be in YYYY-MM-DD format.".to_string()),
            },
        };

        let sex = request
            .sex
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let phone = request
            .phone
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());

        Ok(NewPatient {
            legal_name,
            preferred_name,
            birth_date,
            sex,
            phone,
        })
    }

    /// Returns the authenticated `User` (roles included) on success, so
    /// callers can report role-based information (e.g. `AuthStatus`)
    /// without a second lookup. Also rejects deactivated accounts, which
    /// the previous byte-comparison-only check didn't.
    async fn verify_auth(
        &self,
        username: &str,
        password_hash: &[u8],
    ) -> Option<domain::user::User> {
        match self.user_repo.get_by_username(username).await {
            Ok(Some(user)) if user.is_active && user.password_hash.as_bytes() == password_hash => {
                Some(user)
            }
            _ => None,
        }
    }

    /// Reads the current "incoming patients" queue and packages it as
    /// the full-snapshot packet every client gets right after
    /// authenticating and again after every `queue_changed` tick — see
    /// that field's doc comment and `handle_connection`'s two call
    /// sites above. Pulled out as its own method (rather than inlined
    /// at each call site) so both places build the packet the exact
    /// same way.
    async fn build_queue_snapshot(&self) -> Result<PatientQueueSnapshot> {
        let entries = self.queue_repo.list_active().await?;

        Ok(PatientQueueSnapshot {
            entries: entries.into_iter().map(Into::into).collect(),
        })
    }

    /// Full pipeline for `ProcessTranscriptionPacket`: confirms the
    /// named patient is actually `"with_doctor"` right now (never
    /// trusting the client's own Ctrl+R gating — see that packet's doc
    /// comment), gathers their history, calls the active `AiProvider`,
    /// and persists everything the model proposed.
    ///
    /// Low-risk actions (`ThymeAction::auto_apply`) are written to the
    /// chart immediately as part of this same call — see that method's
    /// doc comment for why this feature deliberately departs from "no
    /// AI result may silently become a clinical fact" for that narrow
    /// set only. Everything else comes back `"pending"` for the doctor
    /// to review via `apply_suggestion`/`apply_all_pending`.
    ///
    /// Returns `Err(message)` — suitable to send straight back as
    /// `AiProcessingError` — for any failure, rather than a
    /// `ServerError`, mirroring `validate_new_patient`'s pattern of
    /// keeping user-facing failure text separate from internal error
    /// plumbing (the real error, where there is one, is still logged).
    async fn process_transcription(
        &self,
        patient_id: Uuid,
        transcript: String,
        authored_by: &str,
    ) -> std::result::Result<AiSuggestionsResponse, String> {
        let transcript = transcript.trim();
        if transcript.is_empty() {
            return Err("The recording didn't transcribe to any text.".to_string());
        }

        match self.queue_repo.is_with_doctor(patient_id).await {
            Ok(true) => {}
            Ok(false) => {
                return Err(
                    "This patient isn't currently with the doctor — start their visit before recording."
                        .to_string(),
                );
            }
            Err(e) => {
                eprintln!("queue lookup failed while processing transcription: {e}");
                return Err("Could not process the recording. Please try again.".to_string());
            }
        }

        let patient = match self.patient_repo.get_by_id(patient_id).await {
            Ok(Some(patient)) => patient,
            Ok(None) => return Err("Unknown patient.".to_string()),
            Err(e) => {
                eprintln!("patient lookup failed while processing transcription: {e}");
                return Err("Could not process the recording. Please try again.".to_string());
            }
        };

        let history = match self.history_repo.fetch(patient_id).await {
            Ok(history) => history,
            Err(e) => {
                eprintln!("history fetch failed while processing transcription: {e}");
                return Err("Could not process the recording. Please try again.".to_string());
            }
        };

        let (actions, raw_response) = match self
            .ai_processor
            .process_transcription(&patient, &history, transcript)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                eprintln!("AI processing failed: {e}");
                return Err(
                    "The AI assistant couldn't process this recording. Please try again."
                        .to_string(),
                );
            }
        };

        let transcription_id = match self
            .ai_repo
            .record_transcription(
                patient_id,
                transcript,
                self.ai_processor.provider_name(),
                &self.ai_processor.model_name(),
                &raw_response,
            )
            .await
        {
            Ok(id) => id,
            Err(e) => {
                eprintln!("failed to record transcription: {e}");
                return Err("Could not save this recording. Please try again.".to_string());
            }
        };

        // Distinguishes an auto-applied chart entry from one a
        // clinician typed themselves, per THYME_DATABASE_SCHEMA.md's
        // "don't lose provenance" stance — `source = "ai_suggested"` on
        // the row already marks it as AI-originated, but `authored_by`
        // being an obviously-not-a-person string keeps that visible in
        // a chart view that only renders `authored_by`.
        let ai_authored_by = format!("AI ({})", self.ai_processor.model_name());
        let mut suggestions = Vec::with_capacity(actions.len());

        for action in &actions {
            // `auto_apply()` is already `false` whenever
            // `action.clarification()` is `Some` (see that method's doc
            // comment), so the two checks below are mutually exclusive —
            // a guess never lands on the chart before a clinician has
            // answered for it.
            let status = if action.auto_apply() {
                "applied"
            } else if action.clarification().is_some() {
                "needs_clarification"
            } else {
                "pending"
            };

            let record = match self
                .ai_repo
                .record_suggested_action(transcription_id, patient_id, action, status, transcript)
                .await
            {
                Ok(record) => record,
                Err(e) => {
                    eprintln!("failed to record suggested action: {e}");
                    continue;
                }
            };

            if status == "applied" {
                if let Err(e) = self
                    .history_repo
                    .apply_action(patient_id, Some(&ai_authored_by), "ai_suggested", action)
                    .await
                {
                    // The suggestion row already says "applied" even
                    // though the chart write itself failed here — still
                    // surfaced to the clinician as a normal suggestion
                    // row (rather than silently dropped) so they at
                    // least see what the model proposed and can add it
                    // manually if it matters.
                    eprintln!("failed to auto-apply suggested action {}: {e}", record.id);
                }
            }

            let (clarification_message, clarification_answer_type, clarification_choices) =
                match action.clarification() {
                    Some(c) => (
                        c.message.clone(),
                        c.answer_type.as_str().to_string(),
                        c.choices.clone(),
                    ),
                    None => (String::new(), String::new(), Vec::new()),
                };

            suggestions.push(SuggestedActionRecord {
                suggestion_id: record.id.to_string(),
                action_type: record.action_type,
                description: record.description,
                payload_json: record.payload_json.to_string(),
                status: record.status,
                clarification_message,
                clarification_answer_type,
                clarification_choices,
            });
        }

        // authored_by (the dictating doctor) isn't used above — every
        // chart write from this call is attributed to `ai_authored_by`
        // instead, since none of them are a clinician's own explicit
        // review yet. Kept as a parameter anyway for symmetry with
        // `apply_suggestion`/`apply_all_pending`, where it *is* the
        // reviewing clinician, and so a future per-doctor audit trail
        // on `transcriptions` has it available without a signature
        // change.
        let _ = authored_by;

        Ok(AiSuggestionsResponse {
            transcription_id: transcription_id.to_string(),
            patient_id: patient_id.to_string(),
            suggestions,
        })
    }

    /// Handles `ApplySuggestedActionRequest`: applies exactly one
    /// still-`"pending"` suggestion, attributing the chart entry to the
    /// clinician who accepted it. Returns `Err(message)` suitable for
    /// `ApplySuggestedActionError` on any failure, including the
    /// suggestion already having been resolved.
    async fn apply_suggestion(
        &self,
        suggestion_id: Uuid,
        authored_by: &str,
    ) -> std::result::Result<String, String> {
        let record = match self.ai_repo.get_pending(suggestion_id).await {
            Ok(Some(record)) => record,
            Ok(None) => return Err("This suggestion was already resolved.".to_string()),
            Err(e) => {
                eprintln!("failed to look up suggestion {suggestion_id}: {e}");
                return Err("Could not apply this suggestion. Please try again.".to_string());
            }
        };

        let action = record.to_action().map_err(|e| {
            eprintln!("{e}");
            "Could not apply this suggestion. Please try again.".to_string()
        })?;

        let summary = self
            .history_repo
            .apply_action(
                record.patient_id,
                Some(authored_by),
                "ai_suggested",
                &action,
            )
            .await
            .map_err(|e| {
                eprintln!("failed to apply suggestion {suggestion_id}: {e}");
                "Could not apply this suggestion. Please try again.".to_string()
            })?;

        if let Err(e) = self.ai_repo.resolve(suggestion_id, "applied").await {
            // The chart entry itself is already written at this point —
            // logged but not surfaced as a failure, since telling the
            // clinician "could not apply" here would be misleading:
            // it *was* applied, only the staging row's bookkeeping
            // failed to update.
            eprintln!("applied suggestion {suggestion_id} but failed to mark it resolved: {e}");
        }

        Ok(summary)
    }

    /// Handles `ResolveClarificationRequest`: folds a clinician's
    /// answer into exactly one still-`"needs_clarification"` suggestion
    /// (`domain::action::ThymeAction::resolve_clarification`), moving it
    /// on to `"pending"` — or straight to `"applied"` in the rare case
    /// the now-fully-specified action turns out to be one of the
    /// low-risk auto-applied types after all (`ThymeAction::auto_apply`),
    /// mirroring the same check `process_transcription` makes when a
    /// suggestion is first proposed. Returns `Err(message)` — suitable
    /// for `ResolveClarificationError` — for any failure, including the
    /// suggestion not actually being one that's awaiting clarification.
    async fn resolve_clarification(
        &self,
        suggestion_id: Uuid,
        raw_answer: &str,
    ) -> std::result::Result<(String, String), String> {
        let record = match self.ai_repo.get_needing_clarification(suggestion_id).await {
            Ok(Some(record)) => record,
            Ok(None) => return Err("This suggestion isn't awaiting clarification.".to_string()),
            Err(e) => {
                eprintln!("failed to look up suggestion {suggestion_id}: {e}");
                return Err("Could not resolve this clarification. Please try again.".to_string());
            }
        };

        let mut action = record.to_action().map_err(|e| {
            eprintln!("{e}");
            "Could not resolve this clarification. Please try again.".to_string()
        })?;

        // Pipe the raw answer through the AI action's own validation/normalization logic.
        let patient = match self.patient_repo.get_by_id(record.patient_id).await {
            Ok(Some(patient)) => patient,
            Ok(None) => return Err("Unknown patient.".to_string()),
            Err(e) => {
                eprintln!("patient lookup failed while processing transcription: {e}");
                return Err("Could not process the recording. Please try again.".to_string());
            }
        };

        let history = match self.history_repo.fetch(record.patient_id).await {
            Ok(history) => history,
            Err(e) => {
                eprintln!("history fetch failed while processing transcription: {e}");
                return Err("Could not process the recording. Please try again.".to_string());
            }
        };

        let answer = self
            .ai_processor
            .process_clarification_answer(&patient, &history, &record, &action, raw_answer)
            .await
            .map_err(|e| {
                eprintln!("clarification answer validation failed for {suggestion_id}: {e}");
                "Could not resolve this clarification. Please try again.".to_string()
            })?;

        action.resolve_clarification(&answer);

        let new_status = if action.auto_apply() {
            "applied"
        } else {
            "pending"
        };
        let description = action.describe();
        let payload_json = serde_json::to_value(&action).map_err(|e| {
            eprintln!("failed to re-serialize resolved action {suggestion_id}: {e}");
            "Could not resolve this clarification. Please try again.".to_string()
        })?;

        self.ai_repo
            .resolve_clarification(suggestion_id, &payload_json, &description, new_status)
            .await
            .map_err(|e| {
                eprintln!("failed to persist resolved clarification {suggestion_id}: {e}");
                "Could not resolve this clarification. Please try again.".to_string()
            })?;

        if new_status == "applied" {
            let ai_authored_by = format!("AI ({})", self.ai_processor.model_name());
            if let Err(e) = self
                .history_repo
                .apply_action(
                    record.patient_id,
                    Some(&ai_authored_by),
                    "ai_suggested",
                    &action,
                )
                .await
            {
                // Mirrors `process_transcription`'s handling of the same
                // situation: the suggestion row already says "applied"
                // even though the chart write itself failed here — still
                // surfaced to the clinician as a resolved suggestion
                // rather than silently dropped.
                eprintln!("failed to auto-apply resolved suggestion {suggestion_id}: {e}");
            }
        }

        Ok((description, new_status.to_string()))
    }

    /// Handles `RejectSuggestedActionRequest`: dismisses exactly one
    /// still-`"pending"` suggestion without ever touching the patient's
    /// chart. Returns `Err(message)` suitable for
    /// `RejectSuggestedActionError`.
    async fn reject_suggestion(&self, suggestion_id: Uuid) -> std::result::Result<(), String> {
        match self.ai_repo.get_pending(suggestion_id).await {
            Ok(Some(_)) => {}
            Ok(None) => return Err("This suggestion was already resolved.".to_string()),
            Err(e) => {
                eprintln!("failed to look up suggestion {suggestion_id}: {e}");
                return Err("Could not dismiss this suggestion. Please try again.".to_string());
            }
        }

        self.ai_repo
            .resolve(suggestion_id, "rejected")
            .await
            .map_err(|e| {
                eprintln!("failed to reject suggestion {suggestion_id}: {e}");
                "Could not dismiss this suggestion. Please try again.".to_string()
            })
    }

    /// Handles `ApplyAllSuggestedActionsRequest`: applies every
    /// still-`"pending"` suggestion from one transcript. Each suggestion
    /// is applied independently — one failing doesn't stop the rest —
    /// and the per-suggestion outcome is reported back
    /// (`AppliedActionResult`) so the client can show exactly what
    /// happened rather than an all-or-nothing result. Only a failure to
    /// even load the pending list surfaces as `Err(message)`
    /// (`ApplyAllSuggestedActionsError`).
    async fn apply_all_pending(
        &self,
        transcription_id: Uuid,
        authored_by: &str,
    ) -> std::result::Result<Vec<AppliedActionResult>, String> {
        let pending = self
            .ai_repo
            .list_pending_for_transcription(transcription_id)
            .await
            .map_err(|e| {
                eprintln!("failed to list pending suggestions for {transcription_id}: {e}");
                "Could not load this transcript's suggestions. Please try again.".to_string()
            })?;

        let mut results = Vec::with_capacity(pending.len());
        for record in pending {
            let suggestion_id = record.id.to_string();

            let action = match record.to_action() {
                Ok(action) => action,
                Err(e) => {
                    eprintln!("{e}");
                    results.push(AppliedActionResult {
                        suggestion_id,
                        success: false,
                        message: "Could not apply this suggestion.".to_string(),
                    });
                    continue;
                }
            };

            match self
                .history_repo
                .apply_action(
                    record.patient_id,
                    Some(authored_by),
                    "ai_suggested",
                    &action,
                )
                .await
            {
                Ok(summary) => {
                    if let Err(e) = self.ai_repo.resolve(record.id, "applied").await {
                        eprintln!(
                            "applied suggestion {} but failed to mark it resolved: {e}",
                            record.id
                        );
                    }
                    results.push(AppliedActionResult {
                        suggestion_id,
                        success: true,
                        message: summary,
                    });
                }
                Err(e) => {
                    eprintln!("failed to apply suggestion {}: {e}", record.id);
                    results.push(AppliedActionResult {
                        suggestion_id,
                        success: false,
                        message: "Could not apply this suggestion.".to_string(),
                    });
                }
            }
        }

        Ok(results)
    }

    /// Handles `DevicePatientItemsRequest`: everything device-originated
    /// (`server::dicom`) on file for one patient — OCT images and any
    /// refraction readings still awaiting review. Returns
    /// `Err(message)` — suitable for `DevicePatientItemsError` — only
    /// when a lookup itself fails; a patient with nothing on file yet
    /// simply gets back two empty lists, not an error (most patients,
    /// most of the time, since this only ever fills in once a device
    /// has actually sent something for them).
    async fn list_device_items(
        &self,
        patient_id: Uuid,
    ) -> std::result::Result<(Vec<OctImageRecordPacket>, Vec<DeviceSuggestedActionRecord>), String> {
        let oct_images = self.device_repo.list_oct_images_for_patient(patient_id).await.map_err(|e| {
            eprintln!("failed to list OCT images for patient {patient_id}: {e}");
            "Could not load this patient's imaging. Please try again.".to_string()
        })?;

        let pending = self.device_repo.list_pending_for_patient(patient_id).await.map_err(|e| {
            eprintln!("failed to list pending device suggestions for patient {patient_id}: {e}");
            "Could not load this patient's device readings. Please try again.".to_string()
        })?;

        let oct_images = oct_images
            .into_iter()
            .map(|image| OctImageRecordPacket {
                image_id: image.id.to_string(),
                eye: image.eye.unwrap_or_default(),
                rows: image.rows.unwrap_or_default(),
                columns: image.columns.unwrap_or_default(),
                number_of_frames: image.number_of_frames.unwrap_or_default(),
                acquisition_datetime: image
                    .acquisition_datetime
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default(),
            })
            .collect();

        let pending_suggestions = pending
            .into_iter()
            .map(|record| DeviceSuggestedActionRecord {
                suggestion_id: record.id.to_string(),
                action_type: record.action_type,
                description: record.description,
                payload_json: record.payload_json.to_string(),
            })
            .collect();

        Ok((oct_images, pending_suggestions))
    }

    /// Handles `ApplyDeviceSuggestionRequest`: accepts one staged
    /// refraction reading, writing it to the patient's chart with
    /// `source = "device"` (see `RefractionEntry::source`'s doc
    /// comment). Mirrors `apply_suggestion` exactly, aside from that
    /// source label and reading from `device_repo` instead of
    /// `ai_repo` — there's no auto-apply/clarification distinction to
    /// carry over, since a device suggestion never has either (see
    /// `db::DeviceRepository::record_suggested_action`'s doc comment).
    async fn apply_device_suggestion(
        &self,
        suggestion_id: Uuid,
        authored_by: &str,
    ) -> std::result::Result<String, String> {
        let record = match self.device_repo.get_pending(suggestion_id).await {
            Ok(Some(record)) => record,
            Ok(None) => return Err("This suggestion was already resolved.".to_string()),
            Err(e) => {
                eprintln!("failed to look up device suggestion {suggestion_id}: {e}");
                return Err("Could not apply this suggestion. Please try again.".to_string());
            }
        };

        let action = record.to_action().map_err(|e| {
            eprintln!("{e}");
            "Could not apply this suggestion. Please try again.".to_string()
        })?;

        let summary = self
            .history_repo
            .apply_action(record.patient_id, Some(authored_by), "device", &action)
            .await
            .map_err(|e| {
                eprintln!("failed to apply device suggestion {suggestion_id}: {e}");
                "Could not apply this suggestion. Please try again.".to_string()
            })?;

        if let Err(e) = self.device_repo.resolve(suggestion_id, "applied").await {
            // Mirrors `apply_suggestion`'s identical handling: the
            // chart entry is already written at this point, so this is
            // logged, not surfaced as a failure to the clinician.
            eprintln!("applied device suggestion {suggestion_id} but failed to mark it resolved: {e}");
        }

        Ok(summary)
    }

    /// Handles `RejectDeviceSuggestionRequest`: dismisses one
    /// still-`"pending"` device suggestion without ever touching the
    /// patient's chart. Mirrors `reject_suggestion` exactly.
    async fn reject_device_suggestion(&self, suggestion_id: Uuid) -> std::result::Result<(), String> {
        match self.device_repo.get_pending(suggestion_id).await {
            Ok(Some(_)) => {}
            Ok(None) => return Err("This suggestion was already resolved.".to_string()),
            Err(e) => {
                eprintln!("failed to look up device suggestion {suggestion_id}: {e}");
                return Err("Could not dismiss this suggestion. Please try again.".to_string());
            }
        }

        self.device_repo.resolve(suggestion_id, "rejected").await.map_err(|e| {
            eprintln!("failed to reject device suggestion {suggestion_id}: {e}");
            "Could not dismiss this suggestion. Please try again.".to_string()
        })
    }

    /// Handles `OctImageDataRequest`: loads one OCT image's metadata
    /// and reads its pixel bytes back off disk — `server::dicom::oct`
    /// is what wrote them there in the first place, at the path this
    /// looks up via `device_repo`. Returns `Err(message)` — suitable
    /// for `OctImageDataError` — if the image is unknown or its file
    /// can no longer be read (e.g. `DICOM_STORAGE_DIR` changed, or the
    /// file was moved/deleted, since the image was received).
    async fn read_oct_image(&self, image_id: Uuid) -> std::result::Result<OctImageDataResponse, String> {
        let record = match self.device_repo.get_oct_image(image_id).await {
            Ok(Some(record)) => record,
            Ok(None) => return Err("Unknown image.".to_string()),
            Err(e) => {
                eprintln!("failed to look up OCT image {image_id}: {e}");
                return Err("Could not load this image. Please try again.".to_string());
            }
        };

        let pixel_data = tokio::fs::read(&record.file_path).await.map_err(|e| {
            eprintln!("failed to read OCT image file {}: {e}", record.file_path);
            "Could not load this image's data. Please try again.".to_string()
        })?;

        Ok(OctImageDataResponse {
            image_id: record.id.to_string(),
            eye: record.eye.unwrap_or_default(),
            rows: record.rows.unwrap_or_default(),
            columns: record.columns.unwrap_or_default(),
            number_of_frames: record.number_of_frames.unwrap_or_default(),
            pixel_data,
        })
    }
}