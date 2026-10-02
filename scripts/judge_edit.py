#!/usr/bin/env python3
"""Judge one edit of an authored deck (PLAN 0.13).

    scripts/judge_edit.py <bundle> <prev> <cur> [--cli target/debug/scaena]

`<bundle>/history/<step>.deck.json` holds the deck after each edit, next to the
bundle's `data/` and `themes/`. For the edit from <prev> to <cur>:

- `validate` and `lint --json` on <cur>;
- whether every theme name <cur> uses exists in its theme: slots per state layout
  (as `inspect` resolves them), roles, presets, durations, springs, shader
  palettes, data scales. Lint cannot say yet (E102 for theme names is PLAN 1.6);
- the structural delta (`scripts/deck_delta.py`);
- which states' resolved snapshots (`scaena inspect --json`) changed, and how.
"""
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def bundle(src, step, into):
    """A bundle around one snapshot: the deck, with links to the bundle's data and themes."""
    d = into / step
    d.mkdir()
    for sub in ('data', 'themes'):
        if (src / sub).exists():
            (d / sub).symlink_to((src / sub).resolve())
    (d / 'deck.json').write_bytes((src / 'history' / f'{step}.deck.json').read_bytes())
    return d / 'deck.json'


def run(cli, *args):
    p = subprocess.run([str(cli), *map(str, args)], capture_output=True, text=True)
    return p.returncode, p.stdout.strip(), p.stderr.strip()


def snapshots(cli, deck):
    code, out, err = run(cli, 'inspect', deck, '--json')
    if code:
        sys.exit(f'inspect {deck}: exit {code}: {err}')
    return {s['state_id']: s for s in json.loads(out)}


def missing_theme_names(deck_path, snaps):
    """Theme names the deck uses that its theme does not define."""
    deck = json.loads(deck_path.read_text())
    theme = deck['theme']
    if isinstance(theme, str):
        theme = json.loads((deck_path.parent / theme).read_text())
    layouts = theme.get('layouts', {})
    roles = set(theme.get('type', {}).get('roles', {}))
    motion = theme.get('motion', {})
    presets, springs = set(motion.get('presets', {})), set(motion.get('springs', {}))
    durations = set(motion.get('durations', {}))
    palettes = set(theme.get('shaders', {}).get('palettes', {}))
    scales = set(theme.get('tokens', {}).get('data', {}))
    missing = set()

    def need(kind, name, where, have):
        if isinstance(name, str) and name not in have:
            missing.add(f'{kind} `{name}` ({where})')

    def props(p, where):
        need('role', p.get('role'), where, roles)
        need('palette', p.get('palette'), where, palettes)
        for enc in ('color', 'series'):
            if isinstance(p.get(enc), dict):
                need('scale', p[enc].get('scale'), where, scales)

    for nid, n in deck.get('nodes', {}).items():
        props(n, f'node {nid}')
    for s in deck.get('states', []):
        where = f"state {s['id']}"
        need('layout', s.get('layout'), where, set(layouts))
        if isinstance(s.get('transition'), str):
            need('transition', s['transition'], where, durations)
        for nid, p in (s.get('props') or {}).items():
            props(p, f'{where} / {nid}')
        for c in s.get('choreography') or []:
            for k in ('enter', 'exit', 'emphasis'):
                need('preset', c.get(k), f'{where} choreography', presets)
            need('spring', c.get('spring'), f'{where} choreography', springs)
    # Slots as resolved: a node's slot must exist in the layout of every state that shows it.
    for sid, snap in snaps.items():
        slots = set(layouts.get(snap['layout'], {}).get('slots', {})) | {'canvas', 'grid'}
        for nid, p in snap['nodes'].items():
            if isinstance(p.get('at'), dict) and 'in' in p['at']:
                need('slot', p['at']['in'], f'{sid} / {nid}', slots)
    return sorted(missing)


def node_delta(a, b):
    out = []
    for n in list(a) + [n for n in b if n not in a]:
        if n not in b:
            out.append(f'-{n}')
        elif n not in a:
            out.append(f'+{n}')
        elif a[n] != b[n]:
            keys = sorted(k for k in set(a[n]) | set(b[n]) if a[n].get(k) != b[n].get(k))
            out.append(f"{n}.{{{', '.join(keys)}}}")
    return out


def main(argv):
    cli = ROOT / 'target/debug/scaena'
    if '--cli' in argv:
        i = argv.index('--cli')
        cli = Path(argv[i + 1])
        argv = argv[:i] + argv[i + 2:]
    if len(argv) != 3:
        sys.exit(__doc__)
    src, prev, cur = Path(argv[0]), argv[1], argv[2]
    with tempfile.TemporaryDirectory() as tmp:
        a, b = bundle(src, prev, Path(tmp)), bundle(src, cur, Path(tmp))
        for cmd in ('validate', 'lint'):
            code, out, err = run(cli, cmd, b, '--json')
            print(f'{cmd} {cur}: exit {code}: {out or err}')
        sa, sb = snapshots(cli, a), snapshots(cli, b)
        missing = missing_theme_names(b, sb)
        print(f"theme names: {'all resolve' if not missing else 'MISSING ' + '; '.join(missing)}")
        delta = [sys.executable, str(ROOT / 'scripts/deck_delta.py')]
        print(subprocess.run([*delta, src / 'history' / f'{prev}.deck.json', src / 'history' / f'{cur}.deck.json'],
                             capture_output=True, text=True).stdout.strip())
        changed = [s for s in sa if s in sb and sa[s] != sb[s]]
        print(f"snapshots: {len([s for s in sa if s in sb and sa[s] == sb[s]])} unchanged; "
              f"changed {', '.join(changed) or 'none'}; added {', '.join(s for s in sb if s not in sa) or 'none'}; "
              f"removed {', '.join(s for s in sa if s not in sb) or 'none'}")
        for s in changed:
            x, y = sa[s], sb[s]
            bits = [f'{k}: {x.get(k)!r} → {y.get(k)!r}' for k in ('slide_id', 'layout', 'entered', 'exited')
                    if x.get(k) != y.get(k)]
            print(f'  {s}: ' + '; '.join(bits + node_delta(x['nodes'], y['nodes'])))


if __name__ == '__main__':
    main(sys.argv[1:])
