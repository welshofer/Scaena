# Review 1 — 2026-10-01 (initial architecture review)

External review of MANIFESTO / SPEC / PLAN at draft 0.1, before any engine code. Verdict: proceed; tighten eight things first. All eight were applied the same day.

| # | Ask | Applied where |
|---|---|---|
| 1 | Resolve canonical-document vs CRDT terminology | SPEC §1.2 principle 4, §3.1 "Authority"; CLAUDE.md invariant 10; MANIFESTO §5 |
| 2 | DSL round-trip is semantic, not source-preserving | SPEC §4 (guarantee stated as `compile(decompile(doc)) == doc`), §14 |
| 3 | Reconcile chart-kind scope between SPEC and PLAN | SPEC §3.7 normative v1 list; `deck.schema.json` enum; PLAN 1.9 |
| 4 | Make Phase 0 adversarial: typography torture deck | PLAN 0.2 (cases, kill criteria vs catalogue), gate 0 criterion 1; SPEC §15 benchmark B4 |
| 5 | Add an agent-authorability spike to gate 0 | PLAN 0.13, gate 0 criterion 7 |
| 6 | Make performance budgets per stage on named decks | SPEC §15 rewritten (B1–B4, stage table, cold vs warm, CPU vs GPU video) |
| 7 | Reserve a node-level communicative-semantic field | `semantic` in schema + SPEC §3.3; narrative lint family W420–W425 reserved in §7.5; example deck annotated |
| 8 | "No PPTX ever" → "PPTX never contaminates the model or engine" | MANIFESTO "What we refuse"; SPEC §1.3; PLAN working agreement 6; CLAUDE.md invariant 8 |

Also taken from the review without a numbered ask: "Identity is the primitive" promoted to SPEC principle 2 (second only to determinism); "layout once, animate geometry" promoted to SPEC principle 5 and MANIFESTO "How we work"; lint reframed as three families (mechanical / design / narrative) with the agent loop stated as generate → critique → repair → look.

Open thread from the review worth keeping in view: the spine may matter more than the deck (narrative compiler with several targets). Not broadened now; constraint discipline stands. Revisit at gate 1 with the authorability transcript in hand.
