import Foundation
import Testing

/// Every chord the iPad's UI tests send goes through their `chord`
/// (`apps/ipad/ScaenaUITests/Opening.swift`), which waits on the chord's effect and, where XCUITest
/// on the simulator delivered it bare, puts back what the key alone did and sends it again
/// (`docs/gate-4.md`). Two are sent once, each where it stands and nowhere else: ⌘. as Escape,
/// which the canvas hears where Escape is never heard, and ⌘D in a text typed in, whose premise is
/// that it does nothing, so no try could tell a bare d from the bug. A key sent with a modifier anywhere else fails here, by its file and line.
@Test func theIPadsUITestsSendEveryChordThroughChord() throws {
    let folder = repository.appending(path: "apps/ipad/ScaenaUITests")
    let names = try FileManager.default.contentsOfDirectory(atPath: folder.path).filter { $0.hasSuffix(".swift") }
    #expect(names.contains("Opening.swift"), "no UI tests at \(folder.path)")
    // Each line allowed, and how many times: `chord`'s own press, then the two sent once.
    let allowed = [
        "app.typeKey(key, modifierFlags: modifiers)": 1,
        #"app.typeKey(".", modifierFlags: .command)"#: 1,
        #"app.typeKey("d", modifierFlags: .command)"#: 1,
    ]
    var seen: [String: Int] = [:]
    var raw: [String] = []
    for name in names.sorted() {
        let lines = try String(contentsOf: folder.appending(path: name), encoding: .utf8).components(separatedBy: "\n")
        let sent = lines.enumerated().filter { $0.element.contains("modifierFlags:") }
        for (i, line) in sent where !line.contains("modifierFlags: [])") {
            let code = line.trimmingCharacters(in: .whitespaces)
            seen[code, default: 0] += 1
            if let most = allowed[code], seen[code, default: 0] <= most { continue }
            raw.append("\(name):\(i + 1): \(code)")
        }
    }
    #expect(raw.isEmpty, "a chord sent outside `chord`: \(raw.joined(separator: "; "))")
}
