# THYME_UI_SPEC.md

# Thyme UI Specification

**Project:** Thyme  
**Artifact:** Desktop User Interface Specification  
**Version:** 0.1  
**Status:** Implementation foundation  
**Date:** 2026-08-16

> This document specifies information architecture, screen behavior, interaction rules and UI safety patterns. It is not a final visual design system. Typography, color palette, exact spacing tokens and component implementation should be decided after usability testing with ophthalmology staff.

---

# 1. UI objectives

Thyme's interface must optimize for:

- clinical context
- rapid navigation
- low interaction cost
- eye-specific clarity
- longitudinal visibility
- safe data entry
- easy correction
- device/result integration
- keyboard-first operation
- large-screen desktop workflows

The most important principle is:

> **The application should help the clinician remember and compare, not make the clinician hunt and reconstruct.**

---

# 2. Primary desktop model

Target environment:

- Windows desktop first
- keyboard + mouse
- large monitor support
- optional dual-monitor workflows
- touch-friendly controls where valuable, but not touch-dependent
- high DPI displays
- printers/scanners/devices

Minimum layout should remain usable at a practical clinic resolution.

The UI should support a scalable window rather than a fixed-size application.

---

# 3. Global information architecture

```text
Dashboard
├── Today's Queue
├── My Work
├── Results
├── Tasks
└── Alerts

Patients
├── Search
├── Recent
└── Favorites

Schedule
├── Day
├── Week
└── Worklists

Clinical
├── Current Encounters
├── Results
├── Referrals
└── Procedures

Administration
├── Users
├── Templates
├── Devices
├── Organization
└── Reports

System
├── Backup/status
├── Integrations
├── Audit
└── Diagnostics
```

Clinical screens should dominate the primary navigation. Administrative features should not compete visually with the patient-care workspace.

---

# 4. Global shell

Recommended shell:

```text
┌─────────────────────────────────────────────────────────────┐
│ Thyme │ Search │ Queue │ Results │ Tasks │ User / Status    │
├──────────────┬──────────────────────────────────────────────┤
│ Primary nav  │                                              │
│              │             Workspace                        │
│ Dashboard    │                                              │
│ Patients     │                                              │
│ Schedule     │                                              │
│ Clinical     │                                              │
│ Admin*       │                                              │
│              │                                              │
└──────────────┴──────────────────────────────────────────────┘
```

`*` visibility depends on permission.

---

# 5. Persistent patient context

Whenever a patient is open, show a persistent patient header.

Example:

```text
┌──────────────────────────────────────────────────────────────┐
│ Jane Doe  •  1978-03-15  •  MRN 00018452                    │
│ Allergies: Penicillin     Active problems: POAG OU            │
│ Current encounter: Follow-up • 16 Aug 2026                  │
└──────────────────────────────────────────────────────────────┘
```

The patient header should not be visually confused with generic application navigation.

It must remain visually distinctive.

---

# 6. Patient safety banner

The header should make high-value safety context immediately visible:

- allergies
- clinically important alerts
- current active encounter
- laterality-specific warnings where appropriate

Do not flood the header with every diagnosis.

Use a compact prioritized summary.

---

# 7. Patient search

## Search entry

Global shortcut:

```text
Ctrl+K
```

or deployment-equivalent.

Search supports:

- name
- MRN
- identifier
- phone
- DOB
- other configured identifiers

## Results

Recommended result row:

```text
Jane Doe
15 Mar 1978
MRN 00018452
+213...
Last visit: 3 Aug 2026
```

Potential duplicate candidates should be visually grouped.

---

# 8. Patient search safety

When creating a patient from search:

```text
Search → Candidate results → "No match" confirmation → Create
```

Do not put "Create new patient" as the visually dominant action before search results are evaluated.

---

# 9. Patient overview

Primary layout:

```text
┌──────────────── Patient Header ─────────────────────┐
│ Alerts / allergies / active problems                │
├─────────────────────────────────────────────────────┤
│ Current summary                                     │
├───────────────┬─────────────────────────────────────┤
│ Recent visits │ Key trends                          │
│               │ IOP / VA / other active measures   │
├───────────────┴─────────────────────────────────────┤
│ Timeline                                             │
└─────────────────────────────────────────────────────┘
```

Tabs/modules:

- Overview
- Timeline
- Encounters
- Diagnoses
- Medications
- Prescriptions
- Diagnostics
- Imaging
- Procedures
- Documents

---

# 10. Timeline

Timeline entries should be clinically meaningful:

```text
16 Aug — Follow-up encounter
14 Aug — OCT
02 Aug — Medication prescription
12 Jul — Procedure
```

Clicking an item opens its source context.

Avoid showing a giant chronological list without filtering.

Filters:

- date
- eye
- type
- diagnosis
- provider

---

# 11. Longitudinal trend panel

The user should be able to choose metrics.

Example:

```text
IOP
OD  ────╲────╱────
OS  ──╲────╲──────

        May Jun Jul Aug
```

The exact visualization should be validated with ophthalmologists.

Important:

- distinguish eye lines clearly
- always show units
- show measurement method where meaningful
- allow hover/select for exact values
- never use color alone to distinguish eyes
- provide a table view

---

# 12. Encounter workspace

This is the most important screen in Thyme.

Recommended structure:

```text
┌──────────────── Patient Header ──────────────────────────────┐
│ allergies / alerts / encounter status                        │
├─────────────┬───────────────────────────┬───────────────────┤
│ Navigation  │ Main clinical workspace   │ History/Context   │
│             │                           │                   │
│ History     │ HPI                       │ Previous values   │
│ Exam        │ Exam                      │ Trends            │
│ Tests       │ Assessment                │ Images            │
│ Diagnoses   │ Plan                      │ Prior notes       │
│ Plan        │                           │                   │
│ Rx          │                           │                   │
│ Follow-up   │                           │                   │
└─────────────┴───────────────────────────┴───────────────────┘
```

The right contextual panel can collapse.

---

# 13. Encounter navigation

The left navigation should show:

```text
✓ History
✓ Pretest
• Examination
○ Tests
○ Assessment
○ Plan
○ Prescriptions
○ Follow-up
```

Use state indicators carefully.

`✓` must mean completed/confirmed, not merely visited.

An incomplete mandatory section should be visually distinguishable.

---

# 14. Clinical workspace behavior

The clinician should be able to:

- use keyboard navigation
- jump between sections
- search concepts
- use saved templates
- see prior values
- add structured data without modal overload
- expand free-text notes when needed

Do not require a modal dialog for every measurement.

---

# 15. Eye-first examination layout

For eye-specific examinations, prefer an explicit two-column structure.

```text
                    OD                  OS
Visual acuity    [20/40]             [20/30]
Pinhole          [20/25]             [20/25]
IOP              [16]                [18]
Sphere           [-2.00]             [-1.50]
Cylinder         [-0.50]             [-0.75]
Axis             [090]               [080]
```

For bilateral values:

```text
OU [Normal]
```

The UI should make bilateral entry obvious.

---

# 16. Laterality design rule

Never place OD/OS as tiny unlabeled visual decoration.

Use explicit labels.

Recommended:

```text
RIGHT EYE (OD)
LEFT EYE (OS)
BOTH EYES (OU)
```

Abbreviations can be secondary.

The visual design must remain understandable at a glance.

---

# 17. History side panel

When entering current findings, show relevant prior values.

Example:

```text
IOP
Current       Previous
OD [ 16 ]     17
OS [ 18 ]     19

Last: 03 Aug 2026
Method: GAT
```

For a field that changes frequently, display the prior value directly adjacent to the current field.

---

# 18. Examination section: visual acuity

Recommended row:

```text
                     OD           OS
Distance UC          [20/40]      [20/30]
Distance BC          [20/25]      [20/25]
Pinhole              [20/30]      [20/25]
```

Controls:

- value entry
- recent-value shortcut
- correction selector
- method selector

The UI should accept clinician-friendly input while storing normalized structured semantics.

---

# 19. Examination section: refraction

Recommended compact grid:

```text
             OD                  OS
SPH        [-2.00]             [-1.50]
CYL        [-0.50]             [-0.75]
AXIS       [090]               [080]
ADD        [+2.00]             [+2.00]
```

Additional controls:

- refraction type
- method
- PD
- acuity

Do not bury refraction in a free-text editor.

---

# 20. Examination section: IOP

```text
IOP
OD   [ 16 ] mmHg   GAT     [12:14]
OS   [ 18 ] mmHg   GAT     [12:14]

+ Add measurement
```

If multiple values exist:

```text
12:14  16
12:18  17
12:21  16 ✓ Selected
```

Selection should be explicit.

---

# 21. Examination section: pupils

Possible layout:

```text
                     OD            OS
Size                 [3 mm]        [3 mm]
Reaction             [Normal]      [Normal]
RAPD                 [None]        [None]
Shape                [Round]       [Round]
```

Do not overuse dropdowns where compact typing/autocomplete is faster.

---

# 22. Slit-lamp workspace

Use a structure hierarchy:

```text
External
Conjunctiva
Sclera
Cornea
Anterior chamber
Iris
Lens
Anterior vitreous
```

Within each section:

```text
Common presets
[Normal]
[Custom]
```

Selecting "Normal" should populate explicit structured normal findings, with the ability to modify individual findings.

---

# 23. Fundus workspace

Suggested:

```text
OD
Optic disc
C/D
Vessels
Macula
Retina
Periphery

OS
Optic disc
C/D
Vessels
Macula
Retina
Periphery
```

Image viewer should be available beside or beneath the findings.

---

# 24. Structured + narrative rule

Every major clinical section should support:

```text
Structured findings
        +
Free-text note
```

Not:

```text
Free-text only
```

and not:

```text
Rigid checklist only
```

---

# 25. "Normal" UI behavior

A section with a normal preset should visibly indicate which fields were set.

Example:

```text
Slit lamp
[✓ Normal baseline]

Cornea       Clear
AC           Deep/quiet
Iris         Normal
Lens         Clear
```

The physician must be able to edit individual findings immediately.

---

# 26. Test ordering UI

Order drawer:

```text
Order diagnostic test

Test:       [ OCT Macula      ▼ ]
Eye:        [ OD              ▼ ]
Priority:   [ Routine         ▼ ]
Indication: [                ]

[Cancel]                    [Place order]
```

For bilateral tests:

```text
Eye: [ OU ]
```

Do not default to OU when the test should be eye-specific unless clinic policy supports it.

---

# 27. Results worklist

Recommended list:

```text
URGENT
● OCT — Jane Doe — unread — today

TO REVIEW
○ Visual field — John Smith — 09:32
○ Fundus photo — Samir A. — yesterday

COMPLETED
✓ OCT — ...
```

Each result should show:

- patient
- test
- date
- eye
- priority
- current state

---

# 28. Imaging viewer

The viewer should support:

- zoom
- pan
- fit
- side-by-side
- compare current/prior
- metadata
- laterality
- date
- modality
- measurement/report access

Never hide patient identity or laterality while viewing a clinical image.

---

# 29. Current vs previous imaging

Comparison mode:

```text
CURRENT                         PREVIOUS
[ OCT image ]                   [ OCT image ]

16 Aug 2026                     10 Jun 2026
OD                              OD
```

The UI should keep:

- patient
- eye
- date
- modality

visible in both panes.

---

# 30. Diagnosis UI

Diagnosis composer:

```text
Diagnosis
[ Primary open-angle glaucoma        ]
Eye: [ OU ]
Stage: [ Moderate ]
Status: [ Active ]

Assessment:
[....................................]

[Add diagnosis]
```

Autocomplete must display terminology/coding context without requiring the clinician to memorize code systems.

---

# 31. Plan UI

The plan should combine:

- medication
- procedure
- diagnostics
- referral
- follow-up
- counseling/instructions

But each should remain a structured object.

Example:

```text
PLAN
☑ Start medication
☑ Repeat OCT
☑ Follow up in 3 months
☐ Referral
```

The checkboxes represent planned actions, not completion states unless explicitly labeled.

---

# 32. Medication prescription UI

Use a guided composer:

```text
Medication        [Timolol]
Strength          [0.5%]
Route             [Ophthalmic]
Eye               [OU]
Dose              [1 drop]
Frequency         [BID]
Duration          [30 days]
Refills           [2]

Instructions
[Instill one drop in each eye twice daily.]

                 [Preview] [Sign]
```

Warnings/alerts should remain visible during composition.

---

# 33. Prescription preview

Before signing:

```text
PATIENT
Jane Doe

MEDICATION
Timolol ophthalmic 0.5%

SIG
Instill 1 drop OU twice daily.

Quantity: ...
Refills: ...
Prescriber: ...
```

The preview must look like an artifact a patient/pharmacy could actually use.

---

# 34. Optical prescription UI

Grid:

```text
                 OD                OS
SPH             [-2.00]           [-1.50]
CYL             [-0.50]           [-0.75]
AXIS            [090]             [080]
ADD             [+2.00]           [+2.00]
PRISM           [...]             [...]

PD:
Total [62]
Right [31]
Left  [31]
```

Provide:

```text
[Use final refraction]
```

but explicitly show what will be copied.

---

# 35. Procedure composer

```text
Procedure: [Intravitreal injection]
Eye:       [OD]
Indication:[...]
Consent:   [Signed]
Anesthesia:[...]
Medication:[...]
Device:    [...]
Findings:  [...]
Outcome:   [...]

[Mark performed]
```

Laterality should be visually prominent immediately before completion.

---

# 36. Safety confirmation pattern

For high-consequence actions use a concise confirmation:

```text
Confirm procedure

Patient: Jane Doe
Procedure: Intravitreal injection
Eye: RIGHT (OD)

[Cancel]     [Confirm and record]
```

Avoid generic:

```text
Are you sure?
```

The confirmation should state exactly what matters.

---

# 37. Signing UI

Before sign:

```text
READY TO SIGN

Patient: Jane Doe
Encounter: Follow-up
Provider: Dr. X

Unreviewed:
• OCT result
• imported medication change

[Review] [Sign]
```

The user may sign when appropriate according to configured policy.

The UI should not silently hide unresolved items.

---

# 38. Unsigned-work indicator

Persistent but non-annoying indicator:

```text
● 2 encounters ready for sign
```

Click opens the signing worklist.

---

# 39. Worklist

Worklist tabs:

- My patients
- Results
- Notes
- Referrals
- Tasks
- Prescriptions
- Procedures

Filter by:

- priority
- status
- date
- provider
- location

---

# 40. Dashboard

For physician:

```text
TODAY
18 scheduled
7 waiting
3 ready
2 results
1 urgent
4 unsigned
```

For technician:

```text
Waiting
Pretests due
Device errors
Completed tests
```

For receptionist:

```text
Arrivals
Late
Waiting
Upcoming
No-shows
```

Dashboards must be role-specific.

---

# 41. Keyboard interaction model

The application should support discoverable shortcuts.

Examples:

```text
Ctrl+K     Global search
Ctrl+P     Print current artifact
Ctrl+S     Save draft
Ctrl+Enter Commit/sign when safe
Esc        Close transient panel
Tab        Next field
Shift+Tab  Previous field
Alt+1      History
Alt+2      Exam
Alt+3      Tests
Alt+4      Assessment
Alt+5      Plan
```

Final shortcuts must be tested against OS/application conventions.

Never bind critical actions to hard-to-reach keys without redundancy.

---

# 42. Keyboard data-entry strategy

Common numeric fields should support:

- direct typing
- Enter → next logical field
- arrow navigation between OD/OS
- recent-value selection
- autocomplete

Example:

```text
OD IOP [16] Enter
→ OS IOP [18] Enter
→ next section
```

This is much faster than mouse-only entry.

---

# 43. Mouse behavior

Common clinical actions should be one or two interactions from the current context.

Avoid:

```text
click
→ modal
→ tab
→ dropdown
→ search
→ select
→ save
```

for frequent measurements.

---

# 44. Autosave

Draft clinical input should autosave where technically safe.

Autosave status:

```text
Saved 4 seconds ago
```

or:

```text
Saving...
Saved
```

Never tell the user data is saved if persistence has not actually been confirmed.

---

# 45. Unsaved state

If closing a draft with pending edits:

```text
Unsaved changes

Save draft
Discard
Cancel
```

Do not make "Discard" the primary action.

---

# 46. Offline status

System-level status should always be visible.

Example:

```text
● Connected
```

or:

```text
△ Offline — local mode
```

or:

```text
⚠ Sync delayed — 3 items pending
```

Never silently hide degraded connectivity.

---

# 47. Device status

For clinics with integrated devices:

```text
Devices
OCT        ● Online
Visual field ● Online
Fundus      ⚠ Needs attention
Tonometer   ● Online
```

Click opens diagnostics.

---

# 48. Error design

Every error should answer:

1. What happened?
2. Is patient data safe?
3. Can I retry?
4. What should I do next?

Bad:

```text
Error 0x802...
```

Good:

```text
OCT import failed.

The original study remains on the device and no patient record was modified.

[Retry import] [Open device help]
```

---

# 49. Clinical validation errors

Validation should be close to the field.

Example:

```text
Axis [190]
      └─ Axis must be between 0 and 180.
```

Do not hide field errors in a notification bar.

---

# 50. Warning hierarchy

Three levels:

### Informational

Non-blocking.

### Warning

Clinician can proceed.

### Critical confirmation

Requires explicit decision.

Never make every warning a blocking modal.

---

# 51. Notifications

Use notification areas for:

- successful saves
- background imports
- noncritical errors
- completed tasks

Use inline UI for clinical data errors.

Use modal confirmation only for consequential actions.

---

# 52. Dialog rules

Dialogs are appropriate for:

- confirmation
- focused complex tasks
- identity merge
- irreversible/important actions
- device configuration

Dialogs are not appropriate for routine:

- IOP entry
- visual acuity
- diagnosis selection
- simple notes

---

# 53. Forms

Forms should be grouped by clinical meaning.

Bad:

```text
Field 1
Field 2
Field 3
...
```

Good:

```text
Visual acuity
Refraction
Tonometry
Slit lamp
Fundus
```

Within sections, follow clinical logic.

---

# 54. Tables

Use tables for:

- historical measurements
- prescriptions
- appointments
- diagnostic results
- worklists

Make row actions discoverable.

Avoid huge tables where the user must horizontally scroll through clinically essential data.

---

# 55. Charts

Charts are useful for:

- IOP
- acuity where meaningful
- repeated numeric disease measurements
- longitudinal treatment events

Charts must always have:

- units
- dates
- clear eye/laterality
- exact-value access
- table fallback

---

# 56. Color semantics

Color must not be the only way to communicate:

- OD vs OS
- normal/abnormal
- urgent/nonurgent
- signed/unsigned

Use:

- text
- icons
- labels
- shape
- positioning

in addition to color.

Color palette should be deliberately restrained to prevent alert fatigue.

---

# 57. Typography

Clinical values should be highly legible.

Use:

- clear numeric typography
- enough spacing around measurement values
- strong hierarchy
- predictable field alignment

Do not use excessively small text to fit more fields.

---

# 58. Density

Thyme should support a clinic-oriented density setting:

```text
Compact
Comfortable
Large
```

This changes:

- row height
- field spacing
- padding

It must not hide essential information.

---

# 59. Dual-monitor workflow

Recommended behavior:

### Primary monitor

- encounter
- documentation
- diagnosis
- prescriptions

### Secondary monitor

- imaging
- prior records
- timeline
- comparison

The user should be able to detach/reposition the contextual panel.

---

# 60. Image viewer behavior

Should be independently zoomable from the main encounter.

Avoid opening a completely separate application window unless necessary for device/system integration.

---

# 61. Patient switch protection

If the user switches patient while editing:

```text
Current patient has unsaved clinical data.

Current: Jane Doe
Switch to: John Smith

[Save draft] [Cancel]
```

For device import workflows, require explicit patient verification.

---

# 62. Role-specific UI

Do not simply hide buttons.

The application should adapt:

### Reception

- patient registration
- scheduling
- check-in
- documents

### Technician

- worklist
- pretest
- devices
- results

### Physician

- clinical encounter
- diagnostic interpretation
- prescriptions
- procedures

### Admin

- settings
- users
- reports
- integrations

---

# 63. Configuration

Configurable:

- templates
- default sections
- visible fields
- terminology display
- printer defaults
- device mappings
- worklists
- appointment types
- role permissions
- notification rules

Not configurable by ordinary users:

- clinical meaning of core fields
- audit semantics
- security guarantees
- signed record immutability

---

# 64. Templates UI

Template editor should show:

```text
Template
├── Sections
├── Fields
├── Defaults
├── Requiredness
├── Ordering
└── Visibility rules
```

It should preview the resulting encounter form.

---

# 65. Accessibility

Minimum:

- keyboard complete navigation
- visible focus
- sufficient contrast
- scalable text
- no color-only meaning
- screen-reader-friendly labels where practical
- sensible tab order
- no tiny click targets

Clinical desktop users may have different visual needs; this requires real usability testing.

---

# 66. Localization

Support:

- language
- date format
- decimal formatting
- units
- prescription formats
- document templates
- terminology display

Changing language must not change stored clinical meaning.

---

# 67. Empty states

Avoid:

```text
No data.
```

Prefer:

```text
No OCT studies have been recorded for this patient.
```

Or:

```text
No result yet — test ordered 16 Aug 2026.
```

Empty states should distinguish:

- none exists
- not performed
- pending
- inaccessible
- failed

---

# 68. Loading states

Use skeletons/placeholders for large clinical contexts.

Never display stale data as current without clear indication.

For images:

```text
Study metadata loaded
Image loading...
```

Do not block the entire patient workspace while a large image downloads.

---

# 69. Patient safety indicators

Potential high-value persistent indicators:

- Allergy alert
- active postoperative status
- critical unresolved result
- pending signature
- device synchronization failure
- patient identity warning

These should be prioritized, not all rendered equally.

---

# 70. Confirmation before high-impact actions

Use explicit summaries for:

- medication sign
- optical prescription sign
- procedure completion
- patient merge
- chart correction
- entered-in-error
- permanent export/deletion actions

The confirmation screen should show the exact relevant clinical data.

---

# 71. Clinical note editor

Recommended:

```text
┌─ Note toolbar ────────────────────────────┐
│ Template  Dictate  Insert  Pull forward   │
├───────────────────────────────────────────┤
│ Chief complaint                            │
│ [                                      ]   │
│                                            │
│ HPI                                        │
│ [                                      ]   │
│                                            │
│ Exam                                       │
│ [structured data summarized here]          │
│                                            │
│ Assessment / Plan                          │
│ [                                      ]   │
└───────────────────────────────────────────┘
```

The note editor is not the source of truth for structured findings.

---

# 72. AI-assisted UI

If enabled:

AI suggestions must appear as suggestions.

Example:

```text
Suggested summary
"IOP has remained approximately stable..."

[Accept]
[Edit]
[Reject]
[View source]
```

Never write AI output directly into signed notes without review.

---

# 73. Audit viewer

Admin/authorized user:

```text
16 Aug 02:14  Dr X  Viewed Patient
16 Aug 02:16  Tech Y Imported OCT
16 Aug 02:18  Dr X Signed Prescription
16 Aug 02:20  Dr X Amended Note
```

Filters:

- patient
- actor
- action
- time
- entity

---

# 74. Device diagnostics UI

Show:

- device
- status
- last connection
- last successful import
- queue
- errors
- retry
- configuration

Do not expose raw technical details as the first layer.

Provide:

```text
What happened?
What should I do?
Technical details
```

---

# 75. Backup status UI

For authorized users:

```text
Last backup: 01:00
Status: Successful
Last restore test: 02 Aug
Pending warnings: None
```

The ordinary clinician should not be distracted by backup implementation details.

---

# 76. Settings architecture

Settings categories:

```text
My profile
Clinical preferences
Templates
Printers
Devices
Integrations
Users & permissions
Organization
Backup
Security
Audit
```

High-risk administrative settings should show impact before modification.

---

# 77. Design system primitives

Suggested primitives:

### Layout

- AppShell
- Sidebar
- Header
- Workspace
- Panel
- SplitPane
- Drawer

### Clinical

- PatientHeader
- LateralityToggle
- MeasurementField
- HistoryComparison
- ClinicalSection
- DiagnosisPicker
- PrescriptionComposer
- ResultCard
- ImagingViewer

### Workflow

- Worklist
- StatusChip
- TaskRow
- ConfirmationDialog
- SignPanel
- ErrorPanel

---

# 78. Component semantic rules

Components should encode semantics.

Example:

```text
<LateralityField />
```

is preferable to:

```text
<Select options={["OD","OS","OU"]} />
```

because the component can enforce:

- labeling
- accessibility
- keyboard controls
- validation
- consistent terminology

---

# 79. No hidden state

A user should be able to tell:

- saved or unsaved
- signed or unsigned
- pending or completed
- current or historical
- imported or manually entered
- reviewed or unreviewed

The UI must surface these states.

---

# 80. UI behavior on failure

If a save fails:

```text
Save failed

Your draft remains open and has not been marked as saved.

[Retry] [Save locally] [Close]
```

Never reset the form and lose the user's work.

---

# 81. UI telemetry

Measure UX metrics without capturing unnecessary clinical content.

Examples:

- time to patient open
- time to encounter save
- time to prescription
- screen navigation latency
- device import latency
- error frequency
- abandoned workflows

Do not collect clinical text merely for UX metrics.

---

# 82. UI acceptance criteria

A screen is not complete until:

- keyboard navigation works
- focus order is logical
- save state is visible
- errors are recoverable
- laterality is explicit
- prior values are accessible where relevant
- permissions are enforced
- loading/empty/error states exist
- high-impact actions have appropriate confirmation
- real-user testing has been performed

---

# 83. Critical usability tests

The first usability study should time these tasks:

1. Find existing patient
2. Start follow-up encounter
3. Record OD/OS visual acuity
4. Enter IOP
5. Review previous IOP
6. Enter refraction
7. Review OCT
8. Add diagnosis
9. Prescribe medication
10. Generate optical prescription
11. Sign encounter
12. Amend an incorrect note
13. Handle a failed device import
14. Find an old result

The product should optimize these workflows before adding broad secondary features.

---

# 84. Final UI principle

Thyme should feel like a clinical instrument.

It should be:

- calm
- dense where useful
- explicit
- fast
- forgiving
- keyboard-friendly
- eye-aware
- context-rich
- safe

The UI should never be visually clever at the expense of clinical clarity.
