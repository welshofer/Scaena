//! Where a link may go (PLAN 2.70, lift-up SEC-2). The deck's schema writes a link's `href` as a
//! web address, but a player opens a bundle without validating it, so every link the engine
//! draws and every link a reader hears is checked here once more: anything else draws and reads
//! as the words it is set on, linking nowhere.

use crate::displaylist::LinkTarget;
use serde_json::Value;

/// Whether `href` is a web address a reader may follow, as the schema writes one: `https:`,
/// `http:`, or `mailto:`, and no white space. Never `javascript:`, `file:`, `data:`, or an app's
/// own scheme, however a deck that skipped validation spells it.
pub fn followable(href: &str) -> bool {
    let rest = ["https://", "http://", "mailto:"].iter().find_map(|scheme| href.strip_prefix(scheme));
    rest.is_some_and(|rest| !rest.is_empty()) && !href.chars().any(char::is_whitespace)
}

/// Where a run's `link` goes: a web address that is [`followable`], or a state of the deck.
pub fn target(link: &Value) -> Option<LinkTarget> {
    match (link.get("href"), link.get("state")) {
        (Some(Value::String(href)), _) => followable(href).then(|| LinkTarget::Href(href.clone())),
        (_, Some(Value::String(state))) => Some(LinkTarget::State(state.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_addresses_are_followed() {
        for href in ["https://example.com/a", "http://example.com", "mailto:jay@example.com"] {
            assert!(followable(href), "{href}");
        }
        for href in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,<script>alert(1)</script>",
            "smb://host/share",
            "shortcuts://run-shortcut?name=x",
            "https://",
            "mailto:",
            "https://example.com/a b",
            " https://example.com",
            "HTTPS://example.com",
            "//example.com",
            "",
        ] {
            assert!(!followable(href), "{href}");
        }
    }

    #[test]
    fn a_link_that_is_not_followed_goes_nowhere() {
        assert_eq!(target(&serde_json::json!({ "href": "javascript:alert(1)" })), None);
        assert_eq!(target(&serde_json::json!({ "href": "javascript:alert(1)", "state": "a" })), None);
        assert_eq!(target(&serde_json::json!({ "state": "a" })), Some(LinkTarget::State("a".into())));
        assert_eq!(
            target(&serde_json::json!({ "href": "https://example.com" })),
            Some(LinkTarget::Href("https://example.com".into()))
        );
    }
}
