-- The administrative record of an encounter ending
-- (`ThymeAction::EndVisit`) — see
-- `domain::patient::VisitSummaryEntry`'s doc comment for why this is a
-- dedicated table rather than another `clinical_notes` row: a visit
-- summary answers "how did this whole encounter wrap up", not "what
-- did someone want recorded mid-visit", even though the two tables are
-- otherwise structurally identical.
CREATE TABLE
    visit_summaries (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        summary TEXT NOT NULL,
        authored_by TEXT,
        -- 'manual' | 'ai_suggested' — see clinical_notes.source
        -- (0013_create_clinical_history_tables.sql) for what these
        -- mean; a visit summary has no `'device'` case, unlike
        -- refractions, since no DICOM device ever ends a visit.
        source TEXT NOT NULL DEFAULT 'ai_suggested',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_visit_summaries_patient_id ON visit_summaries (patient_id, created_at DESC);
