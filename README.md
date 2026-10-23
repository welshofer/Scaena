# Slide transitions through shapes that grow (PLAN 2.149)

For Jay's review. The trails example on Dusk, its title slide (`cover`) into its agenda, each transition over a second, linearly. The agenda's own builds are taken out here so its words show through the shapes.

- `contact-sheet.png`: Circle, Circle Wave, Diamond, Diamond Wave, Radial at 0, 25, 50, 75, and 100%, by `scaena render --t`.
- `frames/`: each frame at 960 × 540.
- `<style>.mp4`: `scaena export --format mp4 --states cover,agenda --fps 30 --size 960x540`.

Each style keeps its default direction: the waves start at the right edge, and the radial starts straight up and turns clockwise.
