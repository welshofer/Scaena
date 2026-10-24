import Foundation
import ScaenaKit
import SwiftUI

#if canImport(FoundationModels)
import FoundationModels
#endif

/// Apple's Foundation Models, on this Mac or iPad (PLAN 3.6, ADR-0022): where the device has a
/// model ready, the writing tools ask it, and nothing leaves the device. Before macOS or iPadOS 26,
/// or without Apple Intelligence, it is not available.
enum OnDevice {
    /// Why no answer came.
    struct Unavailable: LocalizedError {
        var errorDescription: String? {
            "No model to ask: this \(Words.device) has no on-device model, and the assistant has no key. Add one in the assistant's Keys."
        }
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

    /// Why the on-device model gave no answer, in a person's words (PLAN 3.18): what the model
    /// says of it is for its makers, not the window.
    struct Unanswered: LocalizedError {
        var errorDescription: String? {
            "The model on this \(Words.device) couldn't answer that. Ask again, or in other words."
        }
    }

    /// `prompt` answered by the on-device model, told `instructions`. Where it declines what the
    /// user's own model may take, `Declined` says why (lift-up STAB-4).
    static func respond(instructions: String, prompt: String) async throws -> String {
        #if canImport(FoundationModels)
        if #available(macOS 26.0, *) {
            let session = LanguageModelSession(instructions: instructions)
            do {
                let response = try await session.respond(to: prompt)
                return response.content.trimmingCharacters(in: .whitespacesAndNewlines)
            } catch let error as CancellationError {
                throw error
            } catch let error as LanguageModelSession.GenerationError {
                if let declined = declined(error) { throw declined }
                throw Unanswered()
            } catch {
                throw Unanswered()
            }
        }
        #endif
        throw Unavailable()
    }

    #if canImport(FoundationModels)
    /// What the on-device model declined that another model may answer: more than its context
    /// takes, its guardrails, a language it does not write, or its model not on the device yet.
    @available(macOS 26.0, *)
    private static func declined(_ error: LanguageModelSession.GenerationError) -> Declined? {
        switch error {
        case .exceededContextWindowSize: Declined(.tooLong)
        case .guardrailViolation: Declined(.guardrails)
        case .unsupportedLanguageOrLocale: Declined(.language)
        case .assetsUnavailable: Declined(.notReady)
        default: nil
        }
    }
    #endif
}

/// Who answers the writing tools (PLAN 2.101): the on-device model where the device has one,
/// nothing leaving it; else the model the assistant uses, with the user's key from the Keychain,
/// as the browser's writing tools ask it. What each is asked is the engine's (`scaena_core::
/// writing`), the same words on every platform.
enum Writer {
    /// The model the assistant uses and its key: none where no key is kept or no model chosen.
    static func byKey() -> (provider: Provider, model: String, key: String)? {
        let defaults = UserDefaults.standard
        let provider = defaults.string(forKey: "assistant.provider").flatMap(Provider.init(rawValue:)) ?? .anthropic
        let model = defaults.string(forKey: "assistant.model.\(provider.rawValue)") ?? ""
        guard !model.isEmpty, let key = Keychain().key(for: provider), !key.isEmpty else { return nil }
        return (provider, model, key)
    }

    /// Whether there is a model to ask.
    static var available: Bool { OnDevice.available || byKey() != nil }

    /// Where an answer comes from, as the sheet says it.
    static var whence: String {
        if OnDevice.available { return "Answered on this \(Words.device): nothing was sent anywhere." }
        if let who = byKey() { return "Written by \(who.model), with your \(who.provider.name) key." }
        return ""
    }

    /// Whether a picture can be described (PLAN 2.119): the on-device model reads no pictures, so
    /// only the assistant's model, with the user's key, is shown one.
    static var describes: Bool { byKey() != nil }

    /// `writing` answered: on the device, or by the assistant's model with the user's key; one
    /// shown a `picture` by the assistant's model alone. What the device declines, the user's own
    /// model is offered for where a key is kept, and asked only once the person agrees: the words
    /// leave the Mac only when they ask (lift-up STAB-4).
    static func respond(instructions: String, prompt: String, picture: String? = nil) async throws
        -> Answering.Outcome
    {
        let elsewhere = byKey().map { who in
            Answering.Elsewhere(model: who.model, provider: who.provider.name) {
                try await who.provider.write(
                    model: who.model, key: who.key, instructions: instructions, prompt: prompt, picture: picture)
            }
        }
        if OnDevice.available, picture == nil {
            let outcome = try await Answering.answer(
                { try await OnDevice.respond(instructions: instructions, prompt: prompt) }, else: elsewhere)
            // No key kept: the device's model could not answer, as before.
            if case .declined(_, nil) = outcome { throw OnDevice.Unanswered() }
            return outcome
        }
        guard let elsewhere else { throw OnDevice.Unavailable() }
        return .answered(try await elsewhere.agreed())
    }

    /// What each tool is called before the engine names it, and where nothing is asked, as the
    /// browser's writing tools call them.
    static func named(_ kind: WritingKind) -> String {
        switch kind {
        case .proofread: "Proofread"
        case .tighten: "Tighten"
        case .rewrite: "Rewrite"
        case .notes: "Draft Notes"
        case .explain: "Explain"
        case .describe: "Describe"
        case .claim: "Check the Claim"
        }
    }

    /// `ask` of the deck `session` holds, as the sheet offers it: what the model is asked, the
    /// answer shown as it is offered, and the patch that takes it, edited or not, none where it
    /// changes nothing; none for an answer only read. Where nothing can be asked, as of a text box
    /// with no words, the sheet says why.
    @MainActor
    static func asking(_ ask: WritingAsk, of session: ScaenaSession) -> Asked {
        let writing: Writing
        do {
            writing = try session.writing(ask)
        } catch {
            let why = "Nothing to ask. \(Why.said(error))"
            return Asked(title: named(ask.kind), ask: { throw Unasked(errorDescription: why) }, take: nil)
        }
        let (instructions, prompt, picture) = (writing.instructions, writing.prompt, writing.picture)
        let reads = ask.kind == .explain
        // A picture goes to the assistant's model alone, which the sheet says.
        let byKey = byKey().map { "Written by \($0.model), with your \($0.provider.name) key: it was shown the picture." }
        return Asked(
            title: writing.title,
            ask: { try await Writer.respond(instructions: instructions, prompt: prompt, picture: picture) },
            shown: { answer in (try? session.written(ask, answer: answer).text) ?? answer },
            take: reads ? nil : { answer in try session.taking(ask, answer: answer) },
            whence: picture == nil ? whence : byKey ?? whence)
    }

    /// Why nothing was asked.
    struct Unasked: LocalizedError {
        var errorDescription: String?
    }
}

/// A question for a model, the answer as it is offered, and the patch that takes it, if one does.
struct Asked: Identifiable {
    let id = UUID()
    let title: String
    let ask: () async throws -> Answering.Outcome
    /// The model's answer as it is offered.
    var shown: (String) -> String = { $0 }
    /// The patch that takes the answer, edited or not: none where it changes nothing.
    let take: ((String) throws -> [JSONValue]?)?
    /// Where the answer comes from.
    var whence = Writer.whence
}

/// The model's answer as it comes: thinking, then the answer to take or leave, or why none came.
struct OfferSheet: View {
    let title: String
    let ask: () async throws -> Answering.Outcome
    /// The model's answer as it is offered.
    var shown: (String) -> String = { $0 }
    /// The patch that takes the answer, edited or not, none where it changes nothing; no taking
    /// where it is only read.
    let take: ((String) throws -> [JSONValue]?)?
    /// Where the answer comes from.
    var whence = Writer.whence
    /// The answer taken: its patch, or none where it changes nothing.
    let make: ([JSONValue]?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var answer: String?
    @State private var failure: String?
    /// Why the answer was not taken.
    @State private var refused: String?
    /// Where the answer came from, where the model asked first declined (lift-up STAB-4).
    @State private var answered: String?
    /// What the device declined, and the user's own model offered in its place, asked only when the
    /// person presses its button (lift-up STAB-4).
    @State private var declined: (said: String, elsewhere: Answering.Elsewhere)?

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
            } else if let declined {
                Text("\(declined.said). Nothing has left this \(Words.device).").foregroundStyle(.secondary)
            } else {
                // Once the person has agreed to ask their own model, it is that model thinking.
                ProgressView(OnDevice.available && answered == nil ? "Thinking on this \(Words.device)…" : "Asking…")
            }
            if let refused { Text(refused).font(.callout).foregroundStyle(.red) }
            if declined == nil { Text(answered ?? whence).font(.caption).foregroundStyle(.secondary) }
            HStack {
                Spacer()
                if let declined {
                    Button("Cancel") { dismiss() }
                        .keyboardShortcut(.cancelAction)
                    Button(declined.elsewhere.offered) { agree(declined.elsewhere) }
                        .keyboardShortcut(.defaultAction)
                } else if let take, let answer {
                    Button("Leave it") { dismiss() }
                        .keyboardShortcut(.cancelAction)
                    Button("Use it") {
                        do {
                            make(try take(answer))
                            dismiss()
                        } catch {
                            refused = "Not changed. \(Why.said(error))"
                        }
                    }
                    .keyboardShortcut(.defaultAction)
                    // An answer with no words left in it is nothing to take, as no answer is.
                    .disabled(answer.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
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
                switch try await ask() {
                case .answered(let text): offer(text)
                case .declined(let why, let elsewhere?): declined = (why.said, elsewhere)
                case .declined(let why, nil): failure = "\(why.said)."
                }
            } catch {
                failure = Why.said(error)
            }
        }
    }

    /// The user's own model asked, the person having agreed to send the words to it.
    private func agree(_ elsewhere: Answering.Elsewhere) {
        declined = nil
        answered = elsewhere.whence
        Task {
            do {
                offer(try await elsewhere.agreed())
            } catch {
                failure = Why.said(error)
            }
        }
    }

    /// `text` offered to take or leave. Where no answer came, `failure` says why, in a person's
    /// words (PLAN 3.18): the key not accepted, the provider busy, the model on the device unable
    /// to answer.
    private func offer(_ text: String) {
        let said = shown(text)
        // No answer, as a model that refuses or runs out of room gives: nothing to take.
        if said.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            failure = "No answer from the model: it wrote nothing. Ask again, or try another model."
        } else {
            answer = said
        }
    }
}
