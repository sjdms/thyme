-- Supports the newer `ThymeAction` variants that edit or remove an
-- existing chart entry rather than only ever adding new ones (see
-- `domain::action::ThymeAction::AddRefraction` / `UpdateMedication` /
-- `DiscontinueMedication` / `UpdateDiagnosis` / `DeleteDiagnosis`, and
-- `server::db::history_repository::HistoryRepository::apply_action`).

-- A single eye's refraction/prescription reading. One row per
-- eye/visit rather than columns on `patients`, matching every other
-- history table's "one small table per action-shaped concept"
-- philosophy (see 0013_create_clinical_history_tables.sql's header).
CREATE TABLE
    refractions (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        -- 'OD' (right) | 'OS' (left) | 'OU' (both)
        eye TEXT NOT NULL,
        sphere TEXT NOT NULL,
        cylinder TEXT,
        axis INT,
        add TEXT,
        authored_by TEXT,
        source TEXT NOT NULL DEFAULT 'manual',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_refractions_patient_id ON refractions (patient_id, created_at DESC);

-- `UpdateMedication` needs somewhere to put free-text instructions
-- ("instructions" per the action's field), and `DiscontinueMedication`
-- needs somewhere to record why, alongside the `status` column this
-- table already had.
ALTER TABLE medications
ADD COLUMN instructions TEXT,
ADD COLUMN discontinue_reason TEXT;

-- `UpdateDiagnosis` needs a status to update (diagnoses previously had
-- no lifecycle at all — every row was implicitly "active") plus a place
-- for free-text notes. `DeleteDiagnosis` is a soft delete
-- (`deleted_at`/`deleted_reason`) rather than a real `DELETE`, per this
-- schema's general "don't lose provenance" stance on clinical data
-- (0013_create_clinical_history_tables.sql's header) — a diagnosis
-- charted in error still needs to be reconstructable for audit
-- purposes, it just shouldn't show up as current anymore.
ALTER TABLE diagnoses
ADD COLUMN status TEXT NOT NULL DEFAULT 'active',
ADD COLUMN notes TEXT,
ADD COLUMN deleted_at TIMESTAMPTZ,
ADD COLUMN deleted_reason TEXT;
