# Backgrounds, lane A (PLAN 2.147): the contact sheet

Each shipped theme's seven background presets, full bleed behind the theme's own Title slide, drawn by `scaena render` with the CPU painter at t = 0, 2, and 4 s on the shader's clock (the state's hold), at 960 × 540 and scaled to 640 × 360 here.

| Theme | Sheet |
|---|---|
| Dusk | [dusk.png](dusk.png) |
| Daybreak | [daybreak.png](daybreak.png) |
| Ember | [ember.png](ember.png) |
| Salon | [salon.png](salon.png) |

Rows: `backdrop` (mesh) and `texture` (noise), as they shipped; then the new `aurora` (mesh), `lagoon` (caustics), `sweep` (conic gradient), `haze` (noise), and `dust` (particles). `*-lagoon-t*.png` are the caustics frames at full 960 × 540; `torture-caustics.png` is torture case 70's golden.

Contrast, `scaena lint` on each theme's Title and Section slides over each preset: every new preset passes E110/E111 in all four themes. `backdrop` and `texture` fail under the accent kicker on Dusk, Daybreak, and Ember, as they did before this change.
