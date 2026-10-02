---
name: retheme
description: Apply a different Scaena theme to an existing deck and resolve what the new design system changes — lint delta, overrides that stop being theme-safe, slot names that no longer exist. Use when asked to "retheme", "apply the X theme", "make it look like our brand", or "swap the skin".
---

# retheme

A theme change is a pure re-render: swap `theme`, and the next frame is in the new skin (SPEC §2.5, ADR-0005). The work is in the delta.

1. Confirm the target theme validates: `python3 scripts/validate_schema.py` covers examples; for a new theme, validate it against `docs/schema/theme.schema.json` first.
2. Record the baseline: `scaena lint <deck> --json > before.json`.
3. Swap: set `theme` in `deck.json` (PLAN 1.6 adds `scaena theme <deck> --apply`).
4. Check slot and preset names: every `at: { in: … }`, every `layout`, every `enter`/`exit`/`emphasis` preset, every `spring`, `palette`, and `scale` must exist in the new theme. Unknown names are E102 after PLAN 1.6; until then, grep the deck for them and compare with the theme's `layouts`, `motion.presets`, `motion.springs`, `shaders.palettes`.
5. `scaena lint <deck> --json > after.json` and diff. New E100 (overflow) means the new type scale is larger: prefer tightening copy or `fit: shrink` over overrides. New E110/E111 means the palette changed contrast: change the color *role* reference, not the literal. Until PLAN 1.15, lint has no overflow or contrast rules and does not read the theme, so an empty delta proves nothing: compare the new roles' sizes with the old, and check contrast by hand.
6. `overrides` are the enemy of re-theming: report every node with overrides (I402) and propose removing them.
7. Report: what changed visually in one paragraph, the lint delta, and remaining overrides.
