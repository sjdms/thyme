//! Packets for the AI transcription-processing pipeline
//! (`server::ai`/`server::db::ai_repository`).
//!
//! Flow, end to end:
//!   1. A doctor client finishes a Ctrl+R take, transcribes it via
//!      Deepgram (`ui::stt`), and sends `ProcessTranscriptionPacket`.
//!   2. The server checks the named patient is actually
//!      `"with_doctor"` right now (see `server::server`'s handling of
//!      this packet — recording is only meaningful, and only allowed,
//!      while a patient is physically in the room), gathers that
//!      patient's history (`domain::patient::PatientHistory`), and
//!      hands the transcript + history to `server::ai::AiProcessor`.
//!   3. The server persists the transcript and every action the model
//!      proposed (each starting life as `status = "pending"| — see
//!      `server::db::ai_repository`), then answers with
//!      `AiSuggestionsResponse` (or `AiProcessingError` if the model
//!      call or DB write failed).
//!   4. The doctor client shows each still-`"pending"` suggestion for
//!      review; accepting one sends `ApplySuggestedActionRequest`,
//!      rejecting sends `RejectSuggestedActionRequest`, and accepting
//!      every pending suggestion from one transcript at once sends
//!      `ApplyAllSuggestedActionsRequest`. A narrow set of low-risk
//!      action types (`domain::action::ThymeAction::auto_apply`) skip
//!      this step entirely and arrive already `"applied"` — see
//!      that method's doc comment for why this feature deliberately
//!      departs from THYME_PROJECT_DEFINITION.md §44 "AI policy": "No
//!      AI result may silently become a clinical fact" for that narrow
//!      set only. Everything else always waits for one of the three
//!      requests above.
//!   5. A suggestion whose `SuggestedActionRecord::status` is instead
//!      `"needs_clarification"` carries a `Clarification` the model
//!      attached because it had to guess at part of the action (see
//!      `domain::action::ThymeAction::clarification` and the
//!      "Clarifications" section of `server::ai::prompt::build_prompt`).
//!      A doctor client shows this as a question — with either the
//!      answer choices from `clarification_choices` or a free-text/
//!      dictated answer — and sends `ResolveClarificationRequest` with
//!      what the clinician answered; the server folds that answer into
//!      the action (`domain::action::ThymeAction::resolve_clarification`)
//!      and the suggestion becomes an ordinary `"pending"` (or
//!      already-`"applied"`) one, same as if the model had never needed
//!      to ask.

use macros::{PacketField, packet};

/// Sent by a doctor client once a Ctrl+R recording has been transcribed.
/// `patient_id` must name the patient currently `"with_doctor"` in the
/// queue on this connection's terminal — the server re-checks this
/// itself (it does not trust the client), per the same
/// checks-happen-server-side pattern as patient search/creation.
#[packet(id = 0xA01, serverbound)]
pub struct ProcessTranscriptionPacket {
    pub patient_id: String,
    pub transcript: String,
}

/// One action the AI proposed, as it travels over the wire. Transport
/// projection of a single parsed `domain::action::ThymeAction`, paired
/// with the DB-assigned `suggestion_id` a client references later in
/// `ApplySuggestedActionRequest`/`RejectSuggestedActionRequest`.
///
/// `payload_json` is the action's full JSON form (`{"action": "...",
/// ...fields}`, exactly the shape `domain::action::ThymeAction`'s
/// `#[serde(tag = "action")]` produces) — carried as an opaque string
/// rather than decomposed field-by-field so this packet never needs to
/// change shape when `ThymeAction` grows a variant; a client that only
/// wants to display suggestions never needs to parse it; the server
/// re-parses it into a real `ThymeAction` in `ApplySuggestedActionRequest`
/// handling.
#[derive(Debug, Clone, PacketField)]
pub struct SuggestedActionRecord {
    pub suggestion_id: String,
    pub action_type: String,
    pub description: String,
    pub payload_json: String,
    /// `"pending"` | `"needs_clarification"` | `"applied"` |
    /// `"rejected"` — mirrors `ai_suggested_actions.status`.
    /// `"needs_clarification"` means `domain::action::ThymeAction::clarification`
    /// was `Some` when this suggestion was recorded — a client must
    /// send `ResolveClarificationRequest` before this suggestion can be
    /// accepted/rejected like a normal `"pending"` one (see that
    /// packet's doc comment). Almost always `"pending"` otherwise,
    /// *except* for the handful of low-risk action types
    /// `domain::action::ThymeAction::auto_apply` lets through
    /// immediately (see that method's doc comment) — those arrive
    /// already `"applied"`, so a client can render them as a done fact
    /// rather than something awaiting review.
    pub status: String,
    /// The clarifying question to show the clinician — see
    /// `domain::action::Clarification::message`. Empty whenever `status
    /// != "needs_clarification"`.
    pub clarification_message: String,
    /// `"single_choice"` | `"text"` | `""` (the last when there is no
    /// clarification) — mirrors
    /// `domain::action::ClarificationAnswerType::as_str`.
    pub clarification_answer_type: String,
    /// Candidate answers for a `"single_choice"` clarification; always
    /// empty otherwise. See `domain::action::Clarification::choices`.
    pub clarification_choices: Vec<String>,
}

/// Successful response to `ProcessTranscriptionPacket`: every action the
/// model proposed (possibly empty — the model is explicitly allowed to
/// decide nothing is actionable; see `ThymeAction::NoActionNeeded` and
/// the prompt built by `server::ai::prompt`), all still `"pending"`.
#[packet(id = 0xA02, clientbound)]
pub struct AiSuggestionsResponse {
    pub transcription_id: String,
    pub patient_id: String,
    pub suggestions: Vec<SuggestedActionRecord>,
}

/// Sent instead of `AiSuggestionsResponse` when transcription processing
/// could not be completed — the patient isn't currently `"with_doctor"`,
/// the model call failed, or its response couldn't be parsed into valid
/// actions. `message` is suitable to show directly to the user.
#[packet(id = 0xA03, clientbound)]
pub struct AiProcessingError {
    pub message: String,
}

/// Sent by a doctor client to accept one suggested action, writing it to
/// the patient's chart (`server::db::history_repository`) exactly as
/// proposed. There is deliberately no "edit and apply" — per
/// THYME_UI_SPEC.md's stance on AI suggestions being reviewed as-is, a
/// clinician who wants something different should reject this one and
/// (for now) add the correct entry manually once that UI exists.
#[packet(id = 0xA04, serverbound)]
pub struct ApplySuggestedActionRequest {
    pub suggestion_id: String,
}

/// Successful response to `ApplySuggestedActionRequest`. `summary` is a
/// human-readable confirmation (e.g. "Added diagnosis: ...") suitable to
/// show directly to the user.
#[packet(id = 0xA05, clientbound)]
pub struct ApplySuggestedActionResponse {
    pub suggestion_id: String,
    pub summary: String,
}

/// Sent instead of `ApplySuggestedActionResponse` when the suggestion
/// couldn't be applied (already resolved, unknown id, backend failure).
#[packet(id = 0xA06, clientbound)]
pub struct ApplySuggestedActionError {
    pub suggestion_id: String,
    pub message: String,
}

/// Sent by a doctor client to dismiss a suggested action without
/// applying it. Still recorded (`status = "rejected"`) rather than
/// deleted, so the transcript's full set of proposals stays inspectable
/// later — an AI suggestion nobody acted on is itself useful signal for
/// tuning the prompt/model later.
#[packet(id = 0xA07, serverbound)]
pub struct RejectSuggestedActionRequest {
    pub suggestion_id: String,
}

#[packet(id = 0xA08, clientbound)]
pub struct RejectSuggestedActionResponse {
    pub suggestion_id: String,
}

/// Sent instead of `RejectSuggestedActionResponse` when the suggestion
/// couldn't be dismissed (already resolved, unknown id, backend
/// failure) — the reject-side counterpart of `ApplySuggestedActionError`.
#[packet(id = 0xA09, clientbound)]
pub struct RejectSuggestedActionError {
    pub suggestion_id: String,
    pub message: String,
}

/// Sent by a doctor client to answer a still-`"needs_clarification"`
/// suggestion's `Clarification` — see
/// `domain::action::ThymeAction::clarification` and the "Clarifications"
/// section of `server::ai::prompt::build_prompt` for how a suggestion
/// ends up in this state in the first place.
///
/// `answer` is free text either way: for a `"single_choice"`
/// clarification it's expected to be exactly one of that suggestion's
/// `SuggestedActionRecord::clarification_choices` (a client should send
/// whichever choice the clinician picked, verbatim), and for `"text"`
/// it's whatever the clinician typed or dictated (`ui::stt`, re-run in
/// the context of this clarification rather than a fresh
/// `ProcessTranscriptionPacket` take — see `ui::controllers::doctor`).
/// The server does not itself enforce membership in `choices`; a
/// clinician typing something else instead is treated as a corrected
/// answer, not an error.
#[packet(id = 0xA0D, serverbound)]
pub struct ResolveClarificationRequest {
    pub suggestion_id: String,
    pub answer: String,
}

/// Successful response to `ResolveClarificationRequest`: the suggestion
/// moves to `"pending"` (or `"applied"`, more precisely — see below) with
/// `answer` folded into the action itself
/// (`domain::action::ThymeAction::resolve_clarification`) and its
/// `Clarification` cleared, so a client should now render it exactly
/// like any other resolved suggestion — `description` is the freshly
/// recomputed summary, and `status` distinguishes the ordinary case
/// (`"pending"`, awaiting `ApplySuggestedActionRequest` like normal)
/// from the rare one where the now-fully-specified action turned out to
/// be one of the low-risk auto-applied types after all
/// (`domain::action::ThymeAction::auto_apply`) and was written to the
/// chart immediately — `"applied"`.
#[packet(id = 0xA0E, clientbound)]
pub struct ResolveClarificationResponse {
    pub suggestion_id: String,
    pub description: String,
    pub status: String,
}

/// Sent instead of `ResolveClarificationResponse` when the clarification
/// couldn't be resolved (suggestion not found, not actually
/// `"needs_clarification"`, or a backend failure). `message` is
/// suitable to show directly to the user.
#[packet(id = 0xA0F, clientbound)]
pub struct ResolveClarificationError {
    pub suggestion_id: String,
    pub message: String,
}

/// One suggestion's outcome within a bulk `ApplyAllSuggestedActionsRequest`
/// — every still-`"pending"` suggestion is applied independently, so one
/// failing doesn't stop the rest (see that packet's doc comment).
#[derive(Debug, Clone, PacketField)]
pub struct AppliedActionResult {
    pub suggestion_id: String,
    pub success: bool,
    /// A human-readable chart-update summary on success, or a
    /// human-readable failure reason otherwise.
    pub message: String,
}

/// Sent by a doctor client to accept every still-pending suggestion
/// from one transcript in a single action — the "Approve all" button a
/// clinician reaches for once they've skimmed a batch of suggestions
/// and don't disagree with any of them, rather than clicking Accept
/// one at a time. Suggestions already resolved (rejected, or already
/// auto-applied per `ThymeAction::auto_apply`) are simply skipped, not
/// treated as an error.
#[packet(id = 0xA0A, serverbound)]
pub struct ApplyAllSuggestedActionsRequest {
    pub transcription_id: String,
}

/// Successful response to `ApplyAllSuggestedActionsRequest`. `results`
/// has one entry per suggestion that was still `"pending"` at the time
/// of the request, in the order they were originally proposed.
#[packet(id = 0xA0B, clientbound)]
pub struct ApplyAllSuggestedActionsResponse {
    pub transcription_id: String,
    pub results: Vec<AppliedActionResult>,
}

/// Sent instead of `ApplyAllSuggestedActionsResponse` when the request
/// couldn't be processed at all (unknown/invalid `transcription_id`,
/// backend failure fetching the pending list) — distinct from a single
/// suggestion within the batch failing, which is instead reported as a
/// non-`success` `AppliedActionResult` inside a normal response.
#[packet(id = 0xA0C, clientbound)]
pub struct ApplyAllSuggestedActionsError {
    pub transcription_id: String,
    pub message: String,
}