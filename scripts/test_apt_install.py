"""scripts/apt-install.sh against a fake apt-get: a mirror that goes silent, trickles under the
floor, or fails gives way to the next; one that is slow but steady is let finish; and every
mirror failing fails the step."""

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

# A fake apt-get: logs the mirror the sources name and what it was asked (`update`, `download`,
# or `install`), and goes silent, fails, fetches steadily for 3 s, or trickles where $SLOW,
# $DOWN, $STEADY, or $TRICKLE names that mirror; what it fetches grows $CACHE as apt's does.
APT_GET = """#!/usr/bin/env bash
mirror=$(grep -v '^#' "$SOURCES" | grep -m1 -oE '(https?://|mirror[+]file:)[^ ]+')
case " $* " in
  *" update "*) what=update ;;
  *" --download-only "*) what=download ;;
  *) what=install ;;
esac
echo "$mirror $what" >> "$LOG"
if [ -f "$STATE/interrupted" ] && [ "$what" != update ]; then
  echo "E: dpkg was interrupted, you must manually run 'sudo dpkg --configure -a' to correct the problem." >&2
  exit 100
fi
[ "$what" = install ] && exit 0
[[ " $DOWN " == *" $mirror "* ]] && exit 100
[[ " $SLOW " == *" $mirror "* ]] && exec sleep 30
if [[ " $STEADY " == *" $mirror "* ]]; then
  for _ in $(seq 12); do head -c 3000 /dev/zero >> "$CACHE/fetched"; sleep 0.25; done
fi
if [[ " $TRICKLE " == *" $mirror "* ]]; then
  for _ in $(seq 120); do head -c 10 /dev/zero >> "$CACHE/fetched"; sleep 0.25; done
fi
exit 0
"""

# A fake dpkg: `--configure -a` finishes what an interrupted install left, as dpkg's does.
DPKG = """#!/usr/bin/env bash
[ "$*" = "--configure -a" ] && rm -f "$STATE/interrupted"
exit 0
"""

# The runner's own: its classic list, the release notes' address in a comment, and Azure's
# mirrors named by a list of them.
RUNNER = """# See http://help.ubuntu.com/community/UpgradeNotes for how to upgrade to
# newer versions of the distribution.
deb mirror+file:/etc/apt/apt-mirrors.txt noble main restricted
deb mirror+file:/etc/apt/apt-mirrors.txt noble-updates main restricted
deb http://security.ubuntu.com/ubuntu noble-security main restricted
"""
LISTED = "mirror+file:/etc/apt/apt-mirrors.txt"

AZURE = "http://azure.archive.ubuntu.com/ubuntu/"
ARCHIVE = "http://archive.ubuntu.com/ubuntu/"
KERNEL = "http://mirrors.edge.kernel.org/ubuntu/"


class AptInstall(unittest.TestCase):
    def run_script(self, slow="", down="", steady="", trickle="", sources=SOURCES, interrupted=False):
        d = Path(tempfile.mkdtemp())
        (d / "ubuntu.sources").write_text(sources)
        (d / "state").mkdir()
        if interrupted:
            (d / "state/interrupted").write_text("")
        bin_dir = d / "bin"
        bin_dir.mkdir()
        (d / "cache").mkdir()
        fake = bin_dir / "apt-get"
        fake.write_text(APT_GET)
        fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
        dpkg = bin_dir / "dpkg"
        dpkg.write_text(DPKG)
        dpkg.chmod(dpkg.stat().st_mode | stat.S_IEXEC)
        env = dict(
            os.environ,
            PATH=f"{bin_dir}:{os.environ['PATH']}",
            SOURCES=str(d / "ubuntu.sources"),
            LOG=str(d / "log"),
            STATE=str(d / "state"),
            SLOW=slow,
            DOWN=down,
            STEADY=steady,
            TRICKLE=trickle,
            CACHE=str(d / "cache"),
            SCAENA_APT_SOURCES=str(d / "ubuntu.sources"),
            SCAENA_APT_SUDO="",
            SCAENA_APT_CACHE=str(d / "cache"),
            # Reads a second apart, a floor of 1,000 bytes a second, no pause between mirrors.
            SCAENA_APT_LIMITS="1 1000 0",
        )
        start = time.monotonic()
        done = subprocess.run(["bash", str(SCRIPT), "ffmpeg"], env=env, capture_output=True, text=True, timeout=25)
        log = (d / "log").read_text().split("\n") if (d / "log").exists() else []
        return done, [l for l in log if l], (d / "ubuntu.sources").read_text(), time.monotonic() - start

    def test_a_healthy_mirror_installs_at_once(self):
        done, log, sources, _ = self.run_script()
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(log, [f"{AZURE} update", f"{AZURE} download", f"{AZURE} install"])
        self.assertEqual(sources, SOURCES)

    def test_a_slow_mirror_that_keeps_fetching_is_let_finish(self):
        # 12 kB a second for 3 s, each step: over the floor, past any limit of a second or two.
        done, log, sources, _ = self.run_script(steady=AZURE)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(log, [f"{AZURE} update", f"{AZURE} download", f"{AZURE} install"])
        self.assertEqual(sources, SOURCES)

    def test_a_mirror_that_goes_silent_or_trickles_gives_way_to_the_next(self):
        for stuck in ({"slow": AZURE}, {"trickle": AZURE}):
            done, log, sources, took = self.run_script(**stuck)
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertLess(took, 15)
            self.assertEqual(log[-3:], [f"{ARCHIVE} update", f"{ARCHIVE} download", f"{ARCHIVE} install"], stuck)
            self.assertIn(f"URIs: {ARCHIVE}", sources)
            # The security pocket stays where it was.
            self.assertIn("URIs: http://security.ubuntu.com/ubuntu/", sources)

    def test_the_runners_listed_mirrors_give_way_and_the_comments_stay(self):
        # GitHub's runners name Azure's mirrors by `mirror+file:` under a comment with an address
        # of its own: the failover rewrites the deb lines, never the comment or the security pocket.
        done, log, sources, _ = self.run_script(slow=LISTED, sources=RUNNER)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(log[0], f"{LISTED} update")
        self.assertEqual(log[-3:], [f"{ARCHIVE} update", f"{ARCHIVE} download", f"{ARCHIVE} install"])
        self.assertIn(f"deb {ARCHIVE} noble main restricted", sources)
        self.assertIn(f"deb {ARCHIVE} noble-updates main restricted", sources)
        self.assertNotIn(LISTED, sources)
        self.assertIn("# See http://help.ubuntu.com/community/UpgradeNotes for how", sources)
        self.assertIn("deb http://security.ubuntu.com/ubuntu noble-security", sources)

    def test_a_dpkg_left_interrupted_is_configured_and_the_install_goes_on(self):
        done, log, _, _ = self.run_script(interrupted=True)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(log, [f"{AZURE} update", f"{AZURE} download", f"{AZURE} install"])

    def test_every_mirror_failing_fails_after_two_rounds(self):
        done, log, _, _ = self.run_script(down=f"{AZURE} {ARCHIVE} {KERNEL}")
        self.assertEqual(done.returncode, 1)
        self.assertEqual(len(log), 6, log)
        self.assertEqual([l.split()[0] for l in log[:3]], [AZURE, ARCHIVE, KERNEL])


if __name__ == "__main__":
    unittest.main()
