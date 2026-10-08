import ScaenaKit
import SwiftUI

/// The inspector (PLAN 3.4, as the browser's, PLAN 2.33 and 2.36): the state's layers, to select
/// a node, then what the theme offers for the node selected, or for the state with none. Each
/// choice is one patch, a `choose` written where the value lives or a `set_state`, as in the
/// browser.
struct Inspector: View {
    let editor: DeckEditor
    let state: String?
    @Binding var node: String?
    let make: ([JSONValue]) -> Void
    @State private var layers: [Layer] = []
    @State private var choices: Choices?

    var body: some View {
        VStack(spacing: 0) {
            if let state {
                LayerList(layers: layers, node: $node)
                    .frame(minHeight: 90, idealHeight: 150)
                Divider()
                Form {
                    Section {
                        ForEach(choices?.fields ?? []) { field in
                            FieldRow(field: field) { value in choose(field.prop, value, in: state) }
                                .help(lives(field))
                        }
                    } header: {
                        Text(choices?.node ?? "State \(state)")
                    }
                }
                .formStyle(.grouped)
            } else {
                ContentUnavailableView("No state", systemImage: "slider.horizontal.3")
            }
        }
        .task(id: "\(state ?? "")\u{1f}\(node ?? "")\u{1f}\(editor.revision)") { reload() }
    }

    private func reload() {
        guard let state else {
            layers = []
            choices = nil
            return
        }
        layers = (try? editor.session.layers(state: state)) ?? []
        if let node, let offered = try? editor.session.choices(state: state, node: node) {
            choices = offered
        } else {
            choices = try? editor.session.stateChoices(state: state)
        }
    }

    /// Choose `value` for `prop`: the node selected's, or the state's with none. Null takes it
    /// away where it lives, and the theme's shows.
    private func choose(_ prop: String, _ value: JSONValue, in state: String) {
        let op: JSONValue
        if let node = choices?.node {
            op = ["op": "choose", "node": .string(node), "prop": .string(prop), "value": value, "state": .string(state)]
        } else {
            op = ["op": "set_state", "id": .string(state), "prop": .string(prop), "value": value]
        }
        make([op])
    }

    /// Where a field's value lives, which is where a choice is written.
    private func lives(_ field: Field) -> String {
        switch field.lives {
        case .overrides: "An override, in every state"
        case .state(let id): "Set in state \(id)"
        case .node: "Set on the node, in every state"
        case nil: "The theme's"
        }
    }
}

/// A state's layers, the topmost first, each with what it holds, to select from.
private struct LayerList: View {
    let layers: [Layer]
    @Binding var node: String?

    private struct Row {
        let layer: Layer
        let depth: Int
    }

    private var rows: [Row] {
        var rows: [Row] = []
        func add(_ layers: [Layer], _ depth: Int) {
            for layer in layers {
                rows.append(Row(layer: layer, depth: depth))
                add(layer.children, depth + 1)
            }
        }
        add(layers, 0)
        return rows
    }

    var body: some View {
        List(selection: $node) {
            ForEach(rows, id: \.layer.node) { row in
                HStack(spacing: 6) {
                    Image(systemName: symbol(row.layer.type)).frame(width: 16)
                    Text(row.layer.node).foregroundStyle(row.layer.shown ? Color.primary : Color.secondary)
                    if row.layer.locked {
                        Image(systemName: "lock.fill").font(.caption).foregroundStyle(.secondary)
                    }
                }
                .padding(.leading, CGFloat(row.depth) * 12)
            }
        }
    }

    private func symbol(_ type: String) -> String {
        switch type {
        case "text": "textformat"
        case "image": "photo"
        case "chart": "chart.bar"
        case "table": "tablecells"
        case "shader": "sparkles"
        case "shape": "square.on.circle"
        default: "square.stack"
        }
    }
}

/// One field of the inspector: a control for what it takes, showing the value the state shows.
private struct FieldRow: View {
    let field: Field
    let choose: (JSONValue) -> Void

    private var label: String { field.prop.replacingOccurrences(of: "/", with: " ") }

    var body: some View {
        switch field.takes {
        case .name(_, let names, _):
            PickRow(label: label, value: field.value, options: names, choose: choose)
        case .word(let words):
            PickRow(label: label, value: field.value, options: words, choose: choose)
        case .number(_, _, _, let whole):
            NumberRow(label: label, value: field.value?.number, whole: whole, choose: choose)
        case .flag:
            Toggle(label, isOn: Binding(get: { field.value?.bool ?? false }, set: { choose(.bool($0)) }))
        case .text:
            TextRow(label: label, value: field.value?.string ?? "", choose: choose)
        case .fractions(let names):
            // Picked on the canvas (PLAN 3.7); shown here.
            LabeledContent(label) { Text(fractions(names)).monospacedDigit() }
        case .other:
            LabeledContent(label) { Text(field.value.map(written) ?? "theme's") }
        }
    }

    private func fractions(_ names: [String]) -> String {
        guard let parts = field.value?.array else { return "theme's" }
        return zip(names, parts).map { name, part in "\(name) \(part.number.map { String(format: "%.3f", $0) } ?? "?")" }
            .joined(separator: "  ")
    }
}

/// A value as JSON.
private func written(_ value: JSONValue) -> String {
    guard let data = try? JSONEncoder().encode(value) else { return "?" }
    return String(decoding: data, as: UTF8.self)
}

/// One of the theme's names, or one of the words the schema allows; "Theme's" takes the value
/// away, and a value written out shows as one.
private struct PickRow: View {
    let label: String
    let value: JSONValue?
    let options: [String]
    let choose: (JSONValue) -> Void

    var body: some View {
        let current = value?.string ?? value.map(written) ?? ""
        Picker(label, selection: Binding(get: { current }, set: { picked in pick(picked, over: current) })) {
            Text("Theme's").tag("")
            if !current.isEmpty, !options.contains(current) {
                Text("\(current) (written out)").tag(current)
            }
            ForEach(options, id: \.self) { option in
                Text(option).tag(option)
            }
        }
    }

    private func pick(_ picked: String, over current: String) {
        guard picked != current else { return }
        choose(picked.isEmpty ? .null : .string(picked))
    }
}

/// A number, typed: chosen when typed and entered, or when the field is left.
private struct NumberRow: View {
    let label: String
    let value: Double?
    let whole: Bool
    let choose: (JSONValue) -> Void
    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        TextField(label, text: $text, prompt: Text("theme's"))
            .focused($focused)
            .onSubmit(commit)
            .onChange(of: focused) { _, now in
                if !now { commit() }
            }
            .onChange(of: value, initial: true) { _, now in
                text = written(now)
            }
    }

    private func commit() {
        let typed = text.trimmingCharacters(in: .whitespaces)
        if typed.isEmpty {
            if value != nil { choose(.null) }
        } else if let number = Double(typed) {
            let chosen = whole ? number.rounded() : number
            if chosen != value { choose(.number(chosen)) }
        } else {
            text = written(value)
        }
    }

    private func written(_ value: Double?) -> String {
        guard let value else { return "" }
        if value.rounded() == value, abs(value) < 1e12 { return String(Int64(value)) }
        return String(value)
    }
}

/// Words for people, as they are written: chosen when entered, or when the field is left.
private struct TextRow: View {
    let label: String
    let value: String
    let choose: (JSONValue) -> Void
    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        TextField(label, text: $text, axis: .vertical)
            .lineLimit(1...6)
            .focused($focused)
            .onSubmit(commit)
            .onChange(of: focused) { _, now in
                if !now { commit() }
            }
            .onChange(of: value, initial: true) { _, now in
                text = now
            }
    }

    private func commit() {
        guard text != value else { return }
        choose(text.isEmpty ? .null : .string(text))
    }
}
