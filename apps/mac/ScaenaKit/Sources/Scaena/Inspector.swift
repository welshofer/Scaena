import ScaenaKit
import SwiftUI

/// The inspector (PLAN 3.4, 3.18), as a presentation app's: what the theme offers for the object
/// selected, or for the slide with none, in a person's words (`Words`), under the Format tab or the
/// Animate tab. Format shows an object's look in sections by what it is, then where it stands and
/// how it is turned (Arrange), its description, and the rest under More; with nothing selected, the
/// slide's layout and its presenter notes, and the objects on it. Animate shows an object's builds,
/// or the slide's transition and how it advances. Each choice is one patch, a `choose` written
/// where the value lives or a `set_state`, as in the browser (PLAN 2.33, 2.36); a position or a
/// size typed is the `place` a drag there makes. With characters selected in a text typed in, it
/// offers their look (PLAN 2.38, 3.10), each chosen one `style_text`. Where the Mac has the
/// on-device model (PLAN 3.6), it drafts the slide's notes and tightens a text's words, each
/// offered to take as one patch, or to leave.
struct Inspector: View {
    let editor: DeckEditor
    let state: String?
    @Binding var node: String?
    /// The others selected with it (PLAN 3.13): with any, what they all share, and how to arrange
    /// them.
    @Binding var also: [String]
    /// Text typed in on the canvas: its characters selected are what it offers a look for.
    let typing: Typing?
    /// Format or Animate: which part of what is offered it shows.
    let tab: InspectorTab
    /// Ask the on-device model, its answer offered.
    let offer: (Asked) -> Void
    let make: ([JSONValue]) -> Void
    @State private var layers: [Layer] = []
    @State private var choices: Choices?
    /// What every node selected offers, where several are.
    @State private var shared: [Field] = []
    /// What the theme says of its names: swatches, families, seconds, sizes.
    @State private var look: ThemeLook?
    /// Each node's words, as a reader hears them: what names it.
    @State private var words: [String: String] = [:]
    /// The box the node selected is placed by, canvas units, where a position typed can move it.
    @State private var placed: CGRect?
    @State private var showsObjects = false
    @State private var showsMore = false

    var body: some View {
        Group {
            if let state {
                if !also.isEmpty {
                    several(state)
                } else {
                    one(state)
                }
            } else {
                ContentUnavailableView("No Slide", systemImage: "rectangle.on.rectangle")
            }
        }
        .task(id: "\(state ?? "")\u{1f}\(node ?? "")\u{1f}\(also.joined(separator: ","))\u{1f}\(editor.revision)\u{1f}\(selecting)") {
            reload()
        }
    }

    /// The object selected, or the slide with none: what the theme offers for it, under this tab.
    private func one(_ state: String) -> some View {
        Form {
            ForEach(shelves) { shelf in
                if shelf.part == .more {
                    Section {
                        DisclosureGroup("More", isExpanded: $showsMore) {
                            ForEach(shelf.fields) { field in row(field, in: state) }
                        }
                    }
                } else {
                    Section(shelf.title) {
                        ForEach(shelf.fields) { field in row(field, in: state) }
                        if shelf.part == .arrange, let placed, let node = choices?.node {
                            PlaceRows(box: placed) { to in place(node, to: to, in: state) }
                        }
                    }
                }
            }
            if tab == .format, choices?.node == nil, characters == nil {
                objects
            }
            if OnDevice.available, tab == .format, characters == nil {
                Section {
                    if let node = choices?.node {
                        if choices?.type == "text" {
                            Button("Tighten the Wording", systemImage: "sparkles") { tighten(node, in: state) }
                        }
                    } else {
                        Button("Draft Presenter Notes", systemImage: "sparkles") { draft(state) }
                    }
                } footer: {
                    Text("Runs on this Mac").font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
    }

    /// One part of what is offered: a section's title, and its fields.
    private struct Shelf: Identifiable {
        let part: Part
        let title: String
        let fields: [Field]

        var id: Int { part.rawValue }
    }

    /// Where a field shows, in the order the sections stand.
    private enum Part: Int, CaseIterable {
        case main, fitting, arrange, description, notes, transition, advance, builds, more
    }

    /// The fields offered under this tab, in sections.
    private var shelves: [Shelf] {
        guard let choices else { return [] }
        let type = choices.type
        let isNode = choices.node != nil
        var parts: [Part: [Field]] = [:]
        for field in choices.fields {
            let (on, part) = characters != nil ? (InspectorTab.format, Part.main) : Self.part(field.prop, type: type, node: isNode)
            if on == tab { parts[part, default: []].append(field) }
        }
        return Part.allCases.compactMap { part in
            guard let fields = parts[part], !fields.isEmpty else { return nil }
            return Shelf(part: part, title: title(part, type: type, node: isNode), fields: fields)
        }
    }

    /// The tab and the section a property shows under.
    private static func part(_ prop: String, type: String?, node: Bool) -> (InspectorTab, Part) {
        guard node else {
            switch prop {
            case "layout": return (.format, .main)
            case "notes": return (.format, .notes)
            case "hold": return (.animate, .advance)
            default: return prop.hasPrefix("transition/") ? (.animate, .transition) : (.format, .more)
            }
        }
        switch prop {
        case "enter", "exit": return (.animate, .builds)
        case "alt": return (.format, .description)
        case "opacity", "transform/rotate": return (.format, .arrange)
        case "wrap", "maxLines": return (.format, .fitting)
        case "fit": return (.format, type == "text" ? .fitting : .main)
        case "semantic", "x/type", "y/type", "sizeEncoding/field", "key", "labels/role", "focal", "crop":
            return (.format, .more)
        default: return (.format, .main)
        }
    }

    private func title(_ part: Part, type: String?, node: Bool) -> String {
        switch part {
        case .main: characters != nil ? "Selected Text" : node ? Words.kind(type) : "Slide"
        case .fitting: "Fitting"
        case .arrange: "Arrange"
        case .description: "Description"
        case .notes: "Presenter Notes"
        case .transition: "Transition"
        case .advance: "Advance"
        case .builds: "Builds"
        case .more: "More"
        }
    }

    /// A field's control, for what it takes, showing the value the slide shows.
    @ViewBuilder private func row(_ field: Field, in state: String) -> some View {
        let label = Words.label(field.prop)
        let choose = { (value: JSONValue) in self.choose(field.prop, value, in: state) }
        Group {
            switch field.prop {
            case "hold":
                AdvanceRow(value: field.value?.number, choose: choose)
            case "opacity":
                OpacityRow(value: field.value?.number, choose: choose)
            case "style/weight":
                WeightRow(value: field.value?.number, choose: choose)
            case "transform/rotate":
                NumberRow(label: label, value: field.value?.number, whole: false, unit: "°", choose: choose)
            case "align/x":
                AlignmentRow(value: field.value?.string, choose: choose)
            case "notes", "alt":
                TextRow(label: label, value: field.value?.string ?? "", long: true, choose: choose)
            default:
                switch field.takes {
                case .name(let of, let names, let overrides) where of == "color":
                    SwatchRow(label: label, value: field.value, names: names, look: look, any: overrides, choose: choose)
                case .name(let of, let names, _):
                    PickRow(
                        label: label, value: field.value,
                        options: names.map { Option(tag: $0, title: Words.name($0, of: of, theme: look)) }, choose: choose)
                case .word(let words):
                    PickRow(
                        label: label, value: field.value,
                        options: words.map { Option(tag: $0, title: Words.word($0, of: field.prop, type: choices?.type)) },
                        segmented: words.count <= 2, choose: choose)
                case .number(_, _, _, let whole):
                    NumberRow(label: label, value: field.value?.number, whole: whole, unit: nil, choose: choose)
                case .flag:
                    Toggle(label, isOn: Binding(get: { field.value?.bool ?? false }, set: { choose(.bool($0)) }))
                case .text:
                    TextRow(label: label, value: field.value?.string ?? "", long: false, choose: choose)
                case .fractions(let names):
                    // Picked on the canvas (PLAN 3.16); shown here.
                    LabeledContent(label) { Text(fractions(field.value, names)).monospacedDigit() }
                case .other:
                    LabeledContent(label) { Text(field.value.map(written) ?? "Default") }
                }
            }
        }
        .help(lives(field))
    }

    /// The objects on the slide, the topmost first, each named by what it shows: a click selects
    /// one, and the lock beside it locks it, or unlocks it (PLAN 2.95).
    private var objects: some View {
        Section {
            DisclosureGroup("Objects on This Slide", isExpanded: $showsObjects) {
                ForEach(rows, id: \.layer.node) { row in
                    ObjectRow(
                        layer: row.layer, name: Words.node(row.layer.type, words: words[row.layer.node]),
                        depth: row.depth, selected: row.layer.node == node, lock: { lock(row.layer) }
                    ) {
                        node = row.layer.node
                        also = []
                    }
                }
            }
        }
    }

    private struct Row {
        let layer: Layer
        let depth: Int
    }

    /// The layers in the order they are listed, each with how deep it is held.
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

    /// Several selected (PLAN 3.13), as the browser's inspector takes them (PLAN 2.42, 2.43): what
    /// they all share, each choice one patch of a `choose` for each; and buttons that align,
    /// distribute, order, and group them.
    private func several(_ state: String) -> some View {
        Form {
            Section("\(selected.count) Objects") {
                ForEach(shared.filter { Self.part($0.prop, type: nil, node: true).0 == tab }) { field in
                    let label = Words.label(field.prop)
                    let choose = { (value: JSONValue) in chooseAll(field.prop, value, in: state) }
                    switch field.takes {
                    case .name(let of, let names, let overrides) where of == "color":
                        SwatchRow(label: label, value: field.value, names: names, look: look, any: overrides, choose: choose)
                    case .name(let of, let names, _):
                        PickRow(
                            label: label, value: field.value,
                            options: names.map { Option(tag: $0, title: Words.name($0, of: of, theme: look)) },
                            choose: choose)
                    case .word(let words):
                        PickRow(
                            label: label, value: field.value,
                            options: words.map { Option(tag: $0, title: Words.word($0, of: field.prop, type: nil)) },
                            choose: choose)
                    case .number(_, _, _, let whole):
                        NumberRow(label: label, value: field.value?.number, whole: whole, unit: nil, choose: choose)
                    case .flag:
                        Toggle(label, isOn: Binding(get: { field.value?.bool ?? false }, set: { choose(.bool($0)) }))
                    default:
                        EmptyView()
                    }
                }
            }
            if tab == .format {
                Section("Align") {
                    HStack {
                        arrange("Left", "align.horizontal.left", .align("left"), in: state)
                        arrange("Center", "align.horizontal.center", .align("center"), in: state)
                        arrange("Right", "align.horizontal.right", .align("right"), in: state)
                        Divider()
                        arrange("Top", "align.vertical.top", .align("top"), in: state)
                        arrange("Middle", "align.vertical.center", .align("middle"), in: state)
                        arrange("Bottom", "align.vertical.bottom", .align("bottom"), in: state)
                    }
                }
                Section("Distribute") {
                    HStack {
                        arrange("Evenly Across", "distribute.horizontal.center", .spread("across"), in: state)
                        arrange("Evenly Down", "distribute.vertical.center", .spread("down"), in: state)
                    }
                }
                Section("Arrange") {
                    HStack {
                        arrange("Bring to Front", "square.3.layers.3d.top.filled", .order("front"), in: state)
                        arrange("Send to Back", "square.3.layers.3d.bottom.filled", .order("back"), in: state)
                    }
                    Button("Group", systemImage: "rectangle.3.group") { group(in: state) }
                        .help("Put them in a group, where they stand (⌘G)")
                }
            }
        }
        .formStyle(.grouped)
    }

    /// Every node selected, the first first.
    private var selected: [String] { node.map { [$0] + also } ?? [] }

    /// A button that arranges the nodes selected `how`: one patch, one step to undo.
    private func arrange(_ title: String, _ symbol: String, _ how: Arrangement, in state: String) -> some View {
        Button {
            let nodes = selected
            guard let arranged = (try? editor.session.arranging(state: state, nodes: nodes, how: how)) ?? nil,
                !arranged.patch.isEmpty
            else { return }
            make(arranged.patch)
        } label: {
            Label(title, systemImage: symbol)
        }
        .labelStyle(.iconOnly)
        .help(title)
    }

    /// The nodes selected in a new group where they stand, the group selected (PLAN 2.43).
    private func group(in state: String) {
        guard let grouping = try? editor.session.grouping(state: state, nodes: selected) else { return }
        make(grouping.patch)
        node = grouping.id
        also = []
    }

    /// Choose `value` for `prop` on every node selected: one patch.
    private func chooseAll(_ prop: String, _ value: JSONValue, in state: String) {
        make(selected.map { ["op": "choose", "node": .string($0), "prop": .string(prop), "value": value, "state": .string(state)] })
    }

    /// `node` placed by the box `to`, canvas units, where it was left: the `place` a drag there
    /// makes, off the theme's grid (PLAN 3.7). One patch, one step to undo.
    private func place(_ node: String, to: CGRect, in state: String) {
        guard let snapped = (try? editor.session.snap(state: state, node: node, how: .free, to: to)) ?? nil,
            !snapped.patch.isEmpty
        else { return }
        make(snapped.patch)
    }

    /// The characters selected in the text typed in, as `style_text` counts them, where it is in
    /// the state shown: what the inspector offers a look for.
    private var characters: (node: String, from: Int, to: Int)? {
        guard let typing, typing.state == state, let node = typing.node, let carets = typing.carets,
            typing.from < typing.to
        else { return nil }
        return (node, carets.scalars(typing.from), carets.scalars(typing.to))
    }

    /// The characters selected, as what reads the inspector again when they change.
    private var selecting: String {
        characters.map { "\($0.node):\($0.from)-\($0.to)" } ?? ""
    }

    private func reload() {
        let session = editor.session
        look = ThemeLook(try? session.themeText())
        guard let state else {
            layers = []
            choices = nil
            placed = nil
            return
        }
        layers = (try? session.layers(state: state)) ?? []
        words = [:]
        for part in (try? session.reads(state: state)) ?? [] where !part.text.isEmpty {
            words[part.node] = part.text
        }
        placed = nil
        if !also.isEmpty {
            shared = Field.shared(selected.compactMap { try? session.choices(state: state, node: $0) })
            choices = nil
        } else if characters != nil, let offered = typing?.characterChoices() {
            choices = offered
        } else if let node, let offered = try? session.choices(state: state, node: node) {
            choices = offered
            // A position typed moves what a drag moves: a root's box on the theme's grid, or a
            // frame's child's; a stack's or a grid's child goes by its order or its cell.
            if let targets = try? session.targets(state: state, node: node), targets.by == "grid" || targets.by == "frame" {
                placed = targets.cell
            }
        } else {
            choices = try? session.stateChoices(state: state)
        }
    }

    /// Choose `value` for `prop`: the node selected's, or the state's with none. Null takes it
    /// away where it lives, and the theme's shows.
    private func choose(_ prop: String, _ value: JSONValue, in state: String) {
        // The characters selected take it as a look of their own: one `style_text`.
        if characters != nil, let typing {
            typing.give(.object([prop: value]))
            return
        }
        let op: JSONValue
        if let node = choices?.node {
            op = ["op": "choose", "node": .string(node), "prop": .string(prop), "value": value, "state": .string(state)]
        } else {
            op = ["op": "set_state", "id": .string(state), "prop": .string(prop), "value": value]
        }
        make([op])
    }

    /// Lock `layer`'s node by its own lock, or unlock it (PLAN 2.95, 3.11): the canvas passes over
    /// a node locked, in every state. One patch, one step to undo.
    private func lock(_ layer: Layer) {
        let (ops, _) = ScaenaKit.locking([layer.node], own: { _ in layer.locked })
        if !ops.isEmpty { make(ops) }
    }

    /// `node`'s words tightened on this Mac, offered as one `replace_text` of them all.
    private func tighten(_ node: String, in state: String) {
        guard let words = (try? editor.session.text(state: state, node: node)) ?? nil, !words.isEmpty else { return }
        let count = Double(words.unicodeScalars.count)
        offer(
            Asked(
                title: "Tightened", ask: { try await OnDevice.tighten(words) },
                take: { tightened in
                    [
                        [
                            "op": "replace_text", "state": .string(state), "node": .string(node), "from": 0,
                            "to": .number(count), "text": .string(tightened),
                        ]
                    ]
                }))
    }

    /// `state`'s notes drafted on this Mac from how it reads, offered as one `set_state`.
    private func draft(_ state: String) {
        guard let reading = try? editor.session.reading(state: state) else { return }
        offer(
            Asked(
                title: "Presenter Notes", ask: { try await OnDevice.notes(state: state, reading: reading) },
                take: { notes in
                    [["op": "set_state", "id": .string(state), "prop": "notes", "value": .string(notes)]]
                }))
    }

    /// Where a field's value lives, which is where a choice is written, as a person says it.
    private func lives(_ field: Field) -> String {
        switch field.lives {
        case .overrides: "Set on this object, on every slide"
        case .state: "Set on this slide"
        case .node: "Set on this object"
        case nil: characters == nil ? "The theme's" : "The text's own: these characters have no look of their own"
        }
    }

    private func fractions(_ value: JSONValue?, _ names: [String]) -> String {
        guard let parts = value?.array else { return "Default" }
        return zip(names, parts).map { name, part in "\(name) \(part.number.map { String(format: "%.2f", $0) } ?? "?")" }
            .joined(separator: "  ")
    }
}

/// One object on the slide in the list of them: what it is, its name, and its lock.
private struct ObjectRow: View {
    let layer: Layer
    let name: String
    let depth: Int
    let selected: Bool
    let lock: () -> Void
    let select: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            Button(action: select) {
                HStack(spacing: 6) {
                    Image(systemName: Self.symbol(layer.type)).frame(width: 16).foregroundStyle(.secondary)
                    Text(name)
                        .lineLimit(1)
                        .foregroundStyle(layer.shown ? Color.primary : Color.secondary)
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            Button(action: lock) {
                Image(systemName: layer.locked ? "lock.fill" : "lock.open")
                    .font(.caption)
                    .foregroundStyle(layer.locked ? Color.primary : Color.secondary.opacity(0.5))
            }
            .buttonStyle(.borderless)
            .help(layer.locked ? "Unlock it" : "Lock it, so a click or a drag passes over it")
        }
        .padding(.leading, CGFloat(depth) * 12)
        .padding(.vertical, 1)
        .background(selected ? Color.accentColor.opacity(0.15) : Color.clear, in: RoundedRectangle(cornerRadius: 4))
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    static func symbol(_ type: String) -> String {
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

/// A value as JSON.
private func written(_ value: JSONValue) -> String {
    guard let data = try? JSONEncoder().encode(value) else { return "?" }
    return String(decoding: data, as: UTF8.self)
}

/// One choice a picker offers: the value it writes, and what it says.
private struct Option: Identifiable {
    let tag: String
    let title: String

    var id: String { tag }
}

/// One of the theme's names, or one of the words a property takes; Default takes the value away,
/// and a value written out shows as Custom.
private struct PickRow: View {
    let label: String
    let value: JSONValue?
    let options: [Option]
    var segmented = false
    let choose: (JSONValue) -> Void

    var body: some View {
        let current = value?.string ?? value.map(written) ?? ""
        let picker = Picker(label, selection: Binding(get: { current }, set: { picked in pick(picked, over: current) })) {
            Text("Default").tag("")
            if !current.isEmpty, !options.contains(where: { $0.tag == current }) {
                Text("Custom").tag(current)
            }
            ForEach(options) { option in
                Text(option.title).tag(option.tag)
            }
        }
        if segmented {
            picker.pickerStyle(.segmented)
        } else {
            picker
        }
    }

    private func pick(_ picked: String, over current: String) {
        guard picked != current else { return }
        choose(picked.isEmpty ? .null : .string(picked))
    }
}

/// A color: the theme's, as swatches, a role's first and then each color no role names; Default
/// takes the value away; and, where the property takes a color written out, any other, which
/// goes in the deck's overrides once the picker rests.
private struct SwatchRow: View {
    let label: String
    let value: JSONValue?
    let names: [String]
    let look: ThemeLook?
    /// Whether a color written out may be chosen too.
    let any: Bool
    let choose: (JSONValue) -> Void
    @State private var settling: Task<Void, Never>?

    private var current: String? { value?.string }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(label)
                Spacer()
                Text(said(current)).foregroundStyle(.secondary)
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 26, maximum: 30), spacing: 6)], alignment: .leading, spacing: 6) {
                swatch(nil)
                ForEach(shown, id: \.self) { name in swatch(name) }
                if any {
                    ColorPicker("Other Color", selection: other, supportsOpacity: true)
                        .labelsHidden()
                        .help("Any other color, written out on this object")
                }
            }
        }
        .padding(.vertical, 2)
    }

    /// What the swatches offer: each name the theme gives a role, then each color no role names.
    private var shown: [String] {
        guard let look else { return names }
        let roles = look.roles
        let named = Set(roles.values)
        return names.filter { roles[$0] != nil } + names.filter { roles[$0] == nil && !named.contains($0) }
    }

    private func said(_ name: String?) -> String {
        guard let name else { return "Default" }
        return names.contains(name) ? Words.name(name, of: "color", theme: look) : "Custom"
    }

    private func swatch(_ name: String?) -> some View {
        let picked = current == name
        return Button {
            if !picked { choose(name.map { .string($0) } ?? .null) }
        } label: {
            ZStack {
                if let name, let color = look?.color(name) {
                    Circle().fill(color)
                    Circle().strokeBorder(.separator)
                } else {
                    Circle().strokeBorder(.secondary, style: StrokeStyle(lineWidth: 1.5, dash: [3, 2]))
                }
            }
            .frame(width: 20, height: 20)
            .padding(3)
            .overlay {
                if picked { Circle().strokeBorder(Color.accentColor, lineWidth: 2) }
            }
        }
        .buttonStyle(.plain)
        .help(said(name))
        .accessibilityLabel(said(name))
        .accessibilityAddTraits(picked ? .isSelected : [])
    }

    /// Any color: the value written out, where it is one; a color picked, once the picker rests.
    private var other: Binding<Color> {
        Binding(
            get: {
                guard let current else { return .clear }
                return look?.color(current) ?? ThemeLook.hex(current) ?? .clear
            },
            set: { picked in
                guard let hex = ThemeLook.hex(picked), hex.lowercased() != current?.lowercased() else { return }
                settling?.cancel()
                settling = Task { @MainActor in
                    try? await Task.sleep(for: .milliseconds(400))
                    guard !Task.isCancelled else { return }
                    choose(.string(hex))
                }
            })
    }
}

/// A font's weight by its name, Regular to Black; a weight between them shows as its number.
private struct WeightRow: View {
    let value: Double?
    let choose: (JSONValue) -> Void

    private struct Weight: Identifiable {
        let value: Int
        let name: String

        var id: Int { value }
    }

    private static let weights = [
        Weight(value: 100, name: "Thin"), Weight(value: 200, name: "Extra Light"), Weight(value: 300, name: "Light"),
        Weight(value: 400, name: "Regular"), Weight(value: 500, name: "Medium"), Weight(value: 600, name: "Semibold"),
        Weight(value: 700, name: "Bold"), Weight(value: 800, name: "Extra Bold"), Weight(value: 900, name: "Black"),
    ]

    var body: some View {
        let current = value.map { Int($0.rounded()) } ?? 0
        Picker("Weight", selection: Binding(get: { current }, set: { picked in pick(picked, over: current) })) {
            Text("Default").tag(0)
            if current != 0, !Self.weights.contains(where: { $0.value == current }) {
                Text("\(current)").tag(current)
            }
            ForEach(Self.weights) { weight in
                Text(weight.name).tag(weight.value)
            }
        }
    }

    private func pick(_ picked: Int, over current: Int) {
        guard picked != current else { return }
        choose(picked == 0 ? .null : .number(Double(picked)))
    }
}

/// How opaque, as a slider, in percent: chosen once the slider is let go.
private struct OpacityRow: View {
    let value: Double?
    let choose: (JSONValue) -> Void
    @State private var shown = 1.0

    var body: some View {
        LabeledContent("Opacity") {
            HStack {
                Slider(value: $shown, in: 0...1, onEditingChanged: { editing in
                    if !editing { commit() }
                })
                Text(verbatim: "\(Int((shown * 100).rounded()))%")
                    .monospacedDigit()
                    .frame(width: 44, alignment: .trailing)
            }
        }
        .onChange(of: value, initial: true) { _, now in shown = now ?? 1 }
    }

    private func commit() {
        let chosen = (shown * 100).rounded() / 100
        guard chosen != (value ?? 1) else { return }
        choose(.number(chosen))
    }
}

/// Where a text's lines stand across its box (PLAN 3.25), as a presentation app's alignment
/// buttons: left, centered, or right. Where the text sets none, the slot's shows and none is
/// picked.
private struct AlignmentRow: View {
    let value: String?
    let choose: (JSONValue) -> Void

    var body: some View {
        LabeledContent("Alignment") {
            Picker("Alignment", selection: Binding(get: { value }, set: { picked in
                if let picked, picked != value { choose(.string(picked)) }
            })) {
                ForEach(Self.ways, id: \.word) { way in
                    Image(systemName: way.symbol)
                        .accessibilityLabel(Words.word(way.word, of: "align/x", type: "text"))
                        .tag(String?.some(way.word))
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .fixedSize()
        }
    }

    /// Each way, by the word the deck writes, and its symbol.
    private static let ways: [(word: String, symbol: String)] = [
        ("start", "text.alignleft"), ("center", "text.aligncenter"), ("end", "text.alignright"),
    ]
}

/// How the slide goes on to the next: on a click, or by itself after some seconds (its `hold`).
private struct AdvanceRow: View {
    let value: Double?
    let choose: (JSONValue) -> Void
    @State private var seconds = ""

    var body: some View {
        Picker("Advance", selection: Binding(get: { value != nil }, set: { by in advance(by) })) {
            Text("On Click").tag(false)
            Text("Automatically").tag(true)
        }
        if let value {
            LabeledContent("After") {
                HStack(spacing: 4) {
                    TextField("Seconds", text: $seconds)
                        .multilineTextAlignment(.trailing)
                        .frame(width: 64)
                        .onSubmit(commit)
                    Text("s").foregroundStyle(.secondary)
                }
            }
            .onChange(of: value, initial: true) { _, now in seconds = Words.number(now / 1000) }
        }
    }

    /// By itself after three seconds, where it went on a click; on a click, where it went by itself.
    private func advance(_ itself: Bool) {
        if itself, value == nil {
            choose(.number(3000))
        } else if !itself, value != nil {
            choose(.null)
        }
    }

    private func commit() {
        guard let typed = Double(seconds.trimmingCharacters(in: .whitespaces)), typed >= 0 else {
            seconds = value.map { Words.number($0 / 1000) } ?? ""
            return
        }
        let ms = (typed * 1000).rounded()
        if ms != value { choose(.number(ms)) }
    }
}

/// Where an object stands and how big it is, canvas units, each typed: the box moved there is the
/// `place` a drag makes.
private struct PlaceRows: View {
    let box: CGRect
    let place: (CGRect) -> Void

    var body: some View {
        LabeledContent("Position") {
            HStack(spacing: 6) {
                Coordinate(label: "X", value: box.minX) { x in place(CGRect(x: x, y: box.minY, width: box.width, height: box.height)) }
                Coordinate(label: "Y", value: box.minY) { y in place(CGRect(x: box.minX, y: y, width: box.width, height: box.height)) }
            }
        }
        LabeledContent("Size") {
            HStack(spacing: 6) {
                Coordinate(label: "W", value: box.width) { w in
                    if w >= 1 { place(CGRect(x: box.minX, y: box.minY, width: w, height: box.height)) }
                }
                Coordinate(label: "H", value: box.height) { h in
                    if h >= 1 { place(CGRect(x: box.minX, y: box.minY, width: box.width, height: h)) }
                }
            }
        }
    }
}

/// One number of a box, typed: set on Return, or when the field is left.
private struct Coordinate: View {
    let label: String
    let value: CGFloat
    let set: (CGFloat) -> Void
    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 2) {
            TextField(label, text: $text)
                .multilineTextAlignment(.trailing)
                .frame(width: 56)
                .focused($focused)
                .onSubmit(commit)
                .onChange(of: focused) { _, now in
                    if !now { commit() }
                }
            Text(label).font(.caption).foregroundStyle(.secondary)
        }
        .onChange(of: value, initial: true) { _, now in text = Words.number(Double(now)) }
    }

    private func commit() {
        guard let typed = Double(text.trimmingCharacters(in: .whitespaces)) else {
            text = Words.number(Double(value))
            return
        }
        let rounded = typed.rounded()
        if abs(rounded - Double(value)) >= 0.5 { set(CGFloat(rounded)) }
    }
}

/// A number, typed: chosen when typed and entered, or when the field is left; empty is Default.
private struct NumberRow: View {
    let label: String
    let value: Double?
    let whole: Bool
    let unit: String?
    let choose: (JSONValue) -> Void
    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        LabeledContent(label) {
            HStack(spacing: 4) {
                TextField(label, text: $text, prompt: Text("Default"))
                    .labelsHidden()
                    .multilineTextAlignment(.trailing)
                    .frame(minWidth: 60, maxWidth: 90)
                    .focused($focused)
                    .onSubmit(commit)
                    .onChange(of: focused) { _, now in
                        if !now { commit() }
                    }
                if let unit { Text(unit).foregroundStyle(.secondary) }
            }
        }
        .onChange(of: value, initial: true) { _, now in
            text = now.map(Words.number) ?? ""
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
            text = value.map(Words.number) ?? ""
        }
    }
}

/// Words for people, as they are written: chosen when entered, or when the field is left.
private struct TextRow: View {
    let label: String
    let value: String
    /// Several lines, shown under the label: notes, a description.
    let long: Bool
    let choose: (JSONValue) -> Void
    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        Group {
            if long {
                TextField(label, text: $text, prompt: Text(label), axis: .vertical)
                    .labelsHidden()
                    .lineLimit(3...12)
            } else {
                TextField(label, text: $text, axis: .vertical)
                    .lineLimit(1...6)
            }
        }
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
