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
# `offline`, or `install`), and goes silent, fails, fetches steadily for 3 s, or trickles where
# $SLOW, $DOWN, $STEADY, or $TRICKLE names that mirror. An update writes a list into $LISTS and a
# download the package into $ARCHIVES, as apt's do; an install with no network needs both.
APT_GET = """#!/usr/bin/env bash
mirror=$(grep -m1 -oE 'http://[^ ]+' "$SOURCES" )
case " $* " in
  *" update "*) what=update ;;
  *" --download-only "*) what=download ;;
  *" --no-download "*) what=offline ;;
  *) what=install ;;
esac
echo "$mirror $what" >> "$LOG"
if [ "$what" = offline ]; then
  [ -f "$LISTS/ubuntu_Packages" ] && [ -f "$ARCHIVES/ffmpeg.deb" ] && exit 0
  exit 100
fi
[ "$what" = install ] && exit 0
[[ " $DOWN " == *" $mirror "* ]] && exit 100
[[ " $SLOW " == *" $mirror "* ]] && exec sleep 30
into=$LISTS/fetched
[ "$what" = download ] && into=$ARCHIVES/fetched
if [[ " $STEADY " == *" $mirror "* ]]; then
  for _ in $(seq 12); do head -c 3000 /dev/zero >> "$into"; sleep 0.25; done
fi
if [[ " $TRICKLE " == *" $mirror "* ]]; then
  for _ in $(seq 120); do head -c 10 /dev/zero >> "$into"; sleep 0.25; done
fi
[ "$what" = update ] && echo lists > "$LISTS/ubuntu_Packages"
[ "$what" = download ] && echo deb > "$ARCHIVES/ffmpeg.deb"
exit 0
"""

AZURE = "http://azure.archive.ubuntu.com/ubuntu/"
ARCHIVE = "http://archive.ubuntu.com/ubuntu/"
KERNEL = "http://mirrors.edge.kernel.org/ubuntu/"


class AptInstall(unittest.TestCase):
    def run_script(self, slow="", down="", steady="", trickle="", keep=None):
        d = Path(tempfile.mkdtemp())
        self.dir = d
        (d / "ubuntu.sources").write_text(SOURCES)
        bin_dir = d / "bin"
        bin_dir.mkdir()
        (d / "lists").mkdir()
        (d / "archives").mkdir()
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
            STEADY=steady,
            TRICKLE=trickle,
            LISTS=str(d / "lists"),
            ARCHIVES=str(d / "archives"),
            SCAENA_APT_SOURCES=str(d / "ubuntu.sources"),
            SCAENA_APT_SUDO="",
            SCAENA_APT_CACHE=f"{d / 'lists'} {d / 'archives'}",
            SCAENA_APT_KEEP=str(keep) if keep else "",
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

    def test_what_a_run_fetched_is_kept_and_the_next_installs_it_with_no_mirror(self):
        keep = Path(tempfile.mkdtemp()) / "kept"
        done, log, _, _ = self.run_script(keep=keep)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(log, [f"{AZURE} update", f"{AZURE} download", f"{AZURE} install"])
        self.assertTrue((keep / "archives/ffmpeg.deb").is_file())
        self.assertTrue((keep / "lists/ubuntu_Packages").is_file())
        # On a fresh runner with every mirror down, what was kept installs with no network.
        done, log, _, _ = self.run_script(down=f"{AZURE} {ARCHIVE} {KERNEL}", keep=keep)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(log, [f"{AZURE} offline"])
        self.assertIn("with no network", done.stderr)

    def test_what_was_kept_that_does_not_install_is_fetched_again(self):
        keep = Path(tempfile.mkdtemp()) / "kept"
        (keep / "archives").mkdir(parents=True)
        (keep / "lists").mkdir()
        (keep / "archives/other.deb").write_text("deb")
        done, log, _, _ = self.run_script(keep=keep)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(log, [f"{AZURE} update", f"{AZURE} download", f"{AZURE} install"])
        self.assertTrue((keep / "archives/ffmpeg.deb").is_file())
        self.assertFalse((keep / "archives/other.deb").exists())

    def test_every_mirror_failing_fails_after_two_rounds(self):
        done, log, _, _ = self.run_script(down=f"{AZURE} {ARCHIVE} {KERNEL}")
        self.assertEqual(done.returncode, 1)
        self.assertEqual(len(log), 6, log)
        self.assertEqual([l.split()[0] for l in log[:3]], [AZURE, ARCHIVE, KERNEL])


if __name__ == "__main__":
    unittest.main()
