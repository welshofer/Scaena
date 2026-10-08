import Foundation
import ScaenaKit
import SwiftUI

#if canImport(FoundationModels)
import FoundationModels
#endif

/// The on-device model's three tasks (PLAN 3.6, ADR-0022): a finding explained, a state's notes
/// drafted, and a text's words tightened. Apple's Foundation Models answer on this Mac, and
/// nothing leaves it. None edits on its own: each answer is offered, and the user takes it as one
/// patch, or leaves it. Where the Mac has no model ready (before macOS 26, or without Apple
/// Intelligence), none is offered.
enum OnDevice {
    /// Why no answer came.
    struct Unavailable: LocalizedError {
        var errorDescription: String? { "The on-device model is not available on this Mac." }
    }

    /// Whether the on-device model is ready to answer.
    static var available: Bool {
        #if canImport(FoundationModels)
        if #available(macOS 26.0, *) {
            if case .available = SystemLanguageModel.default.availability { return true }
        }
        #endif
        return false
    }

    /// What `finding` means for this deck, in plain words, and what could be done.
    static func explain(_ finding: Finding, text: String?) async throws -> String {
        var prompt = "Finding \(finding.code) (\(finding.severity)): \(finding.message)"
        if let hint = finding.hint { prompt += "\nLint's hint: \(hint)" }
        if let state = finding.state { prompt += "\nIn state: \(state)" }
        if let node = finding.node { prompt += "\nAbout: \(node)" }
        if let text, !text.isEmpty { prompt += "\nIts words: \(text)" }
        return try await respond(
            instructions: """
                You explain a finding of a slide deck's lint to the person writing the deck, in two or \
                three plain sentences: what it means for their slide, and what they could do about it. \
                Say nothing the finding does not support, and write no headings or lists.
                """,
            prompt: prompt)
    }

    /// Notes for the speaker of `state`, drafted from how it reads.
    static func notes(state: String, reading: String) async throws -> String {
        try await respond(
            instructions: """
                You draft speaker notes for one slide of a talk: what the speaker says while it \
                shows, in two to four short sentences, in the slide's language, from the slide's \
                words. Write only the notes, with no heading.
                """,
            prompt: "The slide \(state) reads:\n\(plain(reading))")
    }

    /// `text` tightened: fewer words, the same meaning.
    static func tighten(_ text: String) async throws -> String {
        let tightened = try await respond(
            instructions: """
                You tighten the words of one text on a slide: fewer words, the same meaning, in the \
                same language and tone. Keep every number and name exactly as written. Write only \
                the tightened text, with no quotes and no comment.
                """,
            prompt: text)
        return tightened.trimmingCharacters(in: .whitespacesAndNewlines.union(CharacterSet(charactersIn: "\"“”")))
    }

    private static func respond(instructions: String, prompt: String) async throws -> String {
        #if canImport(FoundationModels)
        if #available(macOS 26.0, *) {
            let session = LanguageModelSession(instructions: instructions)
            let response = try await session.respond(to: prompt)
            return response.content.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        #endif
        throw Unavailable()
    }

    /// HTML as the words it reads.
    private static func plain(_ html: String) -> String {
        var text = html.replacingOccurrences(of: "<[^>]+>", with: " ", options: .regularExpression)
        for (entity, character) in [("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'"), ("&amp;", "&")] {
            text = text.replacingOccurrences(of: entity, with: character)
        }
        return text.split(whereSeparator: \.isWhitespace).joined(separator: " ")
    }
}

/// A question for the on-device model, and the patch that takes its answer, if one does.
struct Asked: Identifiable {
    let id = UUID()
    let title: String
    let ask: () async throws -> String
    let take: ((String) -> [JSONValue])?
}

/// The model's answer as it comes: thinking, then the answer to take or leave, or why none came.
struct OfferSheet: View {
    let title: String
    let ask: () async throws -> String
    /// The patch that takes the answer, edited or not; none where it is only read.
    let take: ((String) -> [JSONValue])?
    let make: ([JSONValue]) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var answer: String?
    @State private var failure: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(title, systemImage: "sparkles").font(.headline)
            if let answer {
                if take != nil {
                    TextEditor(text: Binding(get: { answer }, set: { self.answer = $0 }))
                        .font(.body)
                        .frame(minHeight: 120)
                } else {
                    ScrollView {
                        Text(answer).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .frame(minHeight: 80, maxHeight: 240)
                }
            } else if let failure {
                Text(failure).foregroundStyle(.secondary)
            } else {
                ProgressView("Thinking on this Mac…")
            }
            Text("Answered on this Mac: nothing was sent anywhere.").font(.caption).foregroundStyle(.secondary)
            HStack {
                Spacer()
                if let take, let answer {
                    Button("Leave it") { dismiss() }
                        .keyboardShortcut(.cancelAction)
                    Button("Use it") {
                        make(take(answer))
                        dismiss()
                    }
                    .keyboardShortcut(.defaultAction)
                } else {
                    Button("Done") { dismiss() }
                        .keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding(20)
        .frame(width: 420)
        .task {
            do {
                answer = try await ask()
            } catch {
                failure = error.localizedDescription
            }
        }
    }
}
