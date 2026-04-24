use chrono::{SecondsFormat, Utc};

/// ISO-8601 timestamp in UTC, second precision, `Z` suffix.
/// Used for `Manifest.created_at` and `WriteRecord.timestamp`.
pub fn iso8601_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}
