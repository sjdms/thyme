-- Clinical history tables backing the `ThymeAction` variants that
-- represent durable chart entries (see `domain::action::ThymeAction`
-- and `server::db::history_repository`). Deliberately one small table
-- per action-shaped concept rather than a single generic "events"
-- table, so each keeps its own typed columns and can grow
-- independently — same philosophy as 0005_create_patients.sql
-- splitting identifiers/contacts out of `patients` itself.
--
-- Every table carries `authored_by` + `source` so an entry that started
-- life as an AI suggestion (`source = 'ai_suggested'`) stays
-- distinguishable from one a clinician typed directly (`source =
-- 'manual'`) even after the originating `ai_suggested_actions` row is
-- long gone — per THYME_PROJECT_DEFINITION.md §44 "AI policy", an
-- AI-originated chart entry is never allowed to look indistinguishable
-- from a clinician's own words.

CREATE TABLE
    clinical_notes (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        note TEXT NOT NULL,
        authored_by TEXT,
        -- 'manual' | 'ai_suggested'
        source TEXT NOT NULL DEFAULT 'manual',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_clinical_notes_patient_id ON clinical_notes (patient_id, created_at DESC);

CREATE TABLE
    diagnoses (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        diagnosis TEXT NOT NULL,
        code TEXT,
        authored_by TEXT,
        source TEXT NOT NULL DEFAULT 'manual',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_diagnoses_patient_id ON diagnoses (patient_id, created_at DESC);

CREATE TABLE
    follow_ups (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        weeks INT NOT NULL,
        due_date DATE NOT NULL,
        authored_by TEXT,
        source TEXT NOT NULL DEFAULT 'manual',
        -- 'scheduled' -> 'completed' | 'cancelled'
        status TEXT NOT NULL DEFAULT 'scheduled',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_follow_ups_patient_id ON follow_ups (patient_id, due_date);

CREATE TABLE
    allergies (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        allergen TEXT NOT NULL,
        reaction TEXT,
        severity TEXT,
        authored_by TEXT,
        source TEXT NOT NULL DEFAULT 'manual',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_allergies_patient_id ON allergies (patient_id, created_at DESC);

CREATE TABLE
    medications (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        dosage TEXT,
        frequency TEXT,
        authored_by TEXT,
        source TEXT NOT NULL DEFAULT 'manual',
        -- 'active' | 'discontinued'
        status TEXT NOT NULL DEFAULT 'active',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_medications_patient_id ON medications (patient_id, created_at DESC);

CREATE TABLE
    lab_orders (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        test_name TEXT NOT NULL,
        reason TEXT,
        authored_by TEXT,
        source TEXT NOT NULL DEFAULT 'manual',
        -- 'ordered' -> 'resulted' | 'cancelled'
        status TEXT NOT NULL DEFAULT 'ordered',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_lab_orders_patient_id ON lab_orders (patient_id, created_at DESC);

CREATE TABLE
    referrals (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        specialty TEXT NOT NULL,
        reason TEXT,
        authored_by TEXT,
        source TEXT NOT NULL DEFAULT 'manual',
        -- 'requested' -> 'scheduled' | 'completed'
        status TEXT NOT NULL DEFAULT 'requested',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_referrals_patient_id ON referrals (patient_id, created_at DESC);
