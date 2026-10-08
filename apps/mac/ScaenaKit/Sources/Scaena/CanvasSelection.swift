import AppKit
import ScaenaKit
import SwiftUI

/// Over the canvas (PLAN 3.4, 3.7, ADR-0013): a click selects the node the engine says draws
/// there, the topmost a pointer does not pass over (a locked node, PLAN 2.95), and the node
/// selected is outlined where it is drawn. A drag moves it and a handle resizes it, as the
/// browser's canvas does: the engine draws it moved, laying nothing out, and says where it lands,
/// snapped to the theme's grid, into a slot, or among a stack's children, with the guides it
/// meets; the drop is that one `place` patch, written where the placement lives, or kept to the
/// state shown with Option. Shift takes a node off the theme's grid, as a `rect`, or puts one back
/// on it; a node off the grid is flagged, as lint flags it (W301). A double click types in a text
/// where it stands (PLAN 3.9): a press in it puts the caret there, and one outside it stops
/// typing. Every box and caret is the engine's, at rest; nothing here lays out.
struct CanvasSelection: View {
    let editor: DeckEditor
    let state: String
    /// The canvas, in canvas units.
    let size: CGSize
    @Binding var node: String?
    /// Text typed in place, which takes the keys while it types.
    let typing: Typing
    /// Make the patch a drag ended in: one step to undo.
    let make: ([JSONValue]) -> Void
    @State private var boxes: [NodeBox] = []
    /// Where each node the state shows is placed.
    @State private var placements: [String: JSONValue] = [:]
    @State private var press: Press?
    /// A resize that paused, shown laid out as its patch would make it.
    @State private var pausing: Task<Void, Never>?

    /// How long a resize pauses before the canvas shows its text reflowed (as the browser's).
    private static let pause = Duration.milliseconds(300)
    /// How near, in points, an edge moved off the grid goes onto another's (PLAN 2.57).
    private static let reach = 6.0
    /// How far outside the text typed in, in points, a press still puts the caret in it.
    private static let slop = 4.0

    /// A press on the canvas: a click until it moves, then a drag, or one refused; or a press in
    /// the text typed in, which selects as it drags.
    private enum Press {
        case pressing
        case dragging(Drag)
        case refused(String)
        case typing
    }

    /// A drag under way: the node, how it is held, and where it would land now.
    private struct Drag {
        let node: String
        let targets: Targets
        let at: JSONValue?
        /// The handle a resize holds: none for a move.
        let edge: Edge?
        let revision: Int
        var by = CGVector.zero
        var how: SnapMode?
        var fork = false
        var snapped: Snapped?
        var states: [String] = []
        var reached: [JSONValue] = []
    }

    /// A handle of the box selected: a corner, or the middle of an edge.
    private enum Edge: CaseIterable {
        case n, s, e, w, ne, nw, se, sw

        var north: Bool { self == .n || self == .ne || self == .nw }
        var south: Bool { self == .s || self == .se || self == .sw }
        var east: Bool { self == .e || self == .ne || self == .se }
        var west: Bool { self == .w || self == .nw || self == .sw }

        /// Where it stands on `r`.
        func point(_ r: CGRect) -> CGPoint {
            CGPoint(x: west ? r.minX : east ? r.maxX : r.midX, y: north ? r.minY : south ? r.maxY : r.midY)
        }
    }

    private var drag: Drag? {
        if case .dragging(let d) = press { return d }
        return nil
    }

    var body: some View {
        GeometryReader { geometry in
            let scale = geometry.size.width / max(size.width, 1)
            ZStack(alignment: .topLeading) {
                // Under the rest: it takes the keys while a text is typed in, and no press.
                TypingHost(typing: typing, canvas: size)
                    .allowsHitTesting(false)
                Color.clear
                    .contentShape(Rectangle())
                    .gesture(pressing(scale))
                // Nodes off the theme's grid, flagged as lint flags them (W301).
                ForEach(boxes.filter { placements[$0.node]?.offGrid == true }, id: \.node) { flagged in
                    outline(flagged.corners, scale: scale)
                        .stroke(Color.orange, style: StrokeStyle(lineWidth: 1, dash: [4, 3]))
                        .allowsHitTesting(false)
                }
                if let selected = boxes.first(where: { $0.node == node }) {
                    let by = drag?.edge == nil ? (drag?.by ?? .zero) : .zero
                    let offGrid = placements[selected.node]?.offGrid == true
                    outline(selected.corners.map { CGPoint(x: $0.x + by.dx, y: $0.y + by.dy) }, scale: scale)
                        .stroke(offGrid ? Color.orange : Color.accentColor, lineWidth: 1.5)
                        .allowsHitTesting(false)
                    if drag == nil, !typing.typing, selected.transform == nil, selected.locked == nil, let r = box(selected) {
                        ForEach(Edge.allCases, id: \.self) { edge in
                            let p = edge.point(r)
                            Rectangle()
                                .fill(Color.white)
                                .overlay(Rectangle().stroke(Color.accentColor, lineWidth: 1))
                                .frame(width: 7, height: 7)
                                .position(x: p.x * scale, y: p.y * scale)
                                .allowsHitTesting(false)
                        }
                    }
                }
                if let landing = drag?.snapped {
                    let r = landing.cell
                    Path(CGRect(x: r.minX * scale, y: r.minY * scale, width: r.width * scale, height: r.height * scale))
                        .stroke(Color.accentColor, style: StrokeStyle(lineWidth: 1, dash: [6, 4]))
                        .allowsHitTesting(false)
                    ForEach(landing.guides.indices, id: \.self) { i in
                        let line = landing.guides[i]
                        Path { path in
                            path.move(to: CGPoint(x: line[0].x * scale, y: line[0].y * scale))
                            path.addLine(to: CGPoint(x: line[1].x * scale, y: line[1].y * scale))
                        }
                        .stroke(Color.pink, lineWidth: 1)
                        .allowsHitTesting(false)
                    }
                }
                typed(scale)
                    .allowsHitTesting(false)
                if let said = told ?? typing.told {
                    Text(said)
                        .font(.caption)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(.regularMaterial, in: Capsule())
                        .padding(8)
                        .allowsHitTesting(false)
                }
            }
        }
        .task(id: "\(state)\u{1f}\(editor.revision)") {
            boxes = (try? editor.session.boxes(state: state)) ?? []
            placements = (try? editor.session.placements(state: state)) ?? [:]
            // The text typed in, read again where something else changed it.
            typing.sync(shown: state)
        }
        .onChange(of: node) { _, now in
            // Another node selected, in the layers or by a finding, stops typing.
            if typing.typing, typing.node != now { typing.leave() }
        }
    }

    /// The caret, or the selection, in the text typed in, where it is drawn, and what an input
    /// method composes there, underlined.
    @ViewBuilder private func typed(_ scale: CGFloat) -> some View {
        if typing.typing {
            let covered = typing.covered
            ForEach(covered.indices, id: \.self) { i in
                outline(covered[i], scale: scale).fill(Color.accentColor.opacity(0.3))
            }
            let composing = typing.composing
            ForEach(composing.indices, id: \.self) { i in
                line(composing[i][3], composing[i][2], scale: scale).stroke(Color.accentColor, lineWidth: 1.5)
            }
            if typing.from == typing.to, let caret = typing.caret, caret.count == 2 {
                line(caret[0], caret[1], scale: scale).stroke(Color.accentColor, lineWidth: 2)
            }
        }
    }

    /// What the drag does, as the browser's status line says it.
    private var told: String? {
        switch press {
        case .refused(let why):
            return why
        case .dragging(let d):
            guard d.how != nil else { return "\(d.node) is not placed that way" }
            guard let snapped = d.snapped else { return "\(d.node) lands nowhere new" }
            if snapped.patch.isEmpty { return "\(d.node) stays where it is" }
            let n = d.states.count
            let inWhich = n == 1 && d.states.first == state ? "in this state" : "in \(n) states"
            let keep = d.fork ? " · kept to \(state)" : n > 1 ? " · Option keeps it to \(state)" : ""
            return "\(d.node) → \(placed(snapped.patch)) · \(inWhich)\(keep)"
        default:
            return nil
        }
    }

    /// Where a patch places its node, as the status says it.
    private func placed(_ patch: [JSONValue]) -> String {
        guard let at = patch.first?["at"] else { return "placed" }
        if let slot = at["in"]?.string { return "slot \(slot)" }
        if let area = at["area"]?.string { return "area \(area)" }
        if let rect = at["rect"], rect != .null { return "off the grid, as lint will flag (W301)" }
        return "the grid's cells"
    }

    /// A press: a click where it does not move, else a drag of what it pressed.
    private func pressing(_ scale: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 0, coordinateSpace: .local)
            .onChanged { value in
                let moved = hypot(value.translation.width, value.translation.height)
                switch press {
                case nil:
                    // Typing: a press in the text puts the caret there, one outside it stops typing.
                    let at = CGPoint(x: value.startLocation.x / scale, y: value.startLocation.y / scale)
                    if typing.typing {
                        if typing.holds(at, slop: Self.slop / max(scale, 0.01)) {
                            press = .typing
                            let clicks = NSApp.currentEvent?.clickCount ?? 1
                            return typing.press(at: at, clicks: clicks, extend: NSEvent.modifierFlags.contains(.shift))
                        }
                        typing.leave()
                    }
                    press = .pressing
                case .typing:
                    typing.drag(to: CGPoint(x: value.location.x / scale, y: value.location.y / scale))
                case .pressing where moved >= 3:
                    begin(at: CGPoint(x: value.startLocation.x / scale, y: value.startLocation.y / scale), scale: scale)
                    if case .dragging = press { aim(value, scale: scale) }
                case .dragging:
                    aim(value, scale: scale)
                default:
                    break
                }
            }
            .onEnded { value in
                defer { press = nil }
                switch press {
                case .dragging:
                    aim(value, scale: scale)
                    drop()
                case .pressing, nil:
                    pick(CGPoint(x: value.location.x / scale, y: value.location.y / scale))
                case .typing:
                    // The keys stay with the text, whatever took them as the press ended.
                    typing.focus?()
                default:
                    break
                }
            }
    }

    /// Select what draws at `point`, canvas units: nothing where nothing does. The second click of
    /// a double click on a text types in it there, kept to the state with Option (PLAN 3.9).
    private func pick(_ point: CGPoint) {
        let hits = (try? editor.session.hits(state: state, at: point)) ?? []
        node = hits.first { $0.locked == nil }?.node
        if let node, (NSApp.currentEvent?.clickCount ?? 1) >= 2 {
            typing.enter(node, in: state, at: point, fork: NSEvent.modifierFlags.contains(.option))
        }
    }

    /// A drag begun at `point`, canvas units: of a handle of the box selected, a resize; else a
    /// move of what draws there, selected.
    private func begin(at point: CGPoint, scale: CGFloat) {
        var held: (node: String, edge: Edge?)?
        if let selected = boxes.first(where: { $0.node == node }), selected.transform == nil,
            selected.locked == nil, let r = box(selected)
        {
            let near = Self.reach / max(scale, 0.01)
            if let edge = Edge.allCases.first(where: { hypot($0.point(r).x - point.x, $0.point(r).y - point.y) <= near }) {
                held = (selected.node, edge)
            }
        }
        if held == nil {
            let hits = (try? editor.session.hits(state: state, at: point)) ?? []
            guard let hit = hits.first(where: { $0.locked == nil }) else {
                press = .refused("nothing to move here")
                return
            }
            node = hit.node
            guard hit.transform == nil else {
                press = .refused("\(hit.node) is turned or scaled: move it in the browser's editor for now")
                return
            }
            held = (hit.node, nil)
        }
        guard let held else { return }
        do {
            let targets = try editor.session.targets(state: state, node: held.node)
            editor.still()
            press = .dragging(
                Drag(
                    node: held.node, targets: targets, at: placements[held.node], edge: held.edge,
                    revision: editor.revision))
        } catch {
            press = .refused("\(held.node) cannot be moved: \(error)")
        }
    }

    /// Where the drag lands with the pointer where it is now: the node drawn there, and its
    /// landing, guides, and the states it changes asked of the engine.
    private func aim(_ value: DragGesture.Value, scale: CGFloat) {
        guard var d = drag, d.revision == editor.revision else { return }
        let flags = NSEvent.modifierFlags
        d.by = CGVector(dx: value.translation.width / scale, dy: value.translation.height / scale)
        d.fork = flags.contains(.option)
        d.how = d.targets.snap(at: d.at, resize: d.edge != nil, shift: flags.contains(.shift))
        let to = d.edge.map { resized(d.targets.cell, $0, d.by) } ?? d.targets.cell.offsetBy(dx: d.by.dx, dy: d.by.dy)
        if d.edge == nil { editor.move([d.node], by: d.by) }
        if let how = d.how {
            d.snapped = try? editor.session.snap(
                state: state, node: d.node, how: how, to: to, fork: d.fork, reach: Self.reach / max(scale, 0.01))
        } else {
            d.snapped = nil
        }
        let patch = d.snapped?.patch ?? []
        if patch != d.reached {
            d.reached = patch
            d.states = patch.isEmpty ? [] : (try? editor.session.reach(patch)) ?? []
        }
        press = .dragging(d)
        // A resize that pauses shows the node laid out as its patch would make it.
        pausing?.cancel()
        if d.edge != nil, !patch.isEmpty {
            pausing = Task { @MainActor in
                try? await Task.sleep(for: Self.pause)
                guard !Task.isCancelled else { return }
                editor.show(patch)
            }
        }
    }

    /// The drag ends where the pointer let go: the patch where it lands, one step to undo.
    private func drop() {
        pausing?.cancel()
        editor.still()
        guard let d = drag, d.revision == editor.revision else { return }
        guard let patch = d.snapped?.patch, !patch.isEmpty else { return }
        make(patch)
    }

    /// `selected`'s box at rest, canvas units.
    private func box(_ selected: NodeBox) -> CGRect? {
        guard selected.rect.count == 4 else { return nil }
        return CGRect(x: selected.rect[0], y: selected.rect[1], width: selected.rect[2], height: selected.rect[3])
    }

    /// `cell` with the edges `edge` holds moved `by`, never below a unit across.
    private func resized(_ cell: CGRect, _ edge: Edge, _ by: CGVector) -> CGRect {
        var (x, y, w, h) = (cell.minX, cell.minY, cell.width, cell.height)
        if edge.east { w += by.dx }
        if edge.west {
            let dx = min(by.dx, w - 1)
            x += dx
            w -= dx
        }
        if edge.south { h += by.dy }
        if edge.north {
            let dy = min(by.dy, h - 1)
            y += dy
            h -= dy
        }
        return CGRect(x: x, y: y, width: max(1, w), height: max(1, h))
    }

    /// The line from `a` to `b`, canvas units, as a path on the view.
    private func line(_ a: CGPoint, _ b: CGPoint, scale: CGFloat) -> Path {
        Path { path in
            path.move(to: CGPoint(x: a.x * scale, y: a.y * scale))
            path.addLine(to: CGPoint(x: b.x * scale, y: b.y * scale))
        }
    }

    /// `corners`, canvas units, as a path on the view.
    private func outline(_ corners: [CGPoint], scale: CGFloat) -> Path {
        Path { path in
            guard let first = corners.first else { return }
            path.move(to: CGPoint(x: first.x * scale, y: first.y * scale))
            for corner in corners.dropFirst() {
                path.addLine(to: CGPoint(x: corner.x * scale, y: corner.y * scale))
            }
            path.closeSubpath()
        }
    }
}
