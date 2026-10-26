/// Formats a byte count the way Finder does (decimal units).
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

/// Splits a formatted size into its number and unit, for typographic emphasis.
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
