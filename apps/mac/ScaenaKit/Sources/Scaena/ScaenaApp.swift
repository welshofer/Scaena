#if os(macOS)
import AppKit
#endif
import ScaenaKit
import ScaenaRemote
import SwiftUI
import os

/// How long the GPU and each deck took to open, at the debug level, which nothing keeps unless
/// asked: the iPad's UI tests stream it beside their screenshots (`apps/ipad/app-test.sh`).
let openingLog = Logger(subsystem: "com.welshofer.Scaena", category: "opening")

/// Scaena on the Mac (PLAN 3.3, SPEC §9.3): a document-based app over bundles. SwiftUI owns the
/// chrome; every frame, glyph, and layout comes from the engine.
@main
struct ScaenaApp: App {
    init() {
        // The GPU every canvas paints with, made as the app starts rather than when the first deck
        // shows (gate 3); on a queue of its own, since making it blocks. A failure here is said
        // again, by the surface that needs it.
        DispatchQueue.global(qos: .userInitiated).async {
            let began = Date()
            do {
                try ScaenaSurface.warm()
                openingLog.debug("the GPU made in \(Date().timeIntervalSince(began)) s")
            } catch {
                openingLog.debug("no GPU: \(String(describing: error), privacy: .public)")
            }
        }
    }

    #if os(macOS)
    /// A new window's size: room for the slides, the slide, and the inspector, within the screen it
    /// opens on.
    private static var opening: CGSize {
        let screen = NSScreen.main?.visibleFrame.size ?? CGSize(width: 1440, height: 900)
        return CGSize(width: min(1440, screen.width * 0.96), height: min(900, screen.height * 0.96))
    }
    #endif

    var body: some Scene {
        DocumentGroup(newDocument: { ScaenaDocument() }) { file in
            DeckView(document: file.document)
        }
        #if os(macOS)
        // Room for the slides, the slide, and the inspector, as a presentation app opens (PLAN 3.18).
        .defaultSize(Self.opening)
        .defaultPosition(.center)
        #endif
        .commands {
            NodeCommands()
            ViewCommands()
            WindowCommands()
            #if os(macOS)
            // The View menu's Show and Hide Toolbar, Customize Toolbar…, and Show and Hide Sidebar,
            // as a Mac app's (PLAN 3.27).
            ToolbarCommands()
            SidebarCommands()
            #endif
        }
        #if os(macOS)
        // The assistant's keys, kept in the Keychain (PLAN 3.6); on the iPad, a sheet of the
        // assistant's (PLAN 4.2).
        Settings {
            KeysSettings()
        }
        #else
        // The remote, on the iPad: it plays a show a Mac or another iPad presents (PLAN 4.10).
        WindowGroup("Remote", id: RemoteWindow.id) {
            RemoteView()
        }
        #endif
    }
}

/// The remote's window on the iPad (PLAN 4.10).
enum RemoteWindow {
    static let id = "remote"
}
