// What the objects an element of the Mac app's window holds read as to VoiceOver (PLAN 3.17): each
// by its role, what VoiceOver says it is, and its frame in screen points from the top left, one to a
// line; or, with `self`, what the element reads as itself: what VoiceOver says it is, and its value
// after a comma where it has one. SwiftUI keeps a label as an attributed description
// (AXAttributedDescription), which System Events cannot read as text (`frames.js holds` reads none),
// so this asks the accessibility API itself. The walk builds it and reads the canvas with it
// (apps/mac/window.sh).
//
//   swiftc -O -o holds apps/mac/holds.swift
//   holds PID IDENTIFIER [self]
import ApplicationServices
import Foundation

func fail(_ said: String) -> Never {
    FileHandle.standardError.write(Data((said + "\n").utf8))
    exit(1)
}

let arguments = CommandLine.arguments
guard arguments.count == 3 || (arguments.count == 4 && arguments[3] == "self"), let pid = pid_t(arguments[1]) else {
    fail("usage: holds PID IDENTIFIER [self]")
}
guard AXIsProcessTrusted() else { fail("not trusted to read another app's accessibility") }

/// The value of the attribute `name` of `element`; none where it has none or does not answer.
func value(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
    var read: CFTypeRef?
    return AXUIElementCopyAttributeValue(element, name as CFString, &read) == .success ? read : nil
}

func children(_ element: AXUIElement) -> [AXUIElement] {
    (value(element, kAXChildrenAttribute) as? [AXUIElement]) ?? []
}

/// What VoiceOver says an element is: the words of its attributed description, else its
/// description, else its title; none where it has none of them.
func label(_ element: AXUIElement) -> String {
    if let said = value(element, "AXAttributedDescription") as? NSAttributedString { return said.string }
    return (value(element, kAXDescriptionAttribute) as? String) ?? (value(element, kAXTitleAttribute) as? String) ?? ""
}

/// Its frame, rounded to points: x, y, width, height.
func frame(_ element: AXUIElement) -> [Int] {
    var point = CGPoint.zero
    var size = CGSize.zero
    if let at = value(element, kAXPositionAttribute) { AXValueGetValue(at as! AXValue, .cgPoint, &point) }
    if let extent = value(element, kAXSizeAttribute) { AXValueGetValue(extent as! AXValue, .cgSize, &size) }
    return [point.x, point.y, size.width, size.height].map { Int($0.rounded()) }
}

// The first element identified as the tag, breadth first through the app's windows, past its menu
// bar; a busy app is waited on for two seconds a question.
let app = AXUIElementCreateApplication(pid)
AXUIElementSetMessagingTimeout(app, 2)
var level = [app]
var found: AXUIElement?
var seen = 0
search: while !level.isEmpty, seen < 8000 {
    var next: [AXUIElement] = []
    for element in level {
        seen += 1
        if (value(element, "AXIdentifier") as? String) == arguments[2] {
            found = element
            break search
        }
        if (value(element, kAXRoleAttribute) as? String) != kAXMenuBarRole { next += children(element) }
    }
    level = next
}
guard let held = found else { fail("nothing is identified as \(arguments[2])") }
if arguments.count == 4 {
    let said = label(held)
    guard !said.isEmpty else { fail("\(arguments[2]) reads as nothing") }
    let now = (value(held, kAXValueAttribute) as? String) ?? ""
    print(now.isEmpty ? said : "\(said), \(now)")
    exit(0)
}
for element in children(held) {
    let role = (value(element, kAXRoleAttribute) as? String) ?? "?"
    print("\(role) \"\(label(element))\" \(frame(element).map { String($0) }.joined(separator: " "))")
}
