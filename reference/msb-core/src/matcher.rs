//! Tolerant serial/VPID matching + Regel-S semantics (normative, see AGENTS.md §6).
//!
//! - Canonical serials match `^[A-Za-z0-9]{4,32}$` (no hyphen).
//! - Derivation splits at the first `-` (suffix optional; no hyphen → whole string).
//! - Discovery matching is tolerant: VPID form `i:<PEN>_u:<VPID>` and `-suffix` forms
//!   resolve to the portfolio serial.
//! - Anti-enumeration: a query shorter than a portfolio serial returns no match (never a hit).

/// True if `s` is a canonical Steuerbox serial: 4..=32 chars, `[A-Za-z0-9]` only (no hyphen).
pub fn is_canonical(s: &str) -> bool {
    (4..=32).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// HEMS serial format (real-world: `1U0022_CO-4206`): 4..=64 chars of
/// `[A-Za-z0-9_-]`. NEVER validated with the Steuerbox canonical rules —
/// two serials, never mixed.
pub fn is_hems_serial(s: &str) -> bool {
    (4..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Regel-S derivation: split at the first `-`; no hyphen → the whole string.
pub fn regel_s_base(serial: &str) -> &str {
    match serial.find('-') {
        Some(i) => &serial[..i],
        None => serial,
    }
}

/// Extract the VPID part from `i:<IANA-PEN>_u:<VPID>`; non-VPID input passes through.
pub fn vpid_to_serial(query: &str) -> &str {
    if let Some(rest) = query.strip_prefix("i:") {
        if let Some(idx) = rest.find("_u:") {
            return &rest[idx + 3..];
        }
    }
    query
}

/// Tolerant discovery against a portfolio: returns the canonical serial on a hit.
///
/// Steps: VPID normalization → Regel-S split at first `-` → exact portfolio match.
/// Anything else (prefixes, unknown serials) is a miss — this is what makes a
/// shorter-than-portfolio query answer 404 (anti-enumeration).
pub fn discover<'a>(portfolio: &'a [String], query: &str) -> Option<&'a str> {
    let candidate = regel_s_base(vpid_to_serial(query.trim()));
    portfolio
        .iter()
        .find(|s| s.as_str() == candidate)
        .map(|s| s.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portfolio() -> Vec<String> {
        ["SNTEST0001", "SNTEST0002", "AB12"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn hems_serial_format_allows_underscore_and_hyphen() {
        // real-world leaflet serial
        assert!(is_hems_serial("1U0022_CO-4206"));
        assert!(is_hems_serial("SNTEST_CO-0001"));
        assert!(!is_hems_serial("ab")); // too short
        assert!(!is_hems_serial("a b c d")); // space
                                             // the Steuerbox rules are DIFFERENT on purpose:
        assert!(!is_canonical("1U0022_CO-4206")); // underscore → not a Steuerbox serial
    }

    #[test]
    fn canonical_rules() {
        assert!(is_canonical("SNTEST0001"));
        assert!(is_canonical("AB12"));
        assert!(!is_canonical("SNTEST0001-A1")); // hyphen
        assert!(!is_canonical("SN")); // too short
        assert!(!is_canonical("ABC")); // too short
        assert!(!is_canonical(&"A".repeat(33))); // too long
        assert!(!is_canonical("SNTE ST01")); // space
    }

    #[test]
    fn regel_s_splits_at_first_hyphen() {
        assert_eq!(regel_s_base("SNTEST0001-A1-B2"), "SNTEST0001");
        assert_eq!(regel_s_base("SNTEST0001"), "SNTEST0001");
        assert_eq!(regel_s_base("-suffix-only"), "");
    }

    #[test]
    fn vpid_parsing() {
        assert_eq!(vpid_to_serial("i:9901_u:SNTEST0001"), "SNTEST0001");
        assert_eq!(vpid_to_serial("i:9901_u:SNTEST0001-X1"), "SNTEST0001-X1");
        assert_eq!(vpid_to_serial("SNTEST0001"), "SNTEST0001");
        assert_eq!(vpid_to_serial("i:9901_novpid"), "i:9901_novpid");
    }

    #[test]
    fn tolerant_matching_hits() {
        let p = portfolio();
        assert_eq!(discover(&p, "SNTEST0001"), Some("SNTEST0001"));
        assert_eq!(discover(&p, "SNTEST0001-A1"), Some("SNTEST0001"));
        assert_eq!(discover(&p, "SNTEST0001-A1-B2"), Some("SNTEST0001"));
        assert_eq!(discover(&p, "i:9901_u:SNTEST0002-X1"), Some("SNTEST0002"));
        assert_eq!(discover(&p, "AB12"), Some("AB12"));
    }

    #[test]
    fn anti_enumeration_shorter_query_misses() {
        let p = portfolio();
        // prefix of a portfolio serial → must NOT be a hit (404 on discovery)
        assert_eq!(discover(&p, "SNTEST"), None);
        assert_eq!(discover(&p, "SNTEST000"), None);
        assert_eq!(discover(&p, "S"), None);
        // unknown full-length → miss
        assert_eq!(discover(&p, "SNTEST9999"), None);
        // longer than portfolio but unknown base → miss
        assert_eq!(discover(&p, "SNTEST0001ZZZ-EXTRA"), None);
    }
}
