//! DICOM tags `server::dicom` reads directly, as `dicom_core::Tag`
//! constants rather than by keyword lookup through
//! `dicom_dictionary_std`.
//!
//! The handful of universal identity/image tags here (Patient ID,
//! Modality, Rows, ...) are almost certainly already in that crate's
//! standard dictionary under the matching keyword. The DIMSE command
//! tags and the ophthalmic refractive-measurement tags are a much
//! newer, more obscure corner of the standard, and less certain to be
//! present in any given dictionary build — so every tag this module
//! needs is spelled out here directly from the standard (group,
//! element) pair, cited against its defining table, rather than mixing
//! two lookup strategies for what's really one flat list of constants.

use dicom_core::Tag;

// -- DIMSE command set (PS3.7 Annex E / 9.3.1) -------------------------
//
// Every DIMSE message (C-STORE-RQ, C-STORE-RSP, ...) starts with a
// "command set": its own small Implicit-VR-Little-Endian-encoded data
// set, group 0000, sent as its own `PDataValueType::Command` PDV
// before any following `PDataValueType::Data` PDV. These are that
// command set's own tags — unrelated to (and never confused with) the
// tags of the DICOM data set a C-STORE actually carries.

pub const COMMAND_FIELD: Tag = Tag(0x0000, 0x0100);
pub const COMMAND_FIELD_C_ECHO: Tag = Tag(0x0020, 0x8020);
pub const MESSAGE_ID: Tag = Tag(0x0000, 0x0110);
pub const MESSAGE_ID_BEING_RESPONDED_TO: Tag = Tag(0x0000, 0x0120);
pub const AFFECTED_SOP_CLASS_UID: Tag = Tag(0x0000, 0x0002);
pub const AFFECTED_SOP_INSTANCE_UID: Tag = Tag(0x0000, 0x1000);
#[allow(dead_code)]
pub const PRIORITY: Tag = Tag(0x0000, 0x0700);
pub const COMMAND_DATA_SET_TYPE: Tag = Tag(0x0000, 0x0800);
pub const STATUS: Tag = Tag(0x0000, 0x0900);

/// `CommandField` (0000,0100) value identifying a C-STORE-RQ.
pub const COMMAND_FIELD_C_STORE_RQ: u16 = 0x0001;
/// `CommandField` (0000,0100) value identifying a C-STORE-RSP.
pub const COMMAND_FIELD_C_STORE_RSP: u16 = 0x8001;
/// `CommandDataSetType` (0000,0800) sentinel meaning "no data set
/// follows this command" — any other value means one does.
pub const COMMAND_DATA_SET_TYPE_NONE: u16 = 0x0101;
/// `Status` (0000,0900) value meaning the operation succeeded.
pub const STATUS_SUCCESS: u16 = 0x0000;

// -- Patient / general identity (PS3.3 C.7.1.1) ------------------------

pub const PATIENT_ID: Tag = Tag(0x0010, 0x0020);
pub const PATIENT_NAME: Tag = Tag(0x0010, 0x0010);
pub const MODALITY: Tag = Tag(0x0008, 0x0060);
pub const SOP_CLASS_UID: Tag = Tag(0x0008, 0x0016);
pub const SOP_INSTANCE_UID: Tag = Tag(0x0008, 0x0018);
pub const STUDY_INSTANCE_UID: Tag = Tag(0x0020, 0x000D);

// -- Image Pixel module (PS3.3 C.7.6.3) + ophthalmic laterality --------

pub const ROWS: Tag = Tag(0x0028, 0x0010);
pub const COLUMNS: Tag = Tag(0x0028, 0x0011);
pub const NUMBER_OF_FRAMES: Tag = Tag(0x0028, 0x0008);
pub const PIXEL_DATA: Tag = Tag(0x7FE0, 0x0010);
/// General Series module's laterality attribute — used on the
/// ophthalmic image IODs to say which eye a scan is of.
pub const IMAGE_LATERALITY: Tag = Tag(0x0020, 0x0062);
pub const ACQUISITION_DATE: Tag = Tag(0x0008, 0x0022);
pub const ACQUISITION_TIME: Tag = Tag(0x0008, 0x0032);

// -- Autorefraction Measurements module (PS3.3 C.8.25.9) ---------------
//
// Confirmed against the current DICOM standard's attribute tables for
// this module (part03/sect_C.8.25.9). Right and Left Eye Sequence
// items share the same child tags — a Sequence establishes its own
// nesting scope, so the meaning of e.g. (0046,0146) inside the right
// eye's single Item is identical to its meaning inside the left eye's.

pub const AUTOREFRACTION_RIGHT_EYE_SEQUENCE: Tag = Tag(0x0046, 0x0050);
pub const AUTOREFRACTION_LEFT_EYE_SEQUENCE: Tag = Tag(0x0046, 0x0052);
/// Sphere Power (Double, diopters) — direct child of each eye's
/// sequence item.
pub const SPHERE_POWER: Tag = Tag(0x0046, 0x0146);
/// Cylinder Sequence — present only if astigmatism/astigmatic
/// correction was measured for that eye; its single Item carries
/// `CYLINDER_POWER`/`CYLINDER_AXIS` below.
pub const CYLINDER_SEQUENCE: Tag = Tag(0x0046, 0x0018);
/// Cylinder Power (Double, diopters) — child of `CYLINDER_SEQUENCE`'s
/// Item, not of the eye sequence's Item directly.
pub const CYLINDER_POWER: Tag = Tag(0x0046, 0x0147);
/// Cylinder Axis (Single/float, degrees) — same nesting as
/// `CYLINDER_POWER`.
pub const CYLINDER_AXIS: Tag = Tag(0x0022, 0x0009);
