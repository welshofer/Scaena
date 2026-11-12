"""The workflows' artifact uploads are diagnostics: each goes on when it fails, so an outage of
GitHub's artifact service never turns a job red whose tests passed (it answered every upload with
HTML from 20:30 to 20:43 on 2026-11-11). The one upload that is a job's product, the Mac app the
`mac-app` workflow builds for someone to download, fails its job as it should."""

import re
import unittest
from pathlib import Path

WORKFLOWS = Path(__file__).resolve().parent.parent / ".github" / "workflows"

# Uploads whose artifact is what the job is for, by workflow and artifact name.
PRODUCTS = {("mac-app.yml", "Scaena.app")}


def steps(text):
    """Each step of a workflow as its lines: from a `- ` at a step's indent to the next."""
    lines = text.split("\n")
    found, current, indent = [], None, None
    for line in lines:
        start = re.match(r"^(\s*)- ", line)
        if start and (indent is None or len(start.group(1)) == indent):
            if current is not None:
                found.append(current)
            current, indent = [line], len(start.group(1))
        elif current is not None:
            if line.strip() and len(line) - len(line.lstrip()) < indent:
                found.append(current)
                current, indent = None, None
            else:
                current.append(line)
    if current is not None:
        found.append(current)
    return found


def uploads(text):
    """Each `actions/upload-artifact` step of a workflow: its artifact's name, and whether it goes on."""
    for step in steps(text):
        body = "\n".join(step)
        if "uses: actions/upload-artifact@" not in body:
            continue
        name = re.search(r"^\s*name:\s*(.+)$", body, re.M)
        yield (name.group(1).strip() if name else "?"), bool(re.search(r"continue-on-error:\s*true", body))


class Uploads(unittest.TestCase):
    def test_every_diagnostic_upload_goes_on_when_it_fails(self):
        seen = []
        for path in sorted(WORKFLOWS.glob("*.yml")):
            for name, goes_on in uploads(path.read_text(encoding="utf-8")):
                seen.append(name)
                if (path.name, name) in PRODUCTS:
                    self.assertFalse(goes_on, f"{path.name}: {name} is the job's product, and its upload failing fails it")
                else:
                    self.assertTrue(goes_on, f"{path.name}: the upload of {name} fails the job when the artifact service does")
        # The walk finds the uploads there are, so a change in how steps are written cannot pass it empty.
        for name in ("ipad-screenshots", "parity", "Scaena.app"):
            self.assertIn(name, seen)

    def test_a_step_is_read_from_its_dash_to_the_next(self):
        text = "steps:\n  - if: failure()\n    uses: actions/upload-artifact@v4\n    with:\n      name: x\n  - run: ls\n"
        self.assertEqual(list(uploads(text)), [("x", False)])
        text = text.replace("    uses:", "    continue-on-error: true\n    uses:")
        self.assertEqual(list(uploads(text)), [("x", True)])


if __name__ == "__main__":
    unittest.main()
