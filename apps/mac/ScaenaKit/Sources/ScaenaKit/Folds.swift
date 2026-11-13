import Foundation

/// A fold the Story offers: the slide `state` is on folded into the slide before it, or into the
/// earlier slide of `into` (R1's #1161).
public struct FoldAsk: Hashable, Sendable {
    public let state: String
    public let into: String?

    public init(state: String, into: String? = nil) {
        self.state = state
        self.into = into
    }
}

/// What a fold's button says before it is pressed, and why it cannot be pressed, where it cannot.
public struct FoldOffer: Equatable, Sendable {
    public let label: String
    public let refused: String?
}

extension FoldsTo {
    /// The button for a fold into `before`, a slide as a person counts it, as the browser's Story
    /// names it (#1222): from the fold's own dry run where it has answered, so "Fold Into Slide 3's
    /// Notes" where the words, laid out, would run past slide 3's place, and refused with why where
    /// the fold cannot be; from where the deck's slots say the words go, `slots`, until it answers.
    public static func offer(_ before: String, slots: FoldsTo?, _ folded: Folded?) -> FoldOffer {
        guard let folded else { return FoldOffer(label: label(before, slots), refused: nil) }
        if let refused = folded.refused { return FoldOffer(label: label(before, slots), refused: refused) }
        return FoldOffer(label: label(before, folded.to), refused: nil)
    }
}

extension DeckEditor {
    /// The dry run of each fold in `asks`, made away from the window on a snapshot of the deck, as
    /// the browser's Story asks its worker (#1222): each asked once for the source as it stands,
    /// kept until the source changes, so a Story read again lays out none anew. A fold whose dry
    /// run fails is left out, its button the slots' until then; none where the source does not
    /// compile, or changed while they were made.
    public func foldsAway(_ asks: [FoldAsk], isolation: isolated (any Actor)? = #isolation) async -> [FoldAsk: Folded] {
        if foldsRun.source != source { foldsRun = (source, [:]) }
        let kept = foldsRun.folds
        let wanted = Array(Set(asks.filter { kept[$0] == nil }))
        guard !wanted.isEmpty, valid, let snapshot = try? session.snapshot() else { return kept }
        let began = source
        let format = self.format
        let ran = try? await offMain.run { () -> [FoldAsk: Folded] in
            let apart = try snapshot.opened()
            try apart.setFormat(format)
            var folds: [FoldAsk: Folded] = [:]
            for ask in wanted {
                if let folded = try? apart.folding(ask.state, into: ask.into) { folds[ask] = folded }
            }
            return folds
        }
        guard source == began, foldsRun.source == began else { return [:] }
        if let ran { foldsRun.folds.merge(ran) { kept, _ in kept } }
        return foldsRun.folds
    }
}
