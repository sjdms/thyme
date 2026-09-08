# THYME_DATABASE_SCHEMA.md

# Thyme Database Schema

**Project:** Thyme  
**Artifact:** Relational Database Schema  
**Version:** 0.1  
**Status:** Implementation foundation  
**Date:** 2026-08-16

> This is a logical PostgreSQL-class schema. Exact SQL types, extensions, partitioning, encryption, migration tooling and ORM mappings belong to the database implementation phase.

---

# 1. Design goals

The database must provide:

- Referential integrity
- Eye-aware clinical storage
- Longitudinal history
- Append/version semantics
- Provenance
- Auditability
- Fast patient/encounter access
- Searchable structured measurements
- Large-file references without putting image binaries into core clinical tables
- External identifiers
- Safe concurrent editing
- FHIR/HL7/DICOM interoperability boundaries

The database is a persistence model, not the Thyme domain model. Domain rules remain above it.

---

# 2. Recommended database

PostgreSQL is the reference target because it provides:

- strong relational integrity
- transactions
- rich indexing
- JSONB for controlled extensibility
- full-text/trigram options
- range/time features
- robust concurrency
- mature backup tooling

The model is intentionally portable enough to be adapted later.

---

# 3. Type conventions

```text
UUID        internal identifiers
TIMESTAMPTZ event timestamps
DATE        date-only values
TEXT        narrative fields
VARCHAR     bounded strings
NUMERIC     clinical quantities requiring precision
INTEGER     counts
BOOLEAN     flags
JSONB       configuration/vendor metadata, not core clinical truth
```

Do not use patient names, MRNs, DICOM UIDs, or device serial numbers as internal primary keys.

---

# 4. Logical namespaces

Suggested database namespaces:

```text
core
clinical
diagnostic
treatment
procedure
workflow
document
interop
security
```

A first release may implement them in one physical schema while maintaining these ownership boundaries at the code/module level.

---

# 5. Core tables

## organizations

```sql
organizations (
    id              uuid primary key,
    name            text not null,
    legal_name      text,
    type            text not null,
    status          text not null,
    country_code    varchar(2),
    timezone        text,
    default_locale  text,
    created_at      timestamptz not null,
    updated_at      timestamptz not null
)
```

## locations

```sql
locations (
    id              uuid primary key,
    organization_id uuid not null references organizations(id),
    name            text not null,
    address         jsonb,
    phone           text,
    timezone        text,
    active          boolean not null default true,
    created_at      timestamptz not null,
    updated_at      timestamptz not null
)
```

Indexes:

```text
locations(organization_id, active)
```

---

# 6. Users, roles and permissions

## users

```sql
users (
    id                     uuid primary key,
    organization_id        uuid not null references organizations(id),
    username               text not null,
    display_name           text not null,
    email                  text,
    status                 text not null,
    authentication_subject text,
    last_login_at          timestamptz,
    created_at             timestamptz not null,
    updated_at             timestamptz not null
)
```

Unique identity should be scoped to the organization.

## roles

```sql
roles (
    id              uuid primary key,
    organization_id uuid not null references organizations(id),
    name            text not null,
    description     text,
    created_at      timestamptz not null
)
```

## permissions

```sql
permissions (
    id          uuid primary key,
    code        text not null unique,
    description text not null
)
```

## role_permissions

```sql
role_permissions (
    role_id       uuid not null references roles(id),
    permission_id uuid not null references permissions(id),
    primary key (role_id, permission_id)
)
```

## user_roles

```sql
user_roles (
    user_id     uuid not null references users(id),
    role_id     uuid not null references roles(id),
    location_id uuid references locations(id),
    primary key (user_id, role_id, location_id)
)
```

For global roles, implement an explicit uniqueness strategy because nullable columns in unique constraints require care.

---

# 7. Patient tables

## patients

```sql
patients (
    id                 uuid primary key,
    organization_id    uuid not null references organizations(id),
    status             text not null,
    legal_name         text not null,
    preferred_name     text,
    birth_date         date,
    sex                text,
    deceased_at        timestamptz,
    preferred_language text,
    notes              text,
    created_at         timestamptz not null,
    updated_at         timestamptz not null
)
```

## patient_identifiers

```sql
patient_identifiers (
    id              uuid primary key,
    patient_id      uuid not null references patients(id),
    system          text not null,
    value           text not null,
    identifier_type text not null,
    issuer          text,
    is_primary      boolean not null default false,
    valid_from      date,
    valid_to        date,
    created_at      timestamptz not null
)
```

Unique:

```text
(patient_id, system, value)
```

Additional system-specific uniqueness can be added where appropriate.

## patient_contacts

```sql
patient_contacts (
    id          uuid primary key,
    patient_id  uuid not null references patients(id),
    type        text not null,
    value       text not null,
    use         text,
    preferred   boolean not null default false,
    valid_from  date,
    valid_to    date,
    created_at  timestamptz not null
)
```

## patient_addresses

```sql
patient_addresses (
    id           uuid primary key,
    patient_id   uuid not null references patients(id),
    use          text,
    line1        text,
    line2        text,
    city         text,
    region       text,
    postal_code  text,
    country_code varchar(2),
    valid_from   date,
    valid_to     date,
    created_at   timestamptz not null
)
```

## emergency_contacts

```sql
emergency_contacts (
    id           uuid primary key,
    patient_id   uuid not null references patients(id),
    name         text not null,
    relationship text,
    phone        text,
    email        text,
    notes        text,
    created_at   timestamptz not null,
    updated_at   timestamptz not null
)
```

---

# 8. History tables

## allergies

```sql
allergies (
    id                   uuid primary key,
    patient_id           uuid not null references patients(id),
    substance_concept_id uuid references terminology_concepts(id),
    category             text,
    clinical_status      text not null,
    verification_status  text,
    criticality          text,
    recorded_at          timestamptz not null,
    recorder_id          uuid references users(id),
    note                 text,
    provenance_id        uuid references provenance(id)
)
```

## allergy_reactions

```sql
allergy_reactions (
    id            uuid primary key,
    allergy_id    uuid not null references allergies(id),
    manifestation text not null,
    severity      text,
    onset_at      timestamptz,
    notes         text
)
```

## medication_exposures

```sql
medication_exposures (
    id                    uuid primary key,
    patient_id            uuid not null references patients(id),
    medication_concept_id uuid references medication_concepts(id),
    medication_text       text,
    dose                  numeric,
    dose_unit             text,
    route                 text,
    frequency             text,
    indication            text,
    start_date            date,
    end_date              date,
    status                text not null,
    source                text,
    notes                 text,
    provenance_id         uuid references provenance(id),
    created_at            timestamptz not null,
    updated_at            timestamptz not null
)
```

## ocular_history_entries

```sql
ocular_history_entries (
    id               uuid primary key,
    patient_id       uuid not null references patients(id),
    laterality       varchar(3) not null,
    event_concept_id uuid references terminology_concepts(id),
    event_text       text,
    event_date       date,
    status           text,
    notes            text,
    provenance_id    uuid references provenance(id),
    created_at       timestamptz not null
)
```

## family_history_entries

```sql
family_history_entries (
    id                   uuid primary key,
    patient_id           uuid not null references patients(id),
    condition_concept_id uuid references terminology_concepts(id),
    relationship         text,
    laterality           varchar(3),
    notes                text,
    created_at           timestamptz not null
)
```

---

# 9. Scheduling and encounters

## appointment_types

```sql
appointment_types (
    id               uuid primary key,
    organization_id  uuid not null references organizations(id),
    name             text not null,
    duration_minutes integer,
    active           boolean not null default true
)
```

## appointments

```sql
appointments (
    id                  uuid primary key,
    organization_id     uuid not null references organizations(id),
    location_id         uuid references locations(id),
    patient_id          uuid not null references patients(id),
    provider_id         uuid references users(id),
    appointment_type_id uuid references appointment_types(id),
    status              text not null,
    reason              text,
    start_at            timestamptz not null,
    end_at              timestamptz not null,
    notes               text,
    created_at          timestamptz not null,
    updated_at          timestamptz not null
)
```

Indexes:

```text
appointments(location_id, start_at)
appointments(provider_id, start_at)
appointments(patient_id, start_at DESC)
appointments(status, start_at)
```

## encounters

```sql
encounters (
    id                    uuid primary key,
    patient_id            uuid not null references patients(id),
    organization_id       uuid not null references organizations(id),
    location_id           uuid references locations(id),
    appointment_id        uuid references appointments(id),
    attending_provider_id uuid references users(id),
    encounter_type        text not null,
    status                text not null,
    reason                text,
    started_at            timestamptz not null,
    ended_at              timestamptz,
    signed_at             timestamptz,
    created_at            timestamptz not null,
    updated_at            timestamptz not null,
    provenance_id         uuid references provenance(id)
)
```

Indexes:

```text
encounters(patient_id, started_at DESC)
encounters(attending_provider_id, started_at DESC)
encounters(organization_id, started_at DESC)
```

## encounter_participants

```sql
encounter_participants (
    id           uuid primary key,
    encounter_id uuid not null references encounters(id),
    user_id      uuid not null references users(id),
    role         text not null,
    started_at   timestamptz,
    ended_at     timestamptz
)
```

---

# 10. Terminology

## terminology_concepts

```sql
terminology_concepts (
    id       uuid primary key,
    system   text not null,
    code     text not null,
    display  text not null,
    version  text,
    status   text,
    created_at timestamptz not null
)
```

Unique strategy depends on versioning policy; commonly `(system, code, version)`.

## concept_mappings

```sql
concept_mappings (
    id            uuid primary key,
    source_system text not null,
    source_code   text not null,
    target_system text not null,
    target_code   text not null,
    relationship  text,
    version       text
)
```

---

# 11. Generic observations

## observations

```sql
observations (
    id             uuid primary key,
    patient_id     uuid not null references patients(id),
    encounter_id   uuid references encounters(id),
    concept_id     uuid references terminology_concepts(id),
    scope          text not null,
    laterality     varchar(3),
    value_type     text not null,
    value_numeric  numeric,
    value_text     text,
    value_code_id  uuid references terminology_concepts(id),
    value_boolean  boolean,
    unit           text,
    method_code_id uuid references terminology_concepts(id),
    interpretation text,
    status         text not null,
    observed_at    timestamptz not null,
    recorded_at    timestamptz not null,
    performer_id   uuid references users(id),
    device_id      uuid references devices(id),
    note           text,
    provenance_id  uuid references provenance(id),
    created_at     timestamptz not null
)
```

Application-level validation should ensure that the selected `value_type` matches the populated value column(s).

---

# 12. Ophthalmic measurements

## visual_acuities

```sql
visual_acuities (
    id                 uuid primary key,
    patient_id         uuid not null references patients(id),
    encounter_id       uuid not null references encounters(id),
    laterality         varchar(3) not null,
    test_distance      numeric,
    distance_unit      text,
    correction         text,
    measurement_system text not null,
    normalized_value   numeric,
    display_value      text not null,
    pinhole_display    text,
    method             text,
    performer_id       uuid references users(id),
    tested_at          timestamptz not null,
    provenance_id      uuid references provenance(id)
)
```

Indexes:

```text
visual_acuities(patient_id, laterality, tested_at DESC)
visual_acuities(encounter_id, laterality)
```

## refractions

```sql
refractions (
    id                    uuid primary key,
    patient_id            uuid not null references patients(id),
    encounter_id          uuid not null references encounters(id),
    laterality            varchar(3) not null,
    refraction_type       text not null,
    sphere                numeric(7,3),
    cylinder              numeric(7,3),
    axis                  numeric(6,2),
    add_power             numeric(7,3),
    prism_horizontal      numeric(7,3),
    prism_horizontal_base text,
    prism_vertical        numeric(7,3),
    prism_vertical_base   text,
    pd_total              numeric(7,2),
    pd_right              numeric(7,2),
    pd_left               numeric(7,2),
    acuity_id             uuid references visual_acuities(id),
    method                text,
    performer_id          uuid references users(id),
    performed_at          timestamptz not null,
    notes                 text,
    provenance_id         uuid references provenance(id)
)
```

Axis/power range checks should be added after the domain-specific allowed ranges are confirmed.

## tonometry

```sql
tonometry (
    id             uuid primary key,
    patient_id     uuid not null references patients(id),
    encounter_id   uuid not null references encounters(id),
    laterality     varchar(3) not null,
    pressure       numeric(7,2) not null,
    unit           text not null,
    method         text not null,
    device_id      uuid references devices(id),
    measurement_at timestamptz not null,
    performer_id   uuid references users(id),
    repeat_number  integer,
    selected       boolean not null default false,
    notes          text,
    provenance_id  uuid references provenance(id)
)
```

Index:

```text
tonometry(patient_id, laterality, measurement_at DESC)
```

---

# 13. Structured examinations

## slit_lamp_exams

```sql
slit_lamp_exams (
    id            uuid primary key,
    encounter_id  uuid not null references encounters(id),
    laterality    varchar(3) not null,
    performer_id  uuid references users(id),
    performed_at  timestamptz not null,
    notes         text,
    provenance_id uuid references provenance(id)
)
```

## slit_lamp_findings

```sql
slit_lamp_findings (
    id                   uuid primary key,
    slit_lamp_exam_id    uuid not null references slit_lamp_exams(id),
    anatomical_structure text not null,
    finding_concept_id   uuid references terminology_concepts(id),
    severity             text,
    status               text,
    free_text            text
)
```

## fundus_exams

```sql
fundus_exams (
    id            uuid primary key,
    encounter_id  uuid not null references encounters(id),
    laterality    varchar(3) not null,
    performer_id  uuid references users(id),
    performed_at  timestamptz not null,
    notes         text,
    provenance_id uuid references provenance(id)
)
```

## fundus_findings

```sql
fundus_findings (
    id                 uuid primary key,
    fundus_exam_id     uuid not null references fundus_exams(id),
    structure          text not null,
    finding_concept_id uuid references terminology_concepts(id),
    severity           text,
    status             text,
    free_text          text
)
```

---

# 14. Diagnoses and problems

## problems

```sql
problems (
    id            uuid primary key,
    patient_id    uuid not null references patients(id),
    concept_id    uuid references terminology_concepts(id),
    laterality    varchar(3),
    status        text not null,
    onset_date    date,
    resolved_date date,
    notes         text,
    provenance_id uuid references provenance(id),
    created_at    timestamptz not null,
    updated_at    timestamptz not null
)
```

## diagnoses

```sql
diagnoses (
    id                  uuid primary key,
    patient_id          uuid not null references patients(id),
    encounter_id        uuid not null references encounters(id),
    problem_id          uuid references problems(id),
    concept_id          uuid references terminology_concepts(id),
    laterality          varchar(3),
    clinical_status     text not null,
    verification_status text,
    severity            text,
    stage               text,
    onset_date          date,
    assessed_at         timestamptz not null,
    assessor_id         uuid references users(id),
    assessment_note     text,
    provenance_id       uuid references provenance(id)
)
```

## diagnosis_codes

```sql
diagnosis_codes (
    id            uuid primary key,
    diagnosis_id  uuid not null references diagnoses(id),
    coding_system text not null,
    code          text not null,
    display       text,
    version       text
)
```

Indexes:

```text
diagnoses(patient_id, assessed_at DESC)
diagnoses(problem_id, assessed_at DESC)
diagnosis_codes(diagnosis_id, coding_system)
```

---

# 15. Diagnostic orders and results

## diagnostic_orders

```sql
diagnostic_orders (
    id              uuid primary key,
    patient_id      uuid not null references patients(id),
    encounter_id    uuid not null references encounters(id),
    ordered_by      uuid not null references users(id),
    test_concept_id uuid references terminology_concepts(id),
    laterality      varchar(3),
    priority        text,
    status          text not null,
    reason          text,
    ordered_at      timestamptz not null
)
```

## diagnostic_results

```sql
diagnostic_results (
    id                 uuid primary key,
    order_id           uuid references diagnostic_orders(id),
    patient_id         uuid not null references patients(id),
    encounter_id       uuid references encounters(id),
    test_concept_id    uuid references terminology_concepts(id),
    status             text not null,
    performed_at       timestamptz,
    resulted_at        timestamptz,
    performer_id       uuid references users(id),
    interpreter_id     uuid references users(id),
    interpretation     text,
    report_document_id uuid references documents(id),
    provenance_id      uuid references provenance(id)
)
```

---

# 16. Imaging

## imaging_studies

```sql
imaging_studies (
    id                 uuid primary key,
    patient_id         uuid not null references patients(id),
    encounter_id       uuid references encounters(id),
    diagnostic_order_id uuid references diagnostic_orders(id),
    study_instance_uid text unique,
    modality           text not null,
    body_site          text,
    laterality         varchar(3),
    device_id          uuid references devices(id),
    started_at         timestamptz,
    ended_at           timestamptz,
    status             text not null,
    archive_reference  text,
    provenance_id      uuid references provenance(id),
    created_at         timestamptz not null
)
```

## imaging_series

```sql
imaging_series (
    id                  uuid primary key,
    imaging_study_id    uuid not null references imaging_studies(id),
    series_instance_uid text unique,
    modality            text,
    description         text,
    laterality          varchar(3)
)
```

## imaging_instances

```sql
imaging_instances (
    id               uuid primary key,
    series_id        uuid not null references imaging_series(id),
    sop_instance_uid text unique,
    uri_reference    text,
    storage_backend  text,
    checksum         text,
    mime_type        text,
    created_at       timestamptz not null
)
```

Relational storage contains metadata and references; large binary imaging content belongs in a dedicated storage boundary/PACS/VNA where appropriate.

---

# 17. Diagnostic reports

```sql
diagnostic_reports (
    id                  uuid primary key,
    patient_id          uuid not null references patients(id),
    encounter_id        uuid references encounters(id),
    diagnostic_order_id uuid references diagnostic_orders(id),
    imaging_study_id    uuid references imaging_studies(id),
    report_type         text not null,
    status               text not null,
    conclusion           text,
    interpretation      text,
    author_id            uuid references users(id),
    issued_at            timestamptz,
    document_id          uuid references documents(id)
)
```

---

# 18. Medications and prescriptions

## medication_concepts

```sql
medication_concepts (
    id                 uuid primary key,
    terminology_system text,
    code               text,
    display            text not null,
    strength           text,
    dose_form          text,
    status             text
)
```

## medication_prescriptions

```sql
medication_prescriptions (
    id                    uuid primary key,
    patient_id            uuid not null references patients(id),
    encounter_id          uuid not null references encounters(id),
    prescriber_id         uuid not null references users(id),
    medication_concept_id uuid references medication_concepts(id),
    medication_text       text,
    status                text not null,
    dosage                text,
    dose_numeric          numeric,
    dose_unit             text,
    route                 text,
    frequency             text,
    duration_value        numeric,
    duration_unit         text,
    quantity              numeric,
    quantity_unit         text,
    refills               integer,
    eye_target            varchar(3),
    indication            text,
    start_at              timestamptz,
    end_at                timestamptz,
    instructions          text,
    signed_at             timestamptz,
    provenance_id         uuid references provenance(id),
    created_at            timestamptz not null
)
```

A prescription can retain normalized fields plus human-readable patient instructions.

---

# 19. Vision prescriptions

## vision_prescriptions

```sql
vision_prescriptions (
    id            uuid primary key,
    patient_id    uuid not null references patients(id),
    encounter_id  uuid not null references encounters(id),
    prescriber_id uuid not null references users(id),
    status        text not null,
    issued_at     timestamptz not null,
    expires_at    timestamptz,
    signed_at     timestamptz,
    notes         text,
    provenance_id uuid references provenance(id)
)
```

## vision_prescription_eyes

```sql
vision_prescription_eyes (
    id                     uuid primary key,
    vision_prescription_id uuid not null references vision_prescriptions(id),
    laterality             varchar(3) not null,
    sphere                 numeric(7,3),
    cylinder               numeric(7,3),
    axis                   numeric(6,2),
    add_power              numeric(7,3),
    prism_horizontal       numeric(7,3),
    prism_horizontal_base  text,
    prism_vertical         numeric(7,3),
    prism_vertical_base    text,
    pd                     numeric(7,2),
    lens_type              text,
    material               text,
    coating                text,
    notes                  text
)
```

Unique `(vision_prescription_id, laterality)`.

## contact_lens_prescriptions

```sql
contact_lens_prescriptions (
    id                         uuid primary key,
    vision_prescription_eye_id uuid not null references vision_prescription_eyes(id),
    lens_type                  text,
    base_curve                 numeric(7,3),
    diameter                   numeric(7,3),
    power                      numeric(7,3),
    cylinder                   numeric(7,3),
    axis                      numeric(6,2),
    add_power                  numeric(7,3),
    manufacturer               text,
    model                      text,
    notes                      text
)
```

---

# 20. Procedures, implants and consent

## procedures

```sql
procedures (
    id                   uuid primary key,
    patient_id           uuid not null references patients(id),
    encounter_id         uuid not null references encounters(id),
    procedure_concept_id uuid references terminology_concepts(id),
    laterality           varchar(3),
    status               text not null,
    performed_start      timestamptz,
    performed_end        timestamptz,
    performer_id         uuid references users(id),
    indication           text,
    findings             text,
    outcome              text,
    complications        text,
    notes                text,
    provenance_id        uuid references provenance(id)
)
```

## procedure_medications

```sql
procedure_medications (
    id                    uuid primary key,
    procedure_id          uuid not null references procedures(id),
    medication_concept_id uuid references medication_concepts(id),
    dose                  numeric,
    dose_unit             text,
    route                 text,
    eye_target            varchar(3),
    administered_at       timestamptz
)
```

## implants

```sql
implants (
    id                uuid primary key,
    patient_id        uuid not null references patients(id),
    procedure_id      uuid references procedures(id),
    device_concept_id uuid references terminology_concepts(id),
    manufacturer      text,
    model             text,
    serial_number     text,
    lot_number        text,
    laterality        varchar(3),
    implanted_at      timestamptz,
    removed_at        timestamptz
)
```

## consents

```sql
consents (
    id            uuid primary key,
    patient_id    uuid not null references patients(id),
    encounter_id  uuid references encounters(id),
    procedure_id  uuid references procedures(id),
    consent_type  text not null,
    status        text not null,
    version       text not null,
    signed_by     uuid references users(id),
    signed_at     timestamptz,
    document_id   uuid references documents(id),
    created_at    timestamptz not null
)
```

---

# 21. Care plans, follow-up, referrals and tasks

## care_plans

```sql
care_plans (
    id            uuid primary key,
    patient_id    uuid not null references patients(id),
    encounter_id  uuid references encounters(id),
    author_id     uuid references users(id),
    status        text not null,
    start_date    date,
    review_date   date,
    summary      text,
    provenance_id uuid references provenance(id),
    created_at    timestamptz not null,
    updated_at    timestamptz not null
)
```

## care_plan_items

```sql
care_plan_items (
    id           uuid primary key,
    care_plan_id uuid not null references care_plans(id),
    type         text not null,
    target       text,
    status       text not null,
    due_at       timestamptz,
    notes        text
)
```

## follow_ups

```sql
follow_ups (
    id             uuid primary key,
    patient_id     uuid not null references patients(id),
    encounter_id   uuid references encounters(id),
    requested_by   uuid references users(id),
    interval_value numeric,
    interval_unit  text,
    target_date    date,
    reason         text,
    status         text not null,
    notes          text
)
```

## referrals

```sql
referrals (
    id                          uuid primary key,
    patient_id                  uuid not null references patients(id),
    encounter_id                uuid references encounters(id),
    referring_provider_id       uuid references users(id),
    destination_provider_id    uuid references users(id),
    destination_organization_id uuid references organizations(id),
    reason                      text,
    diagnosis_id                uuid references diagnoses(id),
    status                      text not null,
    created_at                  timestamptz not null,
    appointment_date            date,
    document_id                 uuid references documents(id)
)
```

## tasks

```sql
tasks (
    id            uuid primary key,
    patient_id    uuid references patients(id),
    encounter_id  uuid references encounters(id),
    assignee_id   uuid references users(id),
    creator_id    uuid references users(id),
    task_type     text not null,
    status        text not null,
    priority      text not null,
    due_at        timestamptz,
    completed_at  timestamptz,
    description   text,
    created_at    timestamptz not null
)
```

---

# 22. Documents and notes

## documents

```sql
documents (
    id                uuid primary key,
    patient_id        uuid references patients(id),
    encounter_id      uuid references encounters(id),
    document_type     text not null,
    title             text not null,
    mime_type         text not null,
    source            text,
    author_id         uuid references users(id),
    created_at        timestamptz not null,
    document_version  integer not null default 1,
    storage_reference text not null,
    checksum          text,
    status             text not null,
    provenance_id     uuid references provenance(id)
)
```

Indexes:

```text
documents(patient_id, created_at DESC)
documents(patient_id, document_type, created_at DESC)
```

## clinical_notes

```sql
clinical_notes (
    id            uuid primary key,
    encounter_id  uuid not null references encounters(id),
    note_type     text not null,
    status        text not null,
    author_id     uuid references users(id),
    signed_by     uuid references users(id),
    signed_at     timestamptz,
    content       text,
    template_id   uuid references templates(id),
    version       integer not null default 1,
    provenance_id uuid references provenance(id),
    created_at    timestamptz not null,
    updated_at    timestamptz not null
)
```

## note_versions

```sql
note_versions (
    id               uuid primary key,
    note_id          uuid not null references clinical_notes(id),
    version          integer not null,
    content          text not null,
    created_by       uuid references users(id),
    created_at       timestamptz not null,
    amendment_reason text
)
```

Unique `(note_id, version)`.

---

# 23. Templates

```sql
templates (
    id              uuid primary key,
    organization_id uuid not null references organizations(id),
    name            text not null,
    template_type   text not null,
    specialty       text,
    version         integer not null,
    active          boolean not null default true,
    definition      jsonb not null,
    created_by      uuid references users(id),
    created_at      timestamptz not null,
    updated_at      timestamptz not null
)
```

Template JSONB is deliberate: template configuration is extensible UI/workflow data, unlike core clinical facts.

---

# 24. Devices and device imports

## devices

```sql
devices (
    id              uuid primary key,
    organization_id uuid not null references organizations(id),
    location_id     uuid references locations(id),
    device_type     text not null,
    manufacturer    text,
    model           text,
    serial_number   text,
    protocol        text,
    endpoint        text,
    status          text not null,
    last_seen_at    timestamptz,
    created_at      timestamptz not null,
    updated_at      timestamptz not null
)
```

## device_imports

```sql
device_imports (
    id               uuid primary key,
    device_id        uuid not null references devices(id),
    patient_id       uuid references patients(id),
    encounter_id     uuid references encounters(id),
    received_at      timestamptz not null,
    source_reference text,
    protocol         text,
    status            text not null,
    failure_reason   text,
    created_at       timestamptz not null
)
```

---

# 25. Provenance and audit

## provenance

```sql
provenance (
    id                uuid primary key,
    actor_type        text not null,
    actor_user_id     uuid references users(id),
    source_system     text,
    source_device_id  uuid references devices(id),
    source_identifier text,
    operation         text not null,
    recorded_at       timestamptz not null,
    reason            text,
    metadata          jsonb
)
```

## audit_events

```sql
audit_events (
    id                uuid primary key,
    organization_id   uuid not null references organizations(id),
    actor_user_id     uuid references users(id),
    action            text not null,
    entity_type       text not null,
    entity_id         uuid,
    patient_id        uuid references patients(id),
    occurred_at       timestamptz not null,
    outcome           text not null,
    source_ip         inet,
    device_identifier text,
    metadata          jsonb
)
```

Indexes:

```text
audit_events(organization_id, occurred_at DESC)
audit_events(patient_id, occurred_at DESC)
audit_events(actor_user_id, occurred_at DESC)
audit_events(entity_type, entity_id, occurred_at DESC)
```

Audit events should not be editable through ordinary APIs.

---

# 26. External identifiers

A simple polymorphic table is useful for integration references:

```sql
external_identifiers (
    id              uuid primary key,
    entity_type     text not null,
    entity_id       uuid not null,
    system          text not null,
    value           text not null,
    identifier_type text,
    use             text,
    created_at      timestamptz not null
)
```

Because `entity_id` can target multiple tables, this cannot have an ordinary foreign key. For high-integrity deployments, separate per-entity tables may be preferable.

---

# 27. Global constraints

Use a common laterality constraint where applicable:

```sql
CHECK (laterality IN ('OD', 'OS', 'OU', 'NA', 'UNK'))
```

Other constraints should be semantic, not blindly generic. For example, negative numbers may be clinically meaningful for refractive sphere values, so a universal `>= 0` constraint would be wrong.

Date constraints should include things such as:

```sql
CHECK (end_at IS NULL OR end_at >= start_at)
```

where semantically valid.

---

# 28. Referential integrity and delete policy

For core clinical records, default to restrictive deletion.

Do not cascade-delete a patient's clinical history.

Use state transitions such as:

```text
entered-in-error
amended
corrected
inactive
merged
```

where applicable.

Configuration data may have controlled cascade behavior; clinical data should not.

---

# 29. Versioning

A generic `updated_at` column is not versioning.

For signed clinical artifacts, use explicit versions:

```text
record
  ├── version 1
  ├── amendment → version 2
  └── amendment → version 3
```

Apply to:

- clinical notes
- signed prescriptions
- signed procedure records
- final reports
- consent artifacts
- generated clinical letters

Raw observations normally behave better as append-only facts than as overwritten rows.

---

# 30. Patient merge

Use a dedicated traceable merge record:

```sql
patient_merge_events (
    id                uuid primary key,
    source_patient_id uuid not null references patients(id),
    target_patient_id uuid not null references patients(id),
    reason            text not null,
    performed_by      uuid not null references users(id),
    performed_at      timestamptz not null
)
```

Avoid physically deleting the source patient unless data-governance requirements explicitly demand it.

---

# 31. Concurrency

Recommended for multi-user installations:

```text
row_version bigint
```

or an equivalent optimistic concurrency token.

If a user loads version 7 and someone else saves version 8, the first user's save must fail as stale rather than silently overwrite clinical data.

Signing should use an explicit transaction and stronger conflict checks.

---

# 32. Transaction boundaries

A signed prescription, for example, should be committed coherently:

```text
BEGIN
  create/update prescription
  create signature/provenance
  create audit event
COMMIT
```

Partial records must not appear as valid signed artifacts.

---

# 33. Imaging storage policy

Relational tables contain:

- patient/encounter association
- DICOM study/series/instance identifiers
- modality
- laterality
- timestamps
- device
- storage references
- checksums
- reports

A PACS/VNA/object store/file backend contains the large binary objects.

---

# 34. JSONB policy

JSONB is appropriate for:

- template definitions
- vendor-specific metadata
- integration payload snapshots
- extensibility data that has not yet earned first-class status

Do **not** use JSONB as the sole storage representation for:

- diagnoses
- prescriptions
- IOP
- visual acuity
- eye laterality
- procedures
- patient identity
- encounters

Those are core clinical concepts and must remain queryable relational data.

---

# 35. Longitudinal query indexes

High-value access paths:

```text
IOP:
(patient_id, laterality, measurement_at DESC)

Visual acuity:
(patient_id, laterality, tested_at DESC)

Refraction:
(patient_id, laterality, performed_at DESC)

Diagnosis:
(patient_id, assessed_at DESC)

Imaging:
(patient_id, laterality, started_at DESC)

Encounters:
(patient_id, started_at DESC)
```

Index after measuring actual workload; do not blindly index every column.

---

# 36. Patient search

Support indexed/normalized search for:

- MRN / identifier
- normalized name
- date of birth
- phone
- email

For larger deployments, PostgreSQL trigram/full-text capabilities can improve search latency.

Duplicate detection should happen before a second patient is created.

---

# 37. Migration strategy

Recommended migrations:

```text
0001_extensions
0002_organizations
0003_users_roles
0004_patients
0005_encounters
0006_terminology
0007_observations
0008_examination
0009_diagnostics
0010_medications
0011_prescriptions
0012_procedures
0013_documents
0014_devices
0015_provenance_audit
0016_indexes
0017_constraints
```

Migrations that destroy clinical data are not normal rollback mechanisms.

---

# 38. Initial relational slice

The first clinically useful release needs at least:

```text
organizations
locations
users
roles
permissions
role_permissions
user_roles

patients
patient_identifiers
patient_contacts
patient_addresses
emergency_contacts
allergies
allergy_reactions
medication_exposures
ocular_history_entries
family_history_entries

appointments
encounters
encounter_participants

terminology_concepts
concept_mappings

observations
visual_acuities
refractions
tonometry
slit_lamp_exams
slit_lamp_findings
fundus_exams
fundus_findings

problems
diagnoses
diagnosis_codes

diagnostic_orders
diagnostic_results

medication_concepts
medication_prescriptions
vision_prescriptions
vision_prescription_eyes

procedures
procedure_medications
implants
consents

care_plans
care_plan_items
follow_ups
referrals
tasks

documents
clinical_notes
note_versions
templates

devices
device_imports

provenance
audit_events
external_identifiers
```

That is sufficient for a substantial MVP without prematurely building billing, PACS or every device connector.

---

# 39. Example logical relationship map

```text
organizations
   ├── locations
   ├── users ── roles ── permissions
   ├── devices
   └── patients
        ├── identifiers / contacts / history
        └── encounters
             ├── observations / exams
             ├── diagnoses → problems
             ├── orders → results → reports
             ├── imaging studies → series → instances
             ├── prescriptions
             ├── procedures → implants / consent
             ├── documents / notes
             └── care plan / follow-up / tasks

all clinical objects → provenance
all sensitive actions → audit_events
```

---

# 40. Performance targets

Initial engineering targets, to be benchmarked against realistic clinic datasets:

- Patient search: interactive under normal clinic load
- Open patient: target <500 ms for local structured metadata, excluding large image transfer
- Open encounter: target <500 ms for ordinary structured data
- Save examination: transactional and immediate
- Longitudinal trend query: target <1 s for ordinary history depth
- Image loading: asynchronous and non-blocking

These are product engineering targets, not clinical or regulatory requirements.

---

# 41. Prohibited anti-patterns

Do not implement:

```text
patients.notes = entire chart in one JSON/text blob
```

Do not rely on:

```text
eye = 'right'
```

as uncontrolled free text.

Do not overwrite:

```text
previous IOP → new IOP
```

Do not represent a prescription solely as:

```text
'Timolol 0.5%, 1 drop OU...'
```

Do not treat images as anonymous attachment files without study/series/instance identity.

Do not make audit history ordinary mutable application data.

---

# 42. Schema acceptance criteria

The schema passes implementation review when it can demonstrate:

1. Years of patient history without destructive overwrites.
2. Correct OD/OS/OU representation.
3. Multiple IOP readings in one encounter.
4. Visual acuity values retaining clinically meaningful notation.
5. Refraction distinct from final optical prescription.
6. Diagnoses mapped to multiple coding systems.
7. Imaging linked to DICOM identifiers without storing pixels in core clinical tables.
8. Signed prescriptions with provenance and audit history.
9. Amended notes with recoverable prior versions.
10. Device imports carrying provenance.
11. Traceable patient merges.
12. No accidental cascade deletion of the chart.
13. Detectable concurrent edits.
14. FHIR/HL7/DICOM mappings without making those standards the internal domain model.

---

# 43. Final schema principle

The database exists to answer efficiently and reliably:

> **Show the complete, trustworthy, eye-specific clinical history of this patient, with measurements, diagnostics, prescriptions, procedures, provenance and longitudinal progression.**

It should be a durable clinical persistence layer, not a dumping ground for UI state, raw vendor payloads, or uncontrolled JSON.
