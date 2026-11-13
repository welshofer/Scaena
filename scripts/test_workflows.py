"""The workflows' artifact uploads are diagnostics: each goes on when it fails, so an outage of
GitHub's artifact service never turns a job red whose tests passed (it answered every upload with
HTML from 20:30 to 20:43 on 2026-11-11). The one upload that is a job's product, the Mac app the
`mac-app` workflow builds for someone to download, fails its job as it should. The remote's tests run
under ThreadSanitizer where what builds the apps runs, and a race they find fails the job. On macOS,
what paints and what builds the apps are two jobs, so neither waits out the other's time."""

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


class Sanitizer(unittest.TestCase):
    def test_the_instrumented_build_leaves_the_app_the_window_walks_alone(self):
        # `swift test --sanitize=thread` in the package's own build folder builds the `Scaena`
        # executable again instrumented, and the window's walk, after it, copied that one: #1262's
        # sample of the app found ThreadSanitizer's runtime in it, every main-thread stall
        # stretched by its interceptors. The instrumented build keeps a folder of its own.
        ci = steps((WORKFLOWS / "ci.yml").read_text(encoding="utf-8"))
        found = ["\n".join(step) for step in ci if "name: The remote under ThreadSanitizer" in step[0]]
        self.assertEqual(len(found), 1)
        scratch = re.search(r"swift test --sanitize=thread[^\n]*--scratch-path\s+(\S+)", found[0])
        self.assertIsNotNone(scratch, "the instrumented build has a scratch path of its own")
        self.assertNotIn(scratch.group(1).strip("\"'"), (".build", "./.build"))

    def test_the_remotes_tests_run_under_threadsanitizer_and_a_race_fails_the_job(self):
        ci = steps((WORKFLOWS / "ci.yml").read_text(encoding="utf-8"))
        found = ["\n".join(step) for step in ci if "name: The remote under ThreadSanitizer" in step[0]]
        self.assertEqual(len(found), 1, "one step runs the remote's tests under ThreadSanitizer")
        body = found[0]
        self.assertIn("swift test --sanitize=thread --filter RemoteTests", body)
        self.assertIn("runner.os == 'macOS' && needs.changes.outputs.app == 'true'", body)
        self.assertNotRegex(body, r"continue-on-error:\s*true", "a race it finds fails the job")
        self.assertNotIn("|| true", body)
        # The filter names a suite the tests are in.
        tests = WORKFLOWS.parent.parent / "apps" / "mac" / "ScaenaKit" / "Tests" / "ScaenaKitTests" / "RemoteTests.swift"
        self.assertIn("@Suite struct RemoteTests {", tests.read_text(encoding="utf-8"))



def jobs(text):
    """Each job of a workflow as its text, by its id: from `  ID:` under `jobs:` to the next."""
    found, current = {}, None
    inside = False
    for line in text.split("\n"):
        if line.startswith("jobs:"):
            inside = True
            continue
        if not inside:
            continue
        head = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
        if head:
            current = head.group(1)
            found[current] = []
        elif re.match(r"^  #", line):
            # A comment at a job's own indent heads the job after it.
            current = None
            continue
        elif line and not line.startswith(" ") and not line.startswith("#"):
            break
        if current is not None:
            found[current].append(line)
    return {name: "\n".join(lines) for name, lines in found.items()}


class MacJobs(unittest.TestCase):
    def test_what_paints_and_what_builds_the_apps_are_two_jobs_on_macos(self):
        ci = jobs((WORKFLOWS / "ci.yml").read_text(encoding="utf-8"))
        rust, apps = ci["rust"], ci["apps"]
        # The apps job: on macOS, where what builds the apps runs, under a name of its own.
        self.assertIn("name: apps (macos-latest)", apps)
        self.assertIn("runs-on: macos-latest", apps)
        self.assertIn("needs.changes.outputs.app == 'true'", apps.split("steps:")[0])
        for step in (
            "cargo test -p scaena-ffi --locked",
            "name: The C ABI from C",
            "name: ScaenaKit, the C ABI from Swift",
            "name: The remote under ThreadSanitizer",
            "name: The Mac app's window",
        ):
            self.assertIn(step, apps, f"the apps job runs {step}")
        # The rust job keeps its name on macOS and runs what paints there, and none of the apps' steps.
        self.assertIn("matrix.os", rust)
        for step in ("cargo test --workspace --locked", "cargo clippy --workspace", "--features gpu"):
            self.assertIn(step, rust)
        for step in ("ScaenaKit", "ThreadSanitizer", "The Mac app's window", "scripts/shots.sh"):
            self.assertNotIn(step, rust, f"{step} runs in the apps job alone")
        # macOS joins the rust job's matrix for what paints alone.
        self.assertIn(
            """if [ "$paint" = true ]; then os='["ubuntu-latest","macos-latest"]'; else os='["ubuntu-latest"]'; fi""",
            ci["changes"],
        )

    def test_a_job_is_read_from_its_id_to_the_next(self):
        text = "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - run: ls\n  # b's\n  b:\n    runs-on: y\n"
        self.assertEqual(jobs(text), {"a": "  a:\n    runs-on: x\n    steps:\n      - run: ls", "b": "  b:\n    runs-on: y\n"})


if __name__ == "__main__":
    unittest.main()
