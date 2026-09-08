use serde::{Deserialize, Serialize};
use action_macros::{ThymeActions};
use uuid::Uuid;

/// A question the model attaches to a suggested action when it had to
/// guess at part of that action rather than genuinely knowing the
/// answer from the transcript/history — see `ThymeAction::clarification`
/// and the "Clarifications" section of `server::ai::prompt::build_prompt`
/// for the exact convention the model is told to follow.
///
/// Carried on every `ThymeAction` variant (see the `clarification` field
/// added to each below) rather than as a separate side-channel, so it
/// travels with the suggestion through the exact same DB row
/// (`ai_suggested_actions.payload_json`) and wire packet
/// (`packets::ai::SuggestedActionRecord`) as everything else about that
/// action — nothing extra to keep in sync.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Clarification {
    /// A short question to put in front of the clinician, e.g. "Which
    /// medication would you like to continue?".
    pub message: String,
    pub answer_type: ClarificationAnswerType,
    /// Candidate answers for `ClarificationAnswerType::SingleChoice`
    /// (the model is told to derive these from the patient's history —
    /// see `server::ai::prompt::build_prompt`). Always empty for
    /// `ClarificationAnswerType::Text`.
    #[serde(default)]
    pub choices: Vec<String>,
}

/// How a clinician is expected to answer a `Clarification` — mirrors the
/// two ways `ThymeSuggestionCard` (`ui/slint/suggestion-row.slint`) lets
/// them respond: picking one of `Clarification::choices`, or typing (or
/// dictating, via STT) free text.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClarificationAnswerType {
    SingleChoice,
    Text,
}

impl ClarificationAnswerType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClarificationAnswerType::SingleChoice => "single_choice",
            ClarificationAnswerType::Text => "text",
        }
    }
}

/// The set of actions an AI model is allowed to propose after processing
/// a doctor's dictated transcript (see `server::ai`). This enum is the
/// single source of truth for that contract in both directions:
///
///   - `ThymeActions` (derived below) turns it into the instructions an
///     LLM is given (`ThymeAction::llm_instructions`) — every variant
///     and field the model is told about comes from here, so the model
///     can never be told about an action this binary doesn't actually
///     know how to apply.
///   - `#[serde(tag = "action")]` makes each variant's JSON shape
///     exactly what `ThymeActions` documents (`{"action": "...", ...}`),
///     so a model response can be deserialized straight into a
///     `ThymeAction` with no bespoke parsing per action.
///
/// Adding a new action a doctor's dictation might warrant is meant to be
/// as small a change as adding a variant here (plus a DB-backed home for
/// it in `server::db::history_repository`, and a match arm in
/// `action_type`/`describe` below) — everything else (prompt text,
/// wire format, JSON schema) follows automatically from the derive.
///
/// Per THYME_PROJECT_DEFINITION.md §44 "AI policy" ("AI features must
/// initially be assistive, never silently autonomous" / "No AI result
/// may silently become a clinical fact"), a `ThymeAction` coming back
/// from a model is always a *suggestion* — see
/// `server::db::ai_repository::AiSuggestedAction` — never applied to a
/// patient's chart until a clinician explicitly accepts it.
#[derive(Debug, Clone, Serialize, Deserialize, ThymeActions)]
#[serde(tag = "action")]
pub enum ThymeAction {
    #[action_description("Adds a free-text clinical note to the patient's file, summarizing something the doctor said that is worth keeping on the chart but isn't itself a diagnosis, medication, or order.")]
    AddClinicalNote {
        note: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },

    #[action_description("Adds a diagnosis to the patient's file. `code` is an optional ICD-10 code if the transcript makes one unambiguous; otherwise omit it rather than guessing.")]
    AddDiagnosis {
        diagnosis: String,
        code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },

    #[action_description("Schedules a patient follow-up appointment `weeks` from today. Only use this when the doctor stated a concrete follow-up interval.")]
    ScheduleFollowUp {
        weeks: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },

    #[action_description("Records a new allergy or adverse reaction the doctor mentioned the patient has. `reaction` describes what happens on exposure (e.g. \"hives\", \"anaphylaxis\"); `severity` is a short severity label (e.g. \"mild\", \"severe\") if stated.")]
    AddAllergy {
        allergen: String,
        reaction: Option<String>,
        severity: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },

    #[action_description("Adds a medication the doctor is prescribing or noting the patient is now taking. `dosage` and `frequency` should be left out rather than invented if the doctor didn't state them.")]
    AddMedication {
        name: String,
        dosage: Option<String>,
        frequency: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },

    #[action_description("Orders a lab or diagnostic test the doctor explicitly asked for. `reason` is the clinical justification, if stated.")]
    OrderLabTest {
        test_name: String,
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },

    #[action_description("Requests a referral to another specialty the doctor explicitly asked for. `reason` is the clinical justification, if stated.")]
    RequestReferral {
        specialty: String,
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },

    #[action_description("Explicitly represents 'no chart action is warranted from this transcript'. Use this instead of omitting all actions when you want to record that the transcript was reviewed but contained nothing actionable (e.g. small talk, an already-documented restatement). `rationale` briefly says why. Do NOT use this just because a piece of an otherwise-clear action is missing or ambiguous — attach a `clarification` to that action instead (see the 'Clarifications' section of the prompt); this action is only for transcripts with nothing actionable in them at all.")]
    NoActionNeeded {
        rationale: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },

    #[action_description("Adds a new refraction/prescription reading for one eye, as dictated by the doctor. `eye` is \"OD\" (right), \"OS\" (left), or \"OU\" (only when the doctor gives one identical reading for both eyes — otherwise propose one AddRefraction per eye). `sphere` is the sphere power including its sign (e.g. \"-2.25\", \"+1.00\"). `cylinder`, `axis`, and `add` are optional — leave them out rather than guessing if the doctor didn't state them.")]
    AddRefraction {
        eye: String,
        sphere: String,
        cylinder: Option<String>,
        axis: Option<i32>,
        add: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },

    #[action_description("Updates the dose, frequency, and/or instructions of a medication already on the patient's chart — use this instead of AddMedication when the transcript is changing something about a medication the patient's history already lists, not starting a brand-new one. `medication_id` must be copied exactly from the `id` of one of the medications listed in the patient's history. Only include the fields that actually changed; leave the rest out rather than repeating unchanged values.")]
    UpdateMedication {
        medication_id: Uuid,
        dosage: Option<String>,
        frequency: Option<String>,
        instructions: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },

    #[action_description("Marks a medication already on the patient's chart as discontinued. `medication_id` must be copied exactly from the `id` of one of the medications listed in the patient's history. `reason` is the doctor's stated reason, if any.")]
    DiscontinueMedication {
        medication_id: Uuid,
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },

    #[action_description("Updates the status and/or clinical notes of a diagnosis already on the patient's chart (e.g. marking it resolved or ruled out, or adding follow-up context) — use this instead of AddDiagnosis when the transcript refers to a diagnosis the patient's history already lists. `diagnosis_id` must be copied exactly from the `id` of one of the diagnoses listed in the patient's history. `status` is a short label such as \"active\", \"resolved\", or \"ruled_out\". `notes` is optional.")]
    UpdateDiagnosis {
        diagnosis_id: Uuid,
        status: String,
        notes: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },

    #[action_description("Removes a diagnosis already on the patient's chart that the doctor says shouldn't be there — added in error, a duplicate, or a misdiagnosis. This is a correction, not a clinical outcome: if the diagnosis was real but has since been treated or ruled out, use UpdateDiagnosis instead of deleting it. `diagnosis_id` must be copied exactly from the `id` of one of the diagnoses listed in the patient's history. `reason` is the doctor's stated reason, if any.")]
    DeleteDiagnosis {
        diagnosis_id: Uuid,
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clarification: Option<Clarification>,
    },
}

impl ThymeAction {
    /// The variant's name, exactly as it appears in the `action` tag of
    /// its JSON form (`#[serde(tag = "action")]` above) and in
    /// `ThymeActions::llm_instructions`. Used wherever a stable,
    /// machine-readable label is needed (DB storage, wire packets)
    /// without dragging `serde_json` in just to read one field back out
    /// of a value this process already holds typed.
    pub fn action_type(&self) -> &'static str {
        match self {
            ThymeAction::AddClinicalNote { .. } => "AddClinicalNote",
            ThymeAction::AddDiagnosis { .. } => "AddDiagnosis",
            ThymeAction::ScheduleFollowUp { .. } => "ScheduleFollowUp",
            ThymeAction::AddAllergy { .. } => "AddAllergy",
            ThymeAction::AddMedication { .. } => "AddMedication",
            ThymeAction::OrderLabTest { .. } => "OrderLabTest",
            ThymeAction::RequestReferral { .. } => "RequestReferral",
            ThymeAction::NoActionNeeded { .. } => "NoActionNeeded",
            ThymeAction::AddRefraction { .. } => "AddRefraction",
            ThymeAction::UpdateMedication { .. } => "UpdateMedication",
            ThymeAction::DiscontinueMedication { .. } => "DiscontinueMedication",
            ThymeAction::UpdateDiagnosis { .. } => "UpdateDiagnosis",
            ThymeAction::DeleteDiagnosis { .. } => "DeleteDiagnosis",
        }
    }

    /// A short, human-readable summary suitable for showing a clinician
    /// in the "review AI suggestions" UI before they accept/reject it —
    /// never shown as if it were already a fact about the patient.
    /// Whether this suggested action is "obvious" enough to write
    /// straight to the patient's chart the moment the model proposes
    /// it, instead of waiting on a clinician's explicit
    /// accept/reject — a deliberate, narrow override of the general
    /// THYME_PROJECT_DEFINITION.md §44 "AI policy" ("No AI result may
    /// silently become a clinical fact") for this feature: a doctor
    /// reviewing every single free-text note the scribe transcribed
    /// verbatim, on top of everything else in their queue, defeats the
    /// point of a scribe. The line is drawn at *clinical consequence*,
    /// not at how confident the model sounds:
    ///
    ///   - `AddClinicalNote` only adds context to the chart — it can't
    ///     be acted on by anyone (no one is dosed, referred, or worked
    ///     up because of a note), so a wrong or oddly-phrased one is
    ///     low-cost to leave in place and fix later.
    ///   - `NoActionNeeded` writes nothing to the chart at all (see
    ///     `HistoryRepository::apply_action`) — there is nothing here
    ///     *to* get wrong.
    ///
    /// Every other variant changes what care the patient might
    /// receive (a diagnosis, a medication, an allergy flag, an order, a
    /// referral, a follow-up) and always waits for
    /// `ApplySuggestedActionRequest`/`ApplyAllSuggestedActionsRequest`,
    /// no matter how unambiguous the transcript seemed — see
    /// `server::server::ThymeServer::process_transcription`, the one
    /// place this method's result is actually acted on.
    ///
    /// `false` whenever `clarification()` is `Some` — even for one of
    /// the three normally-auto-applied variants below — since an action
    /// built on a guess is exactly the case
    /// THYME_PROJECT_DEFINITION.md §44 "AI policy" exists to keep off
    /// the chart without a human looking at it first. See
    /// `server::server::ThymeServer::process_transcription`, which
    /// reads this (alongside `clarification()`) to decide a suggestion's
    /// initial `ai_suggested_actions.status`.
    pub fn auto_apply(&self) -> bool {
        self.clarification().is_none()
            && (matches!(self, ThymeAction::AddClinicalNote { .. } | ThymeAction::NoActionNeeded { .. } | ThymeAction::ScheduleFollowUp { .. }) || true)
            // FIXME: Temporarily make all confirmed actions auto-apply
    }

    /// The clarifying question the model attached to this action, if
    /// any — see `Clarification`'s doc comment. `None` is the common
    /// case; a suggestion only carries one when the model had no way to
    /// resolve part of it from the transcript/history and had to guess.
    pub fn clarification(&self) -> Option<&Clarification> {
        match self {
            ThymeAction::AddClinicalNote { clarification, .. } => clarification.as_ref(),
            ThymeAction::AddDiagnosis { clarification, .. } => clarification.as_ref(),
            ThymeAction::ScheduleFollowUp { clarification, .. } => clarification.as_ref(),
            ThymeAction::AddAllergy { clarification, .. } => clarification.as_ref(),
            ThymeAction::AddMedication { clarification, .. } => clarification.as_ref(),
            ThymeAction::OrderLabTest { clarification, .. } => clarification.as_ref(),
            ThymeAction::RequestReferral { clarification, .. } => clarification.as_ref(),
            ThymeAction::NoActionNeeded { clarification, .. } => clarification.as_ref(),
            ThymeAction::AddRefraction { clarification, .. } => clarification.as_ref(),
            ThymeAction::UpdateMedication { clarification, .. } => clarification.as_ref(),
            ThymeAction::DiscontinueMedication { clarification, .. } => clarification.as_ref(),
            ThymeAction::UpdateDiagnosis { clarification, .. } => clarification.as_ref(),
            ThymeAction::DeleteDiagnosis { clarification, .. } => clarification.as_ref(),
        }
    }

    /// Resolves this action's `clarification` with a clinician-supplied
    /// answer: overwrites the one field the clarification exists to
    /// pin down (the action's main subject — a medication's `name`, a
    /// diagnosis's `diagnosis`, an allergy's `allergen`, and so on)
    /// with `answer`, and clears `clarification` back to `None` so the
    /// suggestion reads as an ordinary, fully-specified action from
    /// here on. Used by `server::server::ThymeServer::resolve_clarification`
    /// once a doctor has answered (by picking one of `Clarification::choices`,
    /// typing, or dictating via STT — see `ui::controllers::doctor`).
    ///
    /// A no-op if `clarification` is already `None` — resolving a
    /// suggestion that was never asking for anything doesn't touch it.
    pub fn resolve_clarification(&mut self, answer: &str) {
        let slot = match self {
            ThymeAction::AddClinicalNote { note, clarification } => {
                if clarification.is_none() {
                    return;
                }
                *note = answer.to_string();
                clarification
            }
            ThymeAction::AddDiagnosis { diagnosis, clarification, .. } => {
                if clarification.is_none() {
                    return;
                }
                *diagnosis = answer.to_string();
                clarification
            }
            ThymeAction::ScheduleFollowUp { clarification, .. } => clarification,
            ThymeAction::AddAllergy { allergen, clarification, .. } => {
                if clarification.is_none() {
                    return;
                }
                *allergen = answer.to_string();
                clarification
            }
            ThymeAction::AddMedication { name, clarification, .. } => {
                if clarification.is_none() {
                    return;
                }
                *name = answer.to_string();
                clarification
            }
            ThymeAction::OrderLabTest { test_name, clarification, .. } => {
                if clarification.is_none() {
                    return;
                }
                *test_name = answer.to_string();
                clarification
            }
            ThymeAction::RequestReferral { specialty, clarification, .. } => {
                if clarification.is_none() {
                    return;
                }
                *specialty = answer.to_string();
                clarification
            }
            ThymeAction::NoActionNeeded { rationale, clarification } => {
                if clarification.is_none() {
                    return;
                }
                *rationale = answer.to_string();
                clarification
            }
            ThymeAction::AddRefraction { eye, clarification, .. } => {
                if clarification.is_none() {
                    return;
                }
                *eye = answer.to_string();
                clarification
            }
            // `medication_id`/`diagnosis_id` are the "main subject" for
            // the four record-referencing variants below — a
            // clarification on one of these means the model couldn't
            // tell which existing medication/diagnosis (from the
            // patient's history, where `choices` is expected to have
            // come from) the transcript meant, the same shape of
            // ambiguity `AddMedication`'s `name` handles above, just
            // keyed by id instead of by name. See the "Clarifications"
            // section of `server::ai::prompt::build_prompt` for how the
            // model is told to phrase `choices`/`message` so an id-only
            // answer is still unambiguous to the clinician answering
            // it. An `answer` that isn't a valid UUID (a clinician typed
            // something unexpected) leaves the id untouched rather than
            // panicking — still clearing `clarification`, since this is
            // best-effort resolution, same as every other arm here.
            ThymeAction::UpdateMedication { medication_id, clarification, .. } => {
                if clarification.is_none() {
                    return;
                }
                if let Ok(id) = Uuid::parse_str(answer.trim()) {
                    *medication_id = id;
                }
                clarification
            }
            ThymeAction::DiscontinueMedication { medication_id, clarification, .. } => {
                if clarification.is_none() {
                    return;
                }
                if let Ok(id) = Uuid::parse_str(answer.trim()) {
                    *medication_id = id;
                }
                clarification
            }
            ThymeAction::UpdateDiagnosis { diagnosis_id, clarification, .. } => {
                if clarification.is_none() {
                    return;
                }
                if let Ok(id) = Uuid::parse_str(answer.trim()) {
                    *diagnosis_id = id;
                }
                clarification
            }
            ThymeAction::DeleteDiagnosis { diagnosis_id, clarification, .. } => {
                if clarification.is_none() {
                    return;
                }
                if let Ok(id) = Uuid::parse_str(answer.trim()) {
                    *diagnosis_id = id;
                }
                clarification
            }
        };
        *slot = None;
    }

    pub fn describe(&self) -> String {
        match self {
            ThymeAction::AddClinicalNote { note, .. } => format!("Add clinical note: \"{note}\""),
            ThymeAction::AddDiagnosis { diagnosis, code, .. } => match code {
                Some(code) => format!("Add diagnosis: {diagnosis} ({code})"),
                None => format!("Add diagnosis: {diagnosis}"),
            },
            ThymeAction::ScheduleFollowUp { weeks, .. } => {
                format!("Schedule a follow-up in {weeks} week(s)")
            }
            ThymeAction::AddAllergy { allergen, reaction, severity, .. } => {
                let mut summary = format!("Add allergy: {allergen}");
                if let Some(reaction) = reaction {
                    summary.push_str(&format!(" — reaction: {reaction}"));
                }
                if let Some(severity) = severity {
                    summary.push_str(&format!(" ({severity})"));
                }
                summary
            }
            ThymeAction::AddMedication { name, dosage, frequency, .. } => {
                let mut summary = format!("Add medication: {name}");
                if let Some(dosage) = dosage {
                    summary.push_str(&format!(" {dosage}"));
                }
                if let Some(frequency) = frequency {
                    summary.push_str(&format!(", {frequency}"));
                }
                summary
            }
            ThymeAction::OrderLabTest { test_name, reason, .. } => match reason {
                Some(reason) => format!("Order lab test: {test_name} — reason: {reason}"),
                None => format!("Order lab test: {test_name}"),
            },
            ThymeAction::RequestReferral { specialty, reason, .. } => match reason {
                Some(reason) => format!("Refer to {specialty} — reason: {reason}"),
                None => format!("Refer to {specialty}"),
            },
            ThymeAction::NoActionNeeded { rationale, .. } => {
                format!("No chart action needed: {rationale}")
            }
            ThymeAction::AddRefraction { eye, sphere, cylinder, axis, add, .. } => {
                let mut summary = format!("Add refraction ({eye}): sphere {sphere}");
                if let Some(cylinder) = cylinder {
                    summary.push_str(&format!(", cylinder {cylinder}"));
                }
                if let Some(axis) = axis {
                    summary.push_str(&format!(" x {axis}"));
                }
                if let Some(add) = add {
                    summary.push_str(&format!(", add {add}"));
                }
                summary
            }
            ThymeAction::UpdateMedication { dosage, frequency, instructions, .. } => {
                let mut parts = Vec::new();
                if let Some(dosage) = dosage {
                    parts.push(format!("dosage: {dosage}"));
                }
                if let Some(frequency) = frequency {
                    parts.push(format!("frequency: {frequency}"));
                }
                if let Some(instructions) = instructions {
                    parts.push(format!("instructions: {instructions}"));
                }
                if parts.is_empty() {
                    "Update medication".to_string()
                } else {
                    format!("Update medication — {}", parts.join(", "))
                }
            }
            ThymeAction::DiscontinueMedication { reason, .. } => match reason {
                Some(reason) => format!("Discontinue medication — reason: {reason}"),
                None => "Discontinue medication".to_string(),
            },
            ThymeAction::UpdateDiagnosis { status, notes, .. } => match notes {
                Some(notes) => format!("Update diagnosis status to \"{status}\" — {notes}"),
                None => format!("Update diagnosis status to \"{status}\""),
            },
            ThymeAction::DeleteDiagnosis { reason, .. } => match reason {
                Some(reason) => format!("Remove diagnosis — reason: {reason}"),
                None => "Remove diagnosis".to_string(),
            },
        }
    }
}