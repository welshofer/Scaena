import ScaenaKit
import SwiftUI

/// Over the canvas (PLAN 3.4, ADR-0013): a click selects the node the engine says draws there,
/// the topmost a pointer does not pass over (a locked node, PLAN 2.95), and the node selected is
/// outlined where it is drawn. Every box is the engine's, at rest; nothing here lays out.
struct CanvasSelection: View {
    let editor: DeckEditor
    let state: String
    /// The canvas, in canvas units.
    let size: CGSize
    @Binding var node: String?
    @State private var boxes: [NodeBox] = []

    var body: some View {
        GeometryReader { geometry in
            let scale = geometry.size.width / max(size.width, 1)
            ZStack(alignment: .topLeading) {
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture { at in
                        pick(CGPoint(x: at.x / scale, y: at.y / scale))
                    }
                if let selected = boxes.first(where: { $0.node == node }) {
                    outline(selected.corners, scale: scale)
                        .stroke(Color.accentColor, lineWidth: 1.5)
                        .allowsHitTesting(false)
                }
            }
        }
        .task(id: "\(state)\u{1f}\(editor.revision)") {
            boxes = (try? editor.session.boxes(state: state)) ?? []
        }
    }

    /// Select what draws at `point`, canvas units: nothing where nothing does.
    private func pick(_ point: CGPoint) {
        let hits = (try? editor.session.hits(state: state, at: point)) ?? []
        node = hits.first { $0.locked == nil }?.node
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
