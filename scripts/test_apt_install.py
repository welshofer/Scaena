"""scripts/apt-install.sh against a fake apt-get: a mirror that trickles or fails gives way to the
next within its limits, and every mirror failing fails the step."""

import os
import stat
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("apt-install.sh")

SOURCES = """Types: deb
URIs: http://azure.archive.ubuntu.com/ubuntu/
Suites: noble noble-updates noble-backports
Components: main universe restricted multiverse

Types: deb
URIs: http://security.ubuntu.com/ubuntu/
Suites: noble-security
Components: main universe restricted multiverse
"""

# A fake apt-get: logs the mirror the sources name and what it was asked, and stalls or fails
# where $SLOW or $DOWN names that mirror.
APT_GET = """#!/usr/bin/env bash
mirror=$(grep -m1 -oE 'http://[^ ]+' "$SOURCES" )
echo "$mirror ${@: -1}" >> "$LOG"
[[ " $SLOW " == *" $mirror "* ]] && sleep 30
[[ " $DOWN " == *" $mirror "* ]] && exit 100
exit 0
"""


class AptInstall(unittest.TestCase):
    def run_script(self, slow="", down=""):
        d = Path(tempfile.mkdtemp())
        (d / "ubuntu.sources").write_text(SOURCES)
        bin_dir = d / "bin"
        bin_dir.mkdir()
        fake = bin_dir / "apt-get"
        fake.write_text(APT_GET)
        fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
        env = dict(
            os.environ,
            PATH=f"{bin_dir}:{os.environ['PATH']}",
            SOURCES=str(d / "ubuntu.sources"),
            LOG=str(d / "log"),
            SLOW=slow,
            DOWN=down,
            SCAENA_APT_SOURCES=str(d / "ubuntu.sources"),
            SCAENA_APT_SUDO="",
            SCAENA_APT_LIMITS="1 1 0",
        )
        start = time.monotonic()
        done = subprocess.run(["bash", str(SCRIPT), "ffmpeg"], env=env, capture_output=True, text=True, timeout=25)
        log = (d / "log").read_text().split("\n") if (d / "log").exists() else []
        return done, [l for l in log if l], (d / "ubuntu.sources").read_text(), time.monotonic() - start

    def test_a_healthy_mirror_installs_at_once(self):
        done, log, sources, _ = self.run_script()
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(log, ["http://azure.archive.ubuntu.com/ubuntu/ update", "http://azure.archive.ubuntu.com/ubuntu/ ffmpeg"])
        self.assertEqual(sources, SOURCES)

    def test_a_mirror_that_trickles_gives_way_to_the_next(self):
        done, log, sources, took = self.run_script(slow="http://azure.archive.ubuntu.com/ubuntu/")
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertLess(took, 10)
        self.assertEqual(log[-2:], ["http://archive.ubuntu.com/ubuntu/ update", "http://archive.ubuntu.com/ubuntu/ ffmpeg"])
        self.assertIn("URIs: http://archive.ubuntu.com/ubuntu/", sources)
        # The security pocket stays where it was.
        self.assertIn("URIs: http://security.ubuntu.com/ubuntu/", sources)

    def test_every_mirror_failing_fails_after_two_rounds(self):
        mirrors = "http://azure.archive.ubuntu.com/ubuntu/ http://archive.ubuntu.com/ubuntu/ http://mirrors.edge.kernel.org/ubuntu/"
        done, log, _, _ = self.run_script(down=mirrors)
        self.assertEqual(done.returncode, 1)
        self.assertEqual(len(log), 6, log)
        self.assertEqual([l.split()[0] for l in log[:3]], mirrors.split())


if __name__ == "__main__":
    unittest.main()
