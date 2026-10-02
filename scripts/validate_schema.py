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
# The validation fixtures' clean decks and their theme (PLAN 1.2); the triggers break on purpose.
targets += [(str(p.relative_to(root)), deck_schema) for p in sorted((root / 'tests/lint').glob('*/clean.deck.json'))]
targets += [('tests/lint/theme.json', theme_schema)]
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
# Edits the schemas must reject, and edits they must accept: one mutation of the example deck each.
def mutant(f):
    d = json.load(open(root / 'docs/examples/revenue.deck.json')); f(d); return d
def delta(d, node):
    return d['states'][1].setdefault('props', {}).setdefault(node, {})
reject = {
    'an unknown node prop': lambda d: d['nodes']['title'].update(bogus=1),
    'an unknown state prop': lambda d: delta(d, 'title').update(bogus=1),
    'a bad chart kind': lambda d: d['nodes']['rev'].update(kind='pie3d'),
    "another node type's prop": lambda d: d['nodes']['rev'].update(role='body'),
    'a label policy that does not exist': lambda d: d['nodes']['rev'].update(labels={'show': 'bogus'}),
    'mesh params out of range': lambda d: d['nodes'].update(bg={'type': 'shader', 'kind': 'mesh', 'params': {'points': 40}}),
    'a mesh param that does not exist': lambda d: d['nodes'].update(bg={'type': 'shader', 'kind': 'mesh', 'params': {'zoom': 1}}),
    'null on a node': lambda d: d['nodes']['title'].update(alt=None),
}
accept = {
    'null deleting a prop in a delta': lambda d: delta(d, 'title').update(alt=None),
    'null deleting one key of an object in a delta': lambda d: delta(d, 'title').update(at={'in': None, 'col': [1, 6]}),
    'part of an encoding in a delta': lambda d: delta(d, 'rev').update(y={'format': '$,.0f'}),
    'chart axes settings': lambda d: d['nodes']['rev'].update(axes={'x': {'gridlines': False}}),
    'an image that covers its box': lambda d: d['nodes'].update(img={'type': 'image', 'src': 'assets/x.png', 'fit': 'cover'}),
}
for name, f in reject.items():
    assert list(Draft202012Validator(deck_schema).iter_errors(mutant(f))), f"schema failed to reject {name}"
for name, f in accept.items():
    errs = list(Draft202012Validator(deck_schema).iter_errors(mutant(f)))
    assert not errs, f"schema rejected {name}: {errs[0].message[:200]}"
bad = json.load(open(root / 'docs/examples/themes/dusk.theme.json'))
next(iter(bad['layouts'].values()))['slots'] = {'x': {'align': {'x': 'sideways'}}}
assert list(Draft202012Validator(theme_schema).iter_errors(bad)), "schema failed to reject a slot alignment that does not exist"
print("negative and positive checks: pass")
sys.exit(0 if ok else 1)
