"""What the Mac app's main thread was doing each time it did not answer during the window's walk
(apps/mac/window.sh, apps/mac/hangs.swift): each hang by its length, with the app's own frames on
the heaviest path through its main thread's sample, the deepest first, then every frame by the
time it was on that path for, summed over the hangs, the longest first. The app's own frames are
Scaena's binary, its Swift and the Rust engine linked into it; a Rust frame is demangled.

    python3 scripts/hangs.py FOLDER [--frames N] [--top N]
"""

import argparse
import re
import sys
from pathlib import Path

# A frame of a `sample` call graph: its count, its symbol, and the image it is in.
FRAME = re.compile(r"^(?P<lead>[\s+!:|]*?)(?P<count>\d+) (?P<symbol>.+?)  \(in (?P<image>[^)]+)\)")
# A hang the watch told: its number, when it began, how long it lasted, and its sample's file.
HANG = re.compile(r"^hang (?P<n>\d+) at (?P<at>[\d.]+) s: (?P<ms>\d+) ms(?P<open>\+?) (?P<file>\S+)$")
# What the main thread waits in when it waits for the next event: no work of its own.
IDLE = ("mach_msg2_trap", "mach_msg_trap", "mach_msg2_internal", "mach_msg_overwrite")
APP = "Scaena"


def demangled(symbol):
    """A Rust symbol in its legacy mangling (`_ZN…E`, a hash last) as Rust names it: its path, no
    hash. Any other symbol as it is."""
    if not re.match(r"^_?_ZN", symbol):
        return symbol
    rest = symbol.lstrip("_")[2:]
    parts = []
    while rest and rest[0].isdigit():
        size = re.match(r"\d+", rest).group(0)
        rest = rest[len(size):]
        parts.append(rest[: int(size)])
        rest = rest[int(size):]
    if not parts:
        return symbol
    if re.fullmatch(r"h[0-9a-f]{16}", parts[-1]):
        parts.pop()
    # A part that opens on an escape is written after an underscore.
    parts = [part[1:] if part.startswith("_$") else part for part in parts]
    named = "::".join(parts)
    for code, said in (("$LT$", "<"), ("$GT$", ">"), ("$u20$", " "), ("$RF$", "&"), ("$C$", ","), ("..", "::")):
        named = named.replace(code, said)
    return re.sub(r"\$u([0-9a-f]+)\$", lambda m: chr(int(m.group(1), 16)), named)


class Frame:
    def __init__(self, count, symbol, image, depth):
        self.count, self.symbol, self.image, self.depth = count, symbol, image, depth
        self.children = []

    def busy(self):
        """Its count less the samples under it that waited for the next event."""
        if not self.children:
            return 0 if self.symbol.startswith(IDLE) else self.count
        under = sum(child.count for child in self.children)
        return sum(child.busy() for child in self.children) + max(self.count - under, 0)


def main_thread(text):
    """The main thread's call graph in a `sample` file, as its root frame; none where it has none."""
    lines = text.split("\n")
    start = next((i for i, line in enumerate(lines) if "com.apple.main-thread" in line), None)
    if start is None:
        return None
    root = Frame(int(re.search(r"\d+", lines[start]).group(0)), "main thread", "", -1)
    stack = [root]
    for line in lines[start + 1:]:
        if not line.strip():
            break
        found = FRAME.match(line.strip("\n"))
        if not found:
            # The next thread's line, or the graph's end.
            if re.match(r"^\s*\d+ Thread_", line):
                break
            continue
        depth = len(found.group("lead"))
        frame = Frame(int(found.group("count")), demangled(found.group("symbol").strip()), found.group("image"), depth)
        while stack[-1].depth >= depth:
            stack.pop()
        stack[-1].children.append(frame)
        stack.append(frame)
    return root


def heaviest(root):
    """The heaviest path from `root` by the work under each frame, idle waits aside: each frame on it."""
    path, at = [], root
    while at.children:
        at = max(at.children, key=Frame.busy)
        if at.busy() == 0:
            break
        path.append(at)
    return path


def app_frames(text, most=8):
    """The app's own frames on the heaviest path through the main thread, the deepest first, each
    with its count, at most `most`; and the samples taken, and how many of them were busy."""
    root = main_thread(text)
    if root is None:
        return [], 0, 0
    path = heaviest(root)
    own = [(frame.count, frame.symbol) for frame in path if frame.image == APP]
    return own[::-1][:most], root.count, root.busy()


def hangs(folder):
    """Each hang the watch told in `folder`'s hangs.txt: its number, when, how long, whether it was
    still under way when the watch ended, and its sample's file or none."""
    told = Path(folder) / "hangs.txt"
    if not told.exists():
        return []
    found = []
    for line in told.read_text(encoding="utf-8").splitlines():
        hang = HANG.match(line.strip())
        if hang:
            sampled = None if hang.group("file") == "-" else Path(folder) / hang.group("file")
            found.append((int(hang.group("n")), float(hang.group("at")), int(hang.group("ms")), bool(hang.group("open")), sampled))
    return found


def report(folder, frames=8, top=15):
    """The report, as lines."""
    found = hangs(folder)
    if not found:
        return ["no hang over the threshold"]
    lines = [f"{len(found)} hang(s), {sum(ms for _, _, ms, _, _ in found)} ms in all, the longest first:"]
    summed = {}
    for n, at, ms, still, sampled in sorted(found, key=lambda hang: -hang[2]):
        lines.append(f"hang {n}, at {at:.1f} s: {ms} ms{' and still going when the walk ended' if still else ''}")
        if sampled is None or not sampled.exists():
            lines.append("  (not sampled)")
            continue
        own, taken, busy = app_frames(sampled.read_text(encoding="utf-8", errors="replace"), frames)
        if not own:
            lines.append(f"  no frame of the app's on its heaviest path ({busy} of {taken} samples busy)")
            continue
        for count, symbol in own:
            lines.append(f"  {count:>5} {symbol}")
        for _, symbol in own:
            summed[symbol] = summed.get(symbol, 0) + ms
    if summed:
        lines.append("the app's frames by the hang time they were on the heaviest path for:")
        for symbol, ms in sorted(summed.items(), key=lambda item: (-item[1], item[0]))[:top]:
            lines.append(f"  {ms:>7} ms  {symbol}")
    return lines


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("folder")
    parser.add_argument("--frames", type=int, default=8)
    parser.add_argument("--top", type=int, default=15)
    asked = parser.parse_args()
    print("\n".join(report(asked.folder, asked.frames, asked.top)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
