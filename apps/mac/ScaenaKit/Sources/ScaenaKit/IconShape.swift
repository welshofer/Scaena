import SwiftUI

/// An icon's path drawn in SwiftUI (PLAN 2.104): its data, the absolute moves, lines, cubics, and
/// closes `scripts/build_icons.py` writes, scaled alike both ways so its view fits the rect and
/// centered, as the engine fits a path shape by its `view`. Chrome draws it, the picker's icons;
/// the deck's are the engine's.
public struct IconShape: Shape {
    let elements: [Element]
    let view: CGRect

    public init(_ icon: Icon) {
        elements = Self.parse(icon.path)
        let v = icon.view.count == 4 ? icon.view : [0, 0, 256, 256]
        view = CGRect(x: v[0], y: v[1], width: v[2], height: v[3])
    }

    public func path(in rect: CGRect) -> Path {
        guard view.width > 0, view.height > 0 else { return Path() }
        let scale = min(rect.width / view.width, rect.height / view.height)
        let dx = rect.minX + (rect.width - view.width * scale) / 2 - view.minX * scale
        let dy = rect.minY + (rect.height - view.height * scale) / 2 - view.minY * scale
        let at = { (p: CGPoint) in CGPoint(x: p.x * scale + dx, y: p.y * scale + dy) }
        var path = Path()
        for element in elements {
            switch element {
            case .move(let p): path.move(to: at(p))
            case .line(let p): path.addLine(to: at(p))
            case .curve(let c1, let c2, let p): path.addCurve(to: at(p), control1: at(c1), control2: at(c2))
            case .close: path.closeSubpath()
            }
        }
        return path
    }

    /// A step of the path.
    enum Element: Equatable {
        case move(CGPoint)
        case line(CGPoint)
        case curve(CGPoint, CGPoint, CGPoint)
        case close
    }

    /// `d`'s steps: each command a letter, its numbers after it parted by commas. A step that is
    /// not a move, a line, a cubic, or a close, or that has the wrong count of numbers, is passed
    /// over.
    static func parse(_ d: String) -> [Element] {
        var out: [Element] = []
        var command: Character?
        var field = ""
        var numbers: [CGFloat] = []
        func number() {
            if let v = Double(field) { numbers.append(CGFloat(v)) }
            field = ""
        }
        func step() {
            defer { numbers = [] }
            let p = { (i: Int) in CGPoint(x: numbers[i], y: numbers[i + 1]) }
            switch (command, numbers.count) {
            case ("M", 2): out.append(.move(p(0)))
            case ("L", 2): out.append(.line(p(0)))
            case ("C", 6): out.append(.curve(p(0), p(2), p(4)))
            case ("Z", 0): out.append(.close)
            default: break
            }
        }
        for ch in d {
            if "MLCZ".contains(ch) {
                number()
                step()
                command = ch
            } else if ch == "," {
                number()
            } else {
                field.append(ch)
            }
        }
        number()
        step()
        return out
    }
}
