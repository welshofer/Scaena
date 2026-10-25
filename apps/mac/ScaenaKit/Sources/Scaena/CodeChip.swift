import ScaenaKit
import SwiftUI

/// The language of the code selected, a chip on its top right corner (PLAN 2.133), as the
/// browser's canvas shows it: a click offers the languages, as a person calls them, and one chosen
/// is one `choose` of `code/language`, one step to undo.
struct CodeChip: View {
    let code: NodeBox.CodeLanguage
    /// The languages offered, each as the deck names it and as a person calls it, in the deck's
    /// order: what the inspector's Language offers.
    let languages: () -> [(name: String, said: String)]
    let choose: (String) -> Void

    var body: some View {
        Menu {
            ForEach(languages(), id: \.name) { language in
                Button {
                    if language.name != code.language { choose(language.name) }
                } label: {
                    if language.name == code.language {
                        Label(language.said, systemImage: "checkmark")
                    } else {
                        Text(language.said)
                    }
                }
            }
        } label: {
            Text(code.said)
                .font(.caption.weight(.medium))
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .floating(in: Capsule())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .help("\(code.said): click to change the language")
        .accessibilityLabel("Language")
        .accessibilityValue(code.said)
        .accessibilityIdentifier("code-chip")
    }

    /// About how wide it is drawn, in points: what places it by its trailing edge.
    var width: CGFloat { CGFloat(code.said.count) * 7 + 20 }
}
