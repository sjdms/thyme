# THYME_CLINICAL_WORKFLOWS.md

# Thyme Clinical Workflows

**Project:** Thyme  
**Artifact:** Clinical Workflow Specification  
**Version:** 0.1  
**Status:** Implementation foundation  
**Date:** 2026-08-16

> This document defines operational workflows for Thyme. It is intentionally workflow-first rather than screen-first. Clinical behavior, required fields, order of operations, and safety constraints must be validated with practicing ophthalmologists, technicians, nurses and local compliance specialists before clinical deployment.

---

# 1. Purpose

Thyme exists to support the real work of an ophthalmology practice.

The application should not force clinicians to think in terms of database objects such as `Encounter`, `Observation`, or `Document` while they are seeing patients.

Instead, the software should model the real sequence:

```text
Patient arrives
    ↓
Identity confirmed
    ↓
Pre-visit / pre-testing
    ↓
Ophthalmic encounter
    ↓
History
    ↓
Examination
    ↓
Testing
    ↓
Clinical reasoning
    ↓
Diagnosis
    ↓
Plan / treatment
    ↓
Prescription / procedure / referral
    ↓
Follow-up
    ↓
Clinical sign-off
    ↓
Longitudinal record
```

The workflow engine must permit branches, returns, interruptions, missing data, and asynchronous test results.

---

# 2. Workflow design principles

## WF-001 — Clinical workflow over software workflow

The system should follow the clinic's real workflow rather than forcing the clinic to adapt to database states.

## WF-002 — Human confirmation for consequential actions

The following require an explicit user action before completion:

- final diagnosis
- medication prescription
- optical prescription
- procedure completion
- clinical note signing
- major amendments
- patient identity merge
- destructive administrative actions

## WF-003 — Never silently invent clinical facts

Automation may suggest, normalize, or import.

It must not silently convert a suggestion into confirmed clinical truth.

## WF-004 — Every eye-specific action declares laterality

The user must be able to distinguish:

- OD
- OS
- OU
- not applicable
- unknown

## WF-005 — Prior information is visible but not silently copied

Historical data should be shown beside current input.

Copy-forward must be explicit and reviewable.

## WF-006 — Missing is not the same as normal

An unanswered field must not become:

- normal
- negative
- zero
- absent

unless the workflow explicitly defines that semantics.

## WF-007 — Testing may be asynchronous

Orders can exist before results.

Results can arrive after the encounter.

A result may be reviewed later without reopening the original encounter as if the test happened in real time.

## WF-008 — Exceptions are first-class

Every major workflow must define:

- canceled
- failed
- incomplete
- rejected
- entered-in-error
- amended
- deferred

states where applicable.

---

# 3. Actors

## 3.1 Receptionist

Typical responsibilities:

- patient search
- patient registration
- demographics
- appointment creation
- check-in
- document collection
- administrative communication

Should not have unrestricted clinical access.

---

## 3.2 Technician

Typical responsibilities:

- pre-testing
- visual acuity
- autorefraction
- tonometry
- imaging
- visual fields
- device acquisition
- preliminary data capture

Technician data should remain distinguishable from physician-authored interpretations.

---

## 3.3 Nurse / clinical staff

Typical responsibilities may include:

- medication reconciliation
- pre-procedure preparation
- patient instructions
- procedure assistance
- follow-up tasks
- selected clinical documentation

Exact permissions are deployment-specific.

---

## 3.4 Ophthalmologist

Primary clinical decision-maker.

Typical responsibilities:

- history review
- examination
- interpretation
- diagnosis
- treatment planning
- prescribing
- procedures
- clinical sign-off

---

## 3.5 Practice manager

Typical responsibilities:

- schedules
- reports
- staffing
- workflow configuration
- operational review
- administrative settings

---

## 3.6 System/integration services

Examples:

- device adapters
- DICOM receiver
- FHIR interface
- HL7 interface
- backup service
- notification service

Automated services must have explicit provenance.

---

# 4. Global patient state model

A patient should not be modeled as a simple lifecycle such as "active/inactive."

Recommended conceptual states:

```text
REGISTERED
ACTIVE
INACTIVE
DECEASED
MERGED
ARCHIVED
```

Clinical data can remain accessible according to policy even when the patient is no longer actively receiving care.

---

# 5. Patient registration workflow

## Goal

Create or identify the correct patient before clinical information is attached.

## Primary flow

```text
Search existing patient
       │
       ├── Exact candidate found → Review identity
       │
       ├── Possible duplicates → Resolve before creation
       │
       └── No plausible match → Create patient
```

## Step-by-step

1. Receptionist searches by name, identifier, DOB, phone or other configured fields.
2. System displays likely candidates.
3. User verifies at least two independent identifiers according to clinic policy.
4. Existing patient:
   - open existing chart
   - do not create duplicate.
5. New patient:
   - enter required demographics
   - create internal patient ID
   - optionally assign external identifiers.
6. Patient becomes available for scheduling/check-in.

## Safety gates

### Duplicate warning

Before creation, show:

- exact matches
- fuzzy matches
- DOB
- phone
- existing identifiers

Do not use fuzzy similarity as proof of identity.

### Identity merge

A merge requires:

- appropriate permission
- explicit selection of source and target
- review of differences
- confirmation
- audit event
- recoverable merge history

---

# 6. Appointment workflow

## Appointment states

```text
PROPOSED
BOOKED
CONFIRMED
ARRIVED
CHECKED_IN
IN_PROGRESS
COMPLETED
CANCELED
NO_SHOW
RESCHEDULED
```

## Booking

Inputs:

- patient
- appointment type
- provider
- location
- date/time
- reason
- optional prerequisites

The system should surface:

- provider availability
- room/resource conflicts
- expected appointment duration
- required device/resources
- prerequisite testing

---

# 7. Check-in workflow

```text
Appointment
    ↓
Identity verification
    ↓
Demographic confirmation
    ↓
Administrative updates
    ↓
Arrival status
    ↓
Clinical queue
```

Check-in should not create a clinical encounter automatically unless the configured workflow explicitly does so.

---

# 8. Pre-testing workflow

## Goal

Capture technician/device data before physician evaluation where the clinic uses that model.

Typical sequence:

```text
Patient ready
   ↓
Verify correct patient
   ↓
Visual acuity
   ↓
Refraction/autorefraction
   ↓
IOP
   ↓
Other configured tests
   ↓
Imaging / fields if scheduled
   ↓
Technician review
   ↓
Submit to physician worklist
```

The actual sequence is configurable.

---

# 9. Device verification safety gate

Before acquiring device data:

1. Technician selects patient.
2. Device/worklist association is verified.
3. Patient identity is visible on device/integration screen where supported.
4. Acquisition occurs.
5. Result is linked to patient and, where appropriate, encounter/order.
6. Device metadata is stored.

Never depend solely on a device filename to determine patient identity.

---

# 10. Visual acuity workflow

## Input

For each eye:

- laterality
- distance/near
- correction
- measurement system
- value
- pinhole
- method

## Interaction

Preferred flow:

```text
OD → value
OS → value
Correction → select
Pinhole → optional
```

For OU measurements, the UI must make it clear that the measurement is bilateral.

## Validation

- Require eye for eye-specific testing.
- Preserve original displayed notation.
- Do not silently convert clinically distinct representations.
- Do not interpret a missing measurement as normal.

---

# 11. Refraction workflow

## Typical sequence

```text
Current prescription reviewed
       ↓
Autorefraction imported/entered
       ↓
Manifest/cycloplegic refraction
       ↓
Best corrected acuity
       ↓
Physician selects/refines final correction
       ↓
Final optical prescription generated
```

## Important rule

The current spectacle/contact prescription, measured refraction, and final prescription are three different concepts.

Never overwrite one with another.

---

# 12. IOP / tonometry workflow

## Typical sequence

```text
Select eye
   ↓
Select method/device
   ↓
Measure
   ↓
Optional repeat
   ↓
Review values
   ↓
Select/mark clinically relevant reading
```

Multiple IOP values may coexist.

The system must not enforce "one IOP per eye per encounter."

---

# 13. Generic examination workflow

The examination should support:

```text
History
   ↓
General/visual function
   ↓
Pupils / motility
   ↓
External
   ↓
Anterior segment
   ↓
IOP
   ↓
Posterior segment
   ↓
Additional testing
   ↓
Assessment
```

A physician can jump directly to a clinically relevant section.

The UI must not require every section to be completed in one rigid sequence.

---

# 14. Examination templates

Templates may define:

- sections
- default fields
- options
- ordering
- frequently used findings
- specialty-specific fields
- required fields

Templates must not redefine core clinical semantics.

A template may say:

> "Show glaucoma fields."

It must not create a separate incompatible meaning for IOP.

---

# 15. "Normal" workflow

This is a major safety issue.

A user selecting:

> "Normal anterior segment"

should produce an explicit structured assertion representing normality.

An untouched section means:

> no data entered

Those states are different.

---

# 16. Pull-forward workflow

Historical values may be displayed.

Example:

```text
Previous IOP:
OD 16 mmHg
OS 18 mmHg

Current:
OD [       ]
OS [       ]
```

User can explicitly:

```text
[Use previous]
```

The resulting value becomes a new authored observation with provenance indicating it was copied forward, not the same database row reused.

---

# 17. Diagnostic test ordering workflow

```text
Physician identifies need
        ↓
Create order
        ↓
Select test
        ↓
Select eye/site if relevant
        ↓
Set urgency/priority
        ↓
Specify indication
        ↓
Order becomes work item
```

Orders should be cancellable where appropriate.

---

# 18. Test execution workflow

```text
ORDERED
   ↓
SCHEDULED
   ↓
IN_PROGRESS
   ↓
PERFORMED
   ↓
RESULT_PENDING
   ↓
RESULTED
   ↓
REVIEWED
```

Possible exception states:

```text
CANCELED
FAILED
PATIENT_UNABLE
DEVICE_FAILURE
REJECTED
ENTERED_IN_ERROR
```

---

# 19. Visual field workflow

## Technician

- verify patient
- verify eye
- configure test
- perform
- record reliability
- import data/report

## Physician

- review raw/graphical study
- review reliability
- interpret
- correlate with clinical findings
- document impression when necessary

The raw study and physician interpretation should remain distinct.

---

# 20. OCT workflow

```text
Order
  ↓
Acquisition
  ↓
Import
  ↓
Technical validation
  ↓
Physician review
  ↓
Interpretation
  ↓
Longitudinal comparison
```

Longitudinal comparison should allow current vs historical studies without replacing original studies.

---

# 21. Imaging workflow

## Required clinical linkage

An image/study should be associated with:

- patient
- laterality where applicable
- modality
- acquisition time
- source device
- order/encounter when known

If the external system supplies a DICOM Study Instance UID, preserve it.

---

# 22. Clinical assessment workflow

After examination/testing:

```text
Findings
   ↓
Review relevant history
   ↓
Interpret results
   ↓
Assess conditions
   ↓
Create/update diagnoses
   ↓
Define plan
```

Do not auto-create diagnoses merely because a test has an abnormal flag.

---

# 23. Diagnosis workflow

For each diagnosis:

1. Select or search concept.
2. Select eye if applicable.
3. Set status.
4. Set severity/stage where relevant.
5. Associate supporting findings/tests if desired.
6. Add assessment narrative.
7. Save.

For chronic problems, the system should prefer updating the longitudinal problem while preserving the encounter-specific assessment.

---

# 24. Medication reconciliation workflow

Before prescribing:

```text
Current medications reviewed
       ↓
Changes identified
       ↓
Allergies reviewed
       ↓
New medication selected
       ↓
Instructions entered
       ↓
Prescription preview
       ↓
Clinician signs
```

A new prescription does not automatically mean the old medication exposure disappears.

Medication status must be handled explicitly.

---

# 25. Medication prescription workflow

Required interaction sequence:

```text
Medication
   ↓
Strength/form
   ↓
Dose
   ↓
Route
   ↓
Frequency
   ↓
Eye/site
   ↓
Duration/refills
   ↓
Patient instructions
   ↓
Preview
   ↓
Sign
```

The preview must be readable as the patient will receive it.

---

# 26. Optical prescription workflow

```text
Select final refraction
       ↓
Review OD
       ↓
Review OS
       ↓
Add near/intermediate information
       ↓
PD / monocular PD
       ↓
Lens information if applicable
       ↓
Prescription preview
       ↓
Sign
       ↓
Print/export
```

Selecting a final refraction should copy values into a new prescription object, not mutate the original refraction.

---

# 27. Prescription safety gates

Before signing, validate:

- patient
- prescriber
- medication/prescription
- eye/site where applicable
- dose/power
- route/type
- frequency
- quantity/duration
- required instructions
- allergies/alerts available to prescriber
- legal/signature requirements for the deployment

The application should warn, not pretend to replace clinical judgment.

---

# 28. Procedure workflow

## Common path

```text
Indication
   ↓
Procedure planned
   ↓
Consent
   ↓
Pre-procedure verification
   ↓
Procedure performed
   ↓
Findings/outcome
   ↓
Complications
   ↓
Post-procedure instructions
   ↓
Follow-up
   ↓
Sign
```

---

# 29. Procedure-site verification

For an eye-specific procedure:

1. Patient identity
2. Procedure
3. Correct eye
4. Consent
5. Performer
6. Time

The application should prominently display laterality at the procedure confirmation stage.

---

# 30. Surgical workflow

A more complete surgery workflow may be:

```text
Diagnosis
   ↓
Surgical decision
   ↓
Candidate evaluation
   ↓
Measurements/tests
   ↓
Planning
   ↓
Consent
   ↓
Scheduling
   ↓
Pre-op clearance/process
   ↓
Procedure
   ↓
Implant/device recording
   ↓
Immediate post-op plan
   ↓
Follow-up encounters
```

For cataract/IOL workflows, implant details must be linked to the operated eye and procedure.

---

# 31. Postoperative workflow

At each postoperative visit:

- link to original procedure
- show operative details
- show implanted devices where relevant
- show prior postoperative findings
- show current findings
- document complications/adverse events
- update medications
- issue instructions
- set next follow-up

Do not require the clinician to reconstruct the surgery from an old note.

---

# 32. Referral workflow

```text
Clinical need identified
   ↓
Destination selected
   ↓
Reason/diagnosis
   ↓
Relevant records attached/linked
   ↓
Referral generated
   ↓
Sent/tracked
   ↓
Response received
   ↓
Response reviewed
   ↓
Record updated
```

Referral status should be independently tracked from the encounter.

---

# 33. Follow-up workflow

Follow-up may be:

- interval based
- date based
- condition based
- procedure based
- PRN

When a clinician says:

> return in 3 months

the system can recommend a target date but must permit clinician adjustment.

---

# 34. Clinical note workflow

## Draft

Clinician may:

- type
- dictate
- select template
- insert structured fields
- pull forward
- review imported results

## Review

Before sign:

- verify patient
- verify current encounter
- verify diagnosis/plan
- review key imported data
- inspect unreviewed AI suggestions if used

## Sign

Creates:

- signed state
- timestamp
- signer
- version
- audit event

---

# 35. Note amendment workflow

Signed note:

```text
Signed
  ↓
Amendment requested
  ↓
Original remains intact
  ↓
New amendment/version
  ↓
Reason
  ↓
Signer
```

Never silently rewrite history.

---

# 36. Encounter completion workflow

An encounter should be completable only when mandatory workflow requirements for that encounter type are satisfied.

Possible:

```text
OPEN
DOCUMENTING
AWAITING_RESULTS
READY_FOR_SIGN
SIGNED
AMENDED
CANCELED
ENTERED_IN_ERROR
```

Do not make every field universally mandatory.

Required fields should be encounter-type configurable.

---

# 37. Result-review workflow after encounter

Example:

```text
Encounter signed
     ↓
OCT result arrives next day
     ↓
Result added to patient record
     ↓
Physician review task created
     ↓
Physician reviews
     ↓
Interpretation documented
     ↓
Patient communication/follow-up if required
```

Do not reopen the signed encounter as an ordinary editable form simply because a result arrived later.

---

# 38. Inbox / worklist workflow

The physician should have a unified worklist for:

- unsigned notes
- diagnostic results to review
- imaging
- abnormal/priority results
- refill requests
- referrals
- patient messages
- follow-up tasks

Every item should have:

- patient
- type
- priority
- due date
- state
- assignee

---

# 39. Urgent-result workflow

A configurable critical/urgent result can produce:

```text
Result received
   ↓
Priority classification
   ↓
Urgent worklist item
   ↓
Acknowledgement
   ↓
Clinical action
   ↓
Communication/documentation
   ↓
Closed
```

Do not make automatic clinical severity classification the sole determinant of urgency.

---

# 40. Patient communication workflow

Communication may include:

- phone note
- portal message
- SMS/email through an approved integration
- letter
- instruction sheet

Every clinically relevant communication should be linkable to the patient and, when appropriate, encounter/problem/result.

---

# 41. Administrative-document workflow

For:

- referral letters
- prescriptions
- medical certificates
- patient instructions

the document should be generated from structured data and a versioned template.

Generated document metadata should include:

- source record/version
- author
- generation timestamp
- template version

---

# 42. Error correction workflow

Examples:

### Wrong value entered

Create corrected value with a traceable amendment/history.

### Wrong patient/device import

Stop downstream processing, quarantine imported record, resolve identity, then relink safely.

### Wrong eye selected

Require correction workflow with visible audit.

### Wrong prescription

Do not silently edit an already-issued/signed prescription; create a corrected/cancelled/reissued artifact according to jurisdiction and policy.

---

# 43. Entered-in-error workflow

Use only when appropriate.

The record remains discoverable to authorized users with:

- entered-in-error status
- reason
- actor
- timestamp
- audit trail

It should not behave like physical deletion.

---

# 44. Downtime workflow

Thyme must anticipate:

- network outage
- server outage
- device outage
- database unavailable
- printer unavailable
- DICOM/PACS unavailable

The deployment must have a documented downtime mode.

Possible behavior:

```text
Online
  ↓
Connectivity degraded
  ↓
Local safe queue / cached records
  ↓
Continue permitted work
  ↓
Connectivity restored
  ↓
Reconcile/synchronize
```

Clinical write operations requiring server authority should be blocked rather than falsely reported as saved when the system cannot guarantee persistence.

---

# 45. Conflict-resolution workflow

Two users may edit related information.

The system must show:

```text
Your version
Other version
Current authoritative version
```

For clinical data, never use blind last-write-wins where that could destroy a meaningful change.

---

# 46. Device failure workflow

```text
Acquisition started
   ↓
Device failure
   ↓
Record failed status
   ↓
Reason
   ↓
Retry / alternate device / manual measurement
```

A failed device result must not look like a normal negative clinical result.

---

# 47. Missing-patient-data workflow

When key data is unavailable:

```text
UNKNOWN
NOT PERFORMED
PATIENT DECLINED
NOT APPLICABLE
UNABLE TO OBTAIN
PENDING
```

Do not collapse all of these into blank/NULL.

---

# 48. Patient refuses test

For relevant tests:

- record test not performed
- reason = patient declined
- actor
- date/time
- optional note

The record should distinguish refusal from missing documentation.

---

# 49. Patient transfer workflow

If a patient moves between locations:

- same patient identity
- new encounter/location
- no duplicate chart
- preserve organization/location provenance

Multi-site deployments need clear ownership boundaries.

---

# 50. Patient merge workflow

Before merge:

- verify identities
- compare demographics
- compare identifiers
- review conflicting contacts
- identify clinical histories
- confirm source/target

After merge:

- all affected records remain attributable
- merge event recorded
- old identity remains traceable according to policy

---

# 51. Longitudinal comparison workflow

At patient open:

```text
Current encounter
      ↓
Relevant historical context
      ↓
Current vs previous
      ↓
Current vs baseline
      ↓
Trend
```

Common comparison dimensions:

- OD/OS
- visual acuity
- IOP
- refraction
- OCT
- visual field
- diagnosis status
- medications
- procedures

The system should select clinically relevant trends based on active problems, not show every metric indiscriminately.

---

# 52. Disease-specific workflow: glaucoma

Recommended sequence:

```text
History
 ↓
IOP
 ↓
Gonioscopy where relevant
 ↓
Optic nerve
 ↓
OCT
 ↓
Visual field
 ↓
Medication review
 ↓
Risk/progression assessment
 ↓
Diagnosis/stage
 ↓
Target/treatment plan
 ↓
Follow-up
```

The UI should provide a glaucoma-oriented summary without replacing the complete chart.

---

# 53. Disease-specific workflow: retina

Typical:

```text
Symptoms/history
 ↓
VA
 ↓
Dilated exam
 ↓
OCT / imaging
 ↓
Disease assessment
 ↓
Injection/laser/procedure decision
 ↓
Treatment
 ↓
Next imaging/follow-up
```

The application should keep serial imaging and treatment history adjacent.

---

# 54. Disease-specific workflow: cataract

Typical:

```text
Symptoms/functional impact
 ↓
VA
 ↓
Refraction
 ↓
Lens assessment
 ↓
Ocular/systemic context
 ↓
Surgical candidacy
 ↓
Biometry/testing
 ↓
IOL planning
 ↓
Consent
 ↓
Scheduling
 ↓
Surgery
 ↓
Postoperative follow-up
```

---

# 55. Disease-specific workflow: pediatric ophthalmology

Potential additional workflow:

- age-dependent acuity
- binocular function
- alignment
- cycloplegic refraction
- amblyopia assessment
- developmental considerations

The model must support pediatric-specific workflows without contaminating the common encounter model.

---

# 56. Workflow state machine summary

```text
APPOINTMENT
  ├─ canceled
  ├─ no-show
  └─ checked-in
        ↓
PRETEST
  ├─ incomplete
  ├─ failed
  └─ completed
        ↓
ENCOUNTER
  ├─ documenting
  ├─ awaiting results
  ├─ ready to sign
  └─ signed
        ↓
POST-ENCOUNTER
  ├─ open tasks
  ├─ pending results
  ├─ referral
  └─ follow-up
```

Each subsystem owns its own states; do not implement one giant global state machine.

---

# 57. Workflow observability

Log operational events such as:

- patient opened
- encounter started
- device acquisition started
- device result imported
- order created
- result reviewed
- prescription signed
- note signed
- result escalated
- task completed

Do not confuse operational telemetry with the legal/clinical audit record.

---

# 58. Workflow acceptance criteria

A clinical workflow is complete only when:

- happy path works
- incomplete path works
- cancellation works
- correction works
- audit works
- permissions work
- offline/failure behavior is defined
- provenance is retained
- clinician can recover from mistakes
- patient identity cannot be silently switched

---

# 59. Final workflow principle

Thyme should make the next clinically sensible action obvious while keeping the physician in control.

The application should never make the clinician ask:

> "What state is this record in?"

Instead, it should make the workflow visible through:

- queue
- worklist
- patient context
- current task
- clear status
- safe next action
