use domain::action::ThymeAction;
use domain::patient::{Patient, PatientHistory};

/// Builds the single prompt string sent to whichever `AiProvider` is
/// active for a given transcription request.
///
/// This is the one place the "rigor" the AI pipeline needs actually
/// lives — everything downstream (`AiProcessor::process_transcription`,
/// `parse_actions_from_response`) just calls a provider and parses
/// whatever comes back, so the quality of the suggestions depends
/// almost entirely on getting this text right. The structure below is
/// deliberate:
///
///   1. A role + hard constraints section, stated *before* any patient
///      data, so the model's instruction-following isn't competing
///      with the most recent (and therefore usually most attended-to)
///      part of the prompt — that's reserved for the transcript itself.
///   2. The patient's identity and history, clearly separated from the
///      transcript, so the model can use history as context without
///      confusing something from a past visit for something the doctor
///      just said today.
///   3. The transcript, labeled unambiguously as the thing to actually
///      act on.
///   4. The exact output schema (from `ThymeAction::llm_instructions`,
///      the single source of truth also used to actually decode the
///      response — see `domain::action::ThymeAction`), plus the
///      wrapping `{"actions": [...]}` envelope this pipeline expects,
///      restated at the very end where instruction-following is
///      strongest.
///
/// Per THYME_PROJECT_DEFINITION.md §44 "AI policy", the model is never
/// told it can finalize anything — every rule here is framed around
/// "propose" and "suggest", and `ThymeAction::NoActionNeeded` exists
/// specifically so the model has a safe, explicit way to propose
/// nothing rather than inventing an action just to have output.
pub fn build_prompt(patient: &Patient, history: &PatientHistory, transcript: &str) -> String {
    let mut prompt = String::new();

    prompt.push_str(
        "You are a clinical scribe assistant embedded in an ophthalmology clinic's \
        electronic health record system, Thyme. A doctor has just finished dictating \
        notes about a patient encounter, and speech-to-text has produced a transcript \
        of what they said. Your job is to read that transcript, in light of the \
        patient's existing chart, and propose which structured chart actions (if any) \
        are warranted.\n\n",
    );

    prompt.push_str(
        "## Hard rules — read carefully, these are non-negotiable\n\n\
        1. You are proposing actions for a licensed clinician to review — you are \
        NEVER finalizing anything yourself. Nothing you output is applied to the \
        patient's chart automatically.\n\
        2. Base every action strictly on what the transcript actually says. Do not \
        invent, infer, or extrapolate clinical facts (symptoms, diagnoses, dosages, \
        test names, timeframes) that are not stated or very clearly implied by the \
        doctor's own words. When in doubt, leave a field out (if optional) or omit \
        the action entirely.\n\
        3. Use the patient's history only as context for interpreting the transcript \
        (e.g. recognizing a mentioned medication as a change to an existing one) — \
        never as a source of new actions by itself. If nothing in the transcript is \
        new relative to the history, propose nothing about that topic.\n\
        4. Never propose a diagnosis, medication, or test the doctor did not actually \
        state. If the doctor merely discusses possibilities out loud (\"could be \
        early cataracts, but let's rule out X\"), do not commit to one as a firm \
        diagnosis — either omit it or capture it as a clinical note describing the \
        discussion, whichever the transcript's own wording supports.\n\
        5. If the transcript contains nothing actionable (small talk, a restatement \
        of something already on the chart, an interrupted or incomplete recording), \
        propose a single `NoActionNeeded` action explaining why, rather than \
        proposing nothing at all or inventing something to fill the response. \
        `NoActionNeeded` is for transcripts with nothing to act on — it is NOT a \
        substitute for an action just because one piece of that action is missing \
        or unclear. If the doctor clearly intended a chart action but left \
        something out or ambiguous (which drop, which existing medication, which \
        eye), propose the action itself with your best-effort fields and attach a \
        `clarification` instead of falling back to `NoActionNeeded` — see the \
        \"Clarifications\" section below. Reserve `NoActionNeeded` for genuinely \
        empty or non-clinical transcripts, not for incomplete ones.\n\
        6. When the transcript refers to a diagnosis or medication the patient \
        already has (see \"Patient history\" below, where each carries its own \
        `id`), use `UpdateDiagnosis`/`DeleteDiagnosis`/`UpdateMedication`/\
        `DiscontinueMedication` with that existing `id` rather than proposing a \
        duplicate `AddDiagnosis`/`AddMedication`. Only use the `Add*` actions for \
        something genuinely new to the chart.\n\
        7. Output ONLY valid JSON matching the schema given at the end of this \
        prompt. No prose, no markdown code fences, no explanation before or after \
        the JSON — your entire response must be parseable as JSON on its own.\n\
        8. Properly capitalize all clinical information throughout your entire JSON \
        output. Every string you produce (action fields, notes, clarification messages, \
        medication names) might be read by the user and must be formatted professionally. \
        For example, use \"Latanoprost 0.005%\" instead of \"latanoprost 0.005%\".\n\n",
    );

    prompt.push_str("## Patient\n\n");
    prompt.push_str(&format!("- Name: {}\n", patient.legal_name));
    if let Some(preferred) = &patient.preferred_name {
        prompt.push_str(&format!("- Preferred name: {preferred}\n"));
    }
    if let Some(dob) = &patient.birth_date {
        prompt.push_str(&format!("- Date of birth: {dob}\n"));
    }
    if let Some(sex) = &patient.sex {
        prompt.push_str(&format!("- Sex: {sex}\n"));
    }
    prompt.push('\n');

    prompt.push_str("## Patient history (context only — see hard rule 3)\n\n");
    if history.is_empty() {
        prompt.push_str("No prior history on file for this patient.\n\n");
    } else {
        push_section(
            &mut prompt,
            "Diagnoses",
            history.diagnoses.iter().map(|d| {
                let mut line = format!("id:{} — {}", d.id, d.diagnosis);
                if let Some(code) = &d.code {
                    line.push_str(&format!(" ({code})"));
                }
                line.push_str(&format!(" [{}]", d.status));
                if let Some(notes) = &d.notes {
                    line.push_str(&format!(" — notes: {notes}"));
                }
                line
            }),
        );
        push_section(
            &mut prompt,
            "Allergies",
            history.allergies.iter().map(|a| {
                let mut line = a.allergen.clone();
                if let Some(reaction) = &a.reaction {
                    line.push_str(&format!(" — reaction: {reaction}"));
                }
                if let Some(severity) = &a.severity {
                    line.push_str(&format!(" ({severity})"));
                }
                line
            }),
        );
        push_section(
            &mut prompt,
            "Current/past medications",
            history.medications.iter().map(|m| {
                let mut line = format!("id:{} — {}", m.id, m.name);
                if let Some(dosage) = &m.dosage {
                    line.push_str(&format!(" {dosage}"));
                }
                if let Some(frequency) = &m.frequency {
                    line.push_str(&format!(", {frequency}"));
                }
                if let Some(instructions) = &m.instructions {
                    line.push_str(&format!(" — {instructions}"));
                }
                line.push_str(&format!(" [{}]", m.status));
                if let Some(reason) = &m.discontinue_reason {
                    line.push_str(&format!(" — discontinued: {reason}"));
                }
                line
            }),
        );
        push_section(
            &mut prompt,
            "Recent clinical notes",
            history.clinical_notes.iter().map(|n| n.note.clone()),
        );
        push_section(
            &mut prompt,
            "Follow-ups",
            history
                .follow_ups
                .iter()
                .map(|f| format!("due {} [{}]", f.due_date, f.status)),
        );
        push_section(
            &mut prompt,
            "Lab orders",
            history.lab_orders.iter().map(|l| match &l.reason {
                Some(reason) => format!("{} — {reason} [{}]", l.test_name, l.status),
                None => format!("{} [{}]", l.test_name, l.status),
            }),
        );
        push_section(
            &mut prompt,
            "Referrals",
            history.referrals.iter().map(|r| match &r.reason {
                Some(reason) => format!("{} — {reason} [{}]", r.specialty, r.status),
                None => format!("{} [{}]", r.specialty, r.status),
            }),
        );
        push_section(
            &mut prompt,
            "Refractions",
            history.refractions.iter().map(|r| {
                let mut line = format!("id:{} — {}: sphere {}", r.id, r.eye, r.sphere);
                if let Some(cylinder) = &r.cylinder {
                    line.push_str(&format!(", cylinder {cylinder}"));
                }
                if let Some(axis) = &r.axis {
                    line.push_str(&format!(" x {axis}"));
                }
                if let Some(add) = &r.add {
                    line.push_str(&format!(", add {add}"));
                }
                line
            }),
        );
    }

    prompt.push_str("## Transcript of today's encounter (the thing to act on)\n\n");
    prompt.push_str("\"\"\"\n");
    prompt.push_str(transcript.trim());
    prompt.push_str("\n\"\"\"\n\n");

    prompt.push_str("## Available actions\n\n");
    prompt.push_str(ThymeAction::llm_instructions());
    prompt.push('\n');

    prompt.push_str(
        "## Clarifications — when your only option is to guess\n\n\
        Every action above also accepts an optional `clarification` field — use it \
        ONLY when a required piece of that action is genuinely unrecoverable from the \
        transcript and the patient's history, and your only option is to guess. This is \
        rare; do not use it as a general hedge, and do not use it just because a field \
        is imprecise or informally phrased. If the transcript or the patient's history \
        make the answer reasonably clear, leave `clarification` out entirely, the same \
        as for any other action.\n\n\
        A concrete example: the doctor says \"let's keep him on the drops for now, one \
        drop twice a day\" without naming a medication, and the patient's history lists \
        more than one eye-drop medication with nothing to disambiguate which one is \
        meant. Propose the `AddMedication` action anyway — never omit an action just \
        because you have to guess one field — filling every required field with your \
        single best guess (never leave one blank or invented from nothing), and attach:\n\n\
        ```json\n\
        \"clarification\": {\n  \
        \"message\": \"Which medication would you like to continue?\",\n  \
        \"answer_type\": \"single_choice\",\n  \
        \"choices\": [\"<Option from history patient's the>\", \"<Option from history patient's the>\"]\n\
        }\n\
        ```\n\n\
        `answer_type` is either `\"single_choice\"` (when the patient's history/context \
        gives you a concrete, short list of candidates to choose between — populate \
        `choices` from that context, never invented) or `\"text\"` (when there's nothing \
        to enumerate and the clinician needs to answer in their own words — omit \
        `choices`, or leave it empty, in that case). `message` should be a short, direct \
        question, phrased for the clinician who is about to answer it, not a restatement \
        of the transcript.\n\n\
        A second case: `UpdateMedication`, `DiscontinueMedication`, `UpdateDiagnosis`, \
        and `DeleteDiagnosis` identify what they're changing by `medication_id`/\
        `diagnosis_id`, copied from the `id` shown next to each entry in \"Patient \
        history\" above. If the transcript makes it clear something existing should \
        change but the patient's history has more than one plausible match (e.g. two \
        active glaucoma drops) and nothing in the transcript disambiguates them, still \
        propose the action — filling `medication_id`/`diagnosis_id` with your single \
        best guess — and attach a `clarification`. The `choices` array MUST contain the \
        human-readable names of the options, NEVER their UUIDs or `id` values. \
        Additionally, ANYTHING that might be exposed to the user (such as the `message` \
        and the items in the `choices` array) MUST be properly capitalized. For example, \
        use `\"Latanoprost 0.005%\"` rather than `\"latanoprost 0.005%\"`, and formulate \
        the `message` as `\"Which medication did you mean — Latanoprost 0.005% or \
        Timolol 0.5%?\"`. Do not ask this kind of clarification when only one candidate \
        matches; that's not genuinely ambiguous.\n\n\
        As with everything in this section: use `clarification` sparingly. Most \
        transcripts are clear enough that no action needs one, and most fields that \
        are missing should simply be left out (hard rule 2) rather than escalated. \
        Reach for `clarification` only for the one field a required guess truly can't \
        avoid, not as a routine hedge on every action.\n\n",
    );

    prompt.push_str(
        "## Required output format\n\n\
        Respond with a single JSON object of exactly this shape, and nothing else:\n\n\
        ```json\n\
        {\n  \"actions\": [ /* zero or more objects, each matching one of the action \
        schemas above, including its \"action\" tag */ ]\n}\n\
        ```\n\n\
        Every element of `actions` must be a complete, valid instance of exactly one \
        of the schemas listed under \"Available actions\" above — same field names, \
        same `action` tag values. Do not invent new action types or fields. Prefer \
        fewer, well-supported actions over many speculative ones. Remember hard rule \
        7: output ONLY the JSON object, with no surrounding text.\n",
    );

    prompt
}

/// Builds the prompt sent to the active `AiProvider` once a clinician
/// has answered a `Clarification` attached to a previously-suggested
/// action (see `Clarification` and `ThymeAction::clarification`).
///
/// Unlike `build_prompt`, this is not asking the model to propose
/// something new — the action, and the exact question that was asked,
/// already exist (`record.payload_json` / `action.clarification()`).
/// The model's only job here is to translate the doctor's raw answer
/// into the one value `ThymeAction::resolve_clarification` needs to
/// fill the field the clarification was blocking. Two things make that
/// non-trivial enough to route through the model rather than just
/// using `raw_answer` verbatim (see `AiProcessor::process_clarification_answer`,
/// which still independently validates whatever comes back):
///
///   - For `ClarificationAnswerType::SingleChoice`, the doctor's spoken
///     answer rarely repeats one of `Clarification::choices` verbatim
///     ("the timolol one" vs. `"Timolol 0.5%"`) — the model's job is to
///     resolve that to the matching choice, exactly as written.
///   - For `ClarificationAnswerType::Text`, the raw answer may still
///     need light cleanup (stray filler words from STT) without
///     changing its clinical content.
pub fn build_clarification_prompt(
    patient: &Patient,
    history: &PatientHistory,
    record: &crate::db::AiSuggestedAction,
    action: &ThymeAction,
    raw_answer: &str,
) -> String {
    let mut prompt = String::new();

    prompt.push_str(
        "You are a clinical scribe assistant embedded in an ophthalmology clinic's \
        electronic health record system, Thyme. A doctor previously dictated a note, \
        the AI proposed a chart action from it, and one field of that action was \
        ambiguous enough that a clarifying question was attached to it. The doctor has \
        now answered that question. Your job is to read the doctor's answer, in light of \
        the original transcript, the proposed action, and the patient's chart, and \
        resolve it to the exact value that clarification was asking for.\n\n",
    );

    prompt.push_str(
        "## Hard rules — read carefully, these are non-negotiable\n\n\
        1. You are resolving ONE clarification, not re-proposing the action. Do not \
        change anything about the action other than the single field the clarification \
        question was about.\n\
        2. Base your answer strictly on what the doctor actually said in their answer \
        below, read in light of the original transcript and patient history for context. \
        Do not invent, infer, or extrapolate anything the doctor did not say.\n\
        3. If `answer_type` is `\"single_choice\"`, you MUST resolve the doctor's answer \
        to exactly one of the strings listed in `choices`, copied verbatim (same spelling, \
        capitalization, and punctuation) — never a paraphrase, abbreviation, or a value \
        outside that list. If the doctor's answer clearly does not match any of the \
        `choices` (off-topic, unintelligible, refuses to answer, names something not \
        listed), do not guess — respond with the exact JSON `{\"answer\": null}` instead \
        of forcing a match.\n\
        4. If `answer_type` is `\"text\"`, resolve the doctor's answer to a short, clean, \
        properly capitalized piece of text expressing what they said — you may strip \
        filler words or false starts introduced by speech-to-text, but do not change its \
        clinical meaning or add anything the doctor didn't say. If the doctor's answer \
        is empty, off-topic, or does not actually answer the question asked, respond with \
        the exact JSON `{\"answer\": null}` instead of inventing one.\n\
        5. Output ONLY valid JSON matching the schema given at the end of this prompt. \
        No prose, no markdown code fences, no explanation before or after the JSON.\n\n",
    );

    prompt.push_str("## Patient\n\n");
    prompt.push_str(&format!("- Name: {}\n", patient.legal_name));
    if let Some(preferred) = &patient.preferred_name {
        prompt.push_str(&format!("- Preferred name: {preferred}\n"));
    }
    if let Some(dob) = &patient.birth_date {
        prompt.push_str(&format!("- Date of birth: {dob}\n"));
    }
    if let Some(sex) = &patient.sex {
        prompt.push_str(&format!("- Sex: {sex}\n"));
    }
    prompt.push('\n');

    prompt.push_str("## Patient history (context only)\n\n");
    if history.is_empty() {
        prompt.push_str("No prior history on file for this patient.\n\n");
    } else {
        push_section(
            &mut prompt,
            "Diagnoses",
            history.diagnoses.iter().map(|d| {
                let mut line = format!("id:{} — {}", d.id, d.diagnosis);
                if let Some(code) = &d.code {
                    line.push_str(&format!(" ({code})"));
                }
                line.push_str(&format!(" [{}]", d.status));
                if let Some(notes) = &d.notes {
                    line.push_str(&format!(" — notes: {notes}"));
                }
                line
            }),
        );
        push_section(
            &mut prompt,
            "Current/past medications",
            history.medications.iter().map(|m| {
                let mut line = format!("id:{} — {}", m.id, m.name);
                if let Some(dosage) = &m.dosage {
                    line.push_str(&format!(" {dosage}"));
                }
                if let Some(frequency) = &m.frequency {
                    line.push_str(&format!(", {frequency}"));
                }
                line.push_str(&format!(" [{}]", m.status));
                line
            }),
        );
    }

    prompt.push_str("## Original proposed action (context only — do not re-derive it)\n\n");
    let proposed_action = serde_json::to_string_pretty(&record.payload_json)
        .unwrap_or_else(|_| record.payload_json.to_string());
    prompt.push_str(&format!("```json\n{proposed_action}\n```\n\n"));

    prompt.push_str("## Original transcript\n\n");
    prompt.push_str("\"\"\"\n");
    prompt.push_str(record.transcript.trim());
    prompt.push_str("\n\"\"\"\n\n");

    prompt.push_str("## The clarification that was asked\n\n");
    if let Some(clarification) = action.clarification() {
        prompt.push_str(&format!("- message: {}\n", clarification.message));
        prompt.push_str(&format!(
            "- answer_type: {}\n",
            clarification.answer_type.as_str()
        ));
        if !clarification.choices.is_empty() {
            prompt.push_str("- choices:\n");
            for choice in &clarification.choices {
                prompt.push_str(&format!("  - {choice}\n"));
            }
        }
    }
    prompt.push('\n');

    prompt.push_str("## Doctor's answer to clarification\n\n");
    prompt.push_str("\"\"\"\n");
    prompt.push_str(raw_answer.trim());
    prompt.push_str("\n\"\"\"\n\n");

    prompt.push_str(
        "## Required output format\n\n\
        Respond with a single JSON object of exactly this shape, and nothing else:\n\n\
        ```json\n\
        {\n  \"answer\": \"<the resolved value, or null — see hard rules 3 and 4>\"\n}\n\
        ```\n\n\
        Remember hard rule 5: output ONLY the JSON object, with no surrounding text.\n",
    );

    prompt
}

/// Renders one history section as a `### Heading` followed by one
/// bullet per entry, or nothing at all when `entries` is empty — so an
/// absent history category doesn't pad the prompt with an empty
/// heading the model would otherwise have to read past for no benefit.
fn push_section(prompt: &mut String, heading: &str, entries: impl Iterator<Item = String>) {
    let lines: Vec<String> = entries.collect();
    if lines.is_empty() {
        return;
    }
    prompt.push_str(&format!("### {heading}\n"));
    for line in lines {
        prompt.push_str(&format!("- {line}\n"));
    }
    prompt.push('\n');
}