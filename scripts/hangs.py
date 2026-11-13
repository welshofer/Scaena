"""What the Mac app's main thread was doing each time it did not answer during the window's walk
(apps/mac/window.sh, apps/mac/hangs.swift): each hang by its length, with the app's own frames on
the heaviest path through its main thread's sample, the deepest first, then every frame by the
time it was on that path for, summed over the hangs, the longest first. The app's own frames are
Scaena's binary, its Swift and the Rust engine linked into it; a Rust frame is demangled. Each
hang says where its busy samples were spent, Rust or Swift, by the app's frame nearest each: the
walk runs a debug build, many times slower in Rust and little slower in Swift, so a hang spent in
Swift is one a release build would still have.

    python3 scripts/hangs.py FOLDER [--frames N] [--top N] [--samples FILE]
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


BASIC = {
    "a": "i8", "b": "bool", "c": "char", "d": "f64", "e": "str", "f": "f32", "h": "u8", "i": "isize",
    "j": "usize", "l": "i32", "m": "u32", "n": "i128", "o": "u128", "s": "i16", "t": "u16", "u": "()",
    "v": "...", "x": "i64", "y": "u64", "z": "!", "p": "_",
}


class V0:
    """Rust's v0 mangling (`_R…`) read as far as a stack's frames need it: each path by its names,
    an impl's methods under its type (`<T>::f`) or its trait (`<T as Tr>::f`), a closure as
    `{closure}`, and generic arguments left out, so a frame reads as the code that names it."""

    def __init__(self, text):
        self.text, self.at = text, 0

    def peek(self):
        return self.text[self.at] if self.at < len(self.text) else ""

    def take(self):
        c = self.peek()
        if not c:
            raise ValueError("ends early")
        self.at += 1
        return c

    def base62(self):
        if self.peek() == "_":
            self.at += 1
            return 0
        value = 0
        while self.peek() != "_":
            c = self.take()
            value = value * 62 + "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".index(c)
        self.at += 1
        return value + 1

    def disambiguator(self):
        if self.peek() == "s":
            self.at += 1
            self.base62()

    def ident(self):
        self.disambiguator()
        if self.peek() == "u":
            self.at += 1
        size = re.match(r"\d+", self.text[self.at:])
        if not size:
            raise ValueError("no identifier")
        self.at += len(size.group(0))
        if self.peek() == "_":
            self.at += 1
        name = self.text[self.at : self.at + int(size.group(0))]
        self.at += int(size.group(0))
        return name

    def backref(self, read):
        to = self.base62()
        back, self.at = self.at, to
        try:
            return read()
        finally:
            self.at = back

    def path(self):
        c = self.take()
        if c == "C":
            return self.ident()
        if c == "N":
            ns = self.take()
            inner = self.path()
            name = self.ident()
            if ns == "C":
                return f"{inner}::{{closure}}"
            if ns == "S":
                return f"{inner}::{{shim}}"
            return f"{inner}::{name}" if name else inner
        if c == "M":
            self.disambiguator()
            self.path()
            return f"<{self.type()}>"
        if c == "X":
            self.disambiguator()
            self.path()
            of = self.type()
            return f"<{of} as {self.path()}>"
        if c == "Y":
            of = self.type()
            return f"<{of} as {self.path()}>"
        if c == "I":
            inner = self.path()
            while self.peek() != "E":
                self.arg()
            self.at += 1
            return inner
        if c == "B":
            return self.backref(self.path)
        raise ValueError(f"no path at {c!r}")

    def arg(self):
        if self.peek() == "L":
            self.at += 1
            self.base62()
        elif self.peek() == "K":
            self.at += 1
            self.const()
        else:
            self.type()

    def const(self):
        if self.peek() == "p":
            self.at += 1
            return
        if self.peek() == "B":
            self.at += 1
            self.backref(self.const)
            return
        self.type()
        if self.peek() == "n":
            self.at += 1
        while self.peek() != "_":
            self.take()
        self.at += 1

    def type(self):
        c = self.peek()
        if c in BASIC:
            self.at += 1
            return BASIC[c]
        if c in "RQ":
            self.at += 1
            if self.peek() == "L":
                self.at += 1
                self.base62()
            return ("&" if c == "R" else "&mut ") + self.type()
        if c in "PO":
            self.at += 1
            return ("*const " if c == "P" else "*mut ") + self.type()
        if c == "S":
            self.at += 1
            return f"[{self.type()}]"
        if c == "A":
            self.at += 1
            of = self.type()
            self.const()
            return f"[{of}; _]"
        if c == "T":
            self.at += 1
            parts = []
            while self.peek() != "E":
                parts.append(self.type())
            self.at += 1
            return "(" + ", ".join(parts) + ")"
        if c == "B":
            self.at += 1
            return self.backref(self.type)
        if c in "FD":
            raise ValueError("a function or dyn type")
        return self.path()


def v0(symbol):
    """`symbol` read as a v0-mangled Rust path, the instantiating crate after it left off; as it
    is where it does not read as one."""
    text = symbol.lstrip("_")
    if not text.startswith("R"):
        return symbol
    try:
        reader = V0(text[1:])
        if reader.peek().isdigit():
            while reader.peek().isdigit():
                reader.at += 1
        return reader.path()
    except (ValueError, IndexError):
        return symbol


def demangled(symbol):
    """A Rust symbol as Rust names it: in its legacy mangling (`_ZN…E`, a hash last) its path, no
    hash; in its v0 mangling (`_R…`) its path, an impl's method under its type or trait. Any other
    symbol as it is."""
    if re.match(r"^_?_R[A-Z0-9]", symbol):
        return v0(symbol)
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
    def __init__(self, count, symbol, image, depth, rust=False):
        self.count, self.symbol, self.image, self.depth, self.rust = count, symbol, image, depth, rust
        self.children = []
        self._busy = None

    def busy(self):
        """Its count less the samples under it that waited for the next event: worked out once
        for it and every frame under it, a frame at a time, never by recursion, since a hang's
        stack runs deeper than Python's (#1310's walk: RecursionError)."""
        if self._busy is None:
            order, stack = [], [self]
            while stack:
                frame = stack.pop()
                order.append(frame)
                stack.extend(child for child in frame.children if child._busy is None)
            for frame in reversed(order):
                if not frame.children:
                    frame._busy = 0 if frame.symbol.startswith(IDLE) else frame.count
                else:
                    under = sum(child.count for child in frame.children)
                    frame._busy = sum(child._busy for child in frame.children) + max(frame.count - under, 0)
        return self._busy


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
        symbol = found.group("symbol").strip()
        frame = Frame(int(found.group("count")), demangled(symbol), found.group("image"), depth, bool(RUST.match(symbol)))
        while stack[-1].depth >= depth:
            stack.pop()
        stack[-1].children.append(frame)
        stack.append(frame)
    return root


# A symbol Rust mangled, in either mangling.
RUST = re.compile(r"^_?_(R[A-Z0-9]|ZN)")


def spent(root):
    """The main thread's busy samples by the language of the app's frame nearest each: Rust's and
    Swift's, a pair. A debug build is many times slower in Rust and little slower in Swift, so a hang
    spent in Swift is one a release build would still have."""
    rust = swift = 0
    stack = [(root, None)]
    while stack:
        frame, own = stack.pop()
        if frame.image == APP:
            own = "rust" if frame.rust else "swift"
        alone = frame.busy() - sum(child.busy() for child in frame.children)
        if own == "rust":
            rust += alone
        elif own == "swift":
            swift += alone
        stack.extend((child, own) for child in frame.children)
    return rust, swift


def where(text):
    """Where a hang's sample spent its busy samples: `Rust`, `Swift`, or `system` where no app
    frame is under them, with its share; none where it has no main thread or no busy sample."""
    root = main_thread(text)
    if root is None or root.busy() == 0:
        return None
    rust, swift = spent(root)
    busy = root.busy()
    if rust == swift == 0:
        return "system"
    if rust >= swift:
        return f"Rust {100 * rust // busy}%"
    return f"Swift {100 * swift // busy}%"


def heaviest(root):
    """The heaviest path from `root` by the work under each frame, idle waits aside: each frame on it."""
    path, at = [], root
    while at.children:
        at = max(at.children, key=Frame.busy)
        if at.busy() == 0:
            break
        path.append(at)
    return path


# The frames every stack of the app's main thread opens with, which say nothing of a hang.
ENTRY = ("Scaena_main", "static ScaenaApp.$main()", "main")


def app_frames(text, most=12):
    """The app's own frames on the heaviest path through the main thread, the deepest first, each
    with its count, at most `most`: each once, at the outermost place it stands, so a recursion
    many frames deep shows the frame that began it, and the frames every stack opens with left
    out; and the samples taken, and how many of them were busy."""
    root = main_thread(text)
    if root is None:
        return [], 0, 0
    path = heaviest(root)
    own, seen = [], set()
    for frame in path:
        if frame.image != APP or frame.symbol in ENTRY or frame.symbol in seen:
            continue
        seen.add(frame.symbol)
        own.append((frame.count, frame.symbol))
    return own[::-1][:most], root.count, root.busy()


def graph(text, least=2):
    """The main thread's call graph in a `sample` file as its lines, each frame named as `demangled`
    names it, those held fewer than `least` samples left out: what is kept of a hang's sample where
    the whole of it is not."""
    lines = text.split("\n")
    start = next((i for i, line in enumerate(lines) if "com.apple.main-thread" in line), None)
    if start is None:
        return []
    kept = [lines[start].strip()]
    for line in lines[start + 1 :]:
        if not line.strip() or re.match(r"^\s*\d+ Thread_", line):
            break
        found = FRAME.match(line)
        if found and int(found.group("count")) >= least:
            symbol = demangled(found.group("symbol").strip())
            kept.append(f"{found.group('lead')}{found.group('count')} {symbol}  ({found.group('image')})")
    return kept


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


def samples(folder):
    """Each sampled hang's main thread, the longest hang first, as lines: what the walk keeps of the
    samples beside its shots, which keep no folder."""
    lines = []
    for n, at, ms, _, sampled in sorted(hangs(folder), key=lambda hang: -hang[2]):
        if sampled is None or not sampled.exists():
            continue
        lines.append(f"== hang {n}, at {at:.1f} s: {ms} ms")
        lines.extend(graph(sampled.read_text(encoding="utf-8", errors="replace")))
        lines.append("")
    return lines


def report(folder, frames=12, top=15):
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
        text = sampled.read_text(encoding="utf-8", errors="replace")
        try:
            own, taken, busy = app_frames(text, frames)
            spent_in = where(text)
        except Exception as e:  # noqa: BLE001 - one sample that does not read ends no report
            lines.append(f"  its sample did not read: {type(e).__name__}: {e}")
            continue
        if spent_in:
            lines[-1] += f" [{spent_in}]"
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
    parser.add_argument("--frames", type=int, default=12)
    parser.add_argument("--top", type=int, default=15)
    parser.add_argument("--samples", help="write each sampled hang's main thread, demangled, to this file")
    asked = parser.parse_args()
    print("\n".join(report(asked.folder, asked.frames, asked.top)))
    if asked.samples:
        Path(asked.samples).write_text("\n".join(samples(asked.folder)) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
