#!/usr/bin/env python3
"""What an edit touched, between two versions of a deck.json (PLAN 0.13).

    scripts/deck_delta.py before.deck.json after.deck.json [--json]

Structural, not textual: formatting and key order do not count. Reports top-level
fields that changed; nodes and states added, removed, or changed, down to the props
a state sets per node; whether surviving nodes kept their types and states their
order; and possible identity breaks, a node removed while another of its type is
added in the same edit (a recreation rather than an edit).
"""
import json
import sys


def keyed_delta(a, b):
    """(added, removed, changed) keys between two dicts, in their own order."""
    return [k for k in b if k not in a], [k for k in a if k not in b], [k for k in a if k in b and a[k] != b[k]]


def changed_keys(x, y):
    return sorted(k for k in set(x) | set(y) if x.get(k) != y.get(k))


def delta(a, b):
    out = {}
    added, removed, changed = keyed_delta(a, b)
    out['top'] = {'added': added, 'removed': removed, 'changed': [k for k in changed if k not in ('nodes', 'states')]}

    na, nb = a.get('nodes', {}), b.get('nodes', {})
    added, removed, changed = keyed_delta(na, nb)
    out['nodes'] = {
        'added': added,
        'removed': removed,
        'changed': {k: changed_keys(na[k], nb[k]) for k in changed},
        'unchanged': len([k for k in na if k in nb and na[k] == nb[k]]),
        'types_kept': all(na[k].get('type') == nb[k].get('type') for k in na if k in nb),
        'recreated': sorted({na[r].get('type') for r in removed} & {nb[n].get('type') for n in added}),
    }

    sa = {s['id']: s for s in a.get('states', [])}
    sb = {s['id']: s for s in b.get('states', [])}
    added, removed, changed = keyed_delta(sa, sb)

    def state(x, y):
        keys = changed_keys(x, y)
        props = {}
        if 'props' in keys:
            pa, pb = x.get('props', {}), y.get('props', {})
            p_added, p_removed, p_changed = keyed_delta(pa, pb)
            props = {'added': p_added, 'removed': p_removed, 'changed': {n: changed_keys(pa[n], pb[n]) for n in p_changed}}
        return {'keys': keys, 'props': props} if props else {'keys': keys}

    out['states'] = {
        'added': added,
        'removed': removed,
        'changed': {k: state(sa[k], sb[k]) for k in changed},
        'unchanged': len([k for k in sa if k in sb and sa[k] == sb[k]]),
        'order_kept': [s for s in sa if s in sb] == [s for s in sb if s in sa],
    }
    return out


def report(d):
    lines = []
    top = d['top']
    for what in ('added', 'removed', 'changed'):
        if top[what]:
            lines.append(f"top-level {what}: {', '.join(top[what])}")
    for kind in ('nodes', 'states'):
        x = d[kind]
        parts = []
        if x['added']:
            parts.append(f"added {', '.join(x['added'])}")
        if x['removed']:
            parts.append(f"removed {', '.join(x['removed'])}")
        for k, v in x['changed'].items():
            if kind == 'nodes':
                parts.append(f"changed {k} ({', '.join(v)})")
            else:
                keys = [k for k in v['keys'] if k != 'props']
                props = v.get('props')
                if props:
                    bits = [f"+{n}" for n in props['added']] + [f"-{n}" for n in props['removed']]
                    bits += [f"{n}.{{{', '.join(ks)}}}" for n, ks in props['changed'].items()]
                    keys.append(f"props {' '.join(bits)}")
                detail = '; '.join(keys)
                parts.append(f"changed {k} ({detail})")
        parts.append(f"{x['unchanged']} unchanged")
        lines.append(f"{kind}: " + '; '.join(parts))
    lines.append(f"identity: node types kept: {d['nodes']['types_kept']}; state order kept: {d['states']['order_kept']}; "
                 f"recreations: {', '.join(d['nodes']['recreated']) or 'none'}")
    return '\n'.join(lines)


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if a != '--json']
    if len(args) != 2:
        sys.exit(__doc__)
    d = delta(*(json.load(open(p)) for p in args))
    print(json.dumps(d, indent=2) if '--json' in sys.argv else report(d))
