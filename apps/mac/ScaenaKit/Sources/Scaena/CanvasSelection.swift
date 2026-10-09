#if os(macOS)
import AppKit
#endif
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
/// It works by keys alone (PLAN 3.17, as the browser's, PLAN 2.75, 2.89): Tab and Shift+Tab select
/// in reading order, Return goes into a container or onto a node's handles, the arrows step a node
/// a track (with Shift resize it) or move the handle the keys are on, `[` and `]` turn it, ⌘A
/// selects all beside it, and Space, then Tab and Space, build a selection; each node is an
/// element VoiceOver reads as the reader hears it. On the iPad (PLAN 4.3), a finger makes each of
/// these gestures' patches: a tap is a click and a double tap a double click; a handle is taken
/// within 22 points; a long press offers what the Node menu and the clipboard do to what it
/// pressed; and two fingers zoom and pan, a second finger stopping the first's press as it touches
/// down. Every box and caret is the engine's, at rest; nothing here lays out.
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
    let clip: (Clipping) -> Void
    /// The Edit menu's Find, and ⌘F on the canvas: the deck's find bar (PLAN 3.16).
    let finding: () -> Void
    /// What the Node menu does to the node selected, which a long press offers on the iPad (PLAN 4.3).
    let actions: DeckActions?
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
    /// The node the keys are on while a selection is built by keys (PLAN 2.89): Tab moves it among
    /// those beside it, and Space puts it in the selection or takes it out.
    @State private var keyOn: String?
    /// The handle of the node selected the keys are on (PLAN 2.75), by its place among them.
    @State private var keyed: Int?
    /// How the state shown reads, node by node: what VoiceOver hears of the canvas (PLAN 3.17).
    @State private var reads: [ReadPart] = []
    /// A resize that paused, shown laid out as its patch would make it.
    @State private var pausing: Task<Void, Never>?
    /// Presses counted as a double click or a double tap counts them, and how many the press now
    /// under way makes: two types in the text pressed (PLAN 3.9, 4.3).
    @State private var presses = Presses()
    @State private var clicks = 1
    /// Whether a press is under way: false again as it ends or the system takes it away.
    @GestureState private var touching = false
    #if !os(macOS)
    /// A finger held still, waiting to offer what is done to what it pressed (PLAN 4.3).
    @State private var lingering: Task<Void, Never>?
    /// The menu a long press asks for: a count, new with each ask, and where, view points.
    @State private var offering: (count: Int, at: CGPoint)?
    #endif

    /// How long a resize pauses before the canvas shows its text reflowed (as the browser's).
    private static let pause = Duration.milliseconds(300)
    /// How near, in points, an edge moved off the grid goes onto another's (PLAN 2.57).
    private static let reach = 6.0
    #if os(macOS)
    /// How near a handle, in points, a press takes it: a pointer's reach.
    private static let grab = 6.0
    /// How far, in points, a press goes before it is a drag and not a click.
    private static let still = 3.0
    /// How far outside the text typed in, in points, a press still puts the caret in it.
    private static let slop = 4.0
    #else
    /// How near a handle, in points, a finger takes it: half the 44 points Apple gives a target
    /// for a finger (PLAN 4.3), and less on a box too small for its handles to keep apart.
    private static let grab = 22.0
    /// How far, in points, a finger goes before its press is a drag and not a tap: a finger rolls
    /// further than a pointer as it presses.
    private static let still = 10.0
    /// How far outside the text typed in, in points, a finger still puts the caret in it.
    private static let slop = 12.0
    /// How long a finger stays still before its press offers what is done to what it pressed.
    private static let lingers = Duration.milliseconds(500)
    #endif

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
        /// A finger stayed still: the menu of what is done to what it pressed is offered (PLAN 4.3).
        case offered
        /// Two fingers took the press over, to zoom or pan: the first finger edits nothing.
        case fingers
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
                #if os(macOS)
                // Under the rest: the canvas's keys, the text's while one is typed in. It takes no
                // press. The iPad's come with PLAN 4.4 and 4.6.
                CanvasKeysHost(
                    typing: typing, canvas: size, shown: zoom.view, command: command, clipping: clip,
                    finding: { _ in finding() }, pressed: pressed)
                    .allowsHitTesting(false)
                // The wheel and a pinch over the canvas, read before any view takes them. On the
                // iPad, two fingers do it (`CanvasPinch`, `CanvasPan`, PLAN 4.3).
                CanvasWheel(
                    zoom: { factor, at in zoom.zoom(to: zoom.level * factor, about: fit.canvas(at)) },
                    pan: { by in
                        guard zoom.level > 1 else { return false }
                        zoom.pan(by: CGVector(dx: Double(by.dx) * fit.units, dy: Double(by.dy) * fit.units))
                        return true
                    }
                )
                .allowsHitTesting(false)
                #endif
                Color.clear
                    .contentShape(Rectangle())
                    .gesture(pressing(fit))
                    #if !os(macOS)
                    // Two fingers zoom and pan, as the Mac's pinch and wheel do (PLAN 4.3).
                    .gesture(
                        CanvasPinch(
                            level: { zoom.level }, zoom: { to, at in zoom.zoom(to: to, about: fit.canvas(at)) },
                            fingers: fingers))
                    .gesture(
                        CanvasPan(
                            pan: { by in
                                guard zoom.level > 1 else { return }
                                zoom.pan(by: CGVector(dx: -Double(by.dx) * fit.units, dy: -Double(by.dy) * fit.units))
                            }, fingers: fingers))
                    #endif
                #if !os(macOS)
                // What a long press offers, shown over what the finger pressed (PLAN 4.3).
                CanvasMenu(asked: offering, deck: actions, clip: clip)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .allowsHitTesting(false)
                #endif
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
                // Each node read, where it is drawn, as VoiceOver hears it (PLAN 3.17).
                ForEach(reads.indices, id: \.self) { i in
                    readable(reads[i], order: reads.count - i, fit: fit, in: CGRect(origin: .zero, size: geometry.size))
                }
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
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Words.slide(state, in: editor.slots))
        .accessibilityIdentifier("canvas")
        // How close it is shown, where it is zoomed in (PLAN 3.16).
        .accessibilityValue(zoom.level > 1 + 1e-9 ? "at \(Int((zoom.level * 100).rounded())) percent" : "")
        .task(id: "\(state)\u{1f}\(editor.revision)") {
            boxes = (try? editor.session.boxes(state: state)) ?? []
            placements = (try? editor.session.placements(state: state)) ?? [:]
            reads = (try? editor.session.reads(state: state)) ?? []
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
        .onChange(of: touching) { _, now in
            // A press the system took away (a swipe from the screen's edge, say) has no end of its
            // own: what it began stops once the gesture's own end, where there was one, has run.
            guard !now else { return }
            Task { @MainActor in
                guard !touching, press != nil else { return }
                if case .fingers = press { return }
                if case .dragging = press { editor.still() }
                pausing?.cancel()
                #if !os(macOS)
                lingering?.cancel()
                #endif
                press = nil
            }
        }
        .onChange(of: node) { _, now in
            // Another node selected, in the layers or by a finding, stops typing.
            if typing.typing, typing.node != now { typing.leave() }
            pointPicked = nil
            keyed = nil
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

    #if os(macOS)
    /// What the canvas does with a command its keys make while no text is typed in (PLAN 3.11):
    /// Delete and Shift+Delete take the node selected away; Escape selects what holds it.
    private func command(_ selector: Selector) -> Bool {
        typealias Keys = NSStandardKeyBindingResponding
        switch selector {
        case #selector(Keys.deleteBackward(_:)), #selector(Keys.deleteForward(_:)):
            // A point picked on the shape selected is taken away, not the shape (PLAN 3.16); so is
            // the point the keys are on (PLAN 3.17).
            if let picked = pointPicked, let o = shaped, o.node == node, also.isEmpty {
                unpoint(o, picked)
                return true
            }
            if let k = keyed, let o = shaped, o.node == node, also.isEmpty {
                let list = handles(of: o.node)
                if list.indices.contains(k), case .point(let i) = list[k] {
                    unpoint(o, i)
                    keyed = max(0, k - 1)
                    return true
                }
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
    #endif

    /// What a drag does, where a person needs telling (PLAN 3.18): why it cannot be made; that it
    /// reaches other slides too, and how to keep it to this one; or a turn's angle.
    private var told: String? {
        switch press {
        case .refused(let why):
            return why
        case .dragging(let d):
            if d.patch.isEmpty { return nil }
            if d.fork { return "On this slide only" }
            let others = d.states.filter { $0 != state }.count
            guard others > 0 else { return nil }
            return "Also moves it on \(others) other slide\(others == 1 ? "" : "s") · ⌥ moves it here only"
        case .handling(let h):
            return holdingTold(h)
        default:
            return nil
        }
    }

    /// What a handle held does: a turn's angle, and a change kept to this slide.
    private func holdingTold(_ h: Holding) -> String? {
        let kept = h.fork ? " · on this slide only" : ""
        switch h.handle {
        case .turn(let turn):
            return h.moved ? "\(Int(turn.now))°\(kept)" : "Drag to rotate · Shift turns by 15°"
        default:
            return h.fork && h.moved ? "On this slide only" : nil
        }
    }

    /// A press: a click where it does not move, else a drag of what it pressed. On the iPad a finger
    /// that stays still offers what is done to what it pressed (PLAN 4.3).
    private func pressing(_ fit: Fit) -> some Gesture {
        DragGesture(minimumDistance: 0, coordinateSpace: .local)
            .updating($touching) { _, pressed, _ in pressed = true }
            .onChanged { value in
                let moved = hypot(value.translation.width, value.translation.height)
                switch press {
                case nil:
                    // The canvas takes the keyboard, and Insert lands here next; the pointer takes
                    // over from the keys (PLAN 2.89).
                    let at = fit.canvas(value.startLocation)
                    keyOn = nil
                    keyed = nil
                    typing.focus?()
                    pointed = at
                    said = nil
                    clicks = presses.began(at: value.startLocation)
                    // Typing: a press in the text puts the caret there, one outside it stops typing.
                    let typed = typing.typing
                    if typed {
                        if typing.holds(at, slop: Self.slop * fit.units) {
                            press = .typing
                            return typing.press(at: at, clicks: clicks, extend: Held.shift)
                        }
                        typing.leave()
                    }
                    // A handle of the node selected, which shows while nothing is typed in.
                    if !typed, let held = handle(at: at, scale: fit.scale) {
                        press = .handling(held)
                        return
                    }
                    press = .pressing
                    #if !os(macOS)
                    linger(at: value.startLocation, fit: fit)
                    #endif
                case .typing:
                    typing.drag(to: fit.canvas(value.location))
                case .pressing where moved >= Self.still:
                    #if !os(macOS)
                    lingering?.cancel()
                    #endif
                    begin(at: fit.canvas(value.startLocation), scale: fit.scale)
                    if case .dragging = press { aim(value, scale: fit.scale) }
                case .dragging:
                    aim(value, scale: fit.scale)
                case .banding(let from, _):
                    press = .banding(from: from, to: fit.canvas(value.location))
                case .handling(var h):
                    // A click until it goes further than one.
                    guard h.moved || moved >= Self.still else { break }
                    h.moved = true
                    h.fork = Held.option
                    hold(&h, to: fit.canvas(value.location))
                    press = .handling(h)
                default:
                    break
                }
            }
            .onEnded { value in
                defer { press = nil }
                #if !os(macOS)
                lingering?.cancel()
                #endif
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
                        h.fork = Held.option
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
        let near = grabbing(selected, scale: scale)
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
        // The rotate handle stands clear of the box: a finger takes it from as far as it reaches.
        let arming = Self.grab / units
        if let arm = selected.turnHandle(arm: HandleSpacing.arm / units),
            hypot(arm.at.x - point.x, arm.at.y - point.y) <= arming
        {
            do {
                let turned = try editor.session.turned(state: state, node: selected.node)
                let holder = transform(of: selected.parent)
                return held(.turn(Turn(selected, turned: turned, from: point, holder: holder)))
            } catch {
                said = "Can't be rotated: \(error)"
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
            turn.move(to: at, snap: Held.shift)
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
            said = "The deck changed during the drag: nothing was changed"
            return
        }
        let kept = h.fork ? " · on this slide only" : ""
        switch h.handle {
        case .turn(let turn):
            guard turn.now != turn.start else {
                said = nil
                return
            }
            make(Handling.turning(h.node, to: turn.now, in: state, fork: h.fork))
            said = "Rotated to \(Int(turn.now))°\(kept)"
        case .point(let index, let added):
            guard let o = shaped else { return }
            if !h.moved && !added {
                pointPicked = index
                said = "Point \(index + 1) selected: Delete removes it, a drag moves it"
                return
            }
            if !added && h.points == o.points {
                said = nil
                return
            }
            pointPicked = nil
            make(Handling.reshaping(h.node, to: h.points, in: state, fork: h.fork))
            said = added ? "Point added\(kept)" : "Point moved\(kept)"
        case .corner(let step):
            guard let o = shaped else { return }
            guard h.moved, o.rounds(to: step) else {
                said = h.moved ? nil : "Drag the corner handle to round the corners"
                return
            }
            make(Handling.rounding(h.node, to: step, in: state, fork: h.fork))
            said = "Corners rounded\(kept)"
        case .crop:
            guard let f = imaged else { return }
            guard h.moved, f.changes(crop: h.crop) else {
                said = h.moved ? nil : "Drag a crop handle to crop the picture from that side"
                return
            }
            make(Handling.cropping(h.node, to: h.crop, in: state, fork: h.fork))
            said = Framing.keepsWhole(h.crop) ? "Showing the whole picture\(kept)" : "Cropped\(kept)"
        case .focal:
            guard let f = imaged else { return }
            guard h.moved, f.changes(focal: h.focal) else {
                said = h.moved ? nil : "Drag the focal point to keep that part of the picture in view"
                return
            }
            make(Handling.focusing(h.node, on: h.focal, in: state, fork: h.fork))
            said = "Focal point moved\(kept)"
        }
    }

    /// Point `index` of shape `o` taken away, unless it keeps no fewer: a line or an arrow two, a
    /// polygon three. One `choose` of its points, kept to the state shown with Option.
    private func unpoint(_ o: Outline, _ index: Int) {
        guard let kept = o.removing(index) else {
            let kind = o.kind == "arrow" ? "An arrow" : "A \(o.kind)"
            said = "\(kind) needs at least \(o.fewest == 2 ? "two" : "three") points"
            return
        }
        pointPicked = nil
        let fork = Held.option
        make(Handling.reshaping(o.node, to: kept, in: state, fork: fork))
        said = "Point removed\(fork ? " · on this slide only" : "")"
    }

    /// A node as VoiceOver hears it (PLAN 3.17), where it is drawn: a heading, a picture, a table,
    /// or words, as the reader hears it; selected by its action. Its frame is the part of it `view`
    /// shows, and a node zoomed out of view is not read, so nothing the canvas holds reaches past
    /// it: VoiceOver outlines what is shown, and a pinch's fingers land on the canvas (PLAN 4.3).
    @ViewBuilder private func readable(_ part: ReadPart, order: Int, fit: Fit, in view: CGRect) -> some View {
        if let b = boxes.first(where: { $0.node == part.node }),
            let drawn = Self.shown(b.corners.map { fit.view($0) }, in: view)
        {
            Color.clear
                .frame(width: max(drawn.width, 1), height: max(drawn.height, 1))
                .position(x: drawn.midX, y: drawn.midY)
                .allowsHitTesting(false)
                .accessibilityElement()
                .accessibilityLabel(Text(part.text.isEmpty ? name(part.node, starting: true) : part.text))
                .accessibilityAddTraits(Self.traits(part))
                .accessibilityAddTraits(selection.contains(part.node) ? .isSelected : [])
                .accessibilityHeading(part.level == 1 ? .h1 : part.level == 2 ? .h2 : .unspecified)
                .accessibilityHint("Selects it")
                .accessibilitySortPriority(Double(order))
                .accessibilityAction {
                    node = part.node
                    also = []
                }
        }
    }

    /// As much of the box around `points` as `view` shows; none where it shows none of it.
    private static func shown(_ points: [CGPoint], in view: CGRect) -> CGRect? {
        let shown = bounds(points).intersection(view)
        return shown.isNull ? nil : shown
    }

    /// The box around `points`.
    private static func bounds(_ points: [CGPoint]) -> CGRect {
        let xs = points.map(\.x)
        let ys = points.map(\.y)
        let (x0, y0) = (xs.min() ?? 0, ys.min() ?? 0)
        return CGRect(x: x0, y: y0, width: (xs.max() ?? 0) - x0, height: (ys.max() ?? 0) - y0)
    }

    /// What a part is to VoiceOver.
    private static func traits(_ part: ReadPart) -> AccessibilityTraits {
        switch part.role {
        case "heading": .isHeader
        case "figure": .isImage
        default: .isStaticText
        }
    }

    #if os(macOS)
    /// A key on the canvas while no text is typed in (PLAN 3.17), as the browser's canvas reads it
    /// (PLAN 2.75, 2.89): whether the canvas took it. What it does not take, the input system makes
    /// a command of, and Tab past either end passes the keyboard on.
    private func pressed(_ event: NSEvent) -> Bool {
        guard press == nil else { return false }
        let flags = event.modifierFlags.intersection([.command, .shift, .option, .control])
        guard !flags.contains(.control) else { return false }
        let (shift, option, command) = (flags.contains(.shift), flags.contains(.option), flags.contains(.command))
        switch event.keyCode {
        case 48 where !command && !option:
            return tab(back: shift)
        case 36, 76:
            return command ? false : enter(option: option)
        case 53:
            return escape()
        case 123 where !command:
            return arrow(-1, 0, shift: shift, option: option)
        case 124 where !command:
            return arrow(1, 0, shift: shift, option: option)
        case 125 where !command:
            return arrow(0, 1, shift: shift, option: option)
        case 126 where !command:
            return arrow(0, -1, shift: shift, option: option)
        case 49 where !command && !option:
            return space()
        case 33, 30:
            return command || option ? false : turn(by: Double((event.keyCode == 30 ? 1 : -1) * (shift ? 1 : 15)))
        case 24 where !command:
            return addPoint(option: option)
        case 0 where command && !shift && !option:
            return selectAll()
        default:
            return false
        }
    }
    #endif

    /// Node `selected`'s handles the keys work, in order.
    private func handles(of selected: String) -> [KeyedHandle] {
        KeyedHandle.of(outline: shaped?.node == selected ? shaped : nil, framing: imaged?.node == selected ? imaged : nil)
    }

    /// What the status says of handle `h` of `node`, the `n`th of `of`, and what its keys do.
    /// A node as the window names it (PLAN 3.18): its first words, quoted, where a reader hears
    /// any; else what it is to a reader, or an object.
    private func name(_ node: String, starting: Bool = false) -> String {
        let part = reads.first { $0.node == node }
        if let part, !part.text.isEmpty {
            return "“\(Words.node(nil, words: part.text))”"
        }
        let named = part?.role == "table" ? "a table" : part?.role == "figure" ? "a figure" : "an object"
        return starting ? named.prefix(1).uppercased() + named.dropFirst() : named
    }

    private func handleSaid(_ node: String, _ h: KeyedHandle, _ n: Int, _ of: Int) -> String {
        var does = "arrows move it"
        if case .point = h { does += ", + adds a point after it, Delete takes it away" }
        return "\(name(node, starting: true)): \(h.label), \(n + 1) of \(of) · \(does) · Tab the next · Escape leaves them"
    }

    /// Tab, or Shift+Tab: the next handle the keys are on; the next node keyed while a selection is
    /// built; or the next node beside the one selected, in reading order. Past either end, none: the
    /// keyboard goes on.
    private func tab(back: Bool) -> Bool {
        if let selected = node, also.isEmpty, let k = keyed {
            let list = handles(of: selected)
            if !list.isEmpty {
                let next = (k + (back ? list.count - 1 : 1)) % list.count
                keyed = next
                said = handleSaid(selected, list[next], next, list.count)
                return true
            }
        }
        if let on = keyOn {
            let order = readingOrder(boxes, in: parent(of: on))
            let next = (order.firstIndex(of: on) ?? -1) + (back ? -1 : 1)
            guard order.indices.contains(next) else {
                keyOn = nil
                said = "\(selection.count) selected: the keyboard leaves the slide"
                return false
            }
            keyOn = order[next]
            let inIt = selection.contains(order[next])
            said =
                "\(name(order[next], starting: true)), \(next + 1) of \(order.count)\(inIt ? ", selected" : "") · Space "
                + "\(inIt ? "takes it out of" : "puts it in") the selection · Tab the next · Escape stops"
            return true
        }
        let level = parent(of: node)
        let order = readingOrder(boxes, in: level)
        let at = node.flatMap { order.firstIndex(of: $0) } ?? (back ? order.count : -1)
        let next = at + (back ? -1 : 1)
        guard order.indices.contains(next) else {
            node = nil
            also = []
            said = "Nothing selected: the keyboard leaves the slide"
            return false
        }
        node = order[next]
        also = []
        said = "\(name(order[next], starting: true)) selected, \(next + 1) of \(order.count)\(level.map { " in \(name($0))" } ?? "") · Return goes into it or its handles"
        return true
    }

    /// Return: on the handles, what the one the keys are on is; into a container or a group, its
    /// first node; onto a shape's or an image's handles; or typing in a text where it begins, kept
    /// to the state with Option, as a double click types there.
    private func enter(option: Bool) -> Bool {
        guard let selected = node else { return false }
        if also.isEmpty, let k = keyed {
            let list = handles(of: selected)
            if list.indices.contains(k) { said = handleSaid(selected, list[k], k, list.count) }
            return true
        }
        let inside = readingOrder(boxes, in: selected)
        if also.isEmpty, let first = inside.first {
            node = first
            said = "\(name(first, starting: true)) selected, in \(name(selected)) · Tab goes on, Escape goes back out"
            return true
        }
        if boxes.first(where: { $0.node == selected })?.locked != nil {
            said = "It is locked: ⇧⌘L unlocks it"
            return true
        }
        let list = also.isEmpty ? handles(of: selected) : []
        if let first = list.first, !option {
            keyed = 0
            said = handleSaid(selected, first, 0, list.count)
            return true
        }
        if also.isEmpty, typing.enter(selected, in: state, at: nil, fork: option) { return true }
        said = "Nothing to edit in it here"
        return true
    }

    /// Space (PLAN 2.89), as the browser's canvas takes a tap of it: the node the keys are on put in
    /// the selection, or taken out of it; with none keyed, the keys start on the node selected, to
    /// build a selection from there. One beside another container's starts the selection anew.
    private func space() -> Bool {
        guard let on = keyOn else {
            guard let selected = node else {
                said = "Nothing selected: Tab selects an object, then Space builds a selection from it"
                return true
            }
            keyOn = selected
            said = "\(name(selected, starting: true)) selected · Tab keys the next beside it, and Space puts it in the selection or takes it out"
            return true
        }
        let now = toggled(on, in: selection) { parent(of: $0) }
        node = now.first
        also = Array(now.dropFirst())
        keyOn = on
        said = "\(name(on, starting: true)) \(now.contains(on) ? "put in" : "taken out of") the selection: \(now.count) selected · Tab the next · Escape stops"
        return true
    }

    /// Escape: a selection built by keys stops, what is selected staying; the handles are left; a
    /// point picked is let go; several selected, the first alone; else what holds it is selected.
    private func escape() -> Bool {
        if keyOn != nil {
            keyOn = nil
            said = selection.isEmpty ? "Nothing selected" : "\(selection.count) selected"
            return true
        }
        if keyed != nil {
            keyed = nil
            said = node.map { "\(name($0, starting: true)) selected" } ?? "Nothing selected"
            return true
        }
        if pointPicked != nil {
            pointPicked = nil
            return true
        }
        guard let selected = node else { return false }
        if !also.isEmpty {
            also = []
            return true
        }
        node = parent(of: selected)
        said = node.map { "\(name($0, starting: true)) selected" } ?? "Nothing selected"
        return true
    }

    /// An arrow key: the handle the keys are on moved, as its drag moves it; else the node selected
    /// stepped a track, with Shift resized, kept to the state with Option, and several together.
    private func arrow(_ dx: Int, _ dy: Int, shift: Bool, option: Bool) -> Bool {
        guard let selected = node else { return false }
        if also.isEmpty, let k = keyed {
            let list = handles(of: selected)
            guard list.indices.contains(k) else { return true }
            let patch = list[k].nudged(
                dx: dx, dy: dy, far: shift, outline: shaped, framing: imaged, state: state, fork: option)
            if let patch { make(patch) } else { said = nil }
            return true
        }
        step(selected, dx, dy, grow: shift, fork: option)
        return true
    }

    /// `selected` stepped by an arrow key, as the browser's keys step it (PLAN 2.75): one patch,
    /// the one a drag that far would make.
    private func step(_ selected: String, _ dx: Int, _ dy: Int, grow: Bool, fork: Bool) {
        if let held = boxes.first(where: { selection.contains($0.node) && $0.locked != nil }) {
            said = "It is locked: ⇧⌘L unlocks it"
            return
        }
        if !also.isEmpty && grow {
            said = "Resize one object at a time"
            return
        }
        do {
            let targets = try editor.session.targets(state: state, node: selected)
            guard let how = targets.snap(at: placements[selected], resize: grow, shift: false) else {
                said = "Drag it to move it"
                return
            }
            let drawn = { (n: String) -> CGRect? in boxes.first { $0.node == n }.flatMap { box($0) } }
            guard let to = targets.stepped(selected, dx: dx, dy: dy, how: how, grow: grow, box: drawn) else {
                said = "At the edge"
                return
            }
            if !also.isEmpty {
                let by = CGVector(dx: to.minX - targets.cell.minX, dy: to.minY - targets.cell.minY)
                let arranged = try editor.session.together(
                    state: state, nodes: selection, by: by, free: how == .free, fork: fork)
                guard let arranged, !arranged.patch.isEmpty else {
                    said = nil
                    return
                }
                make(arranged.patch)
                said = nil
                return
            }
            guard let snapped = try editor.session.snap(state: state, node: selected, how: how, to: to, fork: fork),
                !snapped.patch.isEmpty
            else {
                said = nil
                return
            }
            make(snapped.patch)
        } catch {
            said = "Not moved: \(error)"
        }
    }

    /// `[` and `]`: the node selected turned `by` degrees, 15 back or on, with Shift 1, as its
    /// round handle turns it: one `choose`, kept to the state with Option.
    private func turn(by: Double) -> Bool {
        guard let selected = node, also.isEmpty else { return false }
        if boxes.first(where: { $0.node == selected })?.locked != nil {
            said = "It is locked: ⇧⌘L unlocks it"
            return true
        }
        do {
            let start = try editor.session.turned(state: state, node: selected).rotate
            let now = ((start + by) * 1000).rounded() / 1000
            make(Handling.turning(selected, to: now, in: state, fork: Held.option))
            said = "Rotated to \(now.formatted())°"
        } catch {
            said = "Can't be rotated: \(error)"
        }
        return true
    }

    /// `+` on a point the keys are on: a point added after it, at the middle of the edge to the
    /// next, the keys going onto it.
    private func addPoint(option: Bool) -> Bool {
        guard let selected = node, also.isEmpty, let k = keyed, let o = shaped, o.node == selected else { return false }
        let list = handles(of: selected)
        guard list.indices.contains(k), case .point(let i) = list[k] else { return false }
        make(Handling.reshaping(o.node, to: o.adding(after: i), in: state, fork: option))
        keyed = k + 1
        said = "Point added after point \(i + 1)"
        return true
    }

    /// ⌘A: the node selected and all beside it, in what holds it; with none selected, all that
    /// stands on the canvas, but what is locked.
    private func selectAll() -> Bool {
        let level = parent(of: node)
        let all = readingOrder(boxes, in: level)
        guard let first = all.first else { return false }
        node = first
        also = Array(all.dropFirst())
        said = "\(all.count) selected"
        return true
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
        if Held.shift, let hit, node != nil {
            return toggle(hit)
        }
        node = hit
        also = []
        if let node, clicks >= 2 {
            typing.enter(node, in: state, at: point, fork: Held.option)
        }
    }

    #if !os(macOS)
    /// A finger pressing still (PLAN 4.3): held long enough, its press offers what is done to what
    /// it pressed, as the Mac's Node menu and Edit menu do to the node selected.
    private func linger(at start: CGPoint, fit: Fit) {
        lingering?.cancel()
        lingering = Task { @MainActor in
            try? await Task.sleep(for: Self.lingers)
            guard !Task.isCancelled, case .pressing = press else { return }
            press = .offered
            offer(at: fit.canvas(start), shown: start)
        }
    }

    /// Offer what is done to what draws at `point`, canvas units, from `shown`, view points: what
    /// draws there selected first, locked or not, so that Unlock is offered too, unless it is one
    /// of several selected; where nothing draws, nothing, and Paste and Insert land there.
    private func offer(at point: CGPoint, shown: CGPoint) {
        let hit = ((try? editor.session.hits(state: state, at: point)) ?? []).first?.node
        // Several selected stay selected, for what is done to all of them.
        let kept = hit.map { selection.contains($0) } ?? false
        if !kept {
            node = hit
            also = []
        }
        pointed = point
        offering = (count: (offering?.count ?? 0) + 1, at: shown)
    }

    /// Two fingers began, or let go (PLAN 4.3). As they begin the first finger's press stops, and
    /// a drag it began is drawn back, editing nothing; once they let go, the next press is new.
    private func fingers(_ down: Bool) {
        guard down else {
            if !touching, case .fingers = press { press = nil }
            return
        }
        lingering?.cancel()
        pausing?.cancel()
        if case .dragging = press { editor.still() }
        press = .fingers
    }
    #endif

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
            said = "Select objects in the same group together"
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
        let keep = Held.shift && parent(of: node) == nil ? selection : []
        let all = keep + inside.filter { !keep.contains($0) }
        node = all.first
        also = Array(all.dropFirst())
        said = all.isEmpty ? "Nothing lies wholly inside" : "\(all.count) selected"
    }

    /// A drag begun at `point`, canvas units: of a handle of the box selected, a resize; else a
    /// move of what draws there, selected.
    private func begin(at point: CGPoint, scale: CGFloat) {
        var held: (node: String, edge: Edge?)?
        if let selected = boxes.first(where: { $0.node == node }), selected.locked == nil, let r = box(selected) {
            let near = grabbing(selected, scale: scale)
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
        d.by = CGVector(dx: value.translation.width / scale, dy: value.translation.height / scale)
        d.fork = Held.option
        d.how = d.targets.snap(at: d.at, resize: d.edge != nil, shift: Held.shift)
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
                state: state, nodes: d.together, by: held, free: Held.shift, fork: d.fork,
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

    /// How near a handle of `selected`, canvas units, a press takes it: `grab` points, no more than
    /// a quarter of the box's shorter side, so that a finger inside a small box still moves it, and
    /// never less than a pointer's reach (PLAN 4.3).
    private func grabbing(_ selected: NodeBox, scale: CGFloat) -> Double {
        let units = Double(max(scale, 0.01))
        guard let r = box(selected) else { return Self.grab / units }
        return min(Self.grab / units, max(Self.reach / units, Double(min(r.width, r.height)) / 4))
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
