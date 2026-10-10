import ScaenaKit
import SwiftUI

/// How the canvas stands on the view (PLAN 3.16): the part of it shown, from `origin`, canvas units,
/// at `scale` points to the unit. Zoomed out, the whole canvas from its top left.
struct Fit {
    var origin = CGPoint.zero
    var scale: CGFloat = 1

    /// The part of `canvas` shown in a view `width` points wide: `shown`, or none for the whole.
    init(shown: CGRect?, canvas: CGSize, width: CGFloat) {
        let part = shown ?? CGRect(origin: .zero, size: canvas)
        origin = part.origin
        scale = width / max(part.width, 1)
    }

    /// A point on the canvas, where it is on the view.
    func view(_ p: CGPoint) -> CGPoint { CGPoint(x: (p.x - origin.x) * scale, y: (p.y - origin.y) * scale) }

    /// A point on the view, on the canvas.
    func canvas(_ p: CGPoint) -> CGPoint {
        let s = max(scale, 0.0001)
        return CGPoint(x: p.x / s + origin.x, y: p.y / s + origin.y)
    }

    /// What draws a path in canvas units on the view.
    var transform: CGAffineTransform {
        CGAffineTransform(translationX: -origin.x, y: -origin.y).concatenating(CGAffineTransform(scaleX: scale, y: scale))
    }

    /// Canvas units to a point on the view, never none: a view not yet laid out has no width.
    var units: Double { 1 / max(Double(scale), 0.01) }
}

/// A handle of the node selected, held (PLAN 3.16), as the browser's canvas holds one: what it
/// changes as the pointer goes, from where the press began.
struct Holding {
    enum Handle {
        /// The rotate handle: the node turns about its anchor (PLAN 2.51).
        case turn(Turn)
        /// A shape's point, or one a press at the middle of an edge added (PLAN 2.68).
        case point(Int, added: Bool)
        /// A rect's corner: the radius step it rounds to.
        case corner(Int)
        /// A crop bar inside a side of the part of an image that shows (PLAN 2.74).
        case crop(Framing.Side)
        /// An image's focal point.
        case focal
    }

    var handle: Handle
    let node: String
    /// Where the press began, canvas units.
    let from: CGPoint
    /// The deck's revision when it began: an edit since leaves it as it was.
    let revision: Int
    /// Whether the pointer has gone further than a click.
    var moved = false
    var fork = false
    /// A shape's points as the drag leaves them, fractions of its box.
    var points: [CGPoint] = []
    /// An image's crop and focal point as the drag leaves them.
    var crop: [Double] = []
    var focal = CGPoint.zero
}

/// How far the rotate handle stands beyond the box, a rect's corner handle in from its corner at
/// least, and an image's crop bars in from its sides, in points. On the iPad the rotate handle
/// stands further out, so that a finger takes it and not the box's top (PLAN 4.3).
enum HandleSpacing {
    #if os(macOS)
    static let arm = 24.0
    #else
    static let arm = 36.0
    #endif
    static let clear = 12.0
    static let inset = 10.0
}

/// The handles of the node selected (PLAN 3.16), as the browser's canvas draws them: the rotate
/// handle above its box; a line's, an arrow's, or a polygon's points, and the middle of each edge,
/// where a press adds one; a rect's corner; and an image's crop bars inside each side of the part
/// that shows, and its focal point. They take no press: the canvas reads each press itself.
struct HandleMarks: View {
    let box: NodeBox
    let outline: Outline?
    let framing: Framing?
    /// The point picked, which Delete takes away.
    let picked: Int?
    let fit: Fit

    var body: some View {
        ZStack(alignment: .topLeading) {
            if let turn = box.turnHandle(arm: HandleSpacing.arm * fit.units) {
                Path { path in
                    path.move(to: view(turn.top))
                    path.addLine(to: view(turn.at))
                }
                .stroke(Color.accentColor, lineWidth: 1)
                dot(turn.at, radius: 5, fill: .white)
            }
            if let outline, ["line", "arrow", "polygon"].contains(outline.kind) {
                ForEach(outline.middles.indices, id: \.self) { i in
                    dot(outline.middles[i], radius: 3.5, fill: Color.accentColor.opacity(0.35))
                }
                ForEach(outline.drawn.indices, id: \.self) { i in
                    dot(outline.drawn[i], radius: 5, fill: i == picked ? Color.accentColor : .white)
                }
            }
            if let outline, outline.kind == "rect", !outline.radii.isEmpty {
                dot(outline.corner(clear: HandleSpacing.clear * fit.units), radius: 4.5, fill: .yellow)
            }
            if let framing {
                ForEach(Framing.Side.allCases, id: \.self) { side in
                    bar(framing, side)
                }
                focal(framing.focalHandle)
            }
        }
    }

    private func view(_ p: CGPoint) -> CGPoint { fit.view(p) }

    private func dot(_ at: CGPoint, radius: CGFloat, fill: Color) -> some View {
        Circle()
            .fill(fill)
            .overlay(Circle().stroke(Color.accentColor, lineWidth: 1))
            .frame(width: radius * 2, height: radius * 2)
            .position(view(at))
    }

    /// A crop bar, along its side as the image is drawn.
    private func bar(_ framing: Framing, _ side: Framing.Side) -> some View {
        let along = side == .top || side == .bottom
        return RoundedRectangle(cornerRadius: 1.5)
            .fill(Color.white)
            .overlay(RoundedRectangle(cornerRadius: 1.5).stroke(Color.accentColor, lineWidth: 1))
            .frame(width: along ? 16 : 5, height: along ? 5 : 16)
            .rotationEffect(.radians(angle(framing.transform)))
            .position(view(framing.handle(side, inset: HandleSpacing.inset * fit.units)))
    }

    /// The focal point: a ring with a cross through it.
    private func focal(_ at: CGPoint) -> some View {
        let p = view(at)
        return ZStack(alignment: .topLeading) {
            Path { path in
                path.move(to: CGPoint(x: p.x - 10, y: p.y))
                path.addLine(to: CGPoint(x: p.x + 10, y: p.y))
                path.move(to: CGPoint(x: p.x, y: p.y - 10))
                path.addLine(to: CGPoint(x: p.x, y: p.y + 10))
            }
            .stroke(Color.accentColor, lineWidth: 1)
            Circle()
                .stroke(Color.accentColor, lineWidth: 1.5)
                .frame(width: 12, height: 12)
                .position(p)
        }
    }

    /// The angle `map` turns the x axis to, radians.
    private func angle(_ map: [Double]?) -> Double {
        guard let m = map, m.count == 6 else { return 0 }
        return atan2(m[1], m[0])
    }
}

/// A handle held, drawn as it goes (PLAN 3.16), as the browser's canvas draws it: the node's
/// outline turned about its anchor; the outline a point or the corner leaves; or the whole image
/// outlined, with the part the crop keeps over it and the focal point.
struct HandleHolding: View {
    let holding: Holding
    let box: NodeBox
    let outline: Outline?
    let framing: Framing?
    let fit: Fit

    var body: some View {
        switch holding.handle {
        case .turn(let turn):
            turned(turn)
        case .point:
            if let outline { reshaped(outline) }
        case .corner(let step):
            if let outline { rounded(outline, step) }
        case .crop, .focal:
            if let framing { cropped(framing) }
        }
    }

    private static let dashed = StrokeStyle(lineWidth: 1.5, dash: [5, 3])

    /// The box's outline turned as far as the turn has gone, about its pivot.
    private func turned(_ turn: Turn) -> some View {
        let a = turn.turning * .pi / 180
        let (s, c) = (sin(a), cos(a))
        let (px, py) = (Double(turn.pivot.x), Double(turn.pivot.y))
        let corners = box.corners.map { p -> CGPoint in
            let (dx, dy) = (Double(p.x) - px, Double(p.y) - py)
            return CGPoint(x: px + dx * c - dy * s, y: py + dx * s + dy * c)
        }
        return joined(corners, closed: true).stroke(Color.accentColor, style: Self.dashed)
    }

    /// The points as the drag leaves them, joined: a polygon's closed.
    private func reshaped(_ outline: Outline) -> some View {
        joined(holding.points.map { outline.onCanvas($0) }, closed: outline.kind == "polygon")
            .stroke(Color.accentColor, style: Self.dashed)
    }

    /// The rect rounded to the step the corner is dragged to.
    private func rounded(_ outline: Outline, _ step: Int) -> some View {
        let radius = outline.radii.indices.contains(step) ? outline.radii[step] : 0
        return Path(roundedRect: outline.rect, cornerRadius: CGFloat(radius))
            .applying(map(outline.transform).concatenating(fit.transform))
            .stroke(Color.accentColor, style: Self.dashed)
    }

    /// The whole image outlined, what the crop cuts away shaded, the part it keeps, and the focal
    /// point.
    private func cropped(_ framing: Framing) -> some View {
        let m = map(framing.transform).concatenating(fit.transform)
        let kept = framing.kept(holding.crop.count == 4 ? holding.crop : framing.crop)
        let focal = framing.focalPoint(holding.focal)
        return ZStack(alignment: .topLeading) {
            Path { path in
                path.addRect(framing.whole)
                path.addRect(kept)
            }
            .applying(m)
            .fill(Color.black.opacity(0.35), style: FillStyle(eoFill: true))
            Path(framing.whole).applying(m)
                .stroke(Color.white.opacity(0.8), style: StrokeStyle(lineWidth: 1, dash: [4, 3]))
            Path(kept).applying(m).stroke(Color.accentColor, lineWidth: 1.5)
            Circle()
                .stroke(Color.accentColor, lineWidth: 1.5)
                .frame(width: 12, height: 12)
                .position(fit.view(focal))
        }
    }

    /// `points`, canvas units, joined on the view.
    private func joined(_ points: [CGPoint], closed: Bool) -> Path {
        Path { path in
            guard let first = points.first else { return }
            path.move(to: fit.view(first))
            for p in points.dropFirst() {
                path.addLine(to: fit.view(p))
            }
            if closed { path.closeSubpath() }
        }
    }

    /// `[a, b, c, d, e, f]` as an affine transform: none, the identity.
    private func map(_ m: [Double]?) -> CGAffineTransform {
        guard let m, m.count == 6 else { return .identity }
        return CGAffineTransform(
            a: CGFloat(m[0]), b: CGFloat(m[1]), c: CGFloat(m[2]), d: CGFloat(m[3]), tx: CGFloat(m[4]), ty: CGFloat(m[5]))
    }
}

/// The layout's empty slots over the canvas (PLAN 3.30, as the browser's): each outlined, dashed,
/// where a picture, a figure, or words go, through the part of the canvas shown; the canvas puts
/// the prompt's words on it, which a press fills it from (`WaitingButton`). A press there and
/// Insert, or a picture dropped there, fills it too; it takes no press itself.
struct WaitingOverlay: View {
    let editor: DeckEditor
    /// The state shown.
    let state: String
    /// The part of the canvas shown, where it is zoomed in.
    let shown: CGRect?
    @State private var waits: [WaitingSlot] = []
    @State private var canvas: CGSize = .zero

    var body: some View {
        GeometryReader { geometry in
            if canvas.width > 0 {
                let fit = Fit(shown: shown, canvas: canvas, width: geometry.size.width)
                ForEach(waits) { slot in outline(slot, fit: fit) }
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
        .task(id: "\(state)\u{1f}\(editor.revision)\u{1f}\(editor.format ?? "")") {
            canvas = (try? editor.session.grid().canvas) ?? .zero
            waits = (try? editor.session.waiting(state: state)) ?? []
        }
    }

    private func outline(_ slot: WaitingSlot, fit: Fit) -> some View {
        let r = slot.rect.applying(fit.transform)
        let tint = Color.accentColor
        return RoundedRectangle(cornerRadius: 4)
            .fill(tint.opacity(0.05))
            .overlay {
                RoundedRectangle(cornerRadius: 4)
                    .strokeBorder(tint.opacity(0.6), style: StrokeStyle(lineWidth: 1, dash: [5, 4]))
            }
            .frame(width: max(r.width, 0), height: max(r.height, 0))
            .position(x: r.midX, y: r.midY)
    }
}

/// An empty slot's words in the middle of its outline, which a press fills it from, as a
/// presentation app's placeholder does (PLAN 3.30): words in the slot's role, typed in with their
/// words selected, or a picture asked for and put there.
struct WaitingButton: View {
    let slot: WaitingSlot
    let fill: () -> Void

    var body: some View {
        Button(action: fill) {
            Label(slot.words, systemImage: slot.typed ? "text.cursor" : "photo.badge.plus")
                .font(.caption)
                .lineLimit(1)
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(.thinMaterial, in: Capsule())
                .foregroundStyle(.secondary)
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .help(slot.typed ? "Type here" : "Choose a picture to put here")
        .accessibilityLabel(slot.words)
        .accessibilityHint(slot.typed ? "Puts words here to type over" : "Chooses a picture to put here")
        .accessibilityIdentifier("waiting-\(slot.slot)")
    }
}

/// The safe area over the canvas (PLAN 3.29, as the browser's): a strip 3/8 inch deep around the
/// slide's edge, tinted, its inner edge dashed, through the part of the canvas shown. It shows a
/// person where to keep what must not be cut off, keeping nothing out, and takes no press.
struct SafeAreaOverlay: View {
    let editor: DeckEditor
    /// The part of the canvas shown, where it is zoomed in.
    let shown: CGRect?
    @State private var grid: GridLines?

    var body: some View {
        GeometryReader { geometry in
            if let grid, grid.canvas.width > 0, !grid.safe.isNull {
                strip(grid, fit: Fit(shown: shown, canvas: grid.canvas, width: geometry.size.width))
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
        .task(id: editor.revision) { grid = try? editor.session.grid() }
    }

    private func strip(_ grid: GridLines, fit: Fit) -> some View {
        let amber = Color(red: 1, green: 0.62, blue: 0.12)
        return ZStack(alignment: .topLeading) {
            Path { path in
                path.addRect(CGRect(origin: .zero, size: grid.canvas))
                path.addRect(grid.safe)
            }
            .applying(fit.transform)
            .fill(amber.opacity(0.16), style: FillStyle(eoFill: true))
            Path(grid.safe)
                .applying(fit.transform)
                .stroke(amber.opacity(0.75), style: StrokeStyle(lineWidth: 1, dash: [6, 4]))
        }
    }
}

/// The theme's grid in the format shown over the canvas (PLAN 3.16, as the browser's, PLAN 2.57):
/// its columns and rows as bands, the gutters between them, the margins around them dashed, and the
/// baseline grid's lines, through the part of the canvas shown. It takes no press.
struct GridOverlay: View {
    let editor: DeckEditor
    /// The part of the canvas shown, where it is zoomed in.
    let shown: CGRect?
    @State private var grid: GridLines?

    var body: some View {
        GeometryReader { geometry in
            if let grid, grid.canvas.width > 0 {
                ruled(grid, fit: Fit(shown: shown, canvas: grid.canvas, width: geometry.size.width))
            }
        }
        .allowsHitTesting(false)
        .task(id: editor.revision) { grid = try? editor.session.grid() }
    }

    private func ruled(_ grid: GridLines, fit: Fit) -> some View {
        let left = CGFloat(grid.columns.first?.lowerBound ?? 0)
        let right = CGFloat(grid.columns.last?.upperBound ?? Double(grid.canvas.width))
        let top = CGFloat(grid.rows.first?.lowerBound ?? 0)
        let bottom = CGFloat(grid.rows.last?.upperBound ?? Double(grid.canvas.height))
        let pink = Color(red: 1, green: 0.36, blue: 0.66)
        return ZStack(alignment: .topLeading) {
            Path { path in
                for c in grid.columns {
                    let (a, b) = (CGFloat(c.lowerBound), CGFloat(c.upperBound))
                    path.addRect(CGRect(x: a, y: top, width: max(0, b - a), height: max(0, bottom - top)))
                }
                for r in grid.rows {
                    let (a, b) = (CGFloat(r.lowerBound), CGFloat(r.upperBound))
                    path.addRect(CGRect(x: left, y: a, width: max(0, right - left), height: max(0, b - a)))
                }
            }
            .applying(fit.transform)
            .fill(pink.opacity(0.07))
            Path { path in
                for y in grid.baselines {
                    path.move(to: CGPoint(x: left, y: CGFloat(y)))
                    path.addLine(to: CGPoint(x: right, y: CGFloat(y)))
                }
            }
            .applying(fit.transform)
            .stroke(Color(red: 0.25, green: 0.77, blue: 1).opacity(0.3), lineWidth: 0.5)
            Path(CGRect(x: left, y: top, width: right - left, height: bottom - top))
                .applying(fit.transform)
                .stroke(pink.opacity(0.55), style: StrokeStyle(lineWidth: 1, dash: [4, 3]))
        }
    }
}
