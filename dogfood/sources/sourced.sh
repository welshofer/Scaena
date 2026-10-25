#!/usr/bin/env bash
# Dogfood pass 6, the source-grounded deck: the QBR's slides, eight of them citing a CSV or a memo
# (`sourceRef`), made with no model call from `--slides` and the two `--source` files, then
# checked: the sources the deck keeps, each beat's evidence, lint (W423), files, and health.
#   sourced.sh BIN LABEL     # writes $OUT/LABEL/ ($OUT defaults to ./sourced-out)
set -euo pipefail
S=${S:-$(pwd)}
BIN=$1
OUT=${OUT:-$S/sourced-out}/$2
IN=$(cd "$(dirname "$0")" && pwd)
rm -rf "$OUT"
mkdir -p "$OUT/png"
export SOURCE_DATE_EPOCH=1792800000
cd "$IN"
"$BIN" generate "$OUT/qbr-sourced" "Q3 FY26 quarterly business review for Northwind Analytics' executive team" \
  --audience Executives --goal "Update Status" --length "11-15 slides" --arc "Data Storytelling" --theme daybreak \
  --slides "$IN/qbr-sourced.json" --source northwind-q3.csv --source board-memo.md --no-look \
  >"$OUT/generate.txt" 2>&1 || { echo "generate failed"; tail -5 "$OUT/generate.txt"; exit 1; }
D=$OUT/qbr-sourced
python3 -I - "$D/deck.json" >"$OUT/sources.txt" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
n = d.get("meta", {}).get("narrative", {})
print("sources:", json.dumps(n.get("sources"), ensure_ascii=False)[:600])
for sec in d.get("spine", {}).get("sections", []):
    for b in sec.get("beats", []):
        ev = b.get("evidence")
        if ev:
            print("beat", b.get("id"), "evidence:", ev)
print("data sources:", sorted((d.get("data") or {}).keys()))
EOF
"$BIN" lint "$D" --json >"$OUT/lint.json" 2>/dev/null || true
"$BIN" files "$D" >"$OUT/files.txt" 2>&1 || true
"$BIN" health "$D" >"$OUT/health.txt" 2>&1 || true
"$BIN" export "$D" --format png --size 960x540 --out "$OUT/png" >/dev/null 2>&1 || echo "png failed"
python3 -I -c "
import json,sys
l=json.load(open(sys.argv[1]))
f=l.get('findings', l if isinstance(l,list) else [])
from collections import Counter
print('lint:', dict(Counter(x.get('code') for x in f)))
" "$OUT/lint.json" | tee -a "$OUT/sources.txt"
cat "$OUT/sources.txt"
