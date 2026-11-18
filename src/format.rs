/// Formats a byte count the way Finder does (decimal units)
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["bytes", "KB", "MB", "GB", "TB", "PB"];
    if n < 1000 {
        return if n == 1 { "1 byte".into() } else { format!("{n} bytes") };
    }
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Splits a formatted size into its number and unit, for typographic emphasis
pub fn bytes_parts(n: u64) -> (String, String) {
    let s = bytes(n);
    match s.split_once(' ') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (s, String::new()),
    }
}

pub fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A span of time as people say it: "0.8 s", "42 s", "3 min 12 s", "1 h 4 min"
pub fn duration(d: std::time::Duration) -> String {
    let s = d.as_secs_f64();
    match d.as_secs() {
        _ if s < 0.05 => "< 0.1 s".to_string(),
        _ if s < 9.95 => format!("{s:.1} s"),
        n if n < 60 => format!("{} s", s.round() as u64),
        n if n < 3600 => format!("{} min {} s", n / 60, n % 60),
        n => format!("{} h {} min", n / 3600, n % 3600 / 60),
    }
}

/// A running clock for something still going: "0:07", "12:40", "1:02:05"
pub fn clock(d: std::time::Duration) -> String {
    let n = d.as_secs();
    if n < 3600 { format!("{}:{:02}", n / 60, n % 60) } else { format!("{}:{:02}:{:02}", n / 3600, n % 3600 / 60, n % 60) }
}

/// Seconds since 1970, the clock file times are kept in
pub fn unix_now() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u32::try_from(d.as_secs()).unwrap_or(u32::MAX))
}

/// How long ago something changed, as people say it: "Today", "3 weeks ago",
/// "2 years ago". `None` when the time is unknown
pub fn ago(modified: u32, now: u32) -> Option<String> {
    if modified == 0 {
        return None;
    }
    let days = now.saturating_sub(modified) / 86_400;
    let unit = |n: u32, one: &str| if n == 1 { format!("1 {one} ago") } else { format!("{n} {one}s ago") };
    Some(match days {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        2..=6 => unit(days, "day"),
        7..=29 => unit(days / 7, "week"),
        30..=364 => unit((days / 30).min(11), "month"),
        _ => unit(days / 365, "year"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_ages() {
        let day = 86_400;
        let now = 2_000_000_000;
        assert_eq!(ago(0, now), None);
        assert_eq!(ago(now - 60, now).as_deref(), Some("Today"));
        assert_eq!(ago(now + 60, now).as_deref(), Some("Today"));
        assert_eq!(ago(now - day, now).as_deref(), Some("Yesterday"));
        assert_eq!(ago(now - 9 * day, now).as_deref(), Some("1 week ago"));
        assert_eq!(ago(now - 100 * day, now).as_deref(), Some("3 months ago"));
        assert_eq!(ago(now - 362 * day, now).as_deref(), Some("11 months ago"));
        assert_eq!(ago(now - 800 * day, now).as_deref(), Some("2 years ago"));
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(bytes(0), "0 bytes");
        assert_eq!(bytes(999), "999 bytes");
        assert_eq!(bytes(1_500), "1.5 KB");
        assert_eq!(bytes(369_900_000_000), "369.9 GB");
        assert_eq!(bytes(51_700_000_000), "51.7 GB");
    }

    #[test]
    fn formats_counts() {
        assert_eq!(count(7), "7");
        assert_eq!(count(1234567), "1,234,567");
    }

    #[test]
    fn formats_durations() {
        use std::time::Duration;
        assert_eq!(duration(Duration::from_millis(840)), "0.8 s");
        assert_eq!(duration(Duration::from_millis(9_940)), "9.9 s");
        assert_eq!(duration(Duration::from_millis(9_960)), "10 s");
        assert_eq!(duration(Duration::from_secs(192)), "3 min 12 s");
        assert_eq!(duration(Duration::from_secs(3_840)), "1 h 4 min");
        assert_eq!(clock(Duration::from_secs(7)), "0:07");
        assert_eq!(clock(Duration::from_secs(3_725)), "1:02:05");
    }
}
