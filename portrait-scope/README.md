# 9:16 portrait layouts: scope

A prototype, not the build. `derive.py` derives a 9:16 arrangement for a theme's 16:9 layouts on a
portrait grid of 12 columns by 24 rows (the landscape margins turned):

- Slots 10 or more columns wide, and slots in the foot rows (11–12), keep their place, rows doubled.
- The other slots group into column bands. The bands stack top to bottom in the body's rows, two
  across from four bands on. Each slot maps into its band's share in proportion.

`sheet.py` draws each layout named three ways: 16:9, 9:16 as the theme ships, and 9:16 derived.

    python3 portrait-scope/sheet.py dusk out.png two-columns three-columns scorecard stat team-four timeline swot pricing

`sheet-dusk.png` is that run. Two layouts in it need hand-made arrangements:

- scorecard and timeline keep a full-width rule between their figures and labels, which now cuts
  across the 2×2;
- swot stacks in column order (S, O, W, T), not reading order.

The plan, the cost, and the open questions are in the coordinator's thread.
