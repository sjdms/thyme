CREATE TABLE
    patients (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        status TEXT NOT NULL DEFAULT 'active',
        legal_name TEXT NOT NULL,
        preferred_name TEXT,
        birth_date DATE,
        sex TEXT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_patients_legal_name ON patients (lower(legal_name));

CREATE INDEX idx_patients_preferred_name ON patients (lower(preferred_name));

CREATE TABLE
    patient_identifiers (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        system TEXT NOT NULL,
        value TEXT NOT NULL,
        identifier_type TEXT NOT NULL,
        is_primary BOOLEAN NOT NULL DEFAULT FALSE,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        CONSTRAINT patient_identifiers_unique UNIQUE (patient_id, system, value)
    );

CREATE INDEX idx_patient_identifiers_value ON patient_identifiers (value);

CREATE TABLE
    patient_contacts (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        type TEXT NOT NULL,
        value TEXT NOT NULL,
        preferred BOOLEAN NOT NULL DEFAULT FALSE,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_patient_contacts_value ON patient_contacts (value);
