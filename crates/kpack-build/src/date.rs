//! Today's date as YYYY-MM-DD, without a date library.

use std::time::{SystemTime, UNIX_EPOCH};

/// Today's date (UTC).
pub fn today() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    civil_from_days((secs / 86_400) as i64)
}

/// Days since 1970-01-01 -> "YYYY-MM-DD" (Howard Hinnant's algorithm).
pub fn civil_from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        assert_eq!(civil_from_days(0), "1970-01-01");
        assert_eq!(civil_from_days(11_016), "2000-02-29");
        assert_eq!(civil_from_days(20_770), "2026-11-13");
    }
}
