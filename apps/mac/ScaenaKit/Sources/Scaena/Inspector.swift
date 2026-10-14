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
/// offers their look (PLAN 2.38, 3.10), each chosen one `style_text`. Its writing tools (PLAN
/// 3.6, 2.101) proofread, tighten, or rewrite a text's words and draft the slide's notes, on the
/// device where it has a model, else by the assistant's with the user's key, each offered to take
/// as one patch, or to leave.
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
    /// Ask a model, its answer offered.
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
    /// Each node's kind, as the engine names it: a shape of the library by its shape (PLAN 2.106).
    @State private var kinds: [String: String] = [:]
    /// The box the node selected is placed by, canvas units, where a position typed can move it.
    @State private var placed: CGRect?
    @State private var showsObjects = false
    @State private var showsMore = false
    /// Whether a text's fitting (how it fits its box, its line breaks, its most lines) is shown:
    /// folded until asked for, as the inspector's More is, so a text's look comes first.
    @State private var showsFitting = false

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
                if shelf.part == .more || shelf.part == .fitting {
                    Section {
                        DisclosureGroup(shelf.title, isExpanded: shelf.part == .more ? $showsMore : $showsFitting) {
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
                if let furniture = choices?.furniture, furniture.placed {
                    FurnitureSection(furniture: furniture, make: make)
                }
            }
            // A picture is described by the assistant's model alone, which is shown it (PLAN 2.119).
            // Writing is offered for a text, a picture, or the slide, as the browser offers it: a
            // shape, a chart, a table, or a group has no words to write.
            let picture = choices?.type == "image"
            let writes = (choices?.writes ?? true) && (picture ? Writer.describes : Writer.available)
            if writes, tab == .format, characters == nil, also.isEmpty {
                Section {
                    if let node = choices?.node {
                        if choices?.type == "text" {
                            Button("Proofread", systemImage: "checkmark.circle") { write(.proofread, node: node, in: state) }
                            Button("Tighten the Wording", systemImage: "sparkles") { write(.tighten, node: node, in: state) }
                            Button("Rewrite", systemImage: "wand.and.stars") { write(.rewrite, node: node, in: state) }
                        } else if picture {
                            Button("Describe the Picture", systemImage: "text.below.photo") {
                                write(.describe, node: node, in: state)
                            }
                            .accessibilityIdentifier("describe-picture")
                        }
                    } else {
                        Button("Draft Presenter Notes", systemImage: "sparkles") { write(.notes, in: state) }
                    }
                } header: {
                    Text("Writing")
                } footer: {
                    Text(
                        OnDevice.available && !picture
                            ? "Runs on this \(Words.device)" : "By the assistant's model, with your key")
                        .font(.caption).foregroundStyle(.secondary)
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
        case main, adjust, fitting, arrange, description, notes, transition, advance, builds, more
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
        case "enter", "emphasis", "exit": return (.animate, .builds)
        case "alt": return (.format, .description)
        case "opacity", "transform/rotate": return (.format, .arrange)
        case "wrap", "maxLines": return (.format, .fitting)
        case _ where prop.hasPrefix("adjust/"): return (.format, .adjust)
        case "fit": return (.format, type == "text" ? .fitting : .main)
        case "semantic", "x/type", "y/type", "sizeEncoding/field", "key", "labels/role", "focal", "crop":
            return (.format, .more)
        default: return (.format, .main)
        }
    }

    private func title(_ part: Part, type: String?, node: Bool) -> String {
        switch part {
        case .main: characters != nil ? "Selected Text" : node ? Words.kind(type) : "Slide"
        case .adjust: "Adjust Image"
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
        let label = Words.label(field.prop, chart: chartKind)
        let choose = { (value: JSONValue) in self.choose(field.prop, value, in: state) }
        Group {
            switch field.prop {
            case "hold":
                AdvanceRow(value: field.value?.number, choose: choose)
            case "opacity":
                OpacityRow(value: field.value?.number, choose: choose)
            case "style/weight":
                WeightRow(value: field.value?.number, shows: field.theme?.number, choose: choose)
            case "transform/rotate":
                // Unturned where nothing turns it: 0°, as a presentation app shows it.
                NumberRow(
                    label: label, value: field.value?.number, whole: false, unit: "°", shows: 0, step: 1,
                    id: "inspector-\(field.prop)", choose: choose)
            case "align/x":
                AlignmentRow(value: field.value?.string, shows: field.theme?.string, choose: choose)
            case "layout" where choices?.node == nil:
                LayoutsRow(editor: editor, state: state, look: look, make: make) {
                    if case .name(_, let names, _) = field.takes {
                        LayoutField(
                            editor: editor, state: state, label: label, value: field.value?.string, names: names,
                            look: look, choose: choose)
                    }
                }
            case "notes":
                TextRow(
                    label: label, value: field.value?.string ?? "", long: true,
                    prompt: "Notes you see as you present", choose: choose)
            case "alt":
                TextRow(
                    label: label, value: field.value?.string ?? "", long: true,
                    prompt: "Describe it for someone who can’t see it", choose: choose)
            default:
                switch field.takes {
                case .name(let of, let names, let overrides) where of == "color":
                    SwatchRow(
                        label: label, value: field.value, names: names, look: look, any: overrides,
                        shows: field.theme?.string, choose: choose)
                case .name(let of, let names, _):
                    let said = { (name: String) in Option(tag: name, title: Words.name(name, of: of, theme: look)) }
                    PickRow(
                        label: label, value: field.value, options: names.map(said), shows: field.theme?.string.map(said),
                        choose: choose)
                case .word(let words):
                    let said = { (word: String) in
                        Option(tag: word, title: Words.word(word, of: field.prop, type: choices?.type))
                    }
                    PickRow(
                        label: label, value: field.value, options: words.map(said), segmented: words.count <= 2,
                        shows: (field.theme?.string ?? Self.unset[field.prop]).map(said), choose: choose)
                case .number(let least, let above, let most, let whole):
                    if field.prop.hasPrefix("adjust/") {
                        AdjustRow(
                            label: label, value: field.value?.number ?? 0,
                            awayAtNone: field.lives == nil || field.lives == .node, choose: choose)
                    } else {
                        NumberRow(
                            label: label, value: field.value?.number, whole: whole, unit: Self.unit[field.prop],
                            shows: field.theme?.number, step: Self.step[field.prop] ?? 1, least: least, above: above,
                            most: most, id: "inspector-\(field.prop)", choose: choose)
                    }
                case .flag:
                    // Where the deck sets none, the theme's: an italic role reads as italic.
                    Toggle(
                        label,
                        isOn: Binding(
                            get: { field.value?.bool ?? field.theme?.bool ?? false }, set: { choose(.bool($0)) }))
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

    /// The kind of chart selected, which says what its channels are called: a donut's `x` is its
    /// slices.
    private var chartKind: String? {
        guard choices?.type == "chart" else { return nil }
        return choices?.fields.first { $0.prop == "kind" }?.value?.string
    }

    /// What a word the deck may leave unset is where it is: a stack runs down, a chart's bars run
    /// up, and a chart puts its values where its kind does (SPEC §3.3, §3.7). The theme gives none
    /// of these, so Default is called by it.
    private static let unset: [String: String] = ["axis": "y", "orient": "vertical", "labels/show": "auto"]

    /// What a number counts, said after it: a size in points, a turn in degrees.
    private static let unit: [String: String] = ["style/size": "pt", "transform/rotate": "°"]

    /// What a stepper's click moves a number by, where it is not 1: character spacing, in ems.
    private static let step: [String: Double] = ["style/tracking": 0.01]

    /// A field the theme names the values of, picked by name.
    @ViewBuilder private func named(_ field: Field, label: String, choose: @escaping (JSONValue) -> Void) -> some View {
        switch field.takes {
        case .name(let of, let names, _):
            PickRow(
                label: label, value: field.value,
                options: names.map { Option(tag: $0, title: Words.name($0, of: of, theme: look)) }, choose: choose)
        default:
            EmptyView()
        }
    }

    /// The objects on the slide, the topmost first, each named by what it shows: a click selects
    /// one, and the lock beside it locks it, or unlocks it (PLAN 2.95).
    private var objects: some View {
        Section {
            DisclosureGroup("Objects on This Slide", isExpanded: $showsObjects) {
                ForEach(rows, id: \.layer.node) { row in
                    ObjectRow(
                        layer: row.layer,
                        name: Words.node(row.layer.type, words: words[row.layer.node], kind: kinds[row.layer.node]),
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
                    case .number(let least, let above, let most, let whole):
                        NumberRow(
                            label: label, value: field.value?.number, whole: whole, unit: Self.unit[field.prop],
                            step: Self.step[field.prop] ?? 1, least: least, above: above, most: most,
                            id: "inspector-\(field.prop)", choose: choose)
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
        kinds = (try? session.kinds(state: state)) ?? [:]
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

    /// `kind` written for `node`, a text, or for `state`'s notes (PLAN 2.101): on the device, or by the
    /// assistant's model with the user's key, offered to take as one patch or to leave.
    private func write(_ kind: WritingKind, node: String? = nil, in state: String) {
        offer(Writer.asking(WritingAsk(kind: kind, state: state, node: node), of: editor.session))
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
/// and a value written out shows as Custom. Where the theme gives a value (`shows`), Default is
/// called by it, the Fraunces a headline is set in, say, and is not offered again beside it.
private struct PickRow: View {
    let label: String
    let value: JSONValue?
    let options: [Option]
    var segmented = false
    /// What the slide shows where nothing sets it: the theme's, by its name.
    var shows: Option? = nil
    let choose: (JSONValue) -> Void

    var body: some View {
        let current = value?.string ?? value.map(written) ?? ""
        let picker = Picker(label, selection: Binding(get: { current }, set: { picked in pick(picked, over: current) })) {
            Text(shows?.title ?? "Default").tag("")
            if !current.isEmpty, !options.contains(where: { $0.tag == current }) {
                Text("Custom").tag(current)
            }
            ForEach(options.filter { $0.tag != shows?.tag || $0.tag == current }) { option in
                Text(option.title).tag(option.tag)
            }
        }
        if segmented {
            // Its label over it, as the iPad's form shows a segmented control with no label of its
            // own; and on the Mac, a row of them reads alike whether its segments are few or many.
            VStack(alignment: .leading, spacing: 6) {
                Text(label)
                picker.pickerStyle(.segmented).labelsHidden()
            }
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
/// takes the value away; and, where the property takes a color written out, any other, from the
/// well beside its name, as a presentation app's color well stands beside its swatches: it goes in
/// the deck's overrides once the picker rests.
private struct SwatchRow: View {
    let label: String
    let value: JSONValue?
    let names: [String]
    let look: ThemeLook?
    /// Whether a color written out may be chosen too.
    let any: Bool
    /// The color the slide shows where nothing sets it: the theme's, by its name. Its swatch is
    /// picked then, and picking it again takes a color set away.
    var shows: String? = nil
    let choose: (JSONValue) -> Void
    @State private var settling: Task<Void, Never>?

    private var current: String? { value?.string }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(label)
                Spacer()
                Text(said(current)).foregroundStyle(.secondary).lineLimit(1)
                if any {
                    ColorPicker("Other Color", selection: other, supportsOpacity: true)
                        .labelsHidden()
                        .help("Any other color, written out on this object")
                }
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 26, maximum: 30), spacing: 6)], alignment: .leading, spacing: 6) {
                if shows == nil { swatch(nil) }
                ForEach(shown, id: \.self) { name in swatch(name) }
            }
        }
        .padding(.vertical, 2)
    }

    /// What the swatches offer: each name the theme gives a role, then each color no role names. A
    /// color two names give (Background and Text on Accent, say) is one swatch, and so are two that
    /// look alike at a swatch's size (Dusk's Background and Background 2): the first name's, or the
    /// one picked.
    private var shown: [String] {
        guard let look else { return names }
        let roles = look.roles
        let named = Set(roles.values)
        let offered = names.filter { roles[$0] != nil } + names.filter { roles[$0] == nil && !named.contains($0) }
        let picked = current ?? shows
        var kept: [String] = []
        // Each swatch kept, by its color as the theme writes it.
        var colors: [(color: String, at: Int)] = []
        for name in offered {
            guard let color = look.written(name) else {
                kept.append(name)
                continue
            }
            if let alike = colors.first(where: { Swatch.looksAlike($0.color, color) }) {
                if name == picked { kept[alike.at] = name }
            } else {
                colors.append((color: color, at: kept.count))
                kept.append(name)
            }
        }
        return kept
    }

    private func said(_ name: String?) -> String {
        guard let name = name ?? shows else { return "Default" }
        return names.contains(name) ? Words.name(name, of: "color", theme: look) : "Custom"
    }

    private func swatch(_ name: String?) -> some View {
        // The theme's color is picked where nothing sets one, and picked again takes one away.
        let picked = current == name || (current == nil && name != nil && name == shows)
        return Button {
            if !picked { choose(name == nil || name == shows ? .null : .string(name ?? "")) }
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

    /// The color the slide shows: the one set, or the theme's where none is.
    private var inEffect: Color? {
        guard let name = current ?? shows else { return nil }
        return look?.color(name) ?? ThemeLook.hex(name)
    }

    /// Any color: the well shows the color in effect, not a well with none in it; a color picked,
    /// once the picker rests, is written out, and the one shown already changes nothing.
    private var other: Binding<Color> {
        Binding(
            get: { inEffect ?? .clear },
            set: { picked in
                guard let hex = ThemeLook.hex(picked), hex != inEffect.flatMap({ ThemeLook.hex($0) }),
                    hex.lowercased() != current?.lowercased()
                else { return }
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
    /// The weight the slide shows where nothing sets one: the theme's, which Default is called by.
    var shows: Double? = nil
    let choose: (JSONValue) -> Void

    var body: some View {
        let current = value.map { Int($0.rounded()) } ?? 0
        let theirs = shows.map { Int($0.rounded()) }
        Picker("Weight", selection: Binding(get: { current }, set: { picked in pick(picked, over: current) })) {
            Text(theirs.map(Words.weight) ?? "Default").tag(0)
            if current != 0, !Words.weights.contains(where: { $0.value == current }) {
                Text("\(current)").tag(current)
            }
            ForEach(Words.weights.filter { $0.value != theirs || $0.value == current }, id: \.value) { weight in
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
                // On one line, however wide the iPad's type sets it.
                Text(verbatim: "\(Int((shown * 100).rounded()))%")
                    .monospacedDigit()
                    .lineLimit(1)
                    .fixedSize()
                    .frame(minWidth: 44, alignment: .trailing)
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
/// buttons: left, centered, or right, the way in effect picked, as a presentation app always shows
/// one. Where the text sets none, the slot's shows, as the theme gives it (a shape's words stand
/// centered, PLAN 2.108), else its lines start at the left, as the engine sets them.
private struct AlignmentRow: View {
    let value: String?
    /// The way the slide sets its lines where the text sets none: the slot's.
    var shows: String? = nil
    let choose: (JSONValue) -> Void

    var body: some View {
        // A way the buttons do not show, `stretch`, sets a text's lines from the left, as the
        // engine sets them, so Left shows.
        let way = value ?? shows ?? "start"
        let current = Self.ways.contains(where: { $0.word == way }) ? way : "start"
        LabeledContent("Alignment") {
            // A way picked is written as the text's own, even the one shown: where the slot's way
            // is not given, Left shows, and a click on it still sets it.
            Picker("Alignment", selection: Binding(get: { current }, set: { picked in
                if picked != value { choose(.string(picked)) }
            })) {
                ForEach(Self.ways, id: \.word) { way in
                    Image(systemName: way.symbol)
                        .accessibilityLabel(Words.word(way.word, of: "align/x", type: "text"))
                        .tag(way.word)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .fixedSize()
            .accessibilityIdentifier("inspector-align/x")
        }
    }

    /// Each way, by the word the deck writes, and its symbol.
    private static let ways: [(word: String, symbol: String)] = [
        ("start", "text.alignleft"), ("center", "text.aligncenter"), ("end", "text.alignright"),
    ]
}

/// The layouts the slide may take (PLAN 3.26), as a presentation app's layout picker shows them:
/// each drawn small, best first by what lint finds in the slide laid out in it, in every size the
/// deck comes in, the one it takes now outlined, and any that would leave more to put right than
/// the slide has now marked; what the slide has now is marked on the slide itself. The
/// pointer over one shows the slide laid out in it on the canvas, nothing made; a click gives the
/// slide that layout, one step to undo (PLAN 2.92). Above them, the layout it takes, named, opens
/// the theme's layouts by their sections (PLAN 3.31).
private struct LayoutsRow<Named: View>: View {
    let editor: DeckEditor
    let state: String
    let look: ThemeLook?
    let make: ([JSONValue]) -> Void
    @ViewBuilder let named: () -> Named
    @State private var drawn: [Drawn] = []
    /// The slide they were drawn for.
    @State private var drawnFor: String?
    /// The layout the pointer is over, shown on the canvas.
    @State private var over: String?

    private struct Drawn: Identifiable {
        let suggestion: LayoutSuggestion
        let image: CGImage

        var id: String { suggestion.layout }
    }

    /// How high each is painted, pixels: a tile's width on a Retina screen. Computed: a generic
    /// type keeps no static stored property.
    private static var painted: Int { 150 }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            named()
            if drawn.count >= 2 {
                Text("Suggested for This Slide")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 96), spacing: 8)], spacing: 10) {
                    ForEach(drawn) { one in tile(one) }
                }
            }
        }
        // Once the inspector has settled on the slide: each layout laid out, linted in every size,
        // and drawn, a step at a time, the window answering a person between steps.
        .task(id: "\(state)\u{1f}\(editor.revision)\u{1f}\(editor.format ?? "")") {
            if drawnFor != state {
                drawn = []
                drawnFor = state
            }
            try? await Task.sleep(for: .milliseconds(150))
            guard !Task.isCancelled, let found = await judged() else { return }
            drawn = found
        }
        .onDisappear { leave() }
    }

    /// The slide's layouts, best first, each drawn: none where it has one to take; nil where the
    /// slide changed, or an edit came, while they were judged.
    private func judged() async -> [Drawn]? {
        let session = editor.session
        guard let count = try? session.layoutsBegin(state: state) else { return nil }
        guard count > 1 else { return [] }
        while (try? session.layoutsStep()) == true {
            await Task.yield()
            if Task.isCancelled { return nil }
        }
        guard let all = try? session.layoutsDrawn(height: Self.painted) else { return nil }
        return all.map { Drawn(suggestion: $0.suggestion, image: $0.image) }
    }

    /// The layout the slide takes now, which each other is judged beside.
    private var now: LayoutSuggestion? {
        drawn.first { $0.suggestion.current }?.suggestion
    }

    private func tile(_ one: Drawn) -> some View {
        let s = one.suggestion
        let name = Words.name(s.layout, of: "layout", theme: look)
        let more = s.adds(over: now)
        return Button {
            guard !s.current else { return }
            leave()
            make(s.patch)
        } label: {
            VStack(spacing: 4) {
                Image(decorative: one.image, scale: 1)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 4))
                    .overlay {
                        RoundedRectangle(cornerRadius: 4)
                            .strokeBorder(
                                s.current ? Color.accentColor : Color.secondary.opacity(over == s.layout ? 0.8 : 0.3),
                                lineWidth: s.current ? 2.5 : 1)
                    }
                HStack(spacing: 3) {
                    if more.errors + more.warnings > 0 {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(more.errors > 0 ? Color.red : Color.orange)
                            .imageScale(.small)
                    }
                    Text(name)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                .font(.caption)
                .foregroundStyle(s.current ? .primary : .secondary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { inside in hover(s, inside) }
        .help([name, Self.said(s, now: now, look: look)].joined(separator: ". "))
        .accessibilityLabel(name)
        .accessibilityValue(Self.said(s, now: now, look: look))
        .accessibilityAddTraits(s.current ? .isSelected : [])
    }

    /// The pointer over a layout shows the slide laid out in it; off it, as it is.
    private func hover(_ s: LayoutSuggestion, _ inside: Bool) {
        if inside {
            over = s.layout
            if s.current { editor.still() } else { editor.show(s.patch) }
        } else if over == s.layout {
            leave()
        }
    }

    /// The canvas drawn as the deck is again.
    private func leave() {
        guard over != nil else { return }
        over = nil
        editor.still()
    }

    /// What a layout does, in a person's words: whether the slide takes it now, what more there
    /// would be to put right in it than the slide has now, the layouts that look the same, and how
    /// many slides and steps it changes.
    private static func said(_ s: LayoutSuggestion, now: LayoutSuggestion?, look: ThemeLook?) -> String {
        var parts = [s.current ? "The slide's layout now" : "Click to use it"]
        let more = s.adds(over: now)
        let issues = more.errors + more.warnings
        if s.current {
            if s.errors + s.warnings > 0 { parts.append("What the slide has to put right is under View › Issues") }
        } else {
            let said = issues == 1 ? "1 more issue" : "\(issues) more issues"
            parts.append(issues == 0 ? "Nothing more to put right" : "\(said) to put right than the slide has now")
        }
        if !s.alike.isEmpty {
            let alike = s.alike.map { Words.name($0, of: "layout", theme: look) }
            parts.append("Looks the same as \(alike.formatted(.list(type: .or)))")
        }
        if s.reach.count > 1 { parts.append("Changes \(s.reach.count) slides and steps") }
        return parts.joined(separator: ". ")
    }
}

/// The slide's layout, named, as a button that opens the theme's layouts by their sections, each
/// drawn as it is used, those the slide may take offered and the one it takes outlined (PLAN 3.31):
/// one chosen is the slide's, written where its layout lives.
private struct LayoutField: View {
    let editor: DeckEditor
    let state: String
    let label: String
    /// The layout the slide takes now.
    let value: String?
    /// The layouts it may take: each with a slot for each object placed in one.
    let names: [String]
    let look: ThemeLook?
    let choose: (JSONValue) -> Void
    @State private var picking = false

    var body: some View {
        LabeledContent(label) {
            Button {
                picking = true
            } label: {
                HStack(spacing: 4) {
                    Text(value.map { Words.name($0, of: "layout", theme: look) } ?? "None")
                        .lineLimit(1)
                    Image(systemName: "chevron.up.chevron.down")
                        .imageScale(.small)
                        .foregroundStyle(.secondary)
                }
            }
            .help("Choose the slide's layout from the theme's, by their sections")
            .accessibilityLabel(label)
            .accessibilityValue(value.map { Words.name($0, of: "layout", theme: look) } ?? "None")
            .accessibilityIdentifier("layout-choose")
            .popover(isPresented: $picking, arrowEdge: .leading) {
                LayoutPicker(editor: editor, state: state, purpose: .change(names: names, current: value)) { picked in
                    picking = false
                    if let picked, picked != value { choose(.string(picked)) }
                }
            }
        }
    }
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
                        .labelsHidden()
                        .textFieldStyle(.roundedBorder)
                        .multilineTextAlignment(.trailing)
                        .monospacedDigit()
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
                .labelsHidden()
                .textFieldStyle(.roundedBorder)
                .multilineTextAlignment(.trailing)
                .monospacedDigit()
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

/// A number, as a presentation app's inspector shows a font's size: the number the slide shows in
/// the field, the one set or else the theme's, what it counts after it, and a stepper. What is typed
/// is chosen on Return or when the field is left, and a stepper's clicks once they rest, each one
/// step to undo; the number shown already changes nothing, so the theme's stays the theme's.
/// Emptied, it takes the number set away, and the theme's shows.
private struct NumberRow: View {
    let label: String
    let value: Double?
    let whole: Bool
    let unit: String?
    /// The number the slide shows where nothing sets one: the theme's.
    var shows: Double? = nil
    /// What a stepper's click moves it by.
    var step: Double = 1
    /// The least it takes; the number it must be more than; and the most it takes.
    var least: Double? = nil
    var above: Double? = nil
    var most: Double? = nil
    /// What a test finds the field by.
    let id: String
    let choose: (JSONValue) -> Void
    @State private var text = ""
    @State private var settling: Task<Void, Never>?
    @FocusState private var focused: Bool

    /// The number the slide shows: the one set, else the theme's.
    private var inEffect: Double? { value ?? shows }

    var body: some View {
        // A width of its own, so where the row has no room the label stands over the field, as a
        // form sets it on the iPad, rather than the field squeezing the label short.
        LabeledContent(label) {
            HStack(spacing: 4) {
                TextField(label, text: $text, prompt: Text("Default"))
                    .labelsHidden()
                    .textFieldStyle(.roundedBorder)
                    .multilineTextAlignment(.trailing)
                    .monospacedDigit()
                    .frame(width: 64)
                    .focused($focused)
                    .onSubmit(commit)
                    #if !os(macOS)
                    .keyboardType(.numbersAndPunctuation)
                    #endif
                    .accessibilityIdentifier(id)
                if let unit { Text(unit).foregroundStyle(.secondary) }
                Stepper(label, onIncrement: { nudge(step) }, onDecrement: { nudge(-step) })
                    .labelsHidden()
                    .accessibilityIdentifier("\(id)-stepper")
            }
        }
        .onChange(of: inEffect, initial: true) { _, now in
            text = FieldNumber.shown(now)
        }
        .onChange(of: focused) { _, now in
            if !now { commit() }
        }
    }

    private func commit() {
        settling?.cancel()
        settling = nil
        let typed = text.trimmingCharacters(in: .whitespaces)
        guard !typed.isEmpty else {
            // Emptied: the number set taken away, and the theme's shown again.
            if value != nil { choose(.null) } else { text = FieldNumber.shown(inEffect) }
            return
        }
        guard let read = FieldNumber.read(typed) else {
            text = FieldNumber.shown(inEffect)
            return
        }
        let number = kept(whole ? read.rounded() : read)
        // One it must be more than is not taken, and the number shown already changes nothing.
        if let above, number <= above {
            text = FieldNumber.shown(inEffect)
            return
        }
        guard FieldNumber.shown(number) != FieldNumber.shown(inEffect) else {
            text = FieldNumber.shown(inEffect)
            return
        }
        choose(.number(number))
    }

    /// `number` within the least and the most it takes.
    private func kept(_ number: Double) -> Double {
        var n = number
        if let least { n = max(n, least) }
        if let most { n = min(n, most) }
        return n
    }

    /// A stepper's click: the number moved `by`, shown at once and chosen once the clicks rest; one
    /// that would go past what it takes, not moved.
    private func nudge(_ by: Double) {
        let from = FieldNumber.read(text) ?? inEffect ?? 0
        let next = kept(FieldNumber.stepped(whole ? from.rounded() : from, by: by))
        if let above, next <= above { return }
        text = FieldNumber.shown(next)
        settling?.cancel()
        settling = Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            commit()
        }
    }
}

/// One of an image's adjustments (PLAN 2.117), as Adjust Image's sliders show it: from -1 to 1, set
/// when the slider is let go, one step to undo; let go at the middle, it takes the adjustment away.
private struct AdjustRow: View {
    let label: String
    let value: Double
    /// Whether none takes the value away: the object's own, or none set. Written on a slide, none
    /// is written there, so that what the object or an earlier slide sets does not show through.
    let awayAtNone: Bool
    let choose: (JSONValue) -> Void
    /// Where the slider is while it moves, before it is let go.
    @State private var moving: Double?
    @State private var editing = false

    var body: some View {
        LabeledContent(label) {
            HStack(spacing: 6) {
                Slider(
                    value: Binding(
                        get: { moving ?? value },
                        set: { now in
                            moving = now
                            // A step not dragged, as VoiceOver's, is set at once.
                            if !editing { commit() }
                        }),
                    in: -1...1,
                    onEditingChanged: { now in
                        editing = now
                        if !now { commit() }
                    }
                )
                .frame(minWidth: 90, maxWidth: 140)
                .accessibilityValue(Text(said(moving ?? value)))
                Text(said(moving ?? value)).monospacedDigit().foregroundStyle(.secondary).lineLimit(1).fixedSize()
                    .frame(minWidth: 40, alignment: .trailing)
            }
        }
    }

    /// The value as a share of the most it takes, signed: +25, −40, or None.
    private func said(_ v: Double) -> String {
        let percent = Int((abs(v) * 100).rounded())
        return percent == 0 ? "None" : "\(v > 0 ? "+" : "−")\(percent)"
    }

    private func commit() {
        guard let now = moving else { return }
        moving = nil
        let set = (now * 100).rounded() / 100
        if set != value { choose(set == 0 && awayAtNone ? .null : .number(set)) }
    }
}

/// The slide's number and footer (ADR-0028), as a presentation app's Header and Footer offers them:
/// the deck numbers its slides or not, the words at their foot, and whether this slide shows them.
/// Each choice is one patch, one step to undo.
private struct FurnitureSection: View {
    let furniture: FurnitureChoice
    let make: ([JSONValue]) -> Void

    var body: some View {
        Section {
            Toggle("Slide Numbers", isOn: Binding(get: { furniture.number }, set: { make(FurnitureChoice.numbering($0)) }))
                .accessibilityIdentifier("slide-numbers")
            TextRow(label: "Footer", value: furniture.footer ?? "", long: false, trims: true) { value in
                if let patch = furniture.footing(to: value.string ?? "") { make(patch) }
            }
            .accessibilityIdentifier("footer")
            Picker("On This Slide", selection: Binding(get: { furniture.slide.map { $0 ? "shown" : "hidden" } ?? "" }, set: { picked in
                make(furniture.showing(picked.isEmpty ? nil : picked == "shown"))
            })) {
                // Short enough to read whole beside its label in the inspector's column.
                Text("As Layout").tag("")
                Text("Shown").tag("shown")
                Text("Hidden").tag("hidden")
            }
        } header: {
            Text("Slide Number and Footer")
        } footer: {
            Text(furniture.shown ? "This slide shows them." : "This slide doesn’t show them.")
                .font(.caption).foregroundStyle(.secondary)
        }
    }
}

/// Words for people, as they are written: chosen when entered, or when the field is left.
private struct TextRow: View {
    let label: String
    let value: String
    /// Several lines, shown under the label: notes, a description.
    let long: Bool
    /// What an empty field of several lines says goes in it; its label where none is given.
    var prompt: String? = nil
    /// Words that are the value trimmed, as a footer's are, choose nothing: the field is put back.
    var trims = false
    let choose: (JSONValue) -> Void
    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        Group {
            if long {
                TextField(label, text: $text, prompt: Text(prompt ?? label), axis: .vertical)
                    .labelsHidden()
                    .lineLimit(3...12)
            } else {
                TextField(label, text: $text, axis: .vertical)
                    .lineLimit(1...6)
            }
        }
        .textFieldStyle(.roundedBorder)
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
        if trims, text != value, text.trimmingCharacters(in: .whitespacesAndNewlines) == value {
            text = value
            return
        }
        guard text != value else { return }
        choose(text.isEmpty ? .null : .string(text))
    }
}
