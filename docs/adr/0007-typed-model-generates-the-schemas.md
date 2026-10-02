# ADR-0007: The typed model generates the schemas; node properties stay JSON maps at runtime

**Status:** proposed · **Date:** 2026-10-02

## Context

SPEC §3.1 makes the typed model in `scaena-core` the truth and `deck.json` its interchange form. Until PLAN 1.1 the model typed only the document's skeleton. Node properties were an untyped map, and the two JSON schemas were written by hand. The hand-written schemas disagreed with SPEC in eight places, found by the authorability spike (PLAN 0.13) and by PLAN 1.1's diff:

| | hand-written schema | SPEC |
|---|---|---|
| a property of another node type (`role` on a chart) | accepted: one flat property list for every type | §3.3 gives each type its properties |
| `null` in a delta | rejected | §2.2: deletes the property |
| part of an object in a delta (`at: { "in": null, "col": [1, 6] }`, `y: { "format": … }`) | rejected: objects had their required keys | §2.2: merges one level |
| chart `axes` settings | rejected: the name was taken by text `axes`; `axesSpec` held the place | §3.7's example sets them |
| `mesh` params out of range, or unknown | accepted | §3.8: an error naming the node |
| `labels.show: "bogus"` | accepted | §3.7: `all`, `ends`, `none` |
| image `fit: "cover"` | rejected: a text node's `fit` values applied to images too, so no image `fit` validated | §3.3 |
| a layout slot's `align` | any object | §3.4's values |

Node properties have to stay generic at runtime. Tracking merges them one level and deletes with `null` (§2.2), JSON Patch addresses them by path (PLAN 1.16), and the CRDT stores maps (PLAN 1.23).

## Decision

1. **Typed views over JSON maps.** `scaena-core::model` holds one struct per node type (`TextNode` … `GroupNode`, `deny_unknown_fields`), the theme, and the values they are made of. The document keeps a node's properties as an ordered JSON map, and `Node::typed()` gives its type's view. Tracking, patching, and the CRDT work on maps; validation and, stage by stage, the engine read views. A test holds the model to every document in the repository: each node, in every state that shows it, has a view that writes back what it read.
2. **Generated schemas.** `docs/schema/deck.schema.json` and `theme.schema.json` are generated from the model by `schemars` (JSON Schema 2020-12), and a test holds the committed files to the generator. `just bless` regenerates them, and nobody edits them by hand. A post-pass makes the output say what the format says:
   - no `null` on optional properties, and no numeric formats;
   - doc comments as plain text;
   - the properties every node type shares written once (`NodeProps`, joined to each type by `$ref` with `unevaluatedProperties: false`);
   - one keyword order, printed compactly.
3. **Deltas derived, not declared.** `StateDelta` is derived from the node types. It holds every property any type has, in every form it takes, or `null`. An object value merges one level, so each object form gets a `…Delta` definition with every key optional and nullable, and a map's values nullable. A property a node type gains is one a delta may set, with no second list to keep in step. A delta carries no `type`, so whether a property belongs to the node's type is checked on the resolved state (PLAN 1.2).
4. **Params typed per kind.** A shader's `params` are typed by `if`/`then` on `kind`: `mesh` takes `MeshParams`, and the other kinds stay numbers by name until PLAN 1.10 types them.
5. **The deck format goes to 0.2; the theme format stays 0.1.**
   - Deck: chart `axes` settings are new and the `axesSpec` placeholder is gone. The schema also stops accepting what SPEC never allowed (the table above).
   - Theme: its fields did not change, and slot `align` is now checked against §3.4's values.
   - Each version is a constant in `scaena-core` (`FORMAT_VERSION`, `THEME_FORMAT_VERSION`). The schema's `$id` and version pattern come from it.
   - Before 1.0 a minor bump may break (semver item 4), so PLAN's "minor for additive, major for breaking" applies from 1.0. Every document in the repository moved to 0.2 in the same change. The authorability spike's history snapshots stay at 0.1 as the record of what the agents wrote.

## Consequences

- **+** A property is added in one place, the model. The schema, the delta, and the typed view follow, and the schema cannot drift from the code.
- **+** The schema is as strict as SPEC: another type's property, a mesh param out of range, a label policy that does not exist, a slot alignment that does not exist.
- **+** Deltas validate as SPEC §2.2 writes them, `null` and partial objects included. The authorability spike's edit (a) needed `in: null`, which the hand-written schema rejected.
- **−** The deck schema is longer: 1,171 lines against 470. It says more: nine node types instead of one property list, and the delta forms.
- **−** The post-pass in `model/mod.rs` leans on how `schemars` shapes its output, so an upgrade can break it. The golden test catches that. A fuzz run found the compaction exact: 20,000 mutants of the corpus got the same verdict from the compact schema as from the uncompacted output. The only exception was the intended `null` in a map inside a delta.
- **−** `#[serde(flatten)]` loses `deny_unknown_fields`, so the shared node fields are written out by a macro in each type rather than flattened from one struct.
- **−** Two representations at runtime. The engine still reads maps; each Phase 1 stage moves to views as it is rebuilt (PLAN 1.6–1.12).
