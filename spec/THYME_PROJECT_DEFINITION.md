# Thyme — Project Definition

**Codename:** Thyme  
**Product class:** Ophthalmology-focused desktop clinical information system  
**Status:** Research-derived project definition  
**Date:** 2026-08-15

> **Clinical/compliance note:** This document defines a software project. It is not medical or legal advice. Clinical workflows must be validated by practicing ophthalmologists. Regulatory requirements must be validated by qualified healthcare/privacy counsel in each deployment jurisdiction.

---

# 1. Project identity

## 1.1 Name

**Thyme**

The codename refers to the complete product described in this document.

## 1.2 Product

Thyme is a desktop-first ophthalmology clinical system designed to let an ophthalmology practice manage the patient journey from registration through examination, diagnostics, diagnosis, treatment, prescription, procedures and longitudinal follow-up.

## 1.3 Primary product principle

> **Ophthalmology is the domain model, not an optional skin on top of a generic EHR.**

---

# 2. Vision

Thyme should become a fast, trustworthy and extensible clinical workspace in which an ophthalmologist can understand a patient's current state and progression without manually reconstructing the history from disconnected notes, scanned documents and diagnostic-device software.

---

# 3. Mission

Provide clinicians and clinic staff with:

- fast patient retrieval
- complete longitudinal ophthalmic history
- efficient structured examination
- trustworthy prescriptions
- integrated diagnostics
- strong auditability
- robust data ownership
- interoperability
- dependable desktop operation

---

# 4. Problem statement

Existing generic medical systems often fail to model the distinctive characteristics of ophthalmology:

- laterality
- repeated eye-specific measurements
- optical/refraction data
- slit-lamp findings
- IOP
- visual fields
- OCT
- fundus imaging
- procedure/surgical workflows
- device-generated data
- longitudinal eye-specific progression

At the same time, EHR usability research indicates that documentation can consume a significant part of ophthalmology encounters.

Thyme therefore prioritizes:

**specialty correctness + workflow speed + longitudinal visibility + interoperability.**

---

# 5. Target users

## P0

### Ophthalmologist

Primary user.

Capabilities:

- review patient record
- examine
- document findings
- diagnose
- order tests
- interpret tests
- prescribe
- create treatment plans
- document procedures
- schedule follow-up
- sign records

### Ophthalmic technician

- register/pre-test
- capture visual acuity
- refraction
- IOP
- device measurements
- imaging
- visual fields
- route findings to physician

### Clinical nurse/staff

- medication/procedure workflows
- patient preparation
- communication
- selected documentation
- follow-up tasks

### Receptionist

- registration
- scheduling
- check-in
- demographics
- documents
- limited administrative access

### Practice administrator

- operations
- reporting
- staff management
- configuration
- permissions

---

# 6. Scope

## 6.1 Clinical scope

Thyme covers:

- ophthalmic history
- examination
- diagnosis
- measurements
- testing
- imaging
- medications
- prescriptions
- optical prescriptions
- procedures
- surgery planning/documentation
- follow-up
- clinical documents

## 6.2 Administrative scope

Initially:

- registration
- appointments
- encounter workflow
- referrals
- document administration
- basic operational reporting

Later:

- billing
- claims
- payment processing
- optical inventory
- patient communications

---

# 7. Non-goals

Initial Thyme is not:

- a hospital HIS
- a general EHR for every medical specialty
- a complete PACS replacement
- an autonomous diagnostic system
- a replacement for every laboratory/device vendor system
- a full accounting platform
- a universal patient portal
- an autonomous prescribing system

---

# 8. Core workflow

```text
Registration
    ↓
Appointment / Walk-in
    ↓
Check-in
    ↓
Pre-testing
    ↓
Encounter
    ↓
History
    ↓
Examination
    ↓
Orders / Diagnostics
    ↓
Results / Imaging
    ↓
Assessment
    ↓
Diagnosis
    ↓
Plan
    ├── Medication prescription
    ├── Optical prescription
    ├── Procedure
    ├── Referral
    └── Follow-up
    ↓
Clinical sign-off
    ↓
Documents / Billing / Administrative completion
```

The workflow must support branching.

A physician may order testing during the examination. A technician may perform routine testing before physician examination. A diagnostic result may arrive later.

---

# 9. Clinical modules

## 9.1 Patient

P0:

- identity
- identifiers
- demographics
- contact
- emergency contact
- history
- allergies
- medications
- documents
- encounters

## 9.2 Encounter

Fields:

- encounter ID
- patient
- provider
- location
- appointment
- start/end
- encounter type
- status
- reason
- supervising provider where relevant

## 9.3 History

Separate:

- chief complaint
- HPI
- ocular history
- systemic history
- surgical history
- family history
- social history
- medications
- allergies
- review of systems

## 9.4 Examination

First-class modules:

- visual acuity
- refraction
- pupils
- alignment/motility
- external exam
- slit lamp
- anterior chamber
- iris
- lens
- vitreous
- IOP
- gonioscopy
- fundus
- optic nerve
- retina
- macula
- visual fields
- subspecialty extensions

---

# 10. Laterality model

The following are semantic values:

```text
OD = right eye
OS = left eye
OU = both eyes
NA = not applicable
UNK = unknown
```

Laterality is a typed field, not free text.

Every measurement/finding/order/procedure/diagnosis that can be eye-specific must support laterality.

---

# 11. Measurement model

Every measurement should contain approximately:

```text
Measurement
├── id
├── patient_id
├── encounter_id
├── eye
├── concept
├── value
├── unit
├── reference/interpretation
├── method
├── device_id
├── performer_id
├── observed_at
├── recorded_at
├── status
└── provenance_id
```

The system must support repeated observations in the same encounter.

---

# 12. Refraction model

A refraction is not merely a string.

Support:

- sphere
- cylinder
- axis
- add
- prism
- base
- PD
- monocular PD
- corrected acuity
- refraction type
- method
- examiner
- date
- eye

Refraction types:

- wearing
- automated
- manifest
- cycloplegic
- final/prescription

---

# 13. Prescription model

## 13.1 Medication

```text
MedicationPrescription
├── medication
├── strength
├── dosage
├── route
├── frequency
├── duration
├── quantity
├── refills
├── eye/site
├── indication
├── instructions
├── prescriber
├── encounter
├── status
└── signature
```

The UI must make OD/OS/OU selection clear.

## 13.2 Vision prescription

Dedicated entity.

Do not derive a final optical prescription dynamically from the latest refraction.

A final prescription is its own signed clinical artifact.

---

# 14. Diagnosis model

```text
Diagnosis
├── concept
├── code(s)
├── laterality
├── status
├── severity
├── stage
├── onset/first-known date
├── encounter
├── supporting findings
├── assessment
└── provenance
```

Clinical concepts must remain independent from jurisdiction-specific billing codes.

---

# 15. Procedure model

```text
Procedure
├── procedure concept
├── eye
├── indication
├── performer
├── date/time
├── consent
├── anesthesia
├── medications
├── device/implant
├── findings
├── details
├── complications
├── outcome
├── instructions
└── follow-up
```

Procedure-specific templates should be layered on top.

---

# 16. Imaging architecture

Thyme must model imaging as a clinical study rather than as an arbitrary file attachment.

```text
ImagingStudy
├── patient
├── encounter
├── order
├── study_uid
├── modality
├── device
├── performed_at
├── laterality
├── series
│   └── instances
├── report
└── interpretation
```

DICOM should be supported at the architecture boundary.

DICOMweb compatibility should be considered for modern service integration.

---

# 17. Device integration

Create a vendor/device adapter interface.

Example:

```text
Device Adapter
├── discovery
├── patient/worklist synchronization
├── acquisition
├── import
├── metadata
├── status
└── error reporting
```

Potential devices:

- autorefractor
- keratometer
- tonometer
- visual field analyzer
- OCT
- fundus camera
- topographer
- biometer
- pachymeter
- slit-lamp camera
- ultrasound

The clinical core must not depend on one vendor.

---

# 18. Diagnostic workflow

```text
Order
  ↓
Scheduled
  ↓
Performed
  ↓
Imported
  ↓
Validated
  ↓
Interpreted
  ↓
Linked to encounter
  ↓
Referenced in assessment/plan
```

A study should remain traceable to its origin.

---

# 19. Longitudinal timeline

Patient timeline must be able to display:

- encounters
- diagnoses
- surgeries
- procedures
- medications
- prescriptions
- visual acuity
- IOP
- refraction
- visual fields
- OCT
- imaging
- referrals
- documents

Filtering must be available by:

- date
- eye
- diagnosis
- test
- procedure
- provider

---

# 20. Disease dashboards

P1.

### Glaucoma dashboard

At minimum:

- IOP trend
- visual acuity
- C/D ratio
- selected optic nerve metrics
- visual field history
- OCT/RNFL data where available
- medication history
- glaucoma procedures
- target/goal values if documented
- latest clinician interpretation

### Retina dashboard

At minimum:

- acuity
- imaging timeline
- disease-specific measurements
- injection history
- procedures
- current plan

### Cataract/refractive dashboard

- refraction history
- acuity
- lens findings
- surgical evaluation
- postoperative timeline

---

# 21. Notes

Support:

- templates
- custom fields
- free text
- structured findings
- smart phrases
- previous-value display
- explicit pull-forward

Never silently copy historical findings into a new encounter.

---

# 22. Note signing

Signed notes should become immutable versions.

Corrections should:

1. preserve prior content
2. create a new amendment/version
3. record author
4. record timestamp
5. optionally record reason

---

# 23. User and permissions architecture

```text
User
 ├── Roles
 ├── Location scope
 ├── Organization scope
 └── Permission overrides
```

Permissions include:

- view patient
- create encounter
- edit demographics
- create findings
- edit findings
- prescribe
- sign
- interpret diagnostics
- perform procedures
- export
- print
- administer users
- view audit records
- configure templates

---

# 24. Security model

P0:

- Argon2id/password hashing or stronger deployment-specific mechanism
- session management
- RBAC
- encryption at rest
- encrypted transport
- secure backups
- audit logging
- lockout/rate limiting
- secure secrets storage
- protected exports

P1:

- MFA
- device registration
- hardware-backed keys
- centralized security monitoring for larger deployments

---

# 25. Audit model

At minimum record:

- authentication
- patient record access
- creation
- update
- deletion attempt
- export
- print
- signing
- prescription
- diagnostic import
- device activity
- permission change
- configuration change

Audit events should be append-only from the application's perspective and protected from ordinary clinical users.

---

# 26. Provenance model

Every clinically meaningful object should be able to answer:

> Who produced this information, when, how, and from what source?

Examples:

- clinician entered manually
- technician entered
- imported from DICOM device
- imported via FHIR
- copied forward
- generated by template
- suggested by AI
- amended by physician

---

# 27. Documents

Core supported formats:

- PDF
- images
- scanned documents
- clinical reports
- prescriptions
- referrals
- generated letters

Metadata:

- document type
- patient
- encounter
- author
- source
- date
- status
- confidentiality level
- storage location
- checksum/hash

---

# 28. Search

P0:

- patient
- identifier
- phone
- date of birth
- diagnosis
- encounter
- provider
- procedure

P1:

- medication
- imaging
- documents
- eye/laterality
- notes/full text

---

# 29. Scheduling

P0:

- calendar
- appointment type
- provider
- location
- status
- check-in
- cancellation
- rescheduling

P1:

- worklists
- device scheduling
- resource allocation
- reminders
- waitlist

---

# 30. Tasks

P1.

Tasks should link to:

- patient
- encounter
- staff member
- due date
- procedure
- test
- referral
- follow-up

Examples:

- review OCT
- call patient
- schedule surgery
- arrange referral
- repeat test

---

# 31. Interoperability

## FHIR

Strategic mapping:

| Thyme | FHIR candidate |
|---|---|
| Patient | Patient |
| Encounter | Encounter |
| Measurement | Observation |
| Diagnostic report | DiagnosticReport |
| Medication prescription | MedicationRequest |
| Allergy | AllergyIntolerance |
| Procedure | Procedure |
| Imaging study | ImagingStudy |
| Document | DocumentReference |
| Provenance | Provenance |
| Audit | AuditEvent |
| Optical prescription | VisionPrescription |

FHIR is an exchange model, not a requirement that Thyme's internal database become a literal FHIR database.

## HL7 v2

Support through an integration layer where required.

## DICOM

Support at the imaging boundary.

## IHE Eye Care

Track relevant Eye Care profiles as implementation requirements.

---

# 32. Architecture

Recommended logical architecture:

```text
┌───────────────────────────────────────────┐
│              Thyme Desktop UI             │
└──────────────────────┬────────────────────┘
                       │
┌──────────────────────▼────────────────────┐
│            Clinical Application            │
│  encounters / exams / prescriptions       │
│  diagnoses / workflows / permissions      │
└───────┬──────────────┬───────────────┬────┘
        │              │               │
        ▼              ▼               ▼
 Clinical DB      Integration Hub   Document/Media
        │              │               │
        │         FHIR / HL7 /       Imaging
        │         DICOM adapters     storage
        │
        ▼
 Audit / Provenance / Search
        │
        ▼
 Backup / Recovery
```

---

# 33. Desktop requirements

The application must:

- start reliably
- operate without constant internet dependency
- support Windows as first target
- print through native OS mechanisms
- access permitted local devices/files
- support scanners/printers where required
- handle large image viewing without blocking clinical interactions
- update safely
- recover from crashes
- preserve unsaved work where feasible

---

# 34. Local-first strategy

The desktop client should retain enough local functionality to:

- view current locally cached patient information
- complete configured clinical workflows
- create records
- queue integration operations

Synchronization must be explicit and conflict-aware.

Do not silently overwrite concurrent clinical changes.

---

# 35. Multi-user strategy

For LAN deployments:

- centralized database
- authoritative server-side audit
- transactional writes
- optimistic/constrained concurrency
- user sessions
- device coordination
- centralized backup

A single-user local deployment and a multi-user clinic deployment should share the same domain model.

---

# 36. Backup

P0:

- automatic scheduled backups
- encrypted backup
- backup verification
- restore procedure

P1:

- off-device backups
- versioned backups
- immutable/offline backup
- periodic restore test
- recovery-point/recovery-time policies

A backup that has never been restored is not evidence of recoverability.

---

# 37. Data migration

Thyme must eventually support:

- CSV import
- structured clinical import
- document import
- imaging import
- patient identity matching
- duplicate resolution
- source provenance

A migration should create a traceable mapping from source fields to Thyme fields.

---

# 38. UX principles

1. **Patient first**
2. **Eye first when clinically relevant**
3. **Show prior values immediately**
4. **Do not make the physician hunt for historical information**
5. **Use structured input where it improves correctness**
6. **Allow narrative where structure would be artificial**
7. **Prefer keyboard shortcuts**
8. **Minimize unnecessary clicks**
9. **Never hide clinical status behind decorative UI**
10. **Make automation reviewable**

---

# 39. Core screens

P0:

- Login
- Dashboard
- Patient search
- Patient summary
- Patient timeline
- Appointment calendar
- Encounter workspace
- Examination workspace
- Diagnosis/plan
- Prescription
- Documents
- User profile/settings

P1:

- Imaging viewer
- Visual-field viewer
- Disease dashboards
- Procedure planner
- Device manager
- Referral manager
- Task board
- Audit viewer

---

# 40. Encounter workspace concept

Recommended layout:

```text
┌─────────────────────────────────────────────────────────┐
│ Patient / alerts / allergies / current status           │
├────────────┬──────────────────────────────┬─────────────┤
│ History    │ Current Examination          │ Previous    │
│            │                              │ Trends      │
│            │ OD / OS / OU                 │ IOP         │
│            │                              │ VA          │
│            │                              │ Refraction  │
├────────────┴──────────────────────────────┴─────────────┤
│ Tests / Images / Orders / Results                         │
├─────────────────────────────────────────────────────────┤
│ Assessment / Diagnosis / Plan / Rx / Follow-up            │
└─────────────────────────────────────────────────────────┘
```

This is a conceptual layout, not a final visual design.

---

# 41. MVP definition

A release qualifies as Thyme MVP only when an ophthalmologist can:

1. find/create a patient
2. review history
3. start an encounter
4. record a full configurable basic ophthalmic examination
5. record OD/OS/OU data correctly
6. enter diagnosis
7. create medication prescription
8. create optical prescription
9. print/export the result
10. review the patient's previous values
11. sign the encounter
12. later audit who changed/accessed it
13. back up and restore the database

---

# 42. Phase 2

- DICOM
- imaging viewer
- OCT
- visual fields
- device adapters
- scheduling enhancements
- procedures
- referrals
- FHIR import/export
- HL7 integration
- LAN deployment
- disease dashboards

---

# 43. Phase 3

- PACS/VNA
- advanced device ecosystem
- billing integrations
- optical POS
- communications
- patient portal
- analytics
- multi-site deployment
- AI assistance

---

# 44. AI policy

AI features must initially be **assistive, never silently autonomous**.

Permitted examples:

- note drafting
- summarization
- coding suggestion
- finding extraction
- document classification
- trend summarization

Every AI-generated result must indicate:

- generated/suggested status
- model/system provenance
- time
- user who reviewed/accepted it

No AI result may silently become a clinical fact.

---

# 45. Localization

Architecture must support:

- language packs
- date formats
- decimal separators
- measurement units
- terminology
- prescription formats
- document templates
- currency
- country/jurisdiction configuration

Clinical meaning should not depend on the UI language.

---

# 46. Algeria deployment requirements

For Algerian deployments:

**Must validate before production:**

- current Law 18-07 obligations after Law 25-11 amendments
- implementing regulations
- national authority requirements
- health-record requirements
- electronic prescription rules
- electronic signature rules
- data transfer rules
- retention/deletion requirements
- hosting/storage requirements
- applicable Ministry of Health requirements

These are explicit compliance workstreams, not assumptions.

---

# 47. Testing strategy

## Domain tests

Test:

- laterality
- measurement units
- prescription arithmetic/validation
- diagnosis linkage
- encounter lifecycle
- versioning
- audit events

## Clinical workflow tests

Scenario-driven:

- normal examination
- glaucoma follow-up
- cataract evaluation
- postoperative visit
- retina follow-up
- pediatric visit
- urgent eye complaint

## Integration tests

- DICOM
- FHIR
- HL7
- printer
- scanner
- device adapters

## Security tests

- access control
- privilege escalation
- session handling
- export permissions
- audit integrity
- backup encryption

---

# 48. Definition of done

A subsystem is not complete because its screen exists.

It is done when:

- domain model is defined
- validation exists
- permission model exists
- audit/provenance behavior exists
- persistence works
- import/export requirements are handled
- error states are handled
- backup/recovery implications are addressed
- automated tests exist
- clinician workflow has been reviewed
- documentation exists

---

# 49. Pre-implementation decisions

Before serious implementation, the team must decide:

1. Desktop UI framework
2. Language
3. Local database
4. Server database
5. Image/object storage
6. IPC/integration mechanism
7. FHIR library strategy
8. DICOM library strategy
9. HL7 interface strategy
10. authentication model
11. encryption/key management
12. backup architecture
13. update distribution
14. logging/observability
15. plugin/device adapter system
16. terminology architecture
17. document-generation system

Do not choose these merely by developer familiarity.

Score them against the clinical requirements.

---

# 50. Recommended implementation order

## Milestone 1 — Domain foundation

- Patient
- Identity
- Eye/laterality
- Encounter
- User/role
- Audit
- Provenance

## Milestone 2 — Core clinical workflow

- History
- Examination
- Measurements
- Diagnosis
- Plan

## Milestone 3 — Prescribing

- Medication
- Optical prescription
- Printing
- Signing

## Milestone 4 — Longitudinal record

- Timeline
- trends
- previous-value comparison
- disease views

## Milestone 5 — Documents

- upload
- generated documents
- referral workflow

## Milestone 6 — Integration

- device adapter framework
- DICOM
- FHIR
- HL7

## Milestone 7 — Procedures

- office procedures
- surgery planning
- postop workflows

## Milestone 8 — Administration

- scheduling
- tasks
- referrals
- billing integrations

---

# 51. Product success metrics

Measure:

### Clinical efficiency

- time to open patient
- time to start encounter
- time to document
- time to prescribe
- time to find previous IOP/VA/refraction
- clicks per common workflow

### Data quality

- incomplete required fields
- laterality errors
- duplicate patients
- amended notes
- device import failures

### Reliability

- crash rate
- backup success rate
- restore success rate
- device integration failure rate

### Security

- audit completeness
- unauthorized access attempts
- privileged-action frequency

---

# 52. Major risks

| Risk | Severity | Mitigation |
|---|---:|---|
| Wrong clinical data model | Critical | Ophthalmologist-led validation |
| Laterality errors | Critical | Typed model + UI safeguards |
| Data loss | Critical | Automated tested backups |
| Security compromise | Critical | Least privilege + encryption + audit |
| Poor workflow performance | High | Time-motion usability testing |
| Device integration fragmentation | High | Adapter architecture |
| Regulatory mismatch | High | Local legal/compliance review |
| Migration failure | High | Provenance-aware importer |
| Vendor lock-in | Medium/High | Standards-first model |
| AI hallucination | High | Human review + explicit provenance |

---

# 53. Architectural principles

1. **Clinical domain first.**
2. **Laterality first-class.**
3. **Measurements are data, not prose.**
4. **Images are studies, not attachments.**
5. **Prescriptions are clinical artifacts.**
6. **Historical data is never silently destroyed.**
7. **Provenance is part of the data model.**
8. **Auditability is part of the core platform.**
9. **Standards at boundaries; purpose-built domain model internally.**
10. **Offline capability is a reliability feature.**
11. **Automation must remain reviewable.**
12. **Templates accelerate documentation; they do not replace clinical judgment.**

---

# 54. Explicit research-backed requirements vs assumptions

## Established/research-backed

- Ophthalmology has specialty-specific EHR requirements.
- Comprehensive eye examination has a substantial structured component.
- Eye-care workflows include integration with diagnostic devices.
- DICOM and HL7 are relevant to eye-care interoperability.
- DICOMweb provides standardized query/retrieve/store web services.
- Ophthalmology documentation burden is material.
- Longitudinal data and imaging comparison are high-value ophthalmology workflows.
- Allergy, medication, diagnostics, documents and provenance have established interoperability concepts in FHIR.
- Healthcare systems require strong safeguards for confidentiality, integrity and availability.

## Recommended design conclusions

- First-class laterality model
- Study-centric imaging
- structured measurement storage
- separate optical prescription entity
- explicit audit/provenance
- offline-capable desktop architecture
- modular integration layer

## Assumptions requiring validation

- Exact mandatory fields for target clinics
- Exact subspecialty scope of MVP
- Preferred Algerian coding/terminology systems
- Electronic prescription requirements
- local device vendors
- local retention/hosting requirements
- clinic billing requirements
- acceptable offline behavior

---

# 55. Validation owners

| Question | Required validator |
|---|---|
| Examination completeness | Ophthalmologist |
| Subspecialty workflows | Relevant subspecialist |
| Prescription workflow | Ophthalmologist + local pharmacist/regulatory expert |
| Surgical documentation | Ophthalmic surgeon |
| Device protocols | Device vendor + integration engineer |
| DICOM workflows | Imaging interoperability specialist |
| FHIR/HL7 design | Interoperability specialist |
| Algeria privacy | Algerian privacy/legal specialist |
| EU deployment | EU GDPR/privacy specialist |
| US deployment | HIPAA/compliance specialist |
| Security architecture | Security engineer |
| UX | Ophthalmologists + technicians + reception staff |

---

# 56. Final product definition

**Thyme is a desktop-first, ophthalmology-native clinical information system whose primary objects are the patient, encounter, eye, clinical measurement, diagnostic study, diagnosis, treatment, procedure and longitudinal history.**

It must make the following possible:

> **A clinician opens a patient and immediately understands what is happening in each eye, what changed since the prior visit, what evidence supports the current assessment, what was prescribed, and what needs to happen next.**

The system must do this:

- quickly
- safely
- audibly
- offline when required
- interoperably
- without destroying historical provenance

That is the defining product promise of Thyme.

---

# 57. Immediate next artifacts

The next implementation documents should be:

1. `THYME_DOMAIN_MODEL.md`
2. `THYME_DATABASE_SCHEMA.md`
3. `THYME_CLINICAL_WORKFLOWS.md`
4. `THYME_UI_SPEC.md`
5. `THYME_INTEROPERABILITY.md`
6. `THYME_SECURITY_MODEL.md`
7. `THYME_DEVICE_INTEGRATION.md`
8. `THYME_TEST_PLAN.md`
9. `THYME_MVP_ROADMAP.md`

Only after those are reviewed by domain experts should implementation architecture be frozen.
