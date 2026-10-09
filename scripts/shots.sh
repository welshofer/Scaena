#!/usr/bin/env bash
# The screenshots a CI run kept, where whoever reviews the pull request can fetch them without the
# run's artifacts: each PNG under DIR made a JPEG no wider than 1600 pixels, named as the test that
# kept it named it; each screen recording a UI test made (MP4), and what the step that took them
# said (`window.txt`), as they are; in one commit with no parent, force-pushed to the branch
# `shots/NAME`, which the next run replaces. On a Mac (sips); in a job that may push
# (`contents: write`).
#
#   scripts/shots.sh DIR NAME
#   git fetch origin shots/NAME && git archive FETCH_HEAD | tar -x -C SOMEWHERE
set -euo pipefail
dir=$1
name=$2
work=$(mktemp -d)
# xcresulttool names what it exports by UUID, and says in manifest.json what each test named it.
python3 - "$dir" "$work" <<'PY'
import json, os, re, shutil, sys
src, out = sys.argv[1], sys.argv[2]
names = {}
manifest = os.path.join(src, "manifest.json")
if os.path.exists(manifest):
    for test in json.load(open(manifest)):
        for shot in test.get("attachments", []):
            names[shot["exportedFileName"]] = shot.get("suggestedHumanReadableName") or shot["exportedFileName"]
for file in sorted(os.listdir(src)):
    if file == "window.txt":
        shutil.copy(os.path.join(src, file), os.path.join(out, file))
        continue
    ext = os.path.splitext(file)[1].lower()
    if ext not in (".png", ".mp4"):
        continue
    name = re.sub(r"[^A-Za-z0-9._-]+", "-", names.get(file, file))
    name = name if name.lower().endswith(ext) else name + ext
    shutil.copy(os.path.join(src, file), os.path.join(out, name))
PY
shopt -s nullglob
shots=("$work"/*.png)
for shot in "${shots[@]}"; do
  sips -Z 1600 -s format jpeg -s formatOptions 70 "$shot" --out "${shot%.png}.jpg" >/dev/null
  rm "$shot"
done
kept=("$work"/*.jpg "$work"/*.mp4 "$work"/*.txt)
if [ ${#kept[@]} -eq 0 ]; then
  echo "nothing kept under $dir"
  exit 0
fi
tree=$(for file in "${kept[@]}"; do printf '100644 blob %s\t%s\n' "$(git hash-object -w "$file")" "$(basename "$file")"; done | git mktree)
commit=$(git -c user.name="github-actions[bot]" -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
  commit-tree "$tree" -m "Screenshots of ${GITHUB_SHA:-$(git rev-parse HEAD)}")
git push -q -f origin "$commit:refs/heads/shots/$name"
echo "shots/$name: $(cd "$work" && ls | tr '\n' ' ')"
