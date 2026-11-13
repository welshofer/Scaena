"""scripts/hangs.py reads the window walk's hangs: each by its length, with the app's own frames on
the heaviest busy path through its main thread's sample, the deepest first, a Rust frame demangled,
and the time its main thread spent waiting for the next event never counted as work."""

import sys
import re
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import hangs  # noqa: E402

# A main thread sampled ten times: six in the app's work, laying a state out in the engine, three
# waiting for the next event, one in AppKit's own drawing; then another thread, which is not read.
SAMPLE = """Analysis of sampling Scaena (pid 4242) every 1 millisecond
Call graph:
    10 Thread_129100   DispatchQueue_1: com.apple.main-thread  (serial)
    + 10 start  (in dyld) + 6992  [0x180a054e4]
    +   10 Scaena_main  (in Scaena) + 12  [0x1030e11d4]  ScaenaApp.swift:0
    +     10 NSApplicationMain  (in AppKit) + 880  [0x1852967b0]
    +       6 closure #1 in ThemePanel.body.getter  (in Scaena) + 40  [0x1030e2000]  ThemePanel.swift:88
    +       ! 6 DeckEditor.lintShown()  (in Scaena) + 12  [0x1030e3000]  DeckEditor.swift:456
    +       !   6 scaena_call  (in Scaena) + 100  [0x1030e4000]
    +       !     6 _ZN14scaena_session6editor7Session6linted17h0123456789abcdefE  (in Scaena) + 8  [0x1030e5000]
    +       !       5 _ZN13scaena_engine4lint12layout_rules17h0123456789abcdefE  (in Scaena) + 8  [0x1030e6000]
    +       !       : 5 _platform_memmove  (in libsystem_platform.dylib) + 96  [0x180d0000]
    +       !       1 free  (in libsystem_malloc.dylib) + 4  [0x180d1000]
    +       3 __CFRunLoopServiceMachPort  (in CoreFoundation) + 160  [0x180e00000]
    +       ! 3 mach_msg2_trap  (in libsystem_kernel.dylib) + 8  [0x180c00000]
    +       1 -[NSView displayIfNeeded]  (in AppKit) + 20  [0x185200000]
    10 Thread_129101: com.apple.NSEventThread
    + 10 _pthread_start  (in libsystem_pthread.dylib) + 136  [0x180e0000]
    +   10 _ZN3foo3bar17h0000000000000000E  (in Scaena) + 8  [0x1030e7000]

Sort by top of stack, same collapsed (when >= 5):
        mach_msg2_trap  (in libsystem_kernel.dylib)        13
"""


class Demangling(unittest.TestCase):
    def test_a_rust_symbol_reads_as_its_path_without_its_hash(self):
        self.assertEqual(
            hangs.demangled("_ZN14scaena_session6editor7Session6linted17h0123456789abcdefE"),
            "scaena_session::editor::Session::linted",
        )
        self.assertEqual(hangs.demangled("__ZN4core3ptr13drop_in_place17h0123456789abcdefE"), "core::ptr::drop_in_place")
        self.assertEqual(
            hangs.demangled("_ZN49_$LT$alloc..vec..Vec$LT$T$GT$$u20$as$u20$Drop$GT$4drop17h0123456789abcdefE"),
            "<alloc::vec::Vec<T> as Drop>::drop",
        )

    def test_a_v0_symbol_reads_as_its_path_an_impls_method_under_its_type_or_trait(self):
        # Each as CI's Mac sampled it (#1287's walk).
        cases = {
            "_RNvNtNtCsbfBDnDBjFi3_13scaena_engine4lint8contrast6behind": "scaena_engine::lint::contrast::behind",
            "_RNvMs_NtNtCsi4LIHK2nfKK_3std6thread6threadNtB4_6Thread4park": "<std::thread::thread::Thread>::park",
            "_RNvMNtCs6ePwASBLEL8_13scaena_pixels6shaderNtB2_3Job9render_on": "<scaena_pixels::shader::Job>::render_on",
            "_RNCNvNtNtCsbfBDnDBjFi3_13scaena_engine4lint8contrast6behind0B7_": "scaena_engine::lint::contrast::behind::{closure}",
            "_RNvMNtCsjSAAZcmtVt9_4core5sliceSh8split_atCs6ePwASBLEL8_13scaena_pixels": "<[u8]>::split_at",
            "_RNvXs0_NtCs2bgM4g1DgtD_12scaena_paint3cpuNtB5_10CpuPainterNtB7_7Painter5paint": (
                "<scaena_paint::cpu::CpuPainter as scaena_paint::Painter>::paint"
            ),
            "_RINvNtNtCsi4LIHK2nfKK_3std6thread6scoped5scopeNCNvMNtCs6ePwASBLEL8_13scaena_pixels6shaderNtBN_3Job9render_on0uEBP_": (
                "std::thread::scoped::scope"
            ),
        }
        for symbol, path in cases.items():
            self.assertEqual(hangs.demangled(symbol), path)
        # One that does not read is left as it is.
        self.assertEqual(hangs.demangled("_RNvQ"), "_RNvQ")

    def test_any_other_symbol_is_as_it_is(self):
        for symbol in ("DeckEditor.lintShown()", "-[NSView displayIfNeeded]", "free"):
            self.assertEqual(hangs.demangled(symbol), symbol)


class Sampled(unittest.TestCase):
    def test_the_main_threads_heaviest_busy_path_is_the_apps_work_the_deepest_first(self):
        own, taken, busy = hangs.app_frames(SAMPLE)
        self.assertEqual((taken, busy), (10, 7), "three samples waited for the next event")
        self.assertEqual(
            own,
            [
                (5, "scaena_engine::lint::layout_rules"),
                (6, "scaena_session::editor::Session::linted"),
                (6, "scaena_call"),
                (6, "DeckEditor.lintShown()"),
                (6, "closure #1 in ThemePanel.body.getter"),
            ],
            "the frames every stack opens with are left out",
        )
        self.assertEqual(len(hangs.app_frames(SAMPLE, most=2)[0]), 2)

    def test_another_threads_frames_are_not_read(self):
        root = hangs.main_thread(SAMPLE)
        names = []
        stack = [root]
        while stack:
            frame = stack.pop()
            names.append(frame.symbol)
            stack.extend(frame.children)
        self.assertNotIn("foo::bar", names)

    def test_a_main_thread_that_only_waited_has_no_heaviest_path(self):
        idle = """    4 Thread_1   DispatchQueue_1: com.apple.main-thread  (serial)
    + 4 start  (in dyld) + 1  [0x1]
    +   4 Scaena_main  (in Scaena) + 1  [0x2]
    +     4 mach_msg2_trap  (in libsystem_kernel.dylib) + 8  [0x3]
"""
        self.assertEqual(hangs.app_frames(idle), ([], 4, 0))
        self.assertEqual(hangs.app_frames("no threads"), ([], 0, 0))


class Recursion(unittest.TestCase):
    def test_a_recursion_shows_the_frame_that_began_it(self):
        # A comparison of two JSON values, many frames deep under the view that asked for it.
        lines = ["    9 Thread_1   DispatchQueue_1: com.apple.main-thread  (serial)",
                 "    + 9 Scaena_main  (in Scaena) + 1  [0x1]",
                 "    +   9 closure #1 in Inspector.body.getter  (in Scaena) + 1  [0x2]"]
        for depth in range(12):
            name = "static JSONValue.==" if depth % 2 == 0 else "protocol witness for == in JSONValue"
            lines.append("    +   " + "  " * (depth + 1) + f"9 {name}  (in Scaena) + 1  [0x3]")
        own, _, _ = hangs.app_frames("\n".join(lines) + "\n")
        self.assertEqual(
            [symbol for _, symbol in own],
            ["protocol witness for == in JSONValue", "static JSONValue.==", "closure #1 in Inspector.body.getter"],
        )


class Spent(unittest.TestCase):
    def test_a_hangs_busy_samples_go_to_the_app_frame_nearest_them_rust_or_swift(self):
        # Six under the engine's lint, one under AppKit's display beneath the app's Swift entry;
        # the three waiting on the run loop are no one's.
        self.assertEqual(hangs.spent(hangs.main_thread(SAMPLE)), (6, 1))
        self.assertEqual(hangs.where(SAMPLE), "Rust 85%")

    def test_a_hang_in_a_views_body_is_swifts(self):
        swift = SAMPLE.replace("_ZN14scaena_session6editor7Session6linted17h0123456789abcdefE", "DeckEditor.linted()")
        swift = swift.replace("_ZN13scaena_engine4lint12layout_rules17h0123456789abcdefE", "static JSONValue.==")
        self.assertEqual(hangs.where(swift), "Swift 100%")

    def test_a_hang_with_no_busy_sample_or_no_app_frame_says_so(self):
        idle = SAMPLE.replace("DeckEditor.lintShown()  (in Scaena)", "x  (in AppKit)")
        idle = idle.replace("Scaena_main  (in Scaena)", "y  (in AppKit)")
        idle = idle.replace("closure #1 in ThemePanel.body.getter  (in Scaena)", "z  (in AppKit)")
        idle = idle.replace("scaena_call  (in Scaena)", "w  (in AppKit)")
        idle = re.sub(r"(_ZN\w+)  \(in Scaena\)", r"\1  (in AppKit)", idle)
        self.assertEqual(hangs.where(idle), "system")
        self.assertIsNone(hangs.where("no threads"))


class Deep(unittest.TestCase):
    def test_a_stack_deeper_than_pythons_recursion_reads(self):
        # #1310's walk: a hang's main thread 3,000 frames deep ended the report in RecursionError.
        lines = ["    9 Thread_1   DispatchQueue_1: com.apple.main-thread  (serial)",
                 "    + 9 Scaena_main  (in Scaena) + 1  [0x1]"]
        for depth in range(3000):
            symbol = "closure #1 in View.body.getter" if depth % 2 else "_ZN3foo3bar17h0000000000000000E"
            lines.append("    +" + " " * (depth + 2) + f"9 {symbol}  (in Scaena) + 1  [0x3]")
        text = "\n".join(lines) + "\n"
        own, taken, busy = hangs.app_frames(text)
        self.assertEqual((taken, busy), (9, 9))
        self.assertEqual([symbol for _, symbol in own], ["closure #1 in View.body.getter", "foo::bar"])
        self.assertTrue(hangs.where(text).startswith("Swift"), "the deepest app frame, a view's, takes the samples")
        self.assertTrue(any("foo::bar" in line for line in hangs.graph(text)))


class Kept(unittest.TestCase):
    def test_a_samples_main_thread_is_kept_demangled_its_rare_frames_left_out(self):
        kept = hangs.graph(SAMPLE)
        self.assertTrue(kept[0].endswith("com.apple.main-thread  (serial)"))
        self.assertTrue(any("scaena_session::editor::Session::linted  (Scaena)" in line for line in kept), kept)
        self.assertFalse(any("free" in line for line in kept), "a frame one sample held is left out")
        self.assertFalse(any("foo::bar" in line for line in kept), "another thread is not read")

    def test_every_sampled_hang_is_kept_the_longest_first(self):
        with tempfile.TemporaryDirectory() as folder:
            Path(folder, "hang-1.txt").write_text(SAMPLE, encoding="utf-8")
            Path(folder, "hang-2.txt").write_text(SAMPLE, encoding="utf-8")
            Path(folder, "hangs.txt").write_text(
                "hang 1 at 1.0 s: 300 ms hang-1.txt\nhang 2 at 9.0 s: 900 ms hang-2.txt\nhang 3 at 9.5 s: 400 ms -\n",
                encoding="utf-8",
            )
            kept = hangs.samples(folder)
        heads = [line for line in kept if line.startswith("== ")]
        self.assertEqual(heads, ["== hang 2, at 9.0 s: 900 ms", "== hang 1, at 1.0 s: 300 ms"])


class Reported(unittest.TestCase):
    def test_each_hang_is_told_the_longest_first_and_its_frames_summed(self):
        with tempfile.TemporaryDirectory() as folder:
            Path(folder, "hang-1.txt").write_text(SAMPLE, encoding="utf-8")
            Path(folder, "hang-3.txt").write_text(SAMPLE, encoding="utf-8")
            Path(folder, "hangs.txt").write_text(
                "hang 1 at 12.5 s: 300 ms hang-1.txt\n"
                "hang 2 at 20.0 s: 260 ms -\n"
                "hang 3 at 31.2 s: 4100 ms+ hang-3.txt\n",
                encoding="utf-8",
            )
            said = hangs.report(folder, frames=2, top=3)
        self.assertEqual(said[0], "3 hang(s), 4660 ms in all, the longest first:")
        self.assertEqual(said[1], "hang 3, at 31.2 s: 4100 ms and still going when the walk ended [Rust 85%]")
        self.assertEqual(said[2].split(), ["5", "scaena_engine::lint::layout_rules"])
        self.assertIn("hang 2, at 20.0 s: 260 ms", said)
        self.assertIn("  (not sampled)", said)
        summed = said[said.index("the app's frames by the hang time they were on the heaviest path for:") + 1 :]
        self.assertEqual(summed[0].split(), ["4400", "ms", "scaena_engine::lint::layout_rules"])
        self.assertEqual(len(summed), 2)

    def test_no_hang_is_said_so(self):
        with tempfile.TemporaryDirectory() as folder:
            self.assertEqual(hangs.report(folder), ["no hang over the threshold"])
            Path(folder, "hangs.txt").write_text("", encoding="utf-8")
            self.assertEqual(hangs.report(folder), ["no hang over the threshold"])


if __name__ == "__main__":
    unittest.main()
