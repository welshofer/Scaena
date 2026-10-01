Add a lint rule to Scaena: $ARGUMENTS

1. Find the code in `docs/SPEC.md` §7.5. If it is not there, stop and propose the row (code, severity, family, rule) first.
2. Decide the crate: document-level (needs only the document and snapshots) → `crates/scaena-core/src/lint.rs`; layout-level (needs fonts/geometry) → `crates/scaena-engine/src/lint/`.
3. Implement a struct with `code()`, `severity()`, `check()`. Findings carry `path` (JSON pointer), `state`/`node` where applicable, a `hint`, and a `fix` only when the fix is safe and non-content.
4. Add fixtures `tests/lint/<CODE>/trigger.deck.json` (must trigger exactly this code) and `tests/lint/<CODE>/clean.deck.json` (must not), and a test that runs both.
5. Register the rule in the rule list, run `just check`, tick the PLAN task if one exists, commit referencing the code.
