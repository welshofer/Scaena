#if os(macOS)
import AppKit
#else
import UIKit
#endif
import ScaenaKit
import SwiftUI

/// The deck's theme, edited (PLAN 3.15, as the browser's Theme tab, PLAN 2.39, 2.61, 2.94): another
/// theme, one the bundle holds or one that ships; its colors, its type roles, and its spacing,
/// each change one edit of the theme the deck names, RFC 6902 operations on its JSON; and its
/// colors from a photo the bundle holds. Each value is a field that reads as one, set on Return or
/// when the field is left, Escape putting it back, and each change is one step to undo, named in
/// the Edit menu (PLAN 3.28); one is refused with why where the deck would not validate in the
/// theme it leaves. The panel shows the theme frames are drawn in, each name in a person's words.
struct ThemePanel: View {
    let editor: DeckEditor
    let edits: PanelEdits
    @State private var text: ThemeText?
    @State private var themes: Themes?
    @State private var shipped: [ShippedTheme] = []
    @State private var photos: [String] = []
    /// The type roles in the order the theme writes them, as Insert › Text lists them.
    @State private var roleOrder: [String] = []

    private var theme: JSONValue? { text?.json }

    var body: some View {
        Form {
            Section("Theme") {
                // Each theme by its name: those the deck holds, then those that come with Scaena,
                // by family, each family a section (PLAN T.0d).
                Picker("Theme", selection: chosen) {
                    Section("In This Deck") {
                        ForEach(themes?.files ?? [], id: \.self) { file in
                            Text(Words.theme(file)).tag(file)
                        }
                    }
                    ForEach(shipped.byFamily) { family in
                        Section(family.name) {
                            ForEach(family.themes) { ships in
                                Text(Words.phrase(ships.name)).tag("ships:\(ships.name)")
                            }
                        }
                    }
                }
                .help("Put the deck in another theme; one it can’t take says why")
            }
            Section("Colors") {
                ForEach(entries(theme?["tokens"]?["color"])) { entry in
                    let name = Words.name(entry.key, of: "color", theme: nil)
                    ColorRow(key: entry.key, name: name, uses: uses(entry.key), value: entry.value.string ?? "") { value in
                        let path = "/tokens/color/\(escaped(entry.key))"
                        return edit([put(path, .string(value), over: entry.value)], "\(name) Color")
                    }
                }
            }
            Section("Type") {
                ForEach(inOrder(entries(theme?["type"]?["roles"]))) { role in
                    RoleRow(key: role.key, role: role.value) { field, value in
                        let path = "/type/roles/\(escaped(role.key))/\(field.key)"
                        let name = "\(Words.phrase(role.key)) \(field.label)"
                        return edit([put(path, .number(value), over: role.value[field.key])], name)
                    }
                }
            }
            Section("Spacing") {
                let gutter = theme?["grid"]?["gutter"]
                NumberRow(label: "Gutter", id: "theme-gutter", step: 1, unit: "pt", value: gutter?.number) {
                    edit([put("/grid/gutter", .number($0), over: gutter)], "Gutter")
                }
                let baseline = theme?["grid"]?["baseline"]
                NumberRow(label: "Baseline Grid", id: "theme-baseline", step: 1, unit: "pt", value: baseline?.number) {
                    edit([put("/grid/baseline", .number($0), over: baseline)], "Baseline Grid")
                }
                let space = theme?["tokens"]?["space"]?["unit"]
                NumberRow(label: "Space Unit", id: "theme-space-unit", step: 1, unit: "pt", value: space?.number) {
                    edit([put("/tokens/space/unit", .number($0), over: space)], "Space Unit")
                }
            }
            Section("From a photo") {
                if photos.isEmpty {
                    Text("The bundle holds no PNG or JPEG photo.").foregroundStyle(.secondary)
                }
                ForEach(Array(photos.enumerated()), id: \.element) { n, photo in
                    Button(Words.file(photo, type: "image") ?? "Photo \(n + 1)") { fromPhoto(photo) }
                        .help("Give the theme this photo's colors, each color text is set in kept where it reads")
                }
            }
        }
        .formStyle(.grouped)
        .task(id: editor.revision) { read() }
    }

    private func read() {
        let session = editor.session
        text = try? session.themeText()
        themes = try? session.themes()
        if shipped.isEmpty { shipped = (try? session.shippedThemes()) ?? [] }
        let images = (try? session.bundleFiles()) ?? []
        let photo = { (file: BundleFile) in
            file.type == "image" && ["png", "jpg", "jpeg"].contains((file.path as NSString).pathExtension.lowercased())
        }
        photos = images.filter(photo).map(\.path)
        roleOrder = ((try? session.inserts()) ?? []).filter { $0.kind == "Text" }.map(\.name)
    }

    /// The theme the deck names, or another chosen: one that ships by `ships:` and its name.
    private var chosen: Binding<String> {
        Binding(
            get: { themes?.current ?? "" },
            set: { picked in
                guard picked != themes?.current else { return }
                retheme(picked)
            })
    }

    private func retheme(_ picked: String) {
        var recolored = 0
        let made = edits.beside("Theme Change") {
            let session = editor.session
            let themed =
                try picked.hasPrefix("ships:")
                ? session.retheme(ships: String(picked.dropFirst("ships:".count))) : session.retheme(path: picked)
            guard themed.applied else {
                throw Refused("not re-themed", themed.refusals, by: editor, otherwise: "the deck would not open in it")
            }
            recolored = themed.recolored
            return []
        }
        // A text the theme left too faint was set in its color that reads where it stands (PLAN 2.82).
        if made && recolored > 0 {
            let name = picked.hasPrefix("ships:") ? Words.phrase(String(picked.dropFirst("ships:".count))) : Words.theme(picked)
            edits.say("\(recolored) text\(recolored == 1 ? "" : "s") recolored to read in \(name)")
        }
    }

    /// The theme edited by `ops`, one step to undo named `name`, the theme file written back:
    /// whether it was; refused, the window says why.
    @discardableResult
    private func edit(_ ops: [JSONValue], _ name: String) -> Bool {
        edits.beside(name) {
            let edited = try editor.session.themeEdit(ops)
            guard edited.applied else {
                throw Refused(
                    "\(name) not changed", edited.refusals, by: editor, otherwise: "the deck would not open in it")
            }
            return edited.files
        }
    }

    private func fromPhoto(_ photo: String) {
        edits.beside("Colors from Photo") {
            let edited = try editor.session.themeEdit(photo: photo)
            guard edited.applied else {
                // The photo by its file's name, as the panel offers it, not its path in the bundle.
                let named = Words.file(photo, type: "image") ?? "the photo"
                throw Refused(
                    "not taken from \(named)", edited.refusals, by: editor, otherwise: "the deck would not open in it")
            }
            return edited.files
        }
    }

    /// The op that sets `path` to `value`: a replace where `old` is there, an add where nothing is.
    private func put(_ path: String, _ value: JSONValue, over old: JSONValue?) -> JSONValue {
        ["op": .string(old == nil ? "add" : "replace"), "path": .string(path), "value": value]
    }

    /// What the theme gives color `key` to, as the inspector names it there: `Text` for Dusk's ink,
    /// so the color a text is set in is found here by the name the inspector says it by, as the
    /// browser's Theme tab says it beside the color. None where no role takes it, or only the one
    /// its own name says (Dusk's accent, Accent).
    private func uses(_ key: String) -> String? {
        let roles = (theme?["tokens"]?["roles"]?.object ?? [:]).filter { $0.value.string == key }.map(\.key)
        let own = Words.name(key, of: "color", theme: nil)
        let said = roles.map { Words.name($0, of: "color", theme: nil) }.filter { $0 != own }.sorted()
        return said.isEmpty ? nil : said.joined(separator: ", ")
    }

    /// `key` as one step of a JSON Pointer.
    private func escaped(_ key: String) -> String {
        key.replacingOccurrences(of: "~", with: "~0").replacingOccurrences(of: "/", with: "~1")
    }

    /// An object's entries, by key, sorted; none for anything else.
    private func entries(_ value: JSONValue?) -> [ThemeEntry] {
        (value?.object ?? [:]).sorted { $0.key < $1.key }.map { ThemeEntry(key: $0.key, value: $0.value) }
    }

    /// Type roles in the order the theme writes them, as Insert › Text lists them, and one it does
    /// not list after them, by name: an order an edit leaves where it was.
    private func inOrder(_ roles: [ThemeEntry]) -> [ThemeEntry] {
        let at = { (role: ThemeEntry) in roleOrder.firstIndex(of: role.key) ?? roleOrder.count }
        return roles.enumerated().sorted { a, b in
            at(a.element) != at(b.element) ? at(a.element) < at(b.element) : a.offset < b.offset
        }.map(\.element)
    }
}

/// One entry of an object of the theme's: a color, or a type role.
private struct ThemeEntry: Identifiable {
    let key: String
    let value: JSONValue

    var id: String { key }
}

/// A color: its swatch, which picks one, and its value as the theme writes it, in a field that
/// takes any the theme reads (`#rrggbb[aa]`, `oklch(…)`). What is typed is set on Return or when
/// the field is left, and Escape puts it back; a swatch dragged is set once it rests. One refused
/// shows the color as it was.
private struct ColorRow: View {
    /// The color's name in the theme: what a test finds its field by.
    let key: String
    /// The color's name, as a person reads it.
    let name: String
    /// What the theme gives it to, as the inspector names it: `Text`, or `Background, Text on
    /// Accent`.
    let uses: String?
    let value: String
    /// Set the color: whether it was.
    let set: (String) -> Bool
    @State private var typed = ""
    /// The color set and not yet shown back: set once.
    @State private var asked: String?
    @State private var settling: Task<Void, Never>?
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 8) {
            ColorPicker(name, selection: swatch, supportsOpacity: true)
                .labelsHidden()
            // The name, and what the theme sets in it under the name, each read whole: they take
            // the room between the swatch and the field, on as many lines as they need, and the
            // row grows to hold them, the field as wide as ever. Two lines beside a spacer cut
            // Paper's uses short, and set the 2 of Paper 2's on a line of its own.
            VStack(alignment: .leading, spacing: 1) {
                Text(name)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("theme-name-\(key)")
                if let uses {
                    Text(uses)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .help("What the theme sets in it")
                        .accessibilityIdentifier("theme-uses-\(key)")
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            TextField(name, text: $typed)
                .labelsHidden()
                .textFieldStyle(.roundedBorder)
                .monospaced()
                .frame(width: 96)
                .focused($focused)
                .onSubmit(commit)
                #if os(macOS)
                .onExitCommand(perform: putBack)
                #else
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .onKeyPress(.escape) {
                    putBack()
                    return .handled
                }
                #endif
                .accessibilityIdentifier("theme-color-\(key)")
        }
        .onChange(of: value, initial: true) { _, now in
            typed = now
            asked = nil
        }
        .onChange(of: focused) { _, now in
            if now { selectWhole($focused) } else { commit() }
        }
        .onDisappear(perform: commit)
    }

    /// What is typed, set where it is another color than the one shown; nothing typed shows the
    /// color as it was.
    private func commit() {
        let color = typed.trimmingCharacters(in: .whitespaces)
        guard !color.isEmpty, color.lowercased() != value.lowercased() else {
            typed = value
            return
        }
        guard color.lowercased() != asked?.lowercased() else { return }
        asked = color
        if !set(color) {
            asked = nil
            typed = value
        }
    }

    /// The color as it is shown again, what was typed let go.
    private func putBack() {
        typed = value
        focused = false
    }

    /// The swatch: the value as a color, where it is a hex color; a color picked, its hex, once the
    /// picker rests.
    private var swatch: Binding<Color> {
        Binding(
            get: { ThemeLook.hex(value) ?? .clear },
            set: { picked in
                guard let hex = ThemeLook.hex(picked), hex.lowercased() != value.lowercased() else { return }
                settling?.cancel()
                settling = Task { @MainActor in
                    try? await Task.sleep(for: .milliseconds(400))
                    guard !Task.isCancelled else { return }
                    _ = set(hex)
                }
            })
    }
}

/// A type role: its name and its size, opened to its size, weight, line spacing, and character
/// spacing, each a number to type or step.
private struct RoleRow: View {
    let key: String
    let role: JSONValue
    /// One of its fields set to a number: whether it was.
    let set: (RoleField, Double) -> Bool

    var body: some View {
        DisclosureGroup {
            ForEach(RoleField.all) { field in
                if field.key == "weight" {
                    RoleWeight(
                        label: field.label, id: "theme-\(key)-\(field.key)", value: role[field.key]?.number
                    ) { set(field, $0) }
                } else {
                    NumberRow(
                        label: field.label, id: "theme-\(key)-\(field.key)", step: field.step, unit: field.unit,
                        value: role[field.key]?.number
                    ) { set(field, $0) }
                }
            }
        } label: {
            HStack {
                Text(Words.phrase(key))
                Spacer()
                if let size = role["size"]?.number {
                    Text("\(FieldNumber.shown(size)) pt").monospacedDigit().foregroundStyle(.secondary)
                }
            }
        }
    }
}

/// A type role's weight by its name, Thin to Black, as the Format tab names a text's weight; one
/// between two by its number. One chosen is set at once, one step to undo.
private struct RoleWeight: View {
    let label: String
    /// What a test finds the picker by.
    let id: String
    let value: Double?
    /// Set the weight: whether it was.
    let set: (Double) -> Bool

    var body: some View {
        let current = Int((value ?? 400).rounded())
        Picker(label, selection: Binding(get: { current }, set: { picked in
            if picked != current { _ = set(Double(picked)) }
        })) {
            if !Words.weights.contains(where: { $0.value == current }) {
                Text(Words.weight(current)).tag(current)
            }
            ForEach(Words.weights, id: \.value) { weight in
                Text(weight.name).tag(weight.value)
            }
        }
        .accessibilityIdentifier(id)
    }
}

/// One of a type role's numbers: its key in the theme, its label, what a stepper's click moves it
/// by, and what it counts.
private struct RoleField: Identifiable {
    let key: String
    let label: String
    let step: Double
    let unit: String?

    var id: String { key }

    static let all = [
        RoleField(key: "size", label: "Size", step: 1, unit: "pt"),
        RoleField(key: "weight", label: "Weight", step: 50, unit: nil),
        RoleField(key: "leading", label: "Line Spacing", step: 0.05, unit: nil),
        RoleField(key: "tracking", label: "Character Spacing", step: 0.01, unit: nil),
    ]
}

/// A number the theme sets: a field that reads as one, a stepper beside it. What is typed is set
/// on Return or when the field is left, and Escape puts it back; a stepper's clicks are set once
/// they rest. One refused shows the number as it was.
private struct NumberRow: View {
    let label: String
    /// What a test finds the field by.
    let id: String
    /// What a stepper's click moves the number by.
    let step: Double
    /// What the number counts, said after it: `pt`.
    let unit: String?
    let value: Double?
    /// Set the number: whether it was.
    let set: (Double) -> Bool
    @State private var typed = ""
    /// The number set and not yet shown back: set once.
    @State private var asked: Double?
    @State private var settling: Task<Void, Never>?
    @FocusState private var focused: Bool

    var body: some View {
        LabeledContent(label) {
            HStack(spacing: 4) {
                TextField(label, text: $typed)
                    .labelsHidden()
                    .textFieldStyle(.roundedBorder)
                    .multilineTextAlignment(.trailing)
                    .monospacedDigit()
                    .frame(width: 72)
                    .focused($focused)
                    .onSubmit(commit)
                    #if os(macOS)
                    .onExitCommand(perform: putBack)
                    #else
                    .keyboardType(.numbersAndPunctuation)
                    .onKeyPress(.escape) {
                        putBack()
                        return .handled
                    }
                    #endif
                    .accessibilityIdentifier(id)
                // What it counts, beside the number it counts, as a presentation app writes it.
                if let unit {
                    Text(unit).foregroundStyle(.secondary)
                }
                Stepper(label, onIncrement: { nudge(step) }, onDecrement: { nudge(-step) })
                    .labelsHidden()
                    .accessibilityIdentifier("\(id)-stepper")
            }
        }
        .onChange(of: value, initial: true) { _, now in
            typed = FieldNumber.shown(now)
            asked = nil
        }
        .onChange(of: focused) { _, now in
            if now { selectWhole($focused) } else { commit() }
        }
        .onDisappear(perform: commit)
    }

    /// What is typed, set where it is a number other than the one shown; anything else shows the
    /// number as it was.
    private func commit() {
        settling?.cancel()
        settling = nil
        guard let number = FieldNumber.read(typed), FieldNumber.shown(number) != FieldNumber.shown(value) else {
            typed = FieldNumber.shown(value)
            return
        }
        guard FieldNumber.shown(number) != FieldNumber.shown(asked) else { return }
        asked = number
        if !set(number) {
            asked = nil
            typed = FieldNumber.shown(value)
        }
    }

    /// The number as it is shown again, what was typed let go.
    private func putBack() {
        settling?.cancel()
        settling = nil
        typed = FieldNumber.shown(value)
        focused = false
    }

    /// A stepper's click: the number moved `by`, shown at once and set once the clicks rest.
    private func nudge(_ by: Double) {
        let from = FieldNumber.read(typed) ?? value ?? 0
        typed = FieldNumber.shown(FieldNumber.stepped(from, by: by))
        settling?.cancel()
        settling = Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            commit()
        }
    }
}

/// A field's value selected whole as the field takes the keys, so what is typed takes its place, as
/// a presentation app's inspector fields do: a tap or a click left of a number set in from the right
/// would put the caret before it. Once the click that gave it the keys is let go, which sets the
/// caret, and only while the field still has them.
@MainActor
private func selectWhole(_ focused: FocusState<Bool>.Binding) {
    Task { @MainActor in
        #if os(macOS)
        while NSEvent.pressedMouseButtons != 0 { try? await Task.sleep(for: .milliseconds(16)) }
        try? await Task.sleep(for: .milliseconds(16))
        #endif
        guard focused.wrappedValue else { return }
        #if os(macOS)
        _ = NSApp.sendAction(#selector(NSText.selectAll(_:)), to: nil, from: nil)
        #else
        _ = UIApplication.shared.sendAction(
            #selector(UIResponderStandardEditActions.selectAll(_:)), to: nil, from: nil, for: nil)
        #endif
    }
}
