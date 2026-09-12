//! The SOP Classes `server::dicom` negotiates and knows how to file —
//! one constant per Storage SOP Class UID from the DICOM standard
//! (PS3.4 Annex B / PS3.6 Annex A), grouped by which ingestion handler
//! (`super::oct` or `super::refraction`) applies.
//!
//! Nothing in this module or its callers is specific to any one
//! device's make or model — a Storage SCU only ever identifies what
//! it's sending by these standard UIDs, never by vendor, so "which
//! kind of object is this" is entirely a lookup against the standard,
//! not something that varies per manufacturer. `Modality` in an
//! IOD's General Equipment module (`OCT` / `OPTR` /...) is available
//! for logging but is never what this dispatch keys on.

/// Ophthalmic Tomography Image Storage (PS3.3 A.66.1) — the SOP Class
/// under which an OCT topographer sends a scan's pixel data. There are
/// two newer, more specialized OCT SOP Classes in the standard
/// (Ophthalmic OCT En Face Image Storage, Ophthalmic OCT B-scan Volume
/// Analysis Storage); this receiver only negotiates the original one
/// for now; see `super::oct`'s doc comment for what extending that
/// would take.
pub const OPHTHALMIC_TOMOGRAPHY_IMAGE_STORAGE: &str = "1.2.840.10008.5.1.4.1.1.77.1.5.4";

/// Autorefraction Measurements Storage (PS3.3 A.60.2) — the SOP Class
/// under which an auto-refractor/keratometer reports Sphere/Cylinder/
/// Axis per eye. This is the SOP Class `super::refraction` targets, per
/// the "refraction readings" this integration was asked for.
pub const AUTOREFRACTION_MEASUREMENTS_STORAGE: &str = "1.2.840.10008.5.1.4.1.1.78.2";

/// Keratometry Measurements Storage (PS3.3 A.60.3) — the same class of
/// device typically also reports corneal curvature (K readings) under
/// this separate SOP Class. Not negotiated yet: the corneal-curvature
/// side of a combined auto-refractor/keratometer's output has no home
/// on a Thyme chart today the way `RefractionEntry` gives
/// Sphere/Cylinder/Axis one, so there's nowhere meaningful to file it
/// until that lands. Left here, unused, as the obvious next constant
/// to wire up alongside a `KeratometryEntry` domain type.
#[allow(dead_code)]
pub const KERATOMETRY_MEASUREMENTS_STORAGE: &str = "1.2.840.10008.5.1.4.1.1.78.3";

/// Verification SOP Class (C-ECHO) — every conformant SCP is expected
/// to support this regardless of what Storage SOP Classes it offers,
/// since PACS/device administrators routinely C-ECHO a node to check
/// it's alive before ever attempting a real transfer.
pub const VERIFICATION: &str = "1.2.840.10008.1.1";

/// What this receiver does with a successfully-decoded instance of a
/// given SOP Class — the result of looking `sop_class_uid` up against
/// the constants above.
pub enum SopClassKind {
    OphthalmicTomographyImage,
    AutorefractionMeasurements,
}

impl SopClassKind {
    pub fn from_uid(sop_class_uid: &str) -> Option<Self> {
        match sop_class_uid {
            OPHTHALMIC_TOMOGRAPHY_IMAGE_STORAGE => Some(Self::OphthalmicTomographyImage),
            AUTOREFRACTION_MEASUREMENTS_STORAGE => Some(Self::AutorefractionMeasurements),
            _ => None,
        }
    }
}

/// Every Storage SOP Class this receiver proposes as an abstract syntax
/// when negotiating an association — see
/// `super::association::listen_for_associations`. Kept as one list so
/// the set this SCP *offers* and the set `SopClassKind::from_uid`
/// actually *understands* can't drift apart from each other.
pub const SUPPORTED_STORAGE_SOP_CLASSES: &[&str] =
    &[OPHTHALMIC_TOMOGRAPHY_IMAGE_STORAGE, AUTOREFRACTION_MEASUREMENTS_STORAGE];
