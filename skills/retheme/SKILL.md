---
name: retheme
description: Apply a different Scaena theme to an existing deck and resolve what the new design system changes, through the `scaena` CLI or its MCP server. Covers the lint delta, overrides that stop being theme-safe, and slot names that no longer exist. Use when asked to "retheme", "apply the X theme", "make it look like our brand", or "swap the skin".
---

# retheme

A theme change is a pure re-render: swap `theme`, and the next frame is in the new skin (SPEC §2.5, ADR-0005). The work is in the delta.

| To | CLI | MCP |
|---|---|---|
| preview the swap | `scaena theme <bundle> --apply <theme.json> --dry-run --json` | `theme_apply` with `dry_run` |
| swap | the same, without `--dry-run` | `theme_apply` |
| see each text node's new look | `scaena inspect <bundle> --state <id> --resolved` | `deck_inspect` with `resolved` |
| check | `scaena lint <bundle> --json` | `deck_lint` |
| look | `scaena render <bundle> --state <id> --out frame.png` | `deck_render` |

1. **Preview.**
   - The dry run checks the new theme against `docs/schema/theme.schema.json` (`scaena://schema/theme`). It reports the delta in what validation and lint find, as `added` and `removed`.
   - Every name the deck uses that the new theme lacks is a new E102: roles, layouts and slots, presets, durations, easings, springs, palettes, colors, and families.
2. **Fix what the delta reports**, in the deck or by choosing another theme.
   - Names are the swap contract.
   - A palette or slot named for a theme (`dusk`) rather than its job (`ambient`) is the theme's problem, not the deck's.
3. **Swap.**
   - Run it again without the dry run. It copies the theme into `themes/` and points the deck at it.
   - When a saved bundle names fonts by content, families are matched to them by name, and each mapping is reported (`mapped`).
4. **Read the layout findings in the delta.** Lint lays every state out in the new theme, so the delta also says what the new type and colors break:
   - **E100:** text that no longer fits. A larger type scale wants tighter copy (the `tighten-copy` skill) or `fit: shrink`, not overrides.
   - **E110 and E111:** text whose contrast with what is painted behind it drops below WCAG's line. A palette with less contrast wants a different color *role*, not a literal.
   - **W210:** density, if the theme's `maxWordsPerState` is lower.
   - **W221:** a role that snaps to the new theme's baseline grid at a leading off it. Its finding points into the theme file: the theme's to fix, not the deck's.
   - **W310:** chart labels that now collide.
   - **W320 and W321:** motion past the new theme's limits.
5. **Look.**
   - `--resolved` shows each text node's new role, family, size, and color.
   - Render a state of each layout the deck uses. An empty delta means nothing lint checks got worse; it does not mean the deck looks right.
6. **Overrides are the enemy of re-theming.**
   - Report every node with overrides: `--resolved` lists them, and lint I402 names them.
   - Propose removing them.
7. **Report:** what changed visually, in one paragraph; the lint delta; and remaining overrides.
