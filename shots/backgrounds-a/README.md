# Backgrounds, lane A (PLAN 2.147): the contact sheet

Each shipped theme's seven background presets, full bleed behind the theme's own Title slide, drawn by `scaena render` with the CPU painter at t = 0, 2, and 4 s on the shader's clock (the state's hold), at 960 × 540 and scaled to 640 × 360 here.

| Theme | Sheet |
|---|---|
| Dusk | [dusk.png](dusk.png) |
| Daybreak | [daybreak.png](daybreak.png) |
| Ember | [ember.png](ember.png) |
| Salon | [salon.png](salon.png) |

Rows: `backdrop` (mesh) and `texture` (noise), as they shipped; then the new `aurora` (mesh), `lagoon` (caustics), `sweep` (conic gradient), `haze` (noise), and `dust` (particles). `*-lagoon-t*.png` are the caustics frames at full 960 × 540; `torture-caustics.png` is torture case 70's golden.

Second round (after the parent's review): backgrounds capped by the theme's ink, which title and body text are set in, not by the accent kicker; lagoon as thin, uneven filaments brightest where its two layers cross; sweep's center beyond the top-left corner. Each slide is drawn as lint's own contrast fixes leave it: a kicker in the accent or a lede in the muted color may now be set in the ink over a background, as lane B's background choice will set it.

Third round: on Dusk and Ember, lagoon's brightest filament stands out from its water by at most 3:1 (WCAG's contrast), its water a touch lighter, so no line competes with the title; Daybreak and Salon unchanged.

Contrast, `scaena lint` on each theme's Title and Section slides over each preset: no title or body text needs a fix, and after lint's fixes every preset in every theme has no E110 or E111.
