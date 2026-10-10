import SwiftUI

/// The canvas with what shows over and under it, top to bottom (PLAN 3.14, 3.16): the find bar,
/// then the canvas, then the slide in its other sizes and its timeline. Each of the rest stands as
/// tall as it is at the column's width, and the canvas takes the room they leave, however tall the
/// window. A stack shares its room out by what each part will take: under the timeline it gave
/// the canvas its least, 200 points, and the slide sat small between bands of nothing. The part
/// marked `columnCanvas()` is the canvas.
public struct CanvasColumn: Layout {
    public init() {}

    public func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = subviews.reduce(proposal.width ?? 0) { widest, part in
            max(widest, part.sizeThatFits(ProposedViewSize(width: proposal.width, height: nil)).width)
        }
        let rest = Self.rest(subviews, width: width)
        let canvas = subviews.filter { $0[Canvas.self] }
        // As tall as offered, and never so short the canvas has less than its least; offered no
        // height, as tall as the canvas would be.
        guard let height = proposal.height else {
            let ideal = canvas.reduce(rest) { $0 + $1.sizeThatFits(ProposedViewSize(width: width, height: nil)).height }
            return CGSize(width: width, height: ideal)
        }
        let least = canvas.reduce(rest) { $0 + $1.sizeThatFits(ProposedViewSize(width: width, height: 0)).height }
        return CGSize(width: width, height: max(height, least))
    }

    public func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let canvases = subviews.filter { $0[Canvas.self] }.count
        let room = max(bounds.height - Self.rest(subviews, width: bounds.width), 0) / CGFloat(max(canvases, 1))
        var y = bounds.minY
        for part in subviews {
            let height =
                part[Canvas.self] ? room : part.sizeThatFits(ProposedViewSize(width: bounds.width, height: nil)).height
            part.place(
                at: CGPoint(x: bounds.minX, y: y), anchor: .topLeading,
                proposal: ProposedViewSize(width: bounds.width, height: height))
            y += height
        }
    }

    /// How tall the parts that are not the canvas stand at `width`, together.
    private static func rest(_ subviews: Subviews, width: CGFloat) -> CGFloat {
        subviews.reduce(0) { sum, part in
            part[Canvas.self] ? sum : sum + part.sizeThatFits(ProposedViewSize(width: width, height: nil)).height
        }
    }

    /// Whether a part is the canvas.
    struct Canvas: LayoutValueKey {
        static let defaultValue = false
    }
}

extension View {
    /// The part of a `CanvasColumn` that takes the room the rest leave: the canvas.
    public func columnCanvas() -> some View {
        layoutValue(key: CanvasColumn.Canvas.self, value: true)
    }
}
