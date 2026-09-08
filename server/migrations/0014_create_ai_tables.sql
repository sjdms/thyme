-- Backs the AI transcription-processing pipeline (`server::ai` /
-- `server::db::ai_repository`): a raw transcript is recorded once per
-- Ctrl+R take, and every action the model proposed from it is recorded
-- as its own row so a clinician can accept/reject each independently —
-- per THYME_PROJECT_DEFINITION.md §44 "AI policy": "No AI result may
-- silently become a clinical fact." Suggestions are never auto-applied;
-- `ai_suggested_actions.status` only ever leaves 'pending' through an
-- explicit ApplySuggestedActionRequest/RejectSuggestedActionRequest
-- from a doctor terminal (see `packets::ai`).

CREATE TABLE
    transcriptions (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        transcript TEXT NOT NULL,
        ai_provider TEXT,
        ai_model TEXT,
        raw_ai_response TEXT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_transcriptions_patient_id ON transcriptions (patient_id, created_at DESC);

CREATE TABLE
    ai_suggested_actions (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        transcription_id UUID NOT NULL REFERENCES transcriptions (id) ON DELETE CASCADE,
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        action_type TEXT NOT NULL,
        payload_json JSONB NOT NULL,
        description TEXT NOT NULL,
        -- 'pending' -> 'applied' | 'rejected', in that order; see
        -- `db::ai_repository`.
        status TEXT NOT NULL DEFAULT 'pending',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        resolved_at TIMESTAMPTZ
    );

CREATE INDEX idx_ai_suggested_actions_transcription_id ON ai_suggested_actions (transcription_id);

CREATE INDEX idx_ai_suggested_actions_patient_id ON ai_suggested_actions (patient_id, status);
