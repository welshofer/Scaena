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
/// typing. A press takes the keyboard for the canvas: Delete takes the node selected out of the
/// state shown and those after, Shift+Delete out of the deck, and Escape selects what holds it
/// (PLAN 3.11); Copy, Cut, and Paste are the window's (PLAN 3.12). Several are selected (PLAN
/// 3.13), children of one container, by Shift and a click, or a marquee dragged across the
/// canvas from where nothing draws; a drag of one moves them all, the first snapped as it would be
/// alone and the rest as far as it went. The node selected has handles (PLAN 3.16), as the
/// browser's canvas has: one above its box turns it about its anchor, whole degrees or with Shift
/// fifteens; a line's, an arrow's, or a polygon's points move, a press at an edge's middle adds
/// one, and Delete takes the one picked away; a rect's corner rounds it to the theme's radius steps;
/// and an image's crop bars crop it from a side, and its focal point moves. Each is one `choose`,
/// kept to the state with Option. A node turned or scaled moves and resizes through what draws it.
/// Every box and caret is the engine's, at rest; nothing here lays out.
struct CanvasSelection: View {
    let editor: DeckEditor
    let state: String
    /// The canvas, in canvas units.
    let size: CGSize
    /// How close the canvas is shown, and the part of it shown (PLAN 3.16): a pinch, or the wheel
    /// with ⌘, zooms about the pointer, and the wheel pans what is zoomed in.
    @Binding var zoom: Zoom
    /// The node selected, and the others selected with it, children of the same container.
    @Binding var node: String?
    @Binding var also: [String]
    /// Text typed in place, which takes the keys while it types.
    let typing: Typing
    /// Where the pointer last pressed, canvas units: where Insert puts what it inserts.
    @Binding var pointed: CGPoint?
    /// What the window says of an edit the canvas's keys or its menu made, till the next press.
    @Binding var said: String?
    /// Take the node selected out of the state shown and those after, or, `true`, out of the deck.
    let delete: (Bool) -> Void
    /// The Edit menu's Copy, Cut, and Paste while no text is typed in (PLAN 3.12).
    let clip: (CanvasKeys.Clipping) -> Void
    /// The Edit menu's Find, and ⌘F on the canvas: the deck's find bar (PLAN 3.16).
    let finding: (NSTextFinder.Action) -> Void
    /// Make the patch a drag ended in: one step to undo.
    let make: ([JSONValue]) -> Void
    @State private var boxes: [NodeBox] = []
    /// Where each node the state shows is placed.
    @State private var placements: [String: JSONValue] = [:]
    @State private var press: Press?
    /// The shape selected, its outline, and the point picked on it, which Delete takes away; the
    /// image selected, its framing (PLAN 3.16).
    @State private var shaped: Outline?
    @State private var pointPicked: Int?
    @State private var imaged: Framing?
    /// A resize that paused, shown laid out as its patch would make it.
    @State private var pausing: Task<Void, Never>?

    /// How long a resize pauses before the canvas shows its text reflowed (as the browser's).
    private static let pause = Duration.milliseconds(300)
    /// How near, in points, an edge moved off the grid goes onto another's (PLAN 2.57).
    private static let reach = 6.0
    /// How far outside the text typed in, in points, a press still puts the caret in it.
    private static let slop = 4.0

    /// A press on the canvas: a click until it moves, then a drag, or one refused; a press in
    /// the text typed in, which selects as it drags; a marquee, from where nothing draws; or a
    /// handle of the node selected held.
    private enum Press {
        case pressing
        case dragging(Drag)
        case refused(String)
        case typing
        case banding(from: CGPoint, to: CGPoint)
        case handling(Holding)
    }

    /// A drag under way: the node, how it is held, and where it would land now; with others
    /// selected, all of them moved together.
    private struct Drag {
        let node: String
        /// Every node it moves, `node` first: more than one where several are selected.
        let together: [String]
        let targets: Targets
        let at: JSONValue?
        /// The handle a resize holds: none for a move.
        let edge: Edge?
        let revision: Int
        var by = CGVector.zero
        var how: SnapMode?
        var fork = false
        var snapped: Snapped?
        /// Where the nodes moved together land, and the patch.
        var arranged: Arranged?
        var states: [String] = []
        var reached: [JSONValue] = []

        /// The patch where it lands now.
        var patch: [JSONValue] { together.count > 1 ? arranged?.patch ?? [] : snapped?.patch ?? [] }
    }

    /// Every node selected, the first first.
    private var selection: [String] { node.map { [$0] + also } ?? [] }

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

    private var holding: Holding? {
        if case .handling(let h) = press { return h }
        return nil
    }

    var body: some View {
        GeometryReader { geometry in
            let fit = Fit(shown: zoom.view, canvas: size, width: geometry.size.width)
            ZStack(alignment: .topLeading) {
                // Under the rest: the canvas's keys, the text's while one is typed in. It takes no
                // press.
                CanvasKeysHost(
                    typing: typing, canvas: size, shown: zoom.view, command: command, clipping: clip, finding: finding)
                    .allowsHitTesting(false)
                // The wheel and a pinch over the canvas, read before any view takes them.
                CanvasWheel(
                    zoom: { factor, at in zoom.zoom(to: zoom.level * factor, about: fit.canvas(at)) },
                    pan: { by in
                        guard zoom.level > 1 else { return false }
                        zoom.pan(by: CGVector(dx: Double(by.dx) * fit.units, dy: Double(by.dy) * fit.units))
                        return true
                    }
                )
                .allowsHitTesting(false)
                Color.clear
                    .contentShape(Rectangle())
                    .gesture(pressing(fit))
                // Nodes off the theme's grid, flagged as lint flags them (W301).
                ForEach(boxes.filter { placements[$0.node]?.offGrid == true }, id: \.node) { flagged in
                    outline(flagged.corners, fit: fit)
                        .stroke(Color.orange, style: StrokeStyle(lineWidth: 1, dash: [4, 3]))
                        .allowsHitTesting(false)
                }
                // The others selected with the first, each outlined, moved as the drag moves.
                ForEach(boxes.filter { also.contains($0.node) }, id: \.node) { other in
                    let by = drag?.edge == nil ? (drag?.by ?? .zero) : .zero
                    outline(other.corners.map { CGPoint(x: $0.x + by.dx, y: $0.y + by.dy) }, fit: fit)
                        .stroke(Color.accentColor.opacity(0.8), lineWidth: 1.5)
                        .allowsHitTesting(false)
                }
                if let selected = boxes.first(where: { $0.node == node }) {
                    let by = drag?.edge == nil ? (drag?.by ?? .zero) : .zero
                    let offGrid = placements[selected.node]?.offGrid == true
                    outline(selected.corners.map { CGPoint(x: $0.x + by.dx, y: $0.y + by.dy) }, fit: fit)
                        .stroke(offGrid ? Color.orange : Color.accentColor, lineWidth: 1.5)
                        .allowsHitTesting(false)
                    if drag == nil, holding == nil, also.isEmpty, !typing.typing, selected.locked == nil,
                        let r = box(selected)
                    {
                        ForEach(Edge.allCases, id: \.self) { edge in
                            let p = selected.onCanvas(edge.point(r))
                            Rectangle()
                                .fill(Color.white)
                                .overlay(Rectangle().stroke(Color.accentColor, lineWidth: 1))
                                .frame(width: 7, height: 7)
                                .position(fit.view(p))
                                .allowsHitTesting(false)
                        }
                        // Its handles over its box's (PLAN 3.16).
                        HandleMarks(
                            box: selected, outline: shaped?.node == selected.node ? shaped : nil,
                            framing: imaged?.node == selected.node ? imaged : nil, picked: pointPicked, fit: fit
                        )
                        .allowsHitTesting(false)
                    }
                    if let holding, holding.node == selected.node {
                        HandleHolding(holding: holding, box: selected, outline: shaped, framing: imaged, fit: fit)
                            .allowsHitTesting(false)
                    }
                }
                landings(fit)
                    .allowsHitTesting(false)
                if let r = banded {
                    Path(r).applying(fit.transform)
                        .stroke(Color.accentColor, style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
                        .allowsHitTesting(false)
                }
                typed(fit)
                    .allowsHitTesting(false)
                if let words = told ?? typing.told ?? said {
                    Text(words)
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
        .task(id: "\(state)\u{1f}\(editor.revision)\u{1f}\(node ?? "")") {
            // The node selected's handles: a shape's outline, an image's framing (PLAN 3.16).
            let session = editor.session
            shaped = node.flatMap { try? session.outline(state: state, node: $0) }
            imaged = node.flatMap { try? session.framing(state: state, node: $0) }
            if let picked = pointPicked, picked >= shaped?.points.count ?? 0 { pointPicked = nil }
        }
        .onChange(of: node) { _, now in
            // Another node selected, in the layers or by a finding, stops typing.
            if typing.typing, typing.node != now { typing.leave() }
            pointPicked = nil
        }
        .modifier(LinkQuestion(typing: typing))
    }

    /// Where a drag lands now: each box dashed, with the guides it meets.
    @ViewBuilder private func landings(_ fit: Fit) -> some View {
        let cells = landedCells
        let guides = landedGuides
        ForEach(cells.indices, id: \.self) { i in
            let r = cells[i]
            Path(r).applying(fit.transform)
                .stroke(Color.accentColor, style: StrokeStyle(lineWidth: 1, dash: [6, 4]))
        }
        ForEach(guides.indices, id: \.self) { i in
            line(guides[i][0], guides[i][1], fit: fit).stroke(Color.pink, lineWidth: 1)
        }
    }

    /// Where each node a drag moves lands now, canvas units.
    private var landedCells: [CGRect] {
        guard let d = drag else { return [] }
        if d.together.count > 1 { return d.arranged?.landed.map(\.cell) ?? [] }
        guard let snapped = d.snapped else { return [] }
        return [snapped.cell]
    }

    /// The guides the box a drag moves meets where it lands now.
    private var landedGuides: [[CGPoint]] {
        guard let d = drag else { return [] }
        return d.together.count > 1 ? d.arranged?.guides ?? [] : d.snapped?.guides ?? []
    }

    /// The marquee dragged now, canvas units.
    private var banded: CGRect? {
        guard case .banding(let from, let to) = press else { return nil }
        return CGRect(from: from, to: to)
    }

    /// The caret, or the selection, in the text typed in, where it is drawn, and what an input
    /// method composes there, underlined.
    @ViewBuilder private func typed(_ fit: Fit) -> some View {
        if typing.typing {
            let covered = typing.covered
            ForEach(covered.indices, id: \.self) { i in
                outline(covered[i], fit: fit).fill(Color.accentColor.opacity(0.3))
            }
            let composing = typing.composing
            ForEach(composing.indices, id: \.self) { i in
                line(composing[i][3], composing[i][2], fit: fit).stroke(Color.accentColor, lineWidth: 1.5)
            }
            if typing.from == typing.to, let caret = typing.caret, caret.count == 2 {
                line(caret[0], caret[1], fit: fit).stroke(Color.accentColor, lineWidth: 2)
            }
        }
    }

    /// What the canvas does with a command its keys make while no text is typed in (PLAN 3.11):
    /// Delete and Shift+Delete take the node selected away; Escape selects what holds it.
    private func command(_ selector: Selector) -> Bool {
        typealias Keys = NSStandardKeyBindingResponding
        switch selector {
        case #selector(Keys.deleteBackward(_:)), #selector(Keys.deleteForward(_:)):
            // A point picked on the shape selected is taken away, not the shape (PLAN 3.16).
            if let picked = pointPicked, let o = shaped, o.node == node, also.isEmpty {
                unpoint(o, picked)
                return true
            }
            delete(NSApp.currentEvent?.modifierFlags.contains(.shift) == true)
            return true
        case #selector(Keys.cancelOperation(_:)):
            if pointPicked != nil {
                pointPicked = nil
                return true
            }
            node = boxes.first { $0.node == node }?.parent
            also = []
            return true
        default:
            return false
        }
    }

    /// What the drag does, as the browser's status line says it.
    private var told: String? {
        switch press {
        case .refused(let why):
            return why
        case .dragging(let d):
            let them = d.together.count > 1 ? "\(d.together.count) selected" : d.node
            if d.together.count > 1 {
                guard d.arranged != nil else { return "\(them) land nowhere new" }
            } else {
                guard d.how != nil else { return "\(d.node) is not placed that way" }
                guard d.snapped != nil else { return "\(d.node) lands nowhere new" }
            }
            if d.patch.isEmpty { return "\(them) stay where they are" }
            let n = d.states.count
            let inWhich = n == 1 && d.states.first == state ? "in this state" : "in \(n) states"
            let keep = d.fork ? " · kept to \(state)" : n > 1 ? " · Option keeps it to \(state)" : ""
            let to = d.together.count > 1 ? "moved together" : placed(d.patch)
            return "\(them) → \(to) · \(inWhich)\(keep)"
        case .banding:
            return "select what lies wholly inside"
        case .handling(let h):
            return holdingTold(h)
        default:
            return nil
        }
    }

    /// What a handle held does, as the browser's status line says it.
    private func holdingTold(_ h: Holding) -> String? {
        let kept = h.fork ? " · kept to \(state)" : ""
        switch h.handle {
        case .turn(let turn):
            return h.moved
                ? "\(h.node) turns to \(Int(turn.now))°\(kept)"
                : "turning \(h.node) about its anchor · Shift by 15° · Option keeps it to \(state)"
        case .point(let index, let added):
            guard h.moved || added, h.points.indices.contains(index) else { return nil }
            return "\(h.node)'s point \(index + 1) → \(Self.spoken(h.points[index]))\(kept)"
        case .corner(let step):
            return h.moved ? "\(h.node)'s corners round to radius.\(step)\(kept)" : nil
        case .crop:
            guard h.moved else { return nil }
            return "\(h.node) cropped to \(h.crop.map { "\($0)" }.joined(separator: ", ")) of the image\(kept)"
        case .focal:
            return h.moved ? "\(h.node)'s focal point → \(Self.spoken(h.focal))\(kept)" : nil
        }
    }

    /// A point as the status line says it.
    private static func spoken(_ p: CGPoint) -> String { "\(Double(p.x)), \(Double(p.y))" }

    /// Where a patch places its node, as the status says it.
    private func placed(_ patch: [JSONValue]) -> String {
        guard let at = patch.first?["at"] else { return "placed" }
        if let slot = at["in"]?.string { return "slot \(slot)" }
        if let area = at["area"]?.string { return "area \(area)" }
        if let rect = at["rect"], rect != .null { return "off the grid, as lint will flag (W301)" }
        return "the grid's cells"
    }

    /// A press: a click where it does not move, else a drag of what it pressed.
    private func pressing(_ fit: Fit) -> some Gesture {
        DragGesture(minimumDistance: 0, coordinateSpace: .local)
            .onChanged { value in
                let moved = hypot(value.translation.width, value.translation.height)
                switch press {
                case nil:
                    // The canvas takes the keyboard, and Insert lands here next.
                    let at = fit.canvas(value.startLocation)
                    typing.focus?()
                    pointed = at
                    said = nil
                    // Typing: a press in the text puts the caret there, one outside it stops typing.
                    let typed = typing.typing
                    if typed {
                        if typing.holds(at, slop: Self.slop * fit.units) {
                            press = .typing
                            let clicks = NSApp.currentEvent?.clickCount ?? 1
                            return typing.press(at: at, clicks: clicks, extend: NSEvent.modifierFlags.contains(.shift))
                        }
                        typing.leave()
                    }
                    // A handle of the node selected, which shows while nothing is typed in.
                    if !typed, let held = handle(at: at, scale: fit.scale) {
                        press = .handling(held)
                        return
                    }
                    press = .pressing
                case .typing:
                    typing.drag(to: fit.canvas(value.location))
                case .pressing where moved >= 3:
                    begin(at: fit.canvas(value.startLocation), scale: fit.scale)
                    if case .dragging = press { aim(value, scale: fit.scale) }
                case .dragging:
                    aim(value, scale: fit.scale)
                case .banding(let from, _):
                    press = .banding(from: from, to: fit.canvas(value.location))
                case .handling(var h):
                    // A click until it goes further than one.
                    guard h.moved || moved >= 3 else { break }
                    h.moved = true
                    h.fork = NSEvent.modifierFlags.contains(.option)
                    hold(&h, to: fit.canvas(value.location))
                    press = .handling(h)
                default:
                    break
                }
            }
            .onEnded { value in
                defer { press = nil }
                switch press {
                case .dragging:
                    aim(value, scale: fit.scale)
                    drop()
                case .pressing, nil:
                    pick(fit.canvas(value.location))
                case .banding(let from, _):
                    band(CGRect(from: from, to: fit.canvas(value.location)))
                case .typing:
                    // The keys stay with the text, whatever took them as the press ended.
                    typing.focus?()
                case .handling(var h):
                    if h.moved {
                        h.fork = NSEvent.modifierFlags.contains(.option)
                        hold(&h, to: fit.canvas(value.location))
                    }
                    letGo(h)
                default:
                    break
                }
            }
    }

    /// The handle of the node selected under a press at `point`, canvas units, held (PLAN 3.16):
    /// the topmost of an image's focal point and crop bars, a shape's points, its corner, the
    /// middles of its edges, and the rotate handle; none where the press is on none of them.
    private func handle(at point: CGPoint, scale: CGFloat) -> Holding? {
        guard also.isEmpty, let selected = boxes.first(where: { $0.node == node }), selected.locked == nil else {
            return nil
        }
        let near = Self.reach / max(scale, 0.01)
        let close = { (p: CGPoint) -> Bool in hypot(p.x - point.x, p.y - point.y) <= near }
        let units = Double(max(scale, 0.01))
        func held(_ handle: Holding.Handle) -> Holding {
            Holding(
                handle: handle, node: selected.node, from: point, revision: editor.revision,
                points: shaped?.points ?? [], crop: imaged?.crop ?? [], focal: imaged?.focal ?? .zero)
        }
        if let f = imaged, f.node == selected.node {
            if close(f.focalHandle) { return held(.focal) }
            if let side = Framing.Side.allCases.first(where: { close(f.handle($0, inset: HandleSpacing.inset / units)) }) {
                return held(.crop(side))
            }
        }
        if let o = shaped, o.node == selected.node {
            if ["line", "arrow", "polygon"].contains(o.kind) {
                if let i = o.drawn.lastIndex(where: close) { return held(.point(i, added: false)) }
                if let i = o.middles.firstIndex(where: close) {
                    var h = held(.point(i + 1, added: true))
                    h.points = o.adding(after: i)
                    return h
                }
            }
            if o.kind == "rect", !o.radii.isEmpty, close(o.corner(clear: HandleSpacing.clear / units)) {
                return held(.corner(o.step(nearest: o.radius ?? 0)))
            }
        }
        if let arm = selected.turnHandle(arm: HandleSpacing.arm / units), close(arm.at) {
            do {
                let turned = try editor.session.turned(state: state, node: selected.node)
                let holder = transform(of: selected.parent)
                return held(.turn(Turn(selected, turned: turned, from: point, holder: holder)))
            } catch {
                said = "\(selected.node) cannot be turned: \(error)"
            }
        }
        return nil
    }

    /// Handle `h` with the pointer at `at`, canvas units: the node turned as far as the pointer
    /// went round; the point there, kept to the box; the corner's step; the crop's side; or the
    /// focal point there.
    private func hold(_ h: inout Holding, to at: CGPoint) {
        switch h.handle {
        case .turn(var turn):
            turn.move(to: at, snap: NSEvent.modifierFlags.contains(.shift))
            h.handle = .turn(turn)
        case .point(let index, _):
            if let o = shaped, h.points.indices.contains(index) { h.points[index] = o.fraction(at: at) }
        case .corner:
            if let o = shaped { h.handle = .corner(o.step(from: h.from, to: at)) }
        case .crop(let side):
            if let f = imaged { h.crop = f.cropped(side, from: h.from, to: at) }
        case .focal:
            if let f = imaged { h.focal = f.focal(at: at) }
        }
    }

    /// Handle `h` let go: one `choose` of what it changed, written where the value lives, or kept
    /// to the state shown with Option; a point pressed and let go where it was is picked.
    private func letGo(_ h: Holding) {
        guard h.revision == editor.revision else {
            said = "the source changed under the drag: nothing is changed"
            return
        }
        let kept = h.fork ? " · kept to \(state)" : ""
        switch h.handle {
        case .turn(let turn):
            guard turn.now != turn.start else {
                said = "\(h.node) stays as it is"
                return
            }
            make(Handling.turning(h.node, to: turn.now, in: state, fork: h.fork))
            said = "\(h.node) turned to \(Int(turn.now))°\(kept)"
        case .point(let index, let added):
            guard let o = shaped else { return }
            if !h.moved && !added {
                pointPicked = index
                said = "\(h.node)'s point \(index + 1) picked: Delete takes it away, a drag moves it"
                return
            }
            if !added && h.points == o.points {
                said = "\(h.node)'s point \(index + 1) stays where it is"
                return
            }
            pointPicked = nil
            make(Handling.reshaping(h.node, to: h.points, in: state, fork: h.fork))
            said = added ? "\(h.node) has a point added\(kept)" : "\(h.node)'s point \(index + 1) moved\(kept)"
        case .corner(let step):
            guard let o = shaped else { return }
            guard h.moved, o.rounds(to: step) else {
                said =
                    h.moved
                    ? "\(h.node)'s corners stay as they are"
                    : "drag \(h.node)'s corner handle to round it to the theme's radius steps"
                return
            }
            make(Handling.rounding(h.node, to: step, in: state, fork: h.fork))
            said = "\(h.node)'s corners round to radius.\(step)\(kept)"
        case .crop:
            guard let f = imaged else { return }
            guard h.moved, f.changes(crop: h.crop) else {
                said = h.moved ? "\(h.node)'s crop stays as it is" : "drag \(h.node)'s crop handle to crop it from that side"
                return
            }
            make(Handling.cropping(h.node, to: h.crop, in: state, fork: h.fork))
            said = Framing.keepsWhole(h.crop) ? "\(h.node) shows the whole image\(kept)" : "\(h.node) cropped\(kept)"
        case .focal:
            guard let f = imaged else { return }
            guard h.moved, f.changes(focal: h.focal) else {
                said =
                    h.moved
                    ? "\(h.node)'s focal point stays where it is"
                    : "drag \(h.node)'s focal point to keep that part of the image in view"
                return
            }
            make(Handling.focusing(h.node, on: h.focal, in: state, fork: h.fork))
            said = "\(h.node)'s focal point → \(Self.spoken(h.focal))\(kept)"
        }
    }

    /// Point `index` of shape `o` taken away, unless it keeps no fewer: a line or an arrow two, a
    /// polygon three. One `choose` of its points, kept to the state shown with Option.
    private func unpoint(_ o: Outline, _ index: Int) {
        guard let kept = o.removing(index) else {
            let kind = o.kind == "arrow" ? "an arrow" : "a \(o.kind)"
            said = "\(kind) keeps \(o.fewest == 2 ? "two" : "three") points: \(o.node)'s point \(index + 1) stays"
            return
        }
        pointPicked = nil
        let fork = NSEvent.modifierFlags.contains(.option)
        make(Handling.reshaping(o.node, to: kept, in: state, fork: fork))
        said = "\(o.node)'s point \(index + 1) taken away\(fork ? " · kept to \(state)" : "")"
    }

    /// What draws `node`'s box where it is drawn: none for the canvas, or a node nothing turns.
    private func transform(of node: String?) -> [Double]? {
        guard let node else { return nil }
        return boxes.first { $0.node == node }?.transform
    }

    /// Select what draws at `point`, canvas units: nothing where nothing does. The second click of
    /// a double click on a text types in it there, kept to the state with Option (PLAN 3.9). With
    /// Shift, a click adds what draws there to what is selected, or takes it out (PLAN 3.13).
    private func pick(_ point: CGPoint) {
        let hits = (try? editor.session.hits(state: state, at: point)) ?? []
        let hit = hits.first { $0.locked == nil }?.node
        if NSEvent.modifierFlags.contains(.shift), let hit, node != nil {
            return toggle(hit)
        }
        node = hit
        also = []
        if let node, (NSApp.currentEvent?.clickCount ?? 1) >= 2 {
            typing.enter(node, in: state, at: point, fork: NSEvent.modifierFlags.contains(.option))
        }
    }

    /// `hit` added to what is selected, or taken out of it: children of one container alone, as
    /// the browser's canvas selects several (PLAN 2.42).
    private func toggle(_ hit: String) {
        if hit == node {
            node = also.first
            also = Array(also.dropFirst())
        } else if let at = also.firstIndex(of: hit) {
            also.remove(at: at)
        } else if parent(of: hit) == parent(of: node) {
            also.append(hit)
        } else {
            said = "\(hit) is not beside \(node ?? "it"): select children of one container"
        }
    }

    /// What holds `node` in the state shown: none for the canvas.
    private func parent(of node: String?) -> String? {
        boxes.first { $0.node == node }?.parent
    }

    /// The marquee let go over `band`, canvas units: what lies wholly inside it, children of the
    /// canvas that a pointer reaches, selected; with Shift, added to what is.
    private func band(_ band: CGRect) {
        let inside = boxes.filter { b in
            guard b.parent == nil, b.locked == nil, b.draws else { return false }
            let xs = b.corners.map(\.x)
            let ys = b.corners.map(\.y)
            guard let x0 = xs.min(), let x1 = xs.max(), let y0 = ys.min(), let y1 = ys.max() else { return false }
            return band.contains(CGRect(x: x0, y: y0, width: x1 - x0, height: y1 - y0))
        }.map(\.node)
        let keep = NSEvent.modifierFlags.contains(.shift) && parent(of: node) == nil ? selection : []
        let all = keep + inside.filter { !keep.contains($0) }
        node = all.first
        also = Array(all.dropFirst())
        said = all.isEmpty ? "nothing lies wholly inside" : "\(all.count) selected"
    }

    /// A drag begun at `point`, canvas units: of a handle of the box selected, a resize; else a
    /// move of what draws there, selected.
    private func begin(at point: CGPoint, scale: CGFloat) {
        var held: (node: String, edge: Edge?)?
        if let selected = boxes.first(where: { $0.node == node }), selected.locked == nil, let r = box(selected) {
            let near = Self.reach / max(scale, 0.01)
            // Each handle where the box is drawn, turned or scaled as it is.
            let spot = { (edge: Edge) in selected.onCanvas(edge.point(r)) }
            if let edge = Edge.allCases.first(where: { hypot(spot($0).x - point.x, spot($0).y - point.y) <= near }) {
                held = (selected.node, edge)
            }
        }
        if held == nil {
            let hits = (try? editor.session.hits(state: state, at: point)) ?? []
            guard let hit = hits.first(where: { $0.locked == nil }) else {
                // From where nothing draws, a marquee.
                press = .banding(from: point, to: point)
                return
            }
            // A node selected with others moves them all.
            if also.count > 0, selection.contains(hit.node) {
                return beginTogether(hit.node)
            }
            node = hit.node
            also = []
            held = (hit.node, nil)
        }
        guard let held else { return }
        do {
            let targets = try editor.session.targets(state: state, node: held.node)
            editor.still()
            press = .dragging(
                Drag(
                    node: held.node, together: [held.node], targets: targets, at: placements[held.node], edge: held.edge,
                    revision: editor.revision))
        } catch {
            press = .refused("\(held.node) cannot be moved: \(error)")
        }
    }

    /// A drag of `first`, one of several selected: all of them moved together.
    private func beginTogether(_ first: String) {
        let all = [first] + selection.filter { $0 != first }
        if let held = boxes.first(where: { all.contains($0.node) && $0.locked != nil }) {
            press = .refused("\(held.node) is locked: nothing moves")
            return
        }
        do {
            let targets = try editor.session.targets(state: state, node: first)
            editor.still()
            press = .dragging(
                Drag(
                    node: first, together: all, targets: targets, at: placements[first], edge: nil, revision: editor.revision
                ))
        } catch {
            press = .refused("\(first) cannot be moved: \(error)")
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
        // As what holds it lays it out, through whatever turns or scales that; a resize along the
        // node's own sides (PLAN 2.51).
        let held = across(transform(of: parent(of: d.node)), d.by)
        let to =
            d.edge.map { resized(d.targets.cell, $0, across(transform(of: d.node), d.by)) }
            ?? d.targets.cell.offsetBy(dx: held.dx, dy: held.dy)
        if d.edge == nil { editor.move(d.together, by: d.by) }
        if d.together.count > 1 {
            // Moved together: the first snapped as it would be alone, the rest as far as it went.
            d.arranged = (try? editor.session.together(
                state: state, nodes: d.together, by: held, free: flags.contains(.shift), fork: d.fork,
                reach: Self.reach / max(scale, 0.01))) ?? nil
        } else if let how = d.how {
            d.snapped = try? editor.session.snap(
                state: state, node: d.node, how: how, to: to, fork: d.fork, reach: Self.reach / max(scale, 0.01))
        } else {
            d.snapped = nil
        }
        let patch = d.patch
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
        guard let d = drag, d.revision == editor.revision, !d.patch.isEmpty else { return }
        make(d.patch)
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
    private func line(_ a: CGPoint, _ b: CGPoint, fit: Fit) -> Path {
        Path { path in
            path.move(to: fit.view(a))
            path.addLine(to: fit.view(b))
        }
    }

    /// `corners`, canvas units, as a path on the view.
    private func outline(_ corners: [CGPoint], fit: Fit) -> Path {
        Path { path in
            guard let first = corners.first else { return }
            path.move(to: fit.view(first))
            for corner in corners.dropFirst() {
                path.addLine(to: fit.view(corner))
            }
            path.closeSubpath()
        }
    }
}

/// ⌘K's question (PLAN 2.70, 3.10): where the characters selected in the text typed in link to,
/// a web address or a state's id. Left empty, it takes their link away.
private struct LinkQuestion: ViewModifier {
    let typing: Typing
    @State private var to = ""

    func body(content: Content) -> some View {
        content
            .alert("Link", isPresented: asking, presenting: typing.asking) { _ in
                TextField("https://… or a state's id", text: $to)
                Button("Link") { typing.link(to: to) }
                Button("Cancel", role: .cancel) { typing.link(to: nil) }
            } message: { words in
                Text("Where “\(words)” links to: a web address, or a state's id. Left empty, its link is taken away.")
            }
            .onChange(of: typing.asking) { _, now in
                if now != nil { to = "" }
            }
    }

    /// Whether it asks: dismissed, it makes no link.
    private var asking: Binding<Bool> {
        Binding(get: { typing.asking != nil }, set: { if !$0, typing.asking != nil { typing.link(to: nil) } })
    }
}

extension CGRect {
    /// The rectangle with corners `from` and `to`, whichever way they lie.
    fileprivate init(from: CGPoint, to: CGPoint) {
        self.init(x: min(from.x, to.x), y: min(from.y, to.y), width: abs(to.x - from.x), height: abs(to.y - from.y))
    }
}
