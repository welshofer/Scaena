import Foundation

/// A source a deck made from a prompt was grounded in (narrative FUNC-3), as `scaena files`
/// lists it: what the Story panel offers a figure no source backs to cite (W1's dogfood 5).
public struct Grounded: Decodable, Sendable, Equatable {
    public let name: String
    /// `file`, `sec`, `github`, `google`, `web`, or `research`.
    public let kind: String
    /// Where it came from, where it has an address.
    public let url: String?
}

/// What a slide's beat cites as its evidence (SPEC §2.6, `StateChoices.cites`): data, assets,
/// addresses, and the sources the deck was grounded in (`source:NAME`), written by JSON Patch at
/// `at`, its `evidence`.
public struct CiteChoice: Decodable, Sendable, Equatable {
    /// The beat, by its id.
    public let beat: String
    /// The JSON Pointer to the beat's `evidence`.
    public let at: String
    /// What it cites now.
    public let evidence: [String]

    /// The patch that has the beat cite `source` (W1's dogfood 5), as the browser's Story makes
    /// it: `source:NAME` added to its evidence, the list whole where it cites nothing yet; none
    /// where it cites it already.
    public func citing(_ source: String) -> [JSONValue]? {
        let value = "source:\(source)"
        guard !evidence.contains(value) else { return nil }
        if evidence.isEmpty {
            return [["op": "add", "path": .string(at), "value": [.string(value)]]]
        }
        return [["op": "add", "path": .string("\(at)/-"), "value": .string(value)]]
    }
}

extension ScaenaSession {
    /// The sources the deck was grounded in, in the order they were given; none for a deck made
    /// from none (narrative FUNC-3).
    public func grounded() throws -> [Grounded] { try call("grounded") }
}
