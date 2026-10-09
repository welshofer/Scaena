import ScaenaKit
import SwiftUI

/// The deck's theme, edited (PLAN 3.15, as the browser's Theme tab, PLAN 2.39, 2.61, 2.94): another
/// theme, one the bundle holds or one that ships; its colors, its type roles, and its spacing,
/// each change one edit of the theme the deck names, RFC 6902 operations on its JSON; and its
/// colors from a photo the bundle holds. Each is one step to undo, refused with why where the deck
/// would not validate in the theme it leaves. The panel shows the theme frames are drawn in.
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
                    ColorRow(name: entry.key, value: entry.value.string ?? "") { value in
                        let path = "/tokens/color/\(escaped(entry.key))"
                        edit([replace(path, .string(value))], "\(entry.key) set to \(value)")
                    }
                }
            }
            Section("Type") {
                ForEach(entries(theme?["type"]?["roles"])) { role in
                    RoleRow(name: role.key, role: role.value) { key, value in
                        let path = "/type/roles/\(escaped(role.key))/\(key)"
                        let op = role.value[key] == nil ? add(path, value) : replace(path, value)
                        edit([op], "\(role.key)'s \(key) set")
                    }
                }
            }
            Section("Spacing") {
                NumberRow(label: "Gutter", value: theme?["grid"]?["gutter"]?.number) {
                    edit([replace("/grid/gutter", .number($0))], "gutter set")
                }
                NumberRow(label: "Baseline", value: theme?["grid"]?["baseline"]?.number) {
                    edit([replace("/grid/baseline", .number($0))], "baseline set")
                }
                NumberRow(label: "Space unit", value: theme?["tokens"]?["space"]?["unit"]?.number) {
                    edit([replace("/tokens/space/unit", .number($0))], "space unit set")
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
        edits.beside {
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

    /// The theme edited by `ops`: one step to undo, the theme file written back; refused with why.
    private func edit(_ ops: [JSONValue], _ what: String) {
        edits.beside {
            let edited = try editor.session.themeEdit(ops)
            guard edited.applied else {
                let why = edited.why.first ?? "the deck would not validate in it"
                throw Refused(description: "\(what): refused, \(why)")
            }
            return edited.files
        }
    }

    private func fromPhoto(_ photo: String) {
        edits.beside {
            let edited = try editor.session.themeEdit(photo: photo)
            guard edited.applied else {
                let why = edited.why.first ?? "the deck would not validate in it"
                throw Refused(description: "not taken from \(photo): \(why)")
            }
            return edited.files
        }
    }

    private func replace(_ path: String, _ value: JSONValue) -> JSONValue {
        ["op": "replace", "path": .string(path), "value": value]
    }

    private func add(_ path: String, _ value: JSONValue) -> JSONValue {
        ["op": "add", "path": .string(path), "value": value]
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

/// A color: its swatch, which picks one, and its value as the theme writes it, which takes any the
/// theme reads (`#rrggbb[aa]`, `oklch(…)`). A swatch dragged commits once it rests.
private struct ColorRow: View {
    let name: String
    let value: String
    let set: (String) -> Void
    @State private var typed = ""
    @State private var settling: Task<Void, Never>?

    var body: some View {
        HStack {
            ColorPicker(name, selection: swatch, supportsOpacity: true)
                .labelsHidden()
            Text(name)
            Spacer()
            TextField("Value", text: $typed)
                .frame(width: 120)
                .onSubmit { if typed != value { set(typed) } }
        }
        .onAppear { typed = value }
        .onChange(of: value) { _, now in typed = now }
    }

    /// The swatch: the value as a color, where it is a hex color; a color picked, its hex, once the
    /// picker rests.
    private var swatch: Binding<Color> {
        Binding(
            get: { Self.color(value) ?? .clear },
            set: { picked in
                guard let hex = Self.hex(picked), hex.lowercased() != value.lowercased() else { return }
                settling?.cancel()
                settling = Task { @MainActor in
                    try? await Task.sleep(for: .milliseconds(400))
                    guard !Task.isCancelled else { return }
                    set(hex)
                }
            })
    }

    /// `#rrggbb` or `#rrggbbaa` as a color; none for another kind.
    static func color(_ value: String) -> Color? {
        guard value.hasPrefix("#"), value.count == 7 || value.count == 9,
            let n = UInt64(value.dropFirst(), radix: 16)
        else { return nil }
        let alpha = value.count == 9
        let (r, g, b, a) =
            alpha
            ? (Double((n >> 24) & 0xFF), Double((n >> 16) & 0xFF), Double((n >> 8) & 0xFF), Double(n & 0xFF))
            : (Double((n >> 16) & 0xFF), Double((n >> 8) & 0xFF), Double(n & 0xFF), 255.0)
        return Color(.sRGB, red: r / 255, green: g / 255, blue: b / 255, opacity: a / 255)
    }

    /// `color` as `#RRGGBB`, or `#RRGGBBAA` where it is not opaque.
    static func hex(_ color: Color) -> String? {
        guard let srgb = color.srgb else { return nil }
        let byte = { (v: Double) in Int((min(max(v, 0), 1) * 255).rounded()) }
        let (r, g, b) = (byte(srgb.red), byte(srgb.green), byte(srgb.blue))
        let rgb = String(format: "#%02X%02X%02X", r, g, b)
        let alpha = byte(srgb.alpha)
        return alpha == 255 ? rgb : rgb + String(format: "%02X", alpha)
    }
}

/// A type role: its size, weight, leading, and tracking, each set when typed.
private struct RoleRow: View {
    let name: String
    let role: JSONValue
    /// One of its fields set: its key, and the value.
    let set: (String, JSONValue) -> Void

    var body: some View {
        DisclosureGroup(name) {
            ForEach(["size", "weight", "leading", "tracking"], id: \.self) { key in
                NumberRow(label: key.capitalized, value: role[key]?.number) { set(key, .number($0)) }
            }
        }
    }
}

/// A number the theme sets, typed and set on Return.
private struct NumberRow: View {
    let label: String
    let value: Double?
    let set: (Double) -> Void
    @State private var typed = ""

    var body: some View {
        HStack {
            Text(label)
            Spacer()
            TextField(label, text: $typed)
                .multilineTextAlignment(.trailing)
                .frame(width: 90)
                .onSubmit {
                    let number = Double(typed.trimmingCharacters(in: .whitespaces))
                    guard let number, number != value else { return }
                    set(number)
                }
        }
        .onAppear { typed = Self.shown(value) }
        .onChange(of: value) { _, now in typed = Self.shown(now) }
    }

    static func shown(_ value: Double?) -> String {
        guard let value else { return "" }
        return value == value.rounded() ? String(Int(value)) : String(value)
    }
}
