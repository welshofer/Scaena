// Every time the Mac app's main thread does not answer for longer than a threshold, while the walk
// runs (apps/mac/window.sh): what it was doing, sampled. A person sees the spinning wait, and
// VoiceOver hears nothing, as the accessibility API does: so the app's main thread is asked, by that
// API, for its role every 50 ms, each question waited on no longer than the threshold. A question
// that goes unanswered is a hang: `sample` takes the app's main thread then, into `hang-N.txt`
// in the folder given, one at a time, and the hang's length is told once the app answers again,
// one line in `hangs.txt` for each. `scripts/hangs.py` reads them.
//
//   swiftc -O -o hangs apps/mac/hangs.swift
//   hangs PID FOLDER [THRESHOLD_MS]      (until the app quits, or SIGTERM)
import ApplicationServices
import Foundation

func fail(_ said: String) -> Never {
    FileHandle.standardError.write(Data((said + "\n").utf8))
    exit(1)
}

let arguments = CommandLine.arguments
guard (3...4).contains(arguments.count), let pid = pid_t(arguments[1]) else {
    fail("usage: hangs PID FOLDER [THRESHOLD_MS]")
}
let folder = URL(fileURLWithPath: arguments[2], isDirectory: true)
let threshold = arguments.count == 4 ? (Double(arguments[3]) ?? 250) / 1000 : 0.25
guard AXIsProcessTrusted() else { fail("not trusted to read another app's accessibility") }
try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
FileManager.default.createFile(atPath: folder.appendingPathComponent("hangs.txt").path, contents: nil)
guard let log = FileHandle(forWritingAtPath: folder.appendingPathComponent("hangs.txt").path) else {
    fail("cannot write \(folder.path)/hangs.txt")
}

// A SIGTERM ends the watch as the app quitting does, the hang under way told.
nonisolated(unsafe) var stopping: sig_atomic_t = 0
signal(SIGTERM) { _ in stopping = 1 }

/// Whether the app's main thread answers within `timeout` seconds; nil where the app is gone.
func answers(_ app: AXUIElement, within timeout: Double) -> Bool? {
    AXUIElementSetMessagingTimeout(app, Float(timeout))
    var role: CFTypeRef?
    switch AXUIElementCopyAttributeValue(app, kAXRoleAttribute as CFString, &role) {
    case .success: return true
    case .cannotComplete: return kill(pid, 0) == 0 ? false : nil
    default: return kill(pid, 0) == 0 ? true : nil
    }
}

func said(_ line: String) {
    log.write(Data((line + "\n").utf8))
    print(line)
    fflush(stdout)
}

let app = AXUIElementCreateApplication(pid)
let began = Date()
var count = 0
var sampling: Process?
/// The most hangs sampled: past it, each is still told, unsampled.
let most = 40
while stopping == 0 {
    guard let answered = answers(app, within: threshold) else { break }
    if answered {
        usleep(50_000)
        continue
    }
    // A hang, from when the question that went unanswered was asked.
    count += 1
    let from = Date().timeIntervalSince(began) - threshold
    var file = "-"
    if count <= most, sampling?.isRunning != true {
        file = "hang-\(count).txt"
        let taking = Process()
        taking.executableURL = URL(fileURLWithPath: "/usr/bin/sample")
        taking.arguments = ["\(pid)", "1", "1", "-mayDie", "-file", folder.appendingPathComponent(file).path]
        taking.standardOutput = FileHandle.nullDevice
        taking.standardError = FileHandle.nullDevice
        do { try taking.run() } catch { file = "-" }
        sampling = taking
    }
    let start = Date()
    var gone = false
    while stopping == 0 {
        guard let again = answers(app, within: 1) else {
            gone = true
            break
        }
        if again { break }
    }
    let lasted = Int((Date().timeIntervalSince(start) + threshold) * 1000)
    said(String(format: "hang %d at %.1f s: %d ms%@ %@", count, from, lasted, gone || stopping != 0 ? "+" : "", file))
    if gone { break }
}
sampling?.waitUntilExit()
