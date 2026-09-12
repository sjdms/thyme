//! Receives DICOM data pushed directly from ophthalmic devices on the
//! clinic's network — imaging from an OCT topographer
//! (`OphthalmicTomographyImageStorage`, see `oct`) and structured
//! refraction readings from an auto-refractor/keratometer
//! (`AutorefractionMeasurementsStorage`, see `refraction`) — and files
//! them onto the matching patient's chart.
//!
//! This is a DICOM Storage SCP (`association`): it only ever accepts
//! incoming C-STORE associations initiated by a device. It has no
//! Storage SCU, Query/Retrieve, or Modality Worklist support — a
//! device queried against this receiver, or a workflow that needs
//! Thyme to *pull* from a PACS instead of a device pushing to Thyme,
//! is out of scope for what's built so far.
//!
//! See `migrations/0017_create_device_studies_tables.sql` for the
//! staging schema this fills in, and `db::DeviceRepository` for the
//! data access layer over it.

mod association;
mod oct;
mod patient_match;
mod refraction;
mod sop_class;
mod tags;
mod util;

pub use association::DicomScp;
