import ScaenaKit
import SwiftUI

/// Scaena on the Mac (PLAN 3.3, SPEC §9.3): a document-based app over bundles. SwiftUI owns the
/// chrome; every frame, glyph, and layout comes from the engine.
@main
struct ScaenaApp: App {
    var body: some Scene {
        DocumentGroup(newDocument: { ScaenaDocument() }) { file in
            DeckView(document: file.document)
        }
        // The assistant's keys, kept in the Keychain (PLAN 3.6).
        Settings {
            KeysSettings()
        }
    }
}
