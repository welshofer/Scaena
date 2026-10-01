import json, sys
from jsonschema import Draft202012Validator
root = sys.argv[1] if len(sys.argv) > 1 else '.'
deck_schema = json.load(open(f'{root}/docs/schema/deck.schema.json'))
theme_schema = json.load(open(f'{root}/docs/schema/theme.schema.json'))
Draft202012Validator.check_schema(deck_schema); Draft202012Validator.check_schema(theme_schema)
ok = True
for name, inst, sch in [("docs/examples/revenue.deck.json", json.load(open(f'{root}/docs/examples/revenue.deck.json')), deck_schema),
                        ("docs/examples/themes/dusk.theme.json", json.load(open(f'{root}/docs/examples/themes/dusk.theme.json')), theme_schema)]:
    errs = sorted(Draft202012Validator(sch).iter_errors(inst), key=lambda e: list(e.path))
    if errs:
        ok = False; print(f"{name}: {len(errs)} error(s)")
        for e in errs[:20]: print("  -", "/".join(map(str, e.path)), ":", e.message[:200])
    else: print(f"{name}: valid")
# negative checks: schema must reject obvious mistakes
bad = json.load(open(f'{root}/docs/examples/revenue.deck.json'))
bad['nodes']['title']['bogus'] = 1
assert list(Draft202012Validator(deck_schema).iter_errors(bad)), "schema failed to reject unknown node prop"
bad = json.load(open(f'{root}/docs/examples/revenue.deck.json'))
bad['states'][1]['props']['title']['bogus'] = 1
assert list(Draft202012Validator(deck_schema).iter_errors(bad)), "schema failed to reject unknown state prop"
bad = json.load(open(f'{root}/docs/examples/revenue.deck.json'))
bad['nodes']['rev']['kind'] = 'pie3d'
assert list(Draft202012Validator(deck_schema).iter_errors(bad)), "schema failed to reject bad chart kind"
print("negative checks: pass")
sys.exit(0 if ok else 1)
