import json, sys
from pathlib import Path
from jsonschema import Draft202012Validator
root = Path(sys.argv[1] if len(sys.argv) > 1 else '.')
deck_schema = json.load(open(root / 'docs/schema/deck.schema.json'))
theme_schema = json.load(open(root / 'docs/schema/theme.schema.json'))
Draft202012Validator.check_schema(deck_schema); Draft202012Validator.check_schema(theme_schema)
# Every example, plus every fixture and benchmark bundle under tests/ (deck.json and its theme.json).
targets = [('docs/examples/revenue.deck.json', deck_schema), ('docs/examples/themes/dusk.theme.json', theme_schema)]
# The deck the authorability spike's agents wrote (PLAN 0.13), and the themes it moved between.
targets += [('docs/examples/authorability/deck.json', deck_schema)]
targets += [(str(p.relative_to(root)), theme_schema) for p in sorted((root / 'docs/examples/authorability/themes').glob('*.theme.json'))]
for bundle in sorted((root / 'tests/fixtures').glob('*.scaena')) + sorted((root / 'tests/bench').glob('*.scaena')):
    targets.append((str((bundle / 'deck.json').relative_to(root)), deck_schema))
    if (bundle / 'theme.json').exists():
        targets.append((str((bundle / 'theme.json').relative_to(root)), theme_schema))
ok = True
for name, sch in targets:
    inst = json.load(open(root / name, encoding='utf-8'))
    errs = sorted(Draft202012Validator(sch).iter_errors(inst), key=lambda e: list(e.path))
    if errs:
        ok = False; print(f"{name}: {len(errs)} error(s)")
        for e in errs[:20]: print("  -", "/".join(map(str, e.path)), ":", e.message[:200])
    else: print(f"{name}: valid")
# negative checks: schema must reject obvious mistakes
bad = json.load(open(root / 'docs/examples/revenue.deck.json'))
bad['nodes']['title']['bogus'] = 1
assert list(Draft202012Validator(deck_schema).iter_errors(bad)), "schema failed to reject unknown node prop"
bad = json.load(open(root / 'docs/examples/revenue.deck.json'))
bad['states'][1]['props']['title']['bogus'] = 1
assert list(Draft202012Validator(deck_schema).iter_errors(bad)), "schema failed to reject unknown state prop"
bad = json.load(open(root / 'docs/examples/revenue.deck.json'))
bad['nodes']['rev']['kind'] = 'pie3d'
assert list(Draft202012Validator(deck_schema).iter_errors(bad)), "schema failed to reject bad chart kind"
print("negative checks: pass")
sys.exit(0 if ok else 1)
