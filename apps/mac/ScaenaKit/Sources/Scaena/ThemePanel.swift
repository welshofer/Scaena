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

    private var theme: JSONValue? { text?.json }

    var body: some View {
        Form {
            Section("Theme") {
                Picker("Theme", selection: chosen) {
                    ForEach(shipped) { ships in
                        Text("\(ships.name.capitalized) (ships)").tag("ships:\(ships.name)")
                    }
                    ForEach(themes?.files ?? [], id: \.self) { file in
                        Text(file).tag(file)
                    }
                }
                .help("Put the deck in another theme, refused with why where it would not validate in it")
            }
            Section("Colors") {
                ForEach(entries(theme?["tokens"]?["color"])) { entry in
                    let name = Words.name(entry.key, of: "color", theme: nil)
                    ColorRow(key: entry.key, name: name, value: entry.value.string ?? "") { value in
                        let path = "/tokens/color/\(escaped(entry.key))"
                        return edit([put(path, .string(value), over: entry.value)], "\(name) Color")
                    }
                }
            }
            Section("Type") {
                ForEach(entries(theme?["type"]?["roles"])) { role in
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
                ForEach(photos, id: \.self) { photo in
                    Button(photo) { fromPhoto(photo) }
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
        edits.beside("Theme Change") {
            let session = editor.session
            let themed =
                try picked.hasPrefix("ships:")
                ? session.retheme(ships: String(picked.dropFirst("ships:".count))) : session.retheme(path: picked)
            guard themed.applied else {
                throw Refused(description: "not re-themed: \(themed.why.first ?? "the deck would not validate in it")")
            }
            return []
        }
    }

    /// The theme edited by `ops`, one step to undo named `name`, the theme file written back:
    /// whether it was; refused, the window says why.
    @discardableResult
    private func edit(_ ops: [JSONValue], _ name: String) -> Bool {
        edits.beside(name) {
            let edited = try editor.session.themeEdit(ops)
            guard edited.applied else {
                let why = edited.why.first ?? "the deck would not validate in it"
                throw Refused(description: "\(name) not changed: \(why)")
            }
            return edited.files
        }
    }

    private func fromPhoto(_ photo: String) {
        edits.beside("Colors from Photo") {
            let edited = try editor.session.themeEdit(photo: photo)
            guard edited.applied else {
                let why = edited.why.first ?? "the deck would not validate in it"
                throw Refused(description: "not taken from \(photo): \(why)")
            }
            return edited.files
        }
    }

    /// The op that sets `path` to `value`: a replace where `old` is there, an add where nothing is.
    private func put(_ path: String, _ value: JSONValue, over old: JSONValue?) -> JSONValue {
        ["op": .string(old == nil ? "add" : "replace"), "path": .string(path), "value": value]
    }

    /// `key` as one step of a JSON Pointer.
    private func escaped(_ key: String) -> String {
        key.replacingOccurrences(of: "~", with: "~0").replacingOccurrences(of: "/", with: "~1")
    }

    /// An object's entries, by key, sorted; none for anything else.
    private func entries(_ value: JSONValue?) -> [ThemeEntry] {
        (value?.object ?? [:]).sorted { $0.key < $1.key }.map { ThemeEntry(key: $0.key, value: $0.value) }
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
            Text(name)
            Spacer()
            TextField(name, text: $typed)
                .labelsHidden()
                .textFieldStyle(.roundedBorder)
                .monospaced()
                .frame(width: 116)
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
            if !now { commit() }
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
                NumberRow(
                    label: field.label, id: "theme-\(key)-\(field.key)", step: field.step, unit: field.unit,
                    value: role[field.key]?.number
                ) { set(field, $0) }
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
                Stepper(label, onIncrement: { nudge(step) }, onDecrement: { nudge(-step) })
                    .labelsHidden()
                    .accessibilityIdentifier("\(id)-stepper")
                if let unit {
                    Text(unit).foregroundStyle(.secondary)
                }
            }
        }
        .onChange(of: value, initial: true) { _, now in
            typed = FieldNumber.shown(now)
            asked = nil
        }
        .onChange(of: focused) { _, now in
            if !now { commit() }
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
