#!/usr/bin/env bash
# Dogfood pass 7: four decks made from slides written here, with no model call, then linted,
# read for health, and drawn.   run.sh BIN OUT
set -uo pipefail
BIN=$1; OUT=$2; IN=$(cd "$(dirname "$0")" && pwd)
export SOURCE_DATE_EPOCH=1792800000
mkdir -p "$OUT"
make() { # name prompt args...
  local n=$1 p=$2; shift 2
  rm -rf "$OUT/$n"
  "$BIN" generate "$OUT/$n" "$p" --slides "$IN/slides/$n.json" --no-look "$@" >"$OUT/$n.generate.txt" 2>&1 || { echo "$n: generate failed"; tail -5 "$OUT/$n.generate.txt"; return; }
  "$BIN" lint "$OUT/$n" >"$OUT/$n.lint.txt" 2>&1
  "$BIN" health "$OUT/$n" >"$OUT/$n.health.txt" 2>&1
  "$BIN" export "$OUT/$n" --format png --out "$OUT/$n.png" --size 1280x720 >"$OUT/$n.export.txt" 2>&1
  echo "$n: $(ls "$OUT/$n.png" | wc -l) PNGs"
}
make deepdive "How we cut p99 checkout latency by 60%" --audience Engineers --goal Educate --length "11-15 slides" \
  --arc "PARL (Problem, Action, Result, Learning)" --theme dusk
make board-de "Quartalsbericht Q3 2026 für den Vorstand der Nordlicht Energie GmbH" --audience Executives --goal "Update Status" \
  --length "11-15 slides" --arc "Data Storytelling" --theme daybreak --language German
make appeal "Harbor House winter appeal: keep every family room open this winter" --audience Donors --goal Persuade \
  --length "11-15 slides" --arc "Conflict → Struggle → Resolution" --theme ember
make training "Training: writing incident postmortems people actually read" --audience Colleagues --goal Educate \
  --length "11-15 slides" --arc "Learning Journey" --theme salon
