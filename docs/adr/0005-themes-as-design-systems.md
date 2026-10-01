# ADR-0005: Themes are design systems; documents store roles, not values

**Status:** accepted · **Date:** 2026-10-01

## Context

Agents are bad at choosing pixel values and good at choosing roles. Re-theming must be live and must relayout, not just recolor. Slide masters failed at this because the content beneath them was never semantic.

## Decision

A theme is a design system (SPEC §3.6): tokens, typographic roles, layout templates with slots, motion presets and springs, data and shader palettes. Documents reference roles, slots, tokens, and presets. Raw literals are allowed only in an explicit `overrides` section (or are flagged by lint W300). The cascade is theme → node style → state props → overrides. Theme values are properties, so a theme change can be animated with the same morph machinery as a state transition.

## Consequences

- **+** `render(document, theme, …)`: swapping the theme is just another input; the next frame is in the new skin.
- **+** Agent output is well-typeset by construction; lint catches the rest.
- **+** Override counts make "theme-safe" a visible, enforceable property.
- **−** Expressiveness is deliberately constrained; escape hatches exist and are loud.
- **−** Themes are more work to author than a color palette; we ship a small number of excellent ones.
