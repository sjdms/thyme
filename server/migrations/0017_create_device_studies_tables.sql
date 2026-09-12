-- Device-originated ophthalmic data received over DICOM (see
-- `server::dicom`) — imaging pushed from an OCT topographer and
-- structured refraction readings pushed from an auto-refractor/
-- keratometer, both landing here rather than being typed in by a
-- clinician or proposed by the AI pipeline.
--
-- Kept in its own tables rather than reusing `transcriptions`/
-- `ai_suggested_actions` (0014_create_ai_tables.sql): a device reading
-- has no transcript and no AI provider behind it, so forcing it
-- through those tables' NOT NULL foreign keys would misrepresent
-- where the data actually came from. The staging/apply split below
-- still follows the same shape the AI pipeline uses
-- (`HistoryRepository`'s doc comment) — a device-reported refraction
-- is a *proposal* until a clinician accepts it, same as an
-- AI-suggested one, per THYME_PROJECT_DEFINITION.md §44's "no result
-- may silently become a clinical fact": the reading itself is
-- objective instrument output, but which patient it belongs to is
-- only ever the device's word for it (see
-- `server::dicom::patient_match`), so that link still gets a
-- clinician's confirmation before it becomes chart fact.

CREATE TABLE
    device_studies (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        -- Resolved once the incoming DICOM Patient ID has been matched
        -- to a Thyme patient. NULL until then (or forever, if the
        -- device sent an identifier that matches no one), so a study
        -- from a device that hasn't been told the clinic's MRN still
        -- lands somewhere reviewable instead of being silently
        -- dropped.
        patient_id UUID REFERENCES patients (id) ON DELETE CASCADE,
        -- The DICOM Patient ID/Name exactly as received, kept even
        -- after matching (or failing to match) so a clinician
        -- resolving an unmatched study has something to go on.
        source_patient_id TEXT,
        source_patient_name TEXT,
        -- Calling AE title of the sending device — the only "which
        -- device sent this" label this schema needs; a human-readable
        -- make/model isn't derivable from DICOM alone and isn't
        -- required for review.
        device_ae_title TEXT NOT NULL,
        modality TEXT,
        sop_class_uid TEXT NOT NULL,
        sop_instance_uid TEXT NOT NULL,
        study_instance_uid TEXT,
        -- 'unmatched' (no patient_id yet) | 'pending_review' (matched,
        -- has device_suggested_actions rows awaiting a clinician) |
        -- 'applied' | 'rejected'. An OCT-only study (no refraction
        -- suggestion to review) goes straight from 'unmatched' to
        -- 'applied' once matched — see `server::dicom::oct`.
        status TEXT NOT NULL DEFAULT 'unmatched',
        received_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        resolved_at TIMESTAMPTZ,
        CONSTRAINT device_studies_sop_instance_unique UNIQUE (sop_instance_uid)
    );

CREATE INDEX idx_device_studies_patient_id ON device_studies (patient_id, received_at DESC);

CREATE INDEX idx_device_studies_status ON device_studies (status)
WHERE
    status IN ('unmatched', 'pending_review');

-- One ophthalmic image (today: OCT) received for a study. Pixel data
-- is kept on disk (see `server::dicom::oct`), not in this table —
-- `file_path` points at it. Filed straight onto the patient's record
-- once the study is matched, with no separate accept step: an image
-- carries no clinical *interpretation* to review, just imaging data,
-- the same way an uploaded lab PDF would be.
CREATE TABLE
    oct_images (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        study_id UUID NOT NULL REFERENCES device_studies (id) ON DELETE CASCADE,
        -- 'OD' | 'OS' — from Image Laterality/Laterality if present.
        eye TEXT,
        file_path TEXT NOT NULL,
        rows INT,
        columns INT,
        number_of_frames INT,
        acquisition_datetime TIMESTAMPTZ,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE INDEX idx_oct_images_study_id ON oct_images (study_id);

-- A refraction reading extracted from a device study, staged the same
-- way an AI-suggested `ThymeAction` is (`ai_suggested_actions`) until
-- a clinician accepts it via `HistoryRepository::apply_action` — see
-- this file's header. `payload_json` holds a serialized
-- `domain::action::ThymeAction::AddRefraction`, one row per eye (same
-- as the AI pipeline proposes one `AddRefraction` per eye rather than
-- one `OU` row, when both eyes are measured).
CREATE TABLE
    device_suggested_actions (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        study_id UUID NOT NULL REFERENCES device_studies (id) ON DELETE CASCADE,
        patient_id UUID NOT NULL REFERENCES patients (id) ON DELETE CASCADE,
        action_type TEXT NOT NULL,
        payload_json JSONB NOT NULL,
        description TEXT NOT NULL,
        -- 'pending' | 'applied' | 'rejected'
        status TEXT NOT NULL DEFAULT 'pending',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        resolved_at TIMESTAMPTZ
    );

CREATE INDEX idx_device_suggested_actions_study_id ON device_suggested_actions (study_id);

CREATE INDEX idx_device_suggested_actions_patient_id ON device_suggested_actions (patient_id, status);
