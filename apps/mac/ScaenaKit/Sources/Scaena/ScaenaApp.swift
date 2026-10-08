import ScaenaKit
import SwiftUI

/// Scaena on the Mac (PLAN 3.3, SPEC §9.3): a document-based app over bundles. SwiftUI owns the
/// chrome; every frame, glyph, and layout comes from the engine.
@main
struct ScaenaApp: App {
    init() {
        // The GPU every canvas paints with, made as the app starts rather than when the first deck
        // shows (gate 3); on a queue of its own, since making it blocks. A failure here is said
        // again, by the surface that needs it.
        DispatchQueue.global(qos: .userInitiated).async { try? ScaenaSurface.warm() }
    }

    var body: some Scene {
        DocumentGroup(newDocument: { ScaenaDocument() }) { file in
            DeckView(document: file.document)
        }
        .commands {
            NodeCommands()
        }
        // The assistant's keys, kept in the Keychain (PLAN 3.6).
        Settings {
            KeysSettings()
        }
    }
}
