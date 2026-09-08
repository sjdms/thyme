-- Backs the "incoming patients" workflow: a receptionist registers a
-- patient for a doctor to see, and that shows up live on the doctor's
-- dashboard. Deliberately not modeled as a full `encounters`/
-- `appointments` pair (see THYME_DATABASE_SCHEMA.md §9) — this is a
-- same-day walk-up queue, not scheduling, and keeping it a single small
-- table matches how far the rest of the schema (patients + identifiers
-- + contacts only) has been built out so far. A future scheduling pass
-- can grow this into (or alongside) `encounters` without disturbing
-- what's here.
CREATE TABLE
    patient_queue (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        -- 'waiting' -> 'with_doctor' -> 'done', in that order; see
        -- `db::queue_repository`.
        status TEXT NOT NULL DEFAULT 'waiting',
        registered_by TEXT NOT NULL,
        note TEXT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_patient_queue_status ON patient_queue (status, created_at);

CREATE INDEX idx_patient_queue_patient_id ON patient_queue (patient_id);