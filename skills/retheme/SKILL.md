---
name: retheme
description: Apply a different Scaena theme to an existing deck and resolve what the new design system changes — lint delta, overrides that stop being theme-safe, slot names that no longer exist. Use when asked to "retheme", "apply the X theme", "make it look like our brand", or "swap the skin".
---

# retheme

A theme change is a pure re-render: swap `theme`, and the next frame is in the new skin (SPEC §2.5, ADR-0005). The work is in the delta.

1. Preview: `scaena theme <bundle> --apply <theme.json> --dry-run` (add `--json` for the findings). It checks the new theme against `docs/schema/theme.schema.json` and reports the delta in what `validate` and `lint` find: every name the deck uses that the new theme lacks (roles, layouts and slots, presets, durations, easings, springs, palettes, colors, families) is a new E102.
2. Fix what the delta reports, in the deck or by choosing another theme. Names are the swap contract: a palette or slot named for a theme (`dusk`) rather than its job (`ambient`) is the theme's problem, not the deck's.
3. Swap: the same command without `--dry-run`. It copies the theme into `themes/` and points the deck at it. When a saved bundle names fonts by content, families are matched to them by name, and each mapping is reported.
4. Look: `scaena inspect <bundle> --state <id> --resolved` shows each text node's new look (role, family, size, color). Render a state or two. Until PLAN 1.15, lint has no overflow or contrast rules (E100, E110/E111), so an empty delta proves nothing about fit or contrast: compare the new roles' sizes with the old ones, and check contrast by eye. A larger type scale wants tighter copy or `fit: shrink`, not overrides. A palette with less contrast wants a different color *role*, not a literal.
5. `overrides` are the enemy of re-theming: report every node with overrides (`--resolved` lists them; lint I402 names them) and propose removing them.
6. Report: what changed visually in one paragraph, the lint delta, and remaining overrides.
