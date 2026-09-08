//! Owns every raw socket and packet in the client.
//!
//! This is the *only* file in `ui` allowed to construct a `FramedStream`,
//! hold a raw `TcpStream`/`EncryptedStream`, or read/write a
//! `packets::*` type directly. Everything else — `services::auth_service`,
//! `services::patient_service`, and the controllers built on top of them —
//! talks to the server exclusively through the types exposed here.

use auth::EncryptedStream;
use packet_datatypes::PacketInfo;
use packets::{
    ai::{
        AiProcessingError, AiSuggestionsResponse, ApplyAllSuggestedActionsError,
        ApplyAllSuggestedActionsRequest, ApplyAllSuggestedActionsResponse, AppliedActionResult,
        ApplySuggestedActionError, ApplySuggestedActionRequest, ApplySuggestedActionResponse,
        ProcessTranscriptionPacket, RejectSuggestedActionError, RejectSuggestedActionRequest,
        RejectSuggestedActionResponse, ResolveClarificationError, ResolveClarificationRequest,
        ResolveClarificationResponse, SuggestedActionRecord,
    }, auth::{AuthChallenge, AuthResponse, AuthStatus, RequestSTTAPIKeyPacket, STTAPIKeyResponsePacket}, hello::{Hello, HelloOperationMode}, keepalive::{PONG_PACKET, Ping}, patient::{
        PatientCreateError, PatientCreateRequest, PatientCreateResponse, PatientFileError,
        PatientFileRequest, PatientFileResponse, PatientHistoryRecord, PatientRecord,
        PatientSearchError, PatientSearchRequest, PatientSearchResponse,
    }, queue::{
        PatientQueueAdvanceError, PatientQueueAdvanceRequest, PatientQueueRegisterError,
        PatientQueueRegisterRequest, PatientQueueSnapshot, QueueEntryRecord,
    }, stream::FramedStream,
};
use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc},
};

use crate::error::{ClientError, Result};

/// A user the server is currently willing to authenticate, as offered by
/// the most recent `AuthChallenge`.
#[derive(Debug, Clone)]
pub struct AvailableUser {
    pub username: String,
    pub display_name: String,
}

/// Events pushed from the background connection task to any interested
/// service. Subscribers are expected to ignore variants they don't care
/// about.
#[derive(Debug, Clone)]
pub enum ServerEvent {
    PatientSearchResults { query: String, results: Vec<PatientRecord> },
    PatientSearchFailed { query: String, message: String },
    PatientCreated { patient: PatientRecord },
    PatientCreateFailed { message: String },
    /// Pushed after `Connection::request_patient_file` succeeds — the
    /// doctor-facing "Patients" tab's file view. `patient_id` is echoed
    /// back (from `patient.patient_id`, not a separately tracked field)
    /// so a caller that has since opened a different patient can
    /// discard a stale response.
    PatientFileReady { patient: PatientRecord, history: PatientHistoryRecord },
    /// Pushed instead of `PatientFileReady` when the file couldn't be
    /// opened. `patient_id` is echoed back for the same reason.
    PatientFileFailed { patient_id: String, message: String },
    /// Pushed on login and again after every queue change (see
    /// `packets::queue::PatientQueueSnapshot`'s doc comment) — always
    /// the full current queue, not a delta.
    QueueSnapshot { entries: Vec<QueueEntryRecord> },
    QueueRegisterFailed { message: String },
    QueueAdvanceFailed { message: String },
    Disconnected { _reason: String },
    STTKeyRecieved { key: String },

    /// Pushed after `Connection::process_transcription` succeeds — every
    /// action the AI proposed from that transcript (possibly empty),
    /// per `packets::ai::AiSuggestionsResponse`.
    AiSuggestionsReady {
        transcription_id: String,
        patient_id: String,
        suggestions: Vec<SuggestedActionRecord>,
    },
    /// Pushed instead of `AiSuggestionsReady` when the transcript
    /// couldn't be processed (patient not `"with_doctor"`, model
    /// failure, etc.) — `message` is user-facing.
    AiProcessingFailed { message: String },
    /// Pushed after `Connection::apply_suggestion` succeeds.
    SuggestionApplied { suggestion_id: String, summary: String },
    /// Pushed instead of `SuggestionApplied` when applying failed.
    SuggestionApplyFailed { suggestion_id: String, message: String },
    /// Pushed after `Connection::reject_suggestion` succeeds.
    SuggestionRejected { suggestion_id: String },
    /// Pushed instead of `SuggestionRejected` when dismissing failed.
    SuggestionRejectFailed { suggestion_id: String, message: String },
    /// Pushed after `Connection::resolve_clarification` succeeds — the
    /// suggestion's freshly recomputed `description` and its new
    /// `status` (`"pending"` ordinarily, or `"applied"` in the rare case
    /// the now-fully-specified action was one of the auto-applied
    /// types — see `packets::ai::ResolveClarificationResponse`).
    ClarificationResolved { suggestion_id: String, description: String, status: String },
    /// Pushed instead of `ClarificationResolved` when resolving failed.
    ClarificationResolveFailed { suggestion_id: String, message: String },
    /// Pushed after `Connection::apply_all_suggestions` succeeds —
    /// one `AppliedActionResult` per suggestion that was still
    /// pending, each independently succeeded or failed.
    AllSuggestionsApplied { transcription_id: String, results: Vec<AppliedActionResult> },
    /// Pushed instead of `AllSuggestionsApplied` when the batch
    /// couldn't even be loaded (distinct from one suggestion within it
    /// failing, which shows up inside `AllSuggestionsApplied`'s results).
    AllSuggestionsApplyFailed { transcription_id: String, message: String },
}

/// UI-facing input for `Connection::create_patient` — the wire packet
/// itself is built here, not by callers, matching this file's role as
/// the only place `packets::*` types are touched directly.
pub struct NewPatientInput {
    pub legal_name: String,
    pub preferred_name: Option<String>,
    /// ISO `YYYY-MM-DD`; validated server-side.
    pub birth_date: Option<String>,
    pub sex: Option<String>,
    pub phone: Option<String>,
}

/// A TCP connection that has completed `Hello` and is holding an
/// `AuthChallenge`, waiting for credentials.
pub struct PendingConnection {
    stream: FramedStream<TcpStream>,
    challenge_string: String,
    available_users: Vec<AvailableUser>,
}

/// Outcome of submitting credentials on a `PendingConnection`.
pub enum LoginOutcome {
    /// Authenticated. The connection is now encrypted and its background
    /// task is already running. `roles` are the authenticated user's
    /// role names (e.g. `"doctor"`) — the client uses these, not a
    /// client-declared terminal mode, to decide which dashboard to open.
    Authenticated { connection: Connection, roles: Vec<String> },
    /// The server rejected the credentials but is still willing to accept
    /// further attempts; `retry` carries the fresh `AuthChallenge` it
    /// issued immediately afterward.
    Rejected { retry: PendingConnection, message: String },
}

impl PendingConnection {
    /// Connects to `server_addr`, sends `Hello`, and waits for the first
    /// `AuthChallenge`.
    pub async fn connect(
        server_addr: &str,
        computer_name: impl Into<String>,
        mode: HelloOperationMode,
    ) -> Result<Self> {
        let tcp_stream = TcpStream::connect(server_addr).await?;
        tcp_stream.set_nodelay(true)?;

        let mut stream = FramedStream::new(tcp_stream);
        stream
            .write_packet(&Hello { computer_name: computer_name.into(), mode })
            .await?;

        let challenge = stream.read_packet::<AuthChallenge>().await?;
        Ok(Self::from_challenge(stream, challenge))
    }

    fn from_challenge(stream: FramedStream<TcpStream>, challenge: AuthChallenge) -> Self {
        Self {
            stream,
            challenge_string: challenge.challenge_string,
            available_users: challenge
                .available_users
                .into_iter()
                .map(|(username, display_name)| AvailableUser { username, display_name })
                .collect(),
        }
    }

    /// Users the server is currently willing to authenticate, as of the
    /// most recent `AuthChallenge`.
    pub fn available_users(&self) -> &[AvailableUser] {
        &self.available_users
    }

    /// Submits credentials against the challenge this connection is
    /// holding.
    ///
    /// On success, consumes `self` and returns an authenticated,
    /// encrypted `Connection` whose background task is already running.
    /// On a rejection the server is still willing to retry, returns a
    /// fresh `PendingConnection` carrying the next `AuthChallenge`. Any
    /// other failure — including the server closing the socket after the
    /// maximum number of attempts — surfaces as `Err`.
    pub async fn try_login(mut self, username: impl Into<String>, password: &str) -> Result<LoginOutcome> {
        let username = username.into();
    

        self.stream
            .write_packet(&AuthResponse { username: username.clone(), password_hash: password.as_bytes().to_vec() })
            .await?;

        let status = self.stream.read_packet::<AuthStatus>().await?;

        if status.success {
            let raw_tcp = self.stream.into_inner();
            let encrypted = EncryptedStream::new(raw_tcp, &self.challenge_string, &username, &password.as_bytes().to_vec());
            let connection = Connection::spawn(FramedStream::new(encrypted));
            return Ok(LoginOutcome::Authenticated { connection, roles: status.roles });
        }

        // The server keeps the socket open and issues a fresh challenge
        // unless this was the final allowed attempt, in which case it
        // closes the connection and this read fails — which is exactly
        // the "attempts exhausted" signal we want to surface as an error.
        let challenge = self.stream.read_packet::<AuthChallenge>().await?;
        Ok(LoginOutcome::Rejected {
            retry: Self::from_challenge(self.stream, challenge),
            message: "Incorrect username or password.".to_string(),
        })
    }
}

/// Intents a service can ask the background connection task to carry
/// out. Kept private: nothing outside this file ever sees one of these,
/// let alone builds or encodes a packet itself.
enum OutboundRequest {
    SearchPatients(String),
    CreatePatient(NewPatientInput),
    RequestPatientFile { patient_id: String },
    RegisterQueuePatient { patient_id: String, note: Option<String> },
    AdvanceQueueStatus { queue_id: String, status: String },
    RequestSTTAPIKey,
    ProcessTranscription { patient_id: String, transcript: String },
    ApplySuggestion { suggestion_id: String },
    RejectSuggestion { suggestion_id: String },
    ResolveClarification { suggestion_id: String, answer: String },
    ApplyAllSuggestions { transcription_id: String },
}

type EncryptedFramedStream = FramedStream<EncryptedStream<TcpStream>>;

/// An authenticated, encrypted connection to the Thyme server. Cheap to
/// clone — every clone shares the same background task and event stream,
/// so services can each hold their own `Connection` without coordinating.
#[derive(Clone)]
pub struct Connection {
    outbound: mpsc::UnboundedSender<OutboundRequest>,
    events: broadcast::Sender<ServerEvent>,
}

impl Connection {
    fn spawn(mut stream: EncryptedFramedStream) -> Self {
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<OutboundRequest>();
        let (events_tx, _) = broadcast::channel::<ServerEvent>(32);
        let events_tx_task = events_tx.clone();

        tokio::spawn(async move {
            let disconnect_reason = loop {
                tokio::select! {
                    maybe_request = outbound_rx.recv() => {
                        let Some(request) = maybe_request else {
                            // No service still holds a sender; nothing
                            // left for this connection to do.
                            break None;
                        };

                        let write_result = match request {
                            OutboundRequest::SearchPatients(query) => {
                                stream.write_packet(&PatientSearchRequest { query }).await
                            }
                            OutboundRequest::CreatePatient(input) => {
                                stream.write_packet(&PatientCreateRequest {
                                    legal_name: input.legal_name,
                                    preferred_name: input.preferred_name,
                                    birth_date: input.birth_date,
                                    sex: input.sex,
                                    phone: input.phone,
                                }).await
                            }
                            OutboundRequest::RequestPatientFile { patient_id } => {
                                stream.write_packet(&PatientFileRequest { patient_id }).await
                            }
                            OutboundRequest::RegisterQueuePatient { patient_id, note } => {
                                stream.write_packet(&PatientQueueRegisterRequest { patient_id, note }).await
                            }
                            OutboundRequest::AdvanceQueueStatus { queue_id, status } => {
                                stream.write_packet(&PatientQueueAdvanceRequest { queue_id, status }).await
                            }
                            OutboundRequest::RequestSTTAPIKey => stream.write_packet(&RequestSTTAPIKeyPacket).await,
                            OutboundRequest::ProcessTranscription { patient_id, transcript } => {
                                stream.write_packet(&ProcessTranscriptionPacket { patient_id, transcript }).await
                            }
                            OutboundRequest::ApplySuggestion { suggestion_id } => {
                                stream.write_packet(&ApplySuggestedActionRequest { suggestion_id }).await
                            }
                            OutboundRequest::RejectSuggestion { suggestion_id } => {
                                stream.write_packet(&RejectSuggestedActionRequest { suggestion_id }).await
                            }
                            OutboundRequest::ResolveClarification { suggestion_id, answer } => {
                                stream.write_packet(&ResolveClarificationRequest { suggestion_id, answer }).await
                            }
                            OutboundRequest::ApplyAllSuggestions { transcription_id } => {
                                stream.write_packet(&ApplyAllSuggestedActionsRequest { transcription_id }).await
                            }
                        };

                        if let Err(e) = write_result {
                            break Some(e.to_string());
                        }
                    }

                    packet = stream.read_any_packet() => {
                        let (id, payload) = match packet {
                            Ok(p) => p,
                            Err(e) => break Some(e.to_string()),
                        };

                        if let Err(e) = Self::dispatch(id, &payload, &mut stream, &events_tx_task).await {
                            break Some(e.to_string());
                        }
                    }
                }
            };

            let _ = events_tx_task.send(ServerEvent::Disconnected {
                _reason: disconnect_reason.unwrap_or_else(|| "connection closed".to_string()),
            });
        });

        Self { outbound: outbound_tx, events: events_tx }
    }

    async fn dispatch(
        id: u32,
        payload: &[u8],
        stream: &mut EncryptedFramedStream,
        events: &broadcast::Sender<ServerEvent>,
    ) -> Result<()> {
        use packet_datatypes::{Packet, PacketReader};

        match id {
            Ping::ID => {
                stream.write_packet(&PONG_PACKET).await?;
            }
            PatientSearchResponse::ID => {
                let response = PatientSearchResponse::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::PatientSearchResults {
                    query: response.query,
                    results: response.results,
                });
            }
            PatientSearchError::ID => {
                let response = PatientSearchError::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::PatientSearchFailed {
                    query: response.query,
                    message: response.message,
                });
            }
            PatientCreateResponse::ID => {
                let response = PatientCreateResponse::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::PatientCreated { patient: response.patient });
            }
            PatientCreateError::ID => {
                let response = PatientCreateError::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::PatientCreateFailed { message: response.message });
            }
            PatientFileResponse::ID => {
                let response = PatientFileResponse::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::PatientFileReady {
                    patient: response.patient,
                    history: response.history,
                });
            }
            PatientFileError::ID => {
                let response = PatientFileError::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::PatientFileFailed {
                    patient_id: response.patient_id,
                    message: response.message,
                });
            }
            PatientQueueSnapshot::ID => {
                let response = PatientQueueSnapshot::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::QueueSnapshot { entries: response.entries });
            }
            PatientQueueRegisterError::ID => {
                let response = PatientQueueRegisterError::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::QueueRegisterFailed { message: response.message });
            }
            PatientQueueAdvanceError::ID => {
                let response = PatientQueueAdvanceError::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::QueueAdvanceFailed { message: response.message });
            },
            STTAPIKeyResponsePacket::ID => {
                let response = STTAPIKeyResponsePacket::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::STTKeyRecieved { key: response.key });
            }
            AiSuggestionsResponse::ID => {
                let response = AiSuggestionsResponse::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::AiSuggestionsReady {
                    transcription_id: response.transcription_id,
                    patient_id: response.patient_id,
                    suggestions: response.suggestions,
                });
            }
            AiProcessingError::ID => {
                let response = AiProcessingError::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::AiProcessingFailed { message: response.message });
            }
            ApplySuggestedActionResponse::ID => {
                let response = ApplySuggestedActionResponse::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::SuggestionApplied {
                    suggestion_id: response.suggestion_id,
                    summary: response.summary,
                });
            }
            ApplySuggestedActionError::ID => {
                let response = ApplySuggestedActionError::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::SuggestionApplyFailed {
                    suggestion_id: response.suggestion_id,
                    message: response.message,
                });
            }
            RejectSuggestedActionResponse::ID => {
                let response = RejectSuggestedActionResponse::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::SuggestionRejected { suggestion_id: response.suggestion_id });
            }
            RejectSuggestedActionError::ID => {
                let response = RejectSuggestedActionError::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::SuggestionRejectFailed {
                    suggestion_id: response.suggestion_id,
                    message: response.message,
                });
            }
            ResolveClarificationResponse::ID => {
                let response = ResolveClarificationResponse::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::ClarificationResolved {
                    suggestion_id: response.suggestion_id,
                    description: response.description,
                    status: response.status,
                });
            }
            ResolveClarificationError::ID => {
                let response = ResolveClarificationError::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::ClarificationResolveFailed {
                    suggestion_id: response.suggestion_id,
                    message: response.message,
                });
            }
            ApplyAllSuggestedActionsResponse::ID => {
                let response = ApplyAllSuggestedActionsResponse::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::AllSuggestionsApplied {
                    transcription_id: response.transcription_id,
                    results: response.results,
                });
            }
            ApplyAllSuggestedActionsError::ID => {
                let response = ApplyAllSuggestedActionsError::decode(&mut PacketReader::new(payload))?;
                let _ = events.send(ServerEvent::AllSuggestionsApplyFailed {
                    transcription_id: response.transcription_id,
                    message: response.message,
                });
            }
            _ => {
                // Unrecognized packets are ignored rather than treated as
                // fatal, so future server features can roll out without
                // breaking a client mid-connection.
            }
        }

        Ok(())
    }

    /// Subscribes to server-pushed events. Each subscriber gets its own
    /// queue; events sent before subscribing are not replayed.
    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.events.subscribe()
    }

    /// Fires a patient search. The result (or failure) arrives later as a
    /// `ServerEvent` on `subscribe()`, tagged with this exact `query` so a
    /// caller that has since searched again can safely discard a stale
    /// answer (see `packets::patient::PatientSearchResponse`).
    pub fn search_patients(&self, query: impl Into<String>) -> Result<()> {
        self.outbound
            .send(OutboundRequest::SearchPatients(query.into()))
            .map_err(|_| ClientError::Disconnected)
    }

    /// Registers a new patient. The result (or failure) arrives later as
    /// a `ServerEvent` on `subscribe()` — there's only ever one
    /// creation in flight from the receptionist search screen at a
    /// time, so unlike search there's no query to tag it with.
    pub fn create_patient(&self, input: NewPatientInput) -> Result<()> {
        self.outbound
            .send(OutboundRequest::CreatePatient(input))
            .map_err(|_| ClientError::Disconnected)
    }

    /// Opens a patient's full file — identity plus every clinical
    /// history category on record — regardless of whether that patient
    /// is currently in the queue or with the doctor. The result (or
    /// failure) arrives later as a `ServerEvent` on `subscribe()`,
    /// tagged with the patient's id so a caller that has since opened a
    /// different patient can discard a stale answer.
    pub fn request_patient_file(&self, patient_id: impl Into<String>) -> Result<()> {
        self.outbound
            .send(OutboundRequest::RequestPatientFile { patient_id: patient_id.into() })
            .map_err(|_| ClientError::Disconnected)
    }

    /// Registers a patient into the "incoming patients" queue. The
    /// result arrives later as a `ServerEvent` on `subscribe()` — a
    /// `QueueSnapshot` on success (see
    /// `packets::queue::PatientQueueRegisterError`'s doc comment for why
    /// there's no dedicated ack), or `QueueRegisterFailed` otherwise.
    pub fn register_patient(&self, patient_id: impl Into<String>, note: Option<String>) -> Result<()> {
        self.outbound
            .send(OutboundRequest::RegisterQueuePatient { patient_id: patient_id.into(), note })
            .map_err(|_| ClientError::Disconnected)
    }

    /// Advances a queue entry to its next status
    /// (`"waiting"` -> `"with_doctor"` -> `"done"`). Result arrives the
    /// same way `register_patient`'s does.
    pub fn advance_queue_status(&self, queue_id: impl Into<String>, status: impl Into<String>) -> Result<()> {
        self.outbound
            .send(OutboundRequest::AdvanceQueueStatus { queue_id: queue_id.into(), status: status.into() })
            .map_err(|_| ClientError::Disconnected)
    }

    pub fn request_stt_api_key(&self) -> Result<()> {
        self.outbound.send(OutboundRequest::RequestSTTAPIKey).map_err(|_| ClientError::Disconnected)
    }

    /// Sends a finished transcript for AI processing — only meaningful,
    /// and only accepted by the server, while `patient_id` is currently
    /// `"with_doctor"` (see `packets::ai::ProcessTranscriptionPacket`'s
    /// doc comment). The result arrives later as a `ServerEvent` on
    /// `subscribe()`: `AiSuggestionsReady` on success, or
    /// `AiProcessingFailed` otherwise.
    pub fn process_transcription(
        &self,
        patient_id: impl Into<String>,
        transcript: impl Into<String>,
    ) -> Result<()> {
        self.outbound
            .send(OutboundRequest::ProcessTranscription {
                patient_id: patient_id.into(),
                transcript: transcript.into(),
            })
            .map_err(|_| ClientError::Disconnected)
    }

    /// Accepts one still-pending AI-suggested action, writing it to the
    /// patient's chart exactly as proposed. Result arrives as
    /// `ServerEvent::SuggestionApplied`/`SuggestionApplyFailed`.
    pub fn apply_suggestion(&self, suggestion_id: impl Into<String>) -> Result<()> {
        self.outbound
            .send(OutboundRequest::ApplySuggestion { suggestion_id: suggestion_id.into() })
            .map_err(|_| ClientError::Disconnected)
    }

    /// Dismisses one still-pending AI-suggested action without ever
    /// touching the patient's chart. Result arrives as
    /// `ServerEvent::SuggestionRejected`/`SuggestionRejectFailed`.
    pub fn reject_suggestion(&self, suggestion_id: impl Into<String>) -> Result<()> {
        self.outbound
            .send(OutboundRequest::RejectSuggestion { suggestion_id: suggestion_id.into() })
            .map_err(|_| ClientError::Disconnected)
    }

    /// Answers a still-`"needs_clarification"` AI-suggested action's
    /// `Clarification` — `answer` is either the clinician's chosen
    /// `SuggestedActionRecord::clarification_choices` entry or whatever
    /// they typed/dictated. Result arrives as
    /// `ServerEvent::ClarificationResolved`/`ClarificationResolveFailed`.
    pub fn resolve_clarification(
        &self,
        suggestion_id: impl Into<String>,
        answer: impl Into<String>,
    ) -> Result<()> {
        self.outbound
            .send(OutboundRequest::ResolveClarification {
                suggestion_id: suggestion_id.into(),
                answer: answer.into(),
            })
            .map_err(|_| ClientError::Disconnected)
    }

    /// Accepts every still-pending suggestion from one transcript at
    /// once — the "Approve all" bulk-review action. Result arrives as
    /// `ServerEvent::AllSuggestionsApplied`/`AllSuggestionsApplyFailed`.
    pub fn apply_all_suggestions(&self, transcription_id: impl Into<String>) -> Result<()> {
        self.outbound
            .send(OutboundRequest::ApplyAllSuggestions { transcription_id: transcription_id.into() })
            .map_err(|_| ClientError::Disconnected)
    }
}