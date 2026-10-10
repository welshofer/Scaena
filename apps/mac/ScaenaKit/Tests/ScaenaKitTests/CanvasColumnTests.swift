#if os(macOS)
import AppKit
import ScaenaKit
import SwiftUI
import Testing

/// The canvas takes the room the timeline under it leaves (PLAN 3.14): in a window 900 points tall
/// beside the inspector's column, the canvas as the window draws it, at least 200 points tall,
/// over the timeline as it stands, its scrubber over a lane for each motion. A stack gave the
/// canvas its least once the timeline showed, the slide small between bands of nothing; what a
/// stack gives is printed beside what the column gives.
@MainActor
@Test func theCanvasTakesTheRoomTheTimelineLeaves() async throws {
    let column = try await laidOut {
        CanvasColumn {
            canvas
            Divider()
            timeline
        }
    }
    let stacked = try await laidOut {
        VStack(spacing: 0) {
            canvas
            Divider()
            timeline
        }
    }
    print(
        "the timeline shown, a window 900 points tall gives the canvas \(Int(column.canvas.height)) points in a"
            + " column, \(Int(stacked.canvas.height)) in a stack; the timeline stands \(Int(column.timeline.height))")
    #expect(column.timeline.height > 30 && column.timeline.height < 200)
    #expect(abs(column.canvas.width - 600) <= 1)
    // The canvas and the timeline fill the window's height, but for the line between them.
    #expect(column.canvas.height + column.timeline.height > 900 - 3)
    #expect(column.canvas.height > 600)
}

/// The canvas as the window draws it: what it is given, at least 200 points tall, the slide fitted
/// in it. Its room is a plain view the test reads back.
@MainActor private var canvas: some View {
    GeometryReader { _ in Marker(id: "canvas") }
        .frame(minWidth: 320, minHeight: 200)
        .columnCanvas()
}

/// The timeline as it stands under the canvas: play and add, the scrubber over a lane for each of
/// three motions, each named, and the time.
@MainActor private var timeline: some View {
    HStack(spacing: 10) {
        Image(systemName: "play.fill").frame(width: 16)
        Image(systemName: "plus.circle").fixedSize()
        Grid(alignment: .leading, horizontalSpacing: 8, verticalSpacing: 3) {
            GridRow {
                Color.clear.gridCellUnsizedAxes([.horizontal, .vertical])
                Slider(value: .constant(0.5), in: 0...1).controlSize(.small)
            }
            ForEach(["Transition", "Title, enters", "Words, enter"], id: \.self) { name in
                GridRow {
                    Text(name).font(.caption2).lineLimit(1).frame(width: 150, alignment: .leading)
                    GeometryReader { lane in Capsule().frame(width: lane.size.width / 3, height: 6) }
                        .frame(height: 12)
                }
            }
        }
        Text("1.92 s / 1.92 s").font(.caption.monospacedDigit()).frame(width: 96, alignment: .trailing)
    }
    .padding(.horizontal, 12)
    .padding(.vertical, 6)
    .background(Marker(id: "timeline"))
}

/// How big the canvas and the timeline stand once `column` is laid out beside the inspector's
/// column, 300 points wide, in a window 901 by 900 points.
@MainActor
private func laidOut<Column: View>(@ViewBuilder _ column: () -> Column) async throws -> (canvas: CGSize, timeline: CGSize) {
    _ = NSApplication.shared
    let built = column()
    let host = NSHostingView(
        rootView: HStack(spacing: 0) {
            built
            Divider()
            Color.clear.frame(width: 300)
        })
    host.sizingOptions = []
    let frame = CGRect(x: 0, y: 0, width: 901, height: 900)
    let window = NSWindow(contentRect: frame, styleMask: [.borderless], backing: .buffered, defer: false)
    window.contentView = host
    host.frame = frame
    host.layoutSubtreeIfNeeded()
    // Whatever the first pass left for later, laid out too.
    try await Task.sleep(for: .milliseconds(200))
    host.layoutSubtreeIfNeeded()
    let room = try #require(Marker.size("canvas", in: host))
    let bar = try #require(Marker.size("timeline", in: host))
    withExtendedLifetime(window) {}
    return (room, bar)
}

/// A plain AppKit view the window frames, read back by its identifier once laid out.
private struct Marker: NSViewRepresentable {
    let id: String

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        view.identifier = NSUserInterfaceItemIdentifier(id)
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {}

    /// The size of the view marked `id` under `view`: none where there is none.
    @MainActor static func size(_ id: String, in view: NSView) -> CGSize? {
        if view.identifier?.rawValue == id { return view.frame.size }
        for under in view.subviews {
            if let found = size(id, in: under) { return found }
        }
        return nil
    }
}
#endif
