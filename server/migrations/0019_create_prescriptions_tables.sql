-- Formal prescriptions (`ThymeAction::IssueGlassesPrescription`/
-- `IssueMedicationPrescription`) — see
-- `domain::patient::GlassesPrescriptionEntry`/`MedicationPrescriptionEntry`'s
-- doc comments for why these are separate concepts from
-- `refractions`/`medications` respectively, even though a prescription
-- is often issued on the back of a refraction reading or an active
-- medication.
--
-- Both tables use the same `status`/`*_reason` lifecycle shape
-- `medications.status`/`discontinue_reason` already established
-- (0013_create_clinical_history_tables.sql) rather than
-- `diagnoses.deleted_at`'s soft-delete-and-filter shape
-- (0015_add_refractions_and_chart_edit_columns.sql): a voided
-- prescription is still relevant history for anyone looking at why a
-- patient has two prescriptions on file, the same way a discontinued
-- medication still is — it should stay visible, just labeled, not
-- disappear the way a corrected/duplicate diagnosis should.
CREATE TABLE
    glasses_prescriptions (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        od_sphere TEXT,
        od_cylinder TEXT,
        od_axis INT,
        od_add TEXT,
        os_sphere TEXT,
        os_cylinder TEXT,
        os_axis INT,
        os_add TEXT,
        pd TEXT,
        notes TEXT,
        -- 'active' | 'voided'
        status TEXT NOT NULL DEFAULT 'active',
        expires_on DATE,
        voided_reason TEXT,
        authored_by TEXT,
        -- 'manual' | 'ai_suggested'
        source TEXT NOT NULL DEFAULT 'ai_suggested',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_glasses_prescriptions_patient_id ON glasses_prescriptions (patient_id, created_at DESC);

CREATE TABLE
    medication_prescriptions (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        medication_name TEXT NOT NULL,
        strength TEXT,
        form TEXT,
        sig TEXT NOT NULL,
        quantity TEXT,
        refills INT,
        generic_allowed BOOLEAN NOT NULL DEFAULT true,
        notes TEXT,
        -- 'active' | 'voided'
        status TEXT NOT NULL DEFAULT 'active',
        expires_on DATE,
        voided_reason TEXT,
        authored_by TEXT,
        -- 'manual' | 'ai_suggested'
        source TEXT NOT NULL DEFAULT 'ai_suggested',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_medication_prescriptions_patient_id ON medication_prescriptions (patient_id, created_at DESC);
