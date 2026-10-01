//! Node, state, and beat identifiers (SPEC §3.2): `^[a-z][a-z0-9_-]{0,63}$`.

/// Returns `true` if `s` is a valid Scaena id.
pub fn is_valid_id(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return false };
    if !first.is_ascii_lowercase() {
        return false;
    }
    if s.len() > 64 {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_slugs() {
        for ok in ["a", "title", "rev-chart", "q3_rev", "a1"] {
            assert!(is_valid_id(ok), "{ok}");
        }
    }

    #[test]
    fn rejects_bad_ids() {
        for bad in ["", "Title", "1a", "a b", "-a", "émile", &"a".repeat(65)] {
            assert!(!is_valid_id(bad), "{bad}");
        }
    }
}
