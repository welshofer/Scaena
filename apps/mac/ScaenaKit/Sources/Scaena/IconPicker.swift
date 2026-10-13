import ScaenaKit
import SwiftUI

/// The icons that ship, to insert one (PLAN 2.104), as the browser's picker offers them: a field
/// that finds them by their words, Solid for their filled weight, and the icons found, drawn in
/// ink. A click on one inserts it where the pointer last pressed on the slide, or, `replacing`,
/// puts it in the place of the icon selected (PLAN G.23); Return takes the first found.
struct IconPicker: View {
    let session: ScaenaSession
    /// Whether the icon chosen takes the place of the icon selected.
    var replacing = false
    /// Insert the icon chosen.
    let insert: (Icon) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var solid = false
    @State private var found: IconsFound?
    @State private var failed: String?

    /// How many icons are drawn at once: a person narrows the rest by words.
    private static let shown = 240

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(replacing ? "Replace the Icon" : "Insert an Icon").font(.headline)
                Spacer()
                Toggle("Solid", isOn: $solid)
                    .accessibilityIdentifier("icon-solid")
            }
            TextField("Find an icon", text: $query)
                .textFieldStyle(.roundedBorder)
                .accessibilityIdentifier("icon-find")
                .onSubmit {
                    if let first = found?.icons.first { choose(first.icon) }
                }
            ScrollView {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 56, maximum: 72), spacing: 8)], spacing: 8) {
                    ForEach(found?.icons ?? [], id: \.name) { one in
                        Button {
                            choose(one.icon)
                        } label: {
                            IconShape(one.icon)
                                .fill(.primary)
                                .frame(width: 30, height: 30)
                                .padding(10)
                        }
                        .buttonStyle(.bordered)
                        .help(Words.icon(one.name))
                        .accessibilityLabel(Words.icon(one.name))
                        .accessibilityIdentifier("icon-\(one.name)")
                    }
                }
                .padding(2)
            }
            .frame(minHeight: 300)
            Text(counted)
                .font(.caption)
                .foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
        }
        .padding(20)
        .frame(minWidth: 520, idealWidth: 640, minHeight: 480)
        .task(id: "\(solid) \(query)") { find() }
    }

    /// What the field found, said.
    private var counted: String {
        if let failed { return failed }
        guard let found else { return "Finding the icons…" }
        if found.icons.isEmpty { return "No icon answers “\(query.trimmingCharacters(in: .whitespaces))”" }
        if found.found > found.icons.count { return "\(found.icons.count) of \(found.found) icons: type to find fewer" }
        return found.found == 1 ? "1 icon" : "\(found.found) icons"
    }

    private func find() {
        do {
            found = try session.icons(query, fill: solid, limit: Self.shown)
            failed = nil
        } catch {
            failed = "The icons could not be read: \(error.localizedDescription)"
        }
    }

    private func choose(_ icon: Icon) {
        insert(icon)
        dismiss()
    }
}
