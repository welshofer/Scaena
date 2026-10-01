# Scaena — Manifesto

*Why this exists, what we believe, and what we refuse to build.*

---

## The goal

Build the presentation tool we would design today if the slide had never been invented.

Not a better PowerPoint. Not a prettier Keynote. A tool whose primitives are the things a presentation is actually made of — objects with identity, states on a timeline, typography that is correct by construction, data that stays data, motion that is a property rather than an effect — and whose first author is as likely to be an agent as a person.

The output should be indistinguishable from work done by a world-class design studio, and it should be reachable by someone with a CSV, a paragraph, and twenty minutes.

---

## What we believe

### 1. The slide was a mistake we inherited, not a law of nature.

Slides are film frames: discrete, independent, dumb. Everything that was ever good in presentation software — Magic Move, Morph, builds, "appear on click" — was an attempt to smuggle continuity back into a model that had thrown it away. We start from continuity. A deck is one scene over time. "Slides" are bookmarks.

### 2. Identity is the primitive.

An object that exists on slide 4 and slide 5 is the same object. That sentence is the whole animation system — and most of everything else. When identity is the primitive, transitions are not a menu; they are what the object does between two states. Patches become local. Diffs become meaningful. Data updates flow to the mark that changed. A re-theme keeps every object where the argument put it. Lighting consoles figured this out in the 1980s. Cue 12 goes. The unchanged channels track; the changed ones crossfade. We are building a lighting console for ideas.

### 3. Typography is the product.

Most presentation software treats text as a rectangle with a font name. We treat it as the thing people look at. Real shaping. Optical sizes. Balanced headlines, pretty paragraphs, no widows. Cap-height alignment, not bounding boxes. Hanging punctuation. Tabular figures in charts without asking. If the type is wrong, nothing else matters, and if the type is right, almost everything else can be plain.

### 4. Semantics over pixels.

A headline is a `headline`. A chart's colors come from the data palette. A margin is `space.4`. Authors — human or machine — say what things *are*; the design system decides what they *look like*. This is how re-theming becomes a live, animated, lossless operation instead of a weekend. It is also how an agent produces beautiful work: by being prevented from producing ugly work.

### 5. The document is text, and text is the truth.

Diffable. Patchable. Versioned. Readable by a person at 2 a.m. and by an agent at 2 a.m. the next day. A deck that can be reasoned about is a deck that can be improved, checked, regenerated from new data, or projected into another medium. While you edit, a CRDT keeps the history; what it writes to disk is this text, and nothing else is authoritative. Binary blobs are where ideas go to die.

### 6. One engine, pixel-identical everywhere.

The browser, the Mac, the PDF, the video frame, the thumbnail an agent inspects: same glyph positions, same geometry, same colors. Rendering is a pure function of the document and time. Determinism is not an aspiration; it is a test that fails the build.

### 7. The agent is a user, not a feature.

Everything a UI can do is reachable by an agent, with results it can act on: errors that carry fixes, renders it can look at, patches that apply atomically. Authoring is a loop — make, lint, look, fix — and the loop is designed for whoever runs it. The best "AI feature" is a tool that is honest with the machine.

### 8. The deck is a projection of the story.

Beneath the states is a spine: sections, beats, claims, evidence, notes. The deck is one rendering of that spine. The infographic, the PDF, the motion piece, and the podcast are others. Export is not a conversion; it is another projection of the same truth. Narrative is a contract, not a byproduct.

### 9. Local-first. Yours.

A deck is a bundle on your disk. It opens offline, from a USB stick, in ten years. No account, no server, no telemetry. If you want to use a model, you bring your own key or your own agent. We may add sync when there are people to sync with. We will never add a lock-in.

### 10. Constraint is the gift.

Fewer node types, one grid, named motion, a fixed set of shaders, a small chart grammar. Every constraint is chosen so that the default output is excellent and the escape hatch is loud. We would rather ship four exceptional themes than four hundred mediocre ones.

---

## What we refuse

- **PPTX and Keynote inside the model.** Compatibility never touches the document model or the engine: no import filter, no shared abstractions, no concessions to the slide. A lossy, one-way, external projection — Scaena → a dumb PPTX, information deliberately thrown away, nothing flowing back — would be no different from PDF export. We are not building it, and we are not planning it. But the prohibition is architectural, not religious.
- **WYSIWYG first.** Direct manipulation is a late feature that emits patches, not the foundation.
- **Effects.** No entrance-effect gallery. No "bounce in from the left." Motion is a property on a timeline, with named springs and presets that a theme owns.
- **Arbitrary code in documents.** No user shaders, no scripts, no plugins inside the engine. Decks are data.
- **A backend we don't need yet.** Nothing in v1 may assume a server exists.
- **Features that only demo well.** If it doesn't survive a Tuesday-morning board deck, it doesn't ship.

---

## How we work

- Determinism is tested, not hoped for.
- Layout runs once per state; frames only sample. Nothing sneaks layout back into the frame loop.
- Decisions get an ADR; reversals get another.
- Every lint rule ships with the deck that triggers it.
- Phase 0 is adversarial: we try to break the typography before we build anything on it.
- The spike comes before the editor. The agent comes before the UI. The Mac comes after the web.
- When the typography is wrong, nothing else is right.

---

*Scaena: Latin, stage. The thing you build so the performance can happen.*
