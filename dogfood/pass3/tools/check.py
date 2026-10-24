#!/usr/bin/env python3
"""CLI-side checks of a bundle: lint and health summaries, and PNGs of each state.

  check.py snap BUNDLE TAG          # lint + health JSON into evidence/checks/<deck>-<TAG>-*.json, summary printed
  check.py pngs BUNDLE DIR [STATES] # export --format png at 960x540 into DIR
  check.py diff A_TAG B_TAG DECK    # compare two snaps
"""
import collections
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
S = os.path.dirname(ROOT)
SCAENA = os.environ.get("SCAENA", os.path.join(S, "dogfood2/target/release/scaena"))
CHECKS = os.path.join(ROOT, "evidence", "checks")


def run(args):
    p = subprocess.run([SCAENA] + args, capture_output=True, text=True, cwd=ROOT,
                       env=dict(os.environ, SOURCE_DATE_EPOCH="1792800000"))
    return p


def snap(bundle, tag):
    os.makedirs(CHECKS, exist_ok=True)
    deck = os.path.basename(bundle.rstrip("/"))
    lint = run(["lint", bundle, "--json"])
    health = run(["health", bundle, "--json"])
    out = {}
    try:
        lj = json.loads(lint.stdout)
    except Exception:
        lj = {"raw": lint.stdout, "stderr": lint.stderr}
    try:
        hj = json.loads(health.stdout)
    except Exception:
        hj = {"raw": health.stdout, "stderr": health.stderr}
    with open(os.path.join(CHECKS, f"{deck}-{tag}-lint.json"), "w") as f:
        json.dump(lj, f, indent=1, ensure_ascii=False)
    with open(os.path.join(CHECKS, f"{deck}-{tag}-health.json"), "w") as f:
        json.dump(hj, f, indent=1, ensure_ascii=False)
    summarize(deck, tag, lj, hj)


def findings(lj):
    if isinstance(lj, list):
        return lj
    return lj.get("findings", [])


def summarize(deck, tag, lj, hj):
    fs = findings(lj)
    by = collections.Counter((f.get("severity"), f.get("code")) for f in fs)
    print(f"== {deck} [{tag}] lint: {len(fs)} findings;",
          ", ".join(f"{c}({s[0]})x{n}" for (s, c), n in sorted(by.items(), key=lambda x: (x[0][0] != 'error', x[0][1]))))
    if "score" in hj:
        m = {k: (hj[k].get("score") if isinstance(hj.get(k), dict) else hj.get(k)) for k in ("pacing", "balance", "fidelity", "intensity", "tone")}
        print(f"   health: score {hj['score']}; beats {hj.get('beats')}; " + ", ".join(f"{k} {v}" for k, v in m.items()))
        hf = hj.get("findings", [])
        kinds = collections.Counter(f.get("kind") or f.get("code") or f.get("measure") for f in hf)
        print(f"   health findings: {len(hf)}: {dict(kinds)}")
    else:
        print("   health:", json.dumps(hj)[:400])


def pngs(bundle, out, states=None):
    os.makedirs(out, exist_ok=True)
    args = ["export", bundle, "--format", "png", "--out", out, "--size", "960x540", "--json"]
    if states:
        args += ["--states", states]
    p = run(args)
    print(p.stdout[-800:], p.stderr[-800:])


def main():
    cmd = sys.argv[1]
    if cmd == "snap":
        snap(sys.argv[2], sys.argv[3])
    elif cmd == "pngs":
        pngs(sys.argv[2], sys.argv[3], sys.argv[4] if len(sys.argv) > 4 else None)
    elif cmd == "show":
        deck, tag = sys.argv[2], sys.argv[3]
        lj = json.load(open(os.path.join(CHECKS, f"{deck}-{tag}-lint.json")))
        for f in findings(lj):
            print(f.get("severity"), f.get("code"), f.get("state"), f.get("node"), "|", f.get("message")[:200], "| fix" if f.get("fix") else "")
    elif cmd == "health":
        deck, tag = sys.argv[2], sys.argv[3]
        hj = json.load(open(os.path.join(CHECKS, f"{deck}-{tag}-health.json")))
        for f in hj.get("findings", []):
            print(json.dumps(f, ensure_ascii=False)[:400])


if __name__ == "__main__":
    main()
