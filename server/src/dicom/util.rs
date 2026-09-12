//! Small, defensive accessors over `InMemDicomObject`.
//!
//! Every accessor here returns `Option` rather than propagating a
//! decode error: a missing or malformed *optional* attribute (a
//! device that didn't populate Corneal Size, say) should never abort
//! ingesting the attributes that *are* present and usable. Only the
//! handful of attributes this receiver treats as genuinely required
//! (a SOP Instance UID, at least one populated eye sequence, ...) are
//! ever turned into a hard `ServerError::Dicom` by the caller.

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use dicom_core::Tag;
use dicom_object::InMemDicomObject;

pub fn str_value(obj: &InMemDicomObject, tag: Tag) -> Option<String> {
    let value = obj.element(tag).ok()?.to_str().ok()?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

pub fn f64_value(obj: &InMemDicomObject, tag: Tag) -> Option<f64> {
    obj.element(tag).ok()?.to_float64().ok()
}

pub fn i32_value(obj: &InMemDicomObject, tag: Tag) -> Option<i32> {
    obj.element(tag).ok()?.to_int().ok()
}

/// The first (and, per the standard tables this receiver targets, only
/// permitted) item of a sequence attribute — see e.g.
/// `tags::AUTOREFRACTION_RIGHT_EYE_SEQUENCE`'s doc comment: "Only a
/// single Item shall be included in this Sequence."
pub fn sequence_item<'a>(obj: &'a InMemDicomObject, tag: Tag) -> Option<&'a InMemDicomObject> {
    obj.element(tag).ok()?.items()?.first()
}

pub fn pixel_data(obj: &InMemDicomObject) -> Option<Vec<u8>> {
    obj.element(super::tags::PIXEL_DATA).ok()?.to_bytes().ok().map(|b| b.into_owned())
}

/// Combines DICOM `DA` (`YYYYMMDD`) and `TM` (`HHMMSS[.FFFFFF]`, and in
/// practice from real devices sometimes just `HHMM` or `HH`) attributes
/// into a UTC timestamp. DICOM datetimes carry no timezone unless an
/// explicit offset was sent (which this reads as local/UTC-naive
/// either way, same simplification `packets::patient` already makes
/// for plain dates) — good enough for "roughly when was this taken",
/// which is all `oct_images.acquisition_datetime` is for.
pub fn date_time_value(obj: &InMemDicomObject, date_tag: Tag, time_tag: Tag) -> Option<DateTime<Utc>> {
    let date_str = str_value(obj, date_tag)?;
    let date = NaiveDate::parse_from_str(&date_str, "%Y%m%d").ok()?;

    let time = str_value(obj, time_tag).and_then(|time_str| {
        // Some devices pad to full HHMMSS.FFFFFF precision; others
        // truncate. Try the shapes actually seen in the wild rather
        // than requiring exactly one.
        let trimmed = time_str.split('.').next().unwrap_or(&time_str);
        NaiveTime::parse_from_str(trimmed, "%H%M%S")
            .or_else(|_| NaiveTime::parse_from_str(trimmed, "%H%M"))
            .ok()
    });

    let naive = date.and_time(time.unwrap_or_default());
    Some(Utc.from_utc_datetime(&naive))
}
