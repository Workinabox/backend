use std::time::{SystemTime, UNIX_EPOCH};

use wiab_core::meeting_traits::Clock;

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        rfc3339_from_unix(now_unix_seconds())
    }
}

/// The auth layer needs the same wall clock plus computed expiries; serve it from the same
/// helpers so timestamps are formatted identically.
impl authbox_core::Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        rfc3339_from_unix(now_unix_seconds())
    }

    fn rfc3339_in(&self, seconds: i64) -> String {
        let base = now_unix_seconds() as i64 + seconds;
        rfc3339_from_unix(base.max(0) as u64)
    }
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn rfc3339_from_unix(total_seconds: u64) -> String {
    let days_since_epoch = (total_seconds / SECONDS_PER_DAY) as i64;
    let seconds_of_day = (total_seconds % SECONDS_PER_DAY) as u32;
    let (year, month, day) = civil_from_days(days_since_epoch);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

const SECONDS_PER_DAY: u64 = 86_400;

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let shifted_days = days_since_epoch + 719_468;
    let era = if shifted_days >= 0 {
        shifted_days / 146_097
    } else {
        (shifted_days - 146_096) / 146_097
    };
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }

    (year, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::civil_from_days;

    #[test]
    fn converts_unix_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn converts_known_modern_date() {
        assert_eq!(civil_from_days(20_147), (2025, 2, 28));
    }
}
