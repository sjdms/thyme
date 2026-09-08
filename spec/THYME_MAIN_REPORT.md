# Ophthalmology Desktop Application — Deep Research Report

**Project research target:** Ophthalmology clinical desktop software  
**Research date:** 2026-08-15  
**Project codename used for the downstream specification:** **Thyme**

> This research is a product/engineering analysis, not medical, legal, or regulatory advice. Clinical workflows and compliance obligations must be validated with practicing ophthalmologists and qualified local compliance/legal specialists before clinical deployment.

---

## 1. Executive conclusion

A credible ophthalmology application is not a generic electronic medical record with a few eye-examination fields bolted onto it.

The defining characteristics are:

1. **Ophthalmology-specific structured examination data.**
2. **Eye/laterality as a first-class dimension.**
3. **Longitudinal comparison of measurements and disease markers.**
4. **Tight integration with diagnostic devices and imaging.**
5. **Fast, highly customizable documentation workflows.**
6. **Structured prescriptions and procedure records.**
7. **Clinical provenance and auditability.**
8. **Interoperability with EHR/PACS/device ecosystems rather than locking data inside one database.**
9. **Secure local/offline behavior with explicit backup and disaster-recovery mechanisms.**
10. **A UI optimized for a high-throughput clinic instead of a generic form-filling workflow.**

The clinical literature and ophthalmology-specific EHR research reinforce this direction. Comprehensive eye evaluation includes ocular and systemic history, medication/allergy information, family history, social history, visual acuity, refraction, fields, pupils, motility, slit-lamp examination, IOP, and posterior-segment evaluation, with additional testing such as gonioscopy, imaging, or visual fields depending on the clinical context. [AAO Preferred Practice Pattern](https://www.aaojournal.org/article/S0161-6420%2820%2931026-5/fulltext) [Comprehensive eye examination review](https://pmc.ncbi.nlm.nih.gov/articles/PMC7041818/)

IHE's Eye Care domain specifically models workflows between the EHR/practice-management system and eye-care devices, including fundus cameras, slit lamps, refractive devices, visual fields, imaging systems, and DICOM/HL7 exchanges. [IHE Eye Care](https://www.ihe.net/ihe_domains/eye_care/) [Unified Eye Care Workflow](https://wiki.ihe.net/index.php/Unified_Eye_Care_Workflow)

There is also a significant usability imperative. One study found that ophthalmologists spent an average of 3.0 minutes of an 11.2-minute examination on EHR use—27% of examination time. Another found documentation accounted for 57% of trainee patient time after EHR implementation versus 48% on paper. [Time Requirements for EHR Use](https://pubmed.ncbi.nlm.nih.gov/29049512/) [Ophthalmology trainee EHR study](https://pubmed.ncbi.nlm.nih.gov/33954272/)

---

# 2. What ophthalmologists actually need

## 2.1 The clinical encounter

A practical Thyme encounter should support this pipeline:

**Patient scheduled → registered → checked in → pre-tested → examined → diagnostic tests ordered/performed → results reviewed → diagnosis assessed → treatment prescribed → procedures/surgery planned or performed → instructions/referrals/follow-up → billing/administrative completion → longitudinal record updated.**

Importantly, not every patient follows every step.

IHE's Unified Eye Care Workflow explicitly recognizes that some testing occurs routinely before physician examination while other studies are ordered only after the physician evaluates the patient. Thyme therefore needs a flexible order/worklist architecture rather than a rigid fixed sequence. [IHE Unified Eye Care Workflow](https://wiki.ihe.net/index.php/Unified_Eye_Care_Workflow)

---

## 2.2 Patient registration

Core information:

- Patient identifier
- Full name
- Date of birth
- Sex where clinically/administratively required
- Contact information
- Address
- Preferred communication method
- Emergency contact
- Referring physician/provider
- Insurance/payer information where applicable
- Patient status
- Preferred language
- Consent/privacy preferences where applicable
- Patient merge/duplicate handling
- External identifiers

Design requirements:

- Duplicate detection before creating a second chart
- Stable internal patient identifier
- External identifiers allowed
- Audit trail for demographic changes
- Merge/unmerge workflow with careful provenance
- Search tolerant of spelling and transliteration differences
- Rapid identification from device-worklist data

IHE specifically describes registration/appointment workflows and patient identifier consistency as core eye-care interoperability concerns. [IHE Unified Eye Care Workflow](https://wiki.ihe.net/index.php/Unified_Eye_Care_Workflow)

---

# 3. Medical and ophthalmic history

The history model should distinguish:

### Ocular history

- Previous diagnoses
- Eye injuries
- Eye surgeries
- Laser procedures
- Refractive surgery
- Cataract surgery
- Glaucoma surgery
- Retinal surgery
- Corneal procedures
- Prior eye medications
- Previous imaging/tests
- Contact lens history
- Glasses history
- Amblyopia/strabismus history
- Relevant childhood history

### Systemic history

- Chronic diseases
- Previous surgery
- Relevant systemic conditions
- Pregnancy-related state where clinically relevant
- Diabetes and diabetic management
- Hypertension
- Autoimmune/inflammatory disease
- Neurologic disease
- Vascular disease
- Thyroid disease
- Cancer history when relevant
- Medications and supplements

### Allergies/intolerances

Allergies should not be a simple comma-separated string.

The data model should support:

- Substance
- Allergy vs intolerance when known
- Verification status
- Criticality
- Reaction
- Severity
- Onset/recorded date
- Recorder
- Supporting history

FHIR models allergy/intolerance as a clinically meaningful record with patient linkage, verification, criticality, reaction details and provenance. [FHIR AllergyIntolerance](https://hl7.org/fhir/R4/allergyintolerance.html)

### Family history

Especially important:

- Glaucoma
- AMD
- Retinal disease
- Corneal disease
- Strabismus
- Blindness/low vision
- Hereditary disease

### Social/occupational history

Depending on practice and indication:

- Occupation
- Visual demands
- Driving
- Smoking
- Relevant environmental exposure
- Other social factors

The AAO Preferred Practice Pattern includes ocular/systemic history, medications, allergies, family history, social history and directed review of systems as part of the comprehensive medical eye evaluation. [AAO](https://www.aaojournal.org/article/S0161-6420%2820%2931026-5/fulltext)

---

# 4. Examination model

The examination should be composable. A single rigid "eye exam form" will not adequately support general ophthalmology, glaucoma, retina, pediatric, cornea, oculoplastics and surgical workflows.

## 4.1 Visual acuity

Store, at minimum:

- Eye/laterality
- Distance/near
- Corrected/uncorrected
- Correction type
- Measurement
- Testing method/scale
- Pinhole status/value
- Test date/time
- Examiner/device where relevant
- Confidence/quality note if applicable

Do not store only a display string such as `"20/40"`.

Store a normalized representation plus the clinical display.

The representation should permit alternate standards/scales rather than assuming every clinic uses one notation.

AAO's comprehensive examination guidance specifically identifies corrected visual acuity and, where appropriate, near acuity and pinhole testing. [AAO 2025 PPP abstract](https://www.aaojournal.org/article/S0161-6420%2825%2900817-6/abstract)

---

## 4.2 Refraction

Model:

### Existing/wearing prescription
- OD
- OS
- Sphere
- Cylinder
- Axis
- Add
- Prism
- Base direction
- PD
- Monocular PD if used
- Prescription type
- Date

### Automated refraction

Separate instrument-produced data from physician-accepted refraction.

### Manifest refraction

Include the same core optical fields while preserving:

- Refraction method
- Best corrected acuity
- Examiner
- Trial information

### Cycloplegic refraction

Must be explicitly distinguishable from ordinary manifest refraction.

### Final optical prescription

Should not silently overwrite examination refraction.

The system should preserve the relationship:

**Measurement → interpretation → selected refraction → final prescription.**

---

## 4.3 Pupils

Potential structured fields:

- OD size
- OS size
- Light response
- Direct response
- Consensual response
- RAPD
- Shape
- Reactivity
- Pharmacologic state
- Notes

---

## 4.4 Motility/alignment

Potential data:

- Ocular alignment
- Versions
- Ductions
- Cover test
- Alternate cover
- Prism measurements
- Diplopia
- Head posture
- Strabismus findings

This area must be extensible rather than forced into one text box.

---

## 4.5 External examination

Structured areas:

- Lids
- Lashes
- Lacrimal apparatus
- Facial findings
- Globe position
- Periocular findings

---

## 4.6 Slit-lamp examination

At minimum, structured support for:

- Lids/lashes
- Tear film
- Conjunctiva
- Sclera
- Cornea
- Anterior chamber
- Iris
- Lens
- Anterior vitreous
- Additional customizable findings

AAO's practice pattern explicitly identifies slit-lamp assessment of these structures. [AAO](https://www.aaojournal.org/article/S0161-6420%2820%2931026-5/fulltext)

---

## 4.7 IOP

Store:

- OD
- OS
- Measurement
- Unit
- Method/device
- Time
- Position if clinically relevant
- Medication/anesthesia context if relevant
- Examiner
- Repeat measurements
- Selected/final measurement

Common methods include Goldmann applanation, non-contact, rebound and other forms of tonometry. [Comprehensive eye examination review](https://pmc.ncbi.nlm.nih.gov/articles/PMC7041818/)

---

## 4.8 Gonioscopy

Model:

- Eye
- Angle assessment
- Quadrants
- Grading system
- Peripheral anterior synechiae
- Pigmentation
- Neovascularization
- Other findings
- Lens/device used
- Examiner
- Interpretation

Do not encode angle status purely as an uncontrolled paragraph if the practice uses structured grading.

---

## 4.9 Fundus/posterior segment

Structured areas can include:

- Vitreous
- Disc
- C/D ratio
- Rim
- Macula
- Vessels
- Periphery
- Retina
- Choroid where relevant
- Optic nerve findings
- Laterality

Free text should remain available for unusual findings.

---

## 4.10 Visual fields

The record should link:

**Order → performed test → source device → raw/structured study → report → interpretation → encounter**

Useful metadata:

- Device
- Strategy
- Program
- Eye
- Test date/time
- Reliability indices
- Global indices
- Interpretation
- PDF/report
- Original data where available
- Image/study link

Do not reduce a visual field to one "normal/abnormal" checkbox.

---

# 5. Diagnostic imaging

## 5.1 Major categories

Potentially support:

- OCT
- OCT-A
- Fundus photography
- Optic nerve photography
- Anterior-segment photography
- Widefield photography
- Corneal topography/tomography
- Biometry
- Pachymetry
- Ultrasound
- B-scan
- Fluorescein angiography
- Indocyanine green angiography
- Slit-lamp photography
- External photography

The exact device set will vary by clinic.

---

## 5.2 DICOM should be a strategic requirement

Do not make JPEG/PNG/PDF the system's primary imaging abstraction.

Use a study-centric imaging architecture capable of linking:

- Patient
- Encounter
- Order
- Study
- Series
- Instance
- Device/modality
- Acquisition time
- Laterality
- Report
- Interpretation

IHE Eye Care specifically addresses imaging and testing equipment and identifies eye-care modalities such as fundus cameras, slit lamps, refractive devices and visual fields. [IHE Eye Care](https://www.ihe.net/ihe_domains/eye_care/)

DICOMweb provides modern RESTful access through:

- QIDO-RS for query
- WADO-RS for retrieval
- STOW-RS for storage
- UPS-RS for worklists

[DICOMweb](https://www.dicomstandard.org/using/dicomweb)

Thyme should architect an internal imaging abstraction so device adapters can be added without rewriting clinical screens.

---

## 5.3 Image storage options

### Small standalone clinic

Local image/object storage may be acceptable if:

- backups are reliable
- storage is encrypted
- hardware redundancy is sufficient
- recovery is tested

### Larger practice

Prefer a dedicated image/archive subsystem or PACS/VNA integration.

IHE explicitly describes multiple models, including EHR-integrated storage/display and EHR/PACS models. [U-EYECARE](https://wiki.ihe.net/index.php/Unified_Eye_Care_Workflow)

---

# 6. Diagnoses and clinical assessment

A diagnosis record should support:

- Condition
- Eye/laterality
- Primary/secondary status
- Clinical status
- Severity/stage
- Date first identified
- Current status
- Source encounter
- Supporting findings
- Associated tests
- Assessment narrative
- Coding

The application should separate:

**Finding → clinical assessment → diagnosis → plan**

Do not automatically turn every structured finding into a diagnosis.

---

# 7. Longitudinal disease tracking

One of Thyme's strongest differentiators should be longitudinal ophthalmic visualization.

Examples:

### Glaucoma

Show:

- IOP history
- C/D ratio history
- RNFL/OCT trends
- visual field trends
- medications
- procedures
- target IOP
- documented progression assessments

### Retina

Show:

- OCT history
- visual acuity
- injection history
- disease-specific findings
- imaging chronology

### Cataract/refractive

Show:

- refraction history
- acuity
- lens findings
- surgical planning
- postoperative course

The UI should allow a physician to answer:

> "What changed in this eye since the last visit?"

without reading every historical note.

Existing ophthalmology products emphasize timeline views, previous testing, longitudinal IOP/visual acuity/C/D tracking and side-by-side image comparison—strong evidence that these are high-value workflows rather than decorative analytics. [ModMed ophthalmology subspecialties](https://www.modmed.com/specialties/ophthalmology/subspecialties/)

---

# 8. Prescriptions

## 8.1 Medication prescription model

A medication prescription should contain:

- Medication concept
- Formulation
- Strength
- Route
- Dose
- Frequency
- Dose schedule
- Eye/site if relevant
- Duration
- Quantity
- Refills
- Start date
- End date
- Indication
- Taper instructions
- PRN instructions
- Patient directions
- Prescriber
- Encounter
- Status
- Substitution rules where applicable
- Generated prescription artifact
- Signature/authentication status where legally required

FHIR's MedicationRequest provides an interoperability-oriented model for medication orders/requests linked to patient and encounter. [FHIR MedicationRequest](https://hl7.org/fhir/R4/medicationrequest-definitions.html)

---

## 8.2 Eye-specific medication instructions

Examples:

- OD
- OS
- OU
- Both eyes initially then taper one eye
- Specific postoperative eye

This must be structured, not only free text.

---

## 8.3 Optical prescriptions

Thyme should treat an optical prescription as a distinct clinical/business object.

Fields may include:

- OD sphere
- OD cylinder
- OD axis
- OD add
- OD prism/base
- OS equivalents
- PD
- monocular PD
- lens type
- material
- coating
- prescribed use
- contact-lens parameters where applicable
- issue date
- expiration where applicable
- prescriber

FHIR includes a VisionPrescription resource specifically for vision-correction prescriptions. [FHIR VisionPrescription](https://hl7.org/fhir/visionprescription.html)

---

# 9. Procedures and surgery

The application should have a common procedure abstraction plus procedure-specific templates.

Examples:

- Intravitreal injection
- YAG capsulotomy
- SLT
- Laser peripheral iridotomy
- Cataract surgery
- Trabeculectomy
- Glaucoma procedures
- Retinal procedures
- Corneal procedures
- Minor office procedures

Each procedure should be able to capture:

- Indication
- Eye
- Date/time
- Operator
- Assistants
- Consent
- Pre-procedure findings
- Anesthesia
- Equipment
- Medication
- Device/implant
- Procedure details
- Complications
- Outcome
- Post-procedure instructions
- Follow-up
- Billing/coding links

Surgical records should be versioned and auditable.

---

# 10. Clinical notes

Thyme should support:

- Chief complaint
- HPI
- ocular history
- systemic history
- medications
- allergies
- exam
- testing
- assessment
- plan
- patient counseling
- instructions
- follow-up

But do not require everything to be free text.

The strongest design is a **structured-first, narrative-capable** model.

Research on ophthalmology documentation shows that EHR text contains significant information generated through clinicians, templates, copy-forward and structured data pathways. The design should therefore make source/provenance visible and minimize meaningless note duplication. [EHR documentation study](https://pubmed.ncbi.nlm.nih.gov/31811860/)

---

# 11. Templates and customization

Templates are essential but must not become immutable bureaucracy.

Support:

- personal templates
- clinic templates
- specialty templates
- subspecialty templates
- diagnosis-driven templates
- visit protocols
- post-op templates
- customizable field ordering
- default values
- shortcuts
- "pull forward" with explicit review
- smart phrases

The architecture should make it impossible for copied-forward old information to silently masquerade as a newly observed finding.

---

# 12. Practice-management features

## Core

- Patient registration
- Scheduling
- Check-in
- Queue/status
- Encounter lifecycle
- Task assignment
- Referral tracking
- Follow-up scheduling
- Document printing

## Valuable

- Billing
- Claims
- Insurance eligibility
- Payments
- Patient communications
- Inventory
- Optical POS integration
- Analytics

Existing commercial systems increasingly combine EHR, scheduling, billing, patient communication, optical workflows, image management and analytics. [ModMed](https://www.modmed.com/specialties/ophthalmology/) [NextGen](https://www.nextgen.com/markets/specialties/ophthalmology) [Compulink](https://compulinkadvantage.com/ophthalmology/)

Thyme should resist turning every administrative capability into a core module. Integrations can be more maintainable than rebuilding mature business systems.

---

# 13. User roles

At minimum:

### Ophthalmologist
- Full clinical access
- Sign/approve notes
- Diagnose
- Prescribe
- Order/interpret tests
- Perform procedures

### Technician
- Pretesting
- Measurements
- Device acquisition
- Limited clinical updates

### Nurse/clinical staff
- Medication/pre-procedure tasks
- Patient communication
- selected clinical workflows

### Receptionist
- Registration
- Scheduling
- demographics
- limited financial information

### Practice manager
- operational/reporting access
- no unnecessary clinical privilege

### IT/administrator
- technical administration
- should not automatically imply unrestricted clinical access

Roles should be permission sets, not hardcoded screens.

---

# 14. Audit and provenance

Thyme must maintain at least two related concepts:

### Audit trail

Who accessed/changed something and when.

### Clinical provenance

Who or what generated the actual clinical artifact/value.

FHIR explicitly distinguishes provenance from activity auditing: Provenance records who/what/when created or revised a resource, while AuditEvent addresses use/activity. [FHIR Provenance](https://hl7.org/fhir/R4/provenance.html)

Therefore Thyme should preserve:

- Created by
- Modified by
- Signed by
- Time
- Source device
- Source integration
- Previous version
- Reason for correction where applicable
- Imported from
- Original artifact identifier

Clinical records should generally be corrected by creating a visible amendment/version rather than silently rewriting history.

---

# 15. Interoperability strategy

## FHIR

FHIR should be considered the principal modern exchange representation.

Useful mappings include:

- Patient
- Encounter
- Observation
- Condition
- AllergyIntolerance
- MedicationRequest
- Procedure
- DiagnosticReport
- ImagingStudy
- DocumentReference
- Provenance
- AuditEvent
- VisionPrescription

FHIR Observations represent measurements/assertions; DiagnosticReport groups diagnostic findings and can reference results and imaging. [FHIR Observation](https://hl7.org/fhir/R4/observation.html) [FHIR DiagnosticReport](https://hl7.org/fhir/R4/diagnosticreport.html)

## HL7 v2

Still relevant in established clinical environments, especially for:

- registration
- ADT
- scheduling
- device/workflow integrations

IHE U-EYECARE specifically describes HL7 v2.5.1 messaging for registration and appointment workflows. [U-EYECARE](https://wiki.ihe.net/index.php/Unified_Eye_Care_Workflow)

## DICOM

Required strategically for ophthalmic imaging.

## IHE Eye Care

Thyme should explicitly track relevant IHE profiles and their implementation status.

---

# 16. Coding and terminology

Use an abstraction layer rather than hardwiring coding strings into UI code.

Potential terminology layers:

- SNOMED CT
- ICD-10/ICD-10-CM or applicable local equivalent
- CPT/HCPCS in relevant US deployments
- LOINC
- DICOM terminology
- local medication dictionaries
- local prescription dictionaries

Design rule:

> Clinical concepts should exist independently from the billing code assigned to them.

A diagnosis should not cease to exist because a particular coding system changes.

---

# 17. Security, privacy, compliance

## 17.1 General architecture

Assume:

**medical data + personally identifying data + images + prescriptions + audit history = high-value sensitive information.**

Security requirements:

- Encryption at rest
- Encryption in transit
- Strong authentication
- Optional hardware-backed authentication where feasible
- Role-based access
- Least privilege
- Session expiration
- Device enrollment
- Secure credential storage
- Audit logs
- Backup encryption
- Export control
- Secure deletion processes where legally permitted
- Recovery testing
- Tamper-evident audit storage

HIPAA's Security Rule requires safeguards for confidentiality, integrity and availability, including access controls, audit controls, authentication and transmission security for regulated US environments. [HHS](https://www.hhs.gov/hipaa/for-professionals/security/laws-regulations/index.html)

## 17.2 EU/GDPR

Health data is a special category of personal data under GDPR. Article 32 calls for security measures appropriate to risk, including confidentiality, integrity, availability/resilience, restoration capability and regular testing. [EUR-Lex Article 9](https://eur-lex.europa.eu/legal-content/EN/TXT/?qid=1643279388172&uri=CELEX%3A32016R0679) [EUR-Lex Article 32](https://eur-lex.europa.eu/legal-content/EN/ALL/?tid=+111622163&uri=celex%3A32016R0679)

## 17.3 Algeria

Algeria's personal-data framework is especially important for the intended deployment context.

The base law is Law 18-07 of 10 June 2018, and Algeria's legal portal records that it was amended and supplemented by Law 25-11 of 24 July 2025. This means older summaries of Law 18-07 should not be treated as the current legal position. [Algerian Ministry of Justice legal portal](https://droit.mjustice.gov.dz/fr/content/codes-0) [Law 25-11](https://droit.mjustice.gov.dz/sites/default/files/loi_25-11-fr.pdf)

Current implementation details must therefore be validated against the amended law and applicable decrees/regulatory guidance before deployment in Algeria.

**Mandatory validation:** Algerian privacy counsel/compliance specialist.

---

# 18. Desktop architecture

## Recommended direction

Thyme should be designed as a **desktop-first clinical system with a local clinical engine** and a service boundary that can support LAN/multi-user deployments.

A useful architecture:

```text
                 ┌─────────────────────────┐
                 │       Thyme Desktop      │
                 │ Clinical UI / Workflow   │
                 └────────────┬────────────┘
                              │
                      Clinical Application
                              │
       ┌──────────────────────┼───────────────────────┐
       │                      │                       │
  Clinical DB           Imaging Adapter          Integration Hub
       │                      │                       │
       │                  DICOM/PACS              FHIR/HL7
       │                  Device adapters         External systems
       │
  Audit / Provenance / Search / Document services
       │
       └────────────── Secure Storage / Backup
```

The UI should not own clinical business rules.

---

# 19. Local-first and multi-user deployment

## Single workstation

Possible for small practices:

- local DB
- local encrypted image/object store
- automatic backup
- printable documents

## Clinic LAN

Preferred next step:

- server/database host
- multiple desktops
- shared image store
- centralized audit
- role management
- backup service

## Cloud/hybrid

Should be treated as an architectural deployment option rather than the only architecture.

A clinic should be able to continue working safely during temporary loss of internet connectivity where local architecture allows.

---

# 20. Database model

Suggested major entities:

```text
Organization
 ├── Location
 ├── User
 │    └── Role/Permission
 └── Device

Patient
 ├── Identifier
 ├── Contact
 ├── Allergy
 ├── Medication history
 ├── Ocular history
 ├── Family history
 ├── Insurance
 ├── Document
 └── Encounter
      ├── Examination
      │    ├── Eye finding
      │    ├── Measurement
      │    ├── Refraction
      │    └── Assessment
      ├── Diagnostic order
      ├── Diagnostic result
      ├── Imaging study
      ├── Diagnosis
      ├── Medication prescription
      ├── Vision prescription
      ├── Procedure
      ├── Care plan
      └── Follow-up
```

### Critical modeling principle

Do not embed all ophthalmic measurements directly into an Encounter row.

Measurements need:

- code/concept
- value
- unit
- eye/laterality
- method
- timestamp
- device
- examiner
- status
- provenance
- encounter

That architecture enables longitudinal trending and device integration.

---

# 21. Eye/laterality model

Use an enum/value set equivalent to:

- OD
- OS
- OU
- Not applicable
- Unknown

But do not let laterality merely become a string.

Every clinical object should declare whether it is:

- patient-level
- encounter-level
- eye-level
- bilateral
- anatomical/site-specific

Some observations are naturally bilateral while others cannot be duplicated semantically.

---

# 22. Search

Search is a clinical safety and productivity feature.

Must support:

- Patient identity search
- Identifier search
- date/range
- diagnosis
- medication
- procedure
- imaging
- provider
- appointment status
- eye
- document type

Full-text search is useful for notes, but structured search should not depend on full-text indexing.

---

# 23. Document management

Support:

- PDF
- scanned paper
- referrals
- reports
- external records
- prescriptions
- generated clinical letters
- consent documents
- images

FHIR DocumentReference illustrates why documents should have metadata describing type, patient context, author/source and the actual stored document. [FHIR DocumentReference](https://hl7.org/fhir/R4/documentreference.html)

---

# 24. Printing

Printing remains important in many clinics.

Generate robust:

- prescriptions
- referral letters
- consultation reports
- patient instructions
- procedure summaries
- medical certificates where applicable
- billing documents where applicable

Print output should be versioned with the source clinical record.

---

# 25. UX architecture

## Patient header

Persistent:

- Patient name
- DOB
- identifier
- alerts
- allergies
- key diagnoses
- current eye status
- open tasks
- last encounter

## Encounter workspace

A physician should be able to reach:

- history
- prior exams
- current examination
- diagnostics
- images
- assessment
- prescriptions
- procedures
- follow-up

without opening multiple disconnected applications.

## Longitudinal panel

Always-available trend access for relevant diagnoses.

---

# 26. Efficiency requirements

Thyme should optimize for:

- keyboard shortcuts
- low-click common paths
- customizable defaults
- smart templates
- structured measurement entry
- device auto-import
- comparison views
- quick previous-value access
- rapid patient switching with explicit safeguards
- bulk administrative workflows for staff

Research explicitly indicates that EHR use can consume meaningful ophthalmologist time. [Time Requirements](https://pubmed.ncbi.nlm.nih.gov/29049512/)

A good target is not "maximum automation."

It is:

> **minimum interaction necessary to create a trustworthy clinical record.**

---

# 27. AI opportunities

AI may eventually help with:

- note drafting
- extracting structured findings from dictated notes
- summarization
- trend summarization
- document classification
- referral triage
- coding suggestions
- missing-data warnings

But AI output must be:

- explicitly marked as generated/suggested
- reviewable
- editable
- attributable
- never silently committed as fact

The current commercial market already uses AI-assisted ophthalmic documentation, so Thyme should treat this as a potential competitive dimension rather than an unusual future feature. [ModMed](https://www.modmed.com/specialties/ophthalmology/) [NextGen](https://www.nextgen.com/markets/specialties/ophthalmology) [Compulink](https://compulinkadvantage.com/ophthalmology/)

For a clinical MVP, AI should not be allowed to autonomously diagnose, prescribe, or finalize a clinical note.

---

# 28. MVP

## P0

### Patient management
- Patient registry
- Duplicate detection
- Demographics
- Ocular/systemic history
- Allergies
- Current medications

### Clinical
- Encounter
- Chief complaint
- Visual acuity
- Refraction
- Pupils
- Motility
- External exam
- Slit lamp
- IOP
- Fundus
- Assessment/diagnosis
- Plan

### Prescribing
- Medication prescriptions
- Optical prescriptions
- Printable outputs

### History
- Timeline
- Previous examinations
- Previous diagnoses
- Previous medications
- Basic measurement trends

### Security
- User authentication
- RBAC
- Audit trail
- Encryption
- Backup/restore

### Documents
- Upload
- Patient association
- Preview
- Print/export

---

# 29. Phase 2

- DICOM image import/display
- Device integration framework
- Visual fields
- OCT
- Imaging comparison
- Referrals
- Appointment scheduling
- Tasking
- Procedure templates
- Surgery planning
- richer longitudinal dashboards
- FHIR export/import
- HL7 integration layer
- LAN multi-user deployment

---

# 30. Phase 3

- PACS/VNA integration
- advanced device interfaces
- clinic-wide analytics
- optical inventory/POS integration
- billing integrations
- patient portal/communications
- AI-assisted documentation
- advanced reporting
- multi-site architecture
- centralized terminology management
- advanced interoperability

---

# 31. Things not to build too early

Do not start by building:

- a huge billing system
- a full PACS
- autonomous diagnosis AI
- every possible medical specialty
- dozens of brittle device integrations
- a complete hospital information system
- a generic CRM
- complicated patient-facing portal features

The first product must prove:

> **A real ophthalmologist can see a patient and produce a better structured ophthalmic record faster than with a generic EHR or paper workflow.**

---

# 32. Risks

## Clinical risk
Incorrect data representation can affect care.

Mitigation:

- clinician validation
- explicit units
- laterality enforcement
- clear source attribution
- correction/versioning
- safe defaults

## Data loss

Mitigation:

- automated backups
- backup integrity testing
- multiple copies
- disaster-recovery procedure
- restore drills

## Security compromise

Mitigation:

- least privilege
- encryption
- secure updates
- audit logging
- credential hardening
- endpoint hardening

## Device integration

Mitigation:

- adapter abstraction
- standards-first architecture
- device certification/testing
- vendor-specific modules

## UX failure

Mitigation:

- observe real clinicians
- time workflows
- usability testing
- measure clicks/typing/time
- permit customization

## Regulatory uncertainty

Mitigation:

- jurisdiction configuration
- compliance review
- documented assumptions
- no hardcoded legal assertions

---

# 33. Strong differentiation opportunities

1. **Excellent longitudinal eye comparison** instead of historical chart archaeology.
2. **Device-native architecture** instead of treating images as attachments.
3. **Fast structured ophthalmic charting** with genuine customization.
4. **Excellent offline/LAN reliability.**
5. **Clean clinical provenance.**
6. **Better migration/import/export than proprietary incumbents.**
7. **A modular deployment model suitable for smaller practices.**
8. **A terminology/interoperability layer that prevents vendor lock-in.**

---

# 34. Open questions that must be validated

### Clinical validation
- Which measurements are truly mandatory for each encounter type?
- Which defaults are acceptable?
- Which findings must be structured?
- Which subspecialty fields should be first-class?
- What is the preferred workflow for real clinics?

### Algeria
- Current interpretation of Law 18-07 as amended by Law 25-11
- Applicable implementing decrees
- Health-data storage/location requirements
- Electronic prescription requirements
- Electronic signature requirements
- Medical-record retention requirements
- Rules governing data transfer outside Algeria
- National interoperability/coding requirements
- Whether additional Ministry of Health requirements apply

### Hardware
- Exact devices used by target clinics
- Vendor protocols
- DICOM support
- licensing requirements
- drivers/middleware

### Commercial
- Which existing products are most prevalent in the target market?
- Which features are actually used?
- Which features cause user frustration?
- Which integrations are critical enough to justify engineering investment?

---

# 35. Requirements priority summary

| Priority | Meaning | Example |
|---|---|---|
| P0 | Required for viable clinical MVP | Patient, encounter, exam, diagnosis, prescription, audit |
| P1 | High-value near-term | Imaging, DICOM, device integration, longitudinal dashboards |
| P2 | Useful expansion | Billing, patient portal, analytics |
| P3 | Future/optional | Advanced AI, large-scale multi-site ecosystem |

---

# 36. Recommended implementation sequence

1. Clinical domain model
2. Patient/encounter lifecycle
3. Examination and laterality model
4. Diagnosis/plan model
5. Prescription engine
6. Versioning/provenance/audit
7. Secure persistence
8. Core desktop UX
9. Timeline and longitudinal data
10. Printing/documents
11. Backup/restore
12. Device/integration abstraction
13. DICOM
14. FHIR/HL7
15. Procedures/surgery
16. Practice management
17. AI-assisted features

---

# 37. Source register

## Clinical / ophthalmology

- American Academy of Ophthalmology. *Comprehensive Adult Medical Eye Evaluation Preferred Practice Pattern*.  
  https://www.aaojournal.org/article/S0161-6420%2820%2931026-5/fulltext
- American Academy of Ophthalmology. *Comprehensive Adult Medical Eye Evaluation Preferred Practice Pattern* (2025 abstract).  
  https://www.aaojournal.org/article/S0161-6420%2825%2900817-6/abstract
- Kaur et al. *Comprehensive eye examination: what does it mean?*  
  https://pmc.ncbi.nlm.nih.gov/articles/PMC7041818/
- *Evaluation of a glaucoma patient*.  
  https://pmc.ncbi.nlm.nih.gov/articles/PMC3038503/

## Ophthalmology EHR usability

- Sanders et al. *Electronic health record systems in ophthalmology: impact on clinical documentation*.  
  https://pubmed.ncbi.nlm.nih.gov/23683945/
- Henriksen et al. *Electronic Health Records in Ophthalmology: Source and Method of Documentation*.  
  https://pubmed.ncbi.nlm.nih.gov/31811860/
- Rule et al. *Time Requirements for Electronic Health Record Use in an Academic Ophthalmology Center*.  
  https://pubmed.ncbi.nlm.nih.gov/29049512/
- *Impact of an electronic health record implementation on ophthalmology trainee time expenditures*.  
  https://pubmed.ncbi.nlm.nih.gov/33954272/
- *Clinical Documentation in Electronic Health Record Systems: Analysis of Patient Record Review During Outpatient Ophthalmology Visits*.  
  https://pubmed.ncbi.nlm.nih.gov/30815099/
- Fuhrmann & Schargus. *National survey of user-reported usability of electronic medical record software in ophthalmology in Germany*.  
  https://pubmed.ncbi.nlm.nih.gov/37378879/
- *Special requirements for electronic health record systems in ophthalmology*.  
  https://pubmed.ncbi.nlm.nih.gov/21680023/

## Interoperability

- IHE Eye Care.  
  https://www.ihe.net/ihe_domains/eye_care/
- IHE Unified Eye Care Workflow.  
  https://wiki.ihe.net/index.php/Unified_Eye_Care_Workflow
- IHE Core Eye Care Workflow.  
  https://wiki.ihe.net/index.php/Core_Eye_Care_Workflow
- DICOMweb.  
  https://www.dicomstandard.org/using/dicomweb
- DICOMweb resources.  
  https://www.dicomstandard.org/using/dicomweb/restful-structure

## FHIR

- Observation.  
  https://hl7.org/fhir/R4/observation.html
- DiagnosticReport.  
  https://hl7.org/fhir/R4/diagnosticreport.html
- MedicationRequest.  
  https://hl7.org/fhir/R4/medicationrequest-definitions.html
- AllergyIntolerance.  
  https://hl7.org/fhir/R4/allergyintolerance.html
- Encounter.  
  https://hl7.org/fhir/R4/encounter.html
- DocumentReference.  
  https://hl7.org/fhir/R4/documentreference.html
- Provenance.  
  https://hl7.org/fhir/R4/provenance.html
- VisionPrescription.  
  https://hl7.org/fhir/visionprescription.html

## Security / regulation

- HHS. *Summary of the HIPAA Security Rule*.  
  https://www.hhs.gov/hipaa/for-professionals/security/laws-regulations/index.html
- EUR-Lex. GDPR Article 9.  
  https://eur-lex.europa.eu/legal-content/EN/TXT/?qid=1643279388172&uri=CELEX%3A32016R0679
- EUR-Lex. GDPR Article 32.  
  https://eur-lex.europa.eu/legal-content/EN/ALL/?tid=+111622163&uri=celex%3A32016R0679
- Algerian Ministry of Justice legal portal. Law 18-07, amended by Law 25-11.  
  https://droit.mjustice.gov.dz/fr/content/codes-0
- Algeria Law 25-11 PDF.  
  https://droit.mjustice.gov.dz/sites/default/files/loi_25-11-fr.pdf
- Algerian Ministry of Pharmaceutical Industry — clinical-study regulatory framework.  
  https://www.miph.gov.dz/fr/etudes-cliniques/

## Existing commercial products examined

- ModMed Ophthalmology.  
  https://www.modmed.com/specialties/ophthalmology/
- ModMed Ophthalmology EHR.  
  https://www.modmed.com/specialties/ophthalmology/ehr/
- ModMed Ophthalmology subspecialties.  
  https://www.modmed.com/specialties/ophthalmology/subspecialties/
- NextGen Healthcare — Ophthalmology.  
  https://www.nextgen.com/markets/specialties/ophthalmology
- Compulink Advantage — Ophthalmology.  
  https://compulinkadvantage.com/ophthalmology/

---

# 38. Bottom line

Thyme should be built around the following thesis:

> **The patient's eyes, measurements, images, diagnoses, treatments and progression are the primary clinical objects—not merely attachments to a generic encounter record.**

The best architecture therefore makes ophthalmology-specific structure native while keeping interoperability generic enough to exchange data with the broader healthcare ecosystem.

The next engineering artifact should be the separate **Thyme Project Definition**, which converts these findings into implementation-level scope and decisions.
