import ScaenaKit
import SwiftUI

/// The bundle's files (PLAN 3.15, as the browser's Files tab, PLAN 2.59), as `scaena files` lists
/// them: its images, fonts, and data, each with what names it and the nodes drawn from it. One
/// nothing names can be taken out, one step to undo, which puts it back; the next save takes it out
/// where the bundle is kept.
struct FilesPanel: View {
    let editor: DeckEditor
    let edits: PanelEdits
    @State private var files: [BundleFile] = []

    var body: some View {
        List {
            ForEach(["image", "font", "data"], id: \.self) { type in
                let held = files.filter { $0.type == type }
                if !held.isEmpty {
                    Section(Self.heading(type)) {
                        ForEach(held) { file in
                            row(file)
                        }
                    }
                }
            }
        }
        .overlay {
            if files.isEmpty {
                ContentUnavailableView(
                    "No files", systemImage: "doc", description: Text("The bundle holds no images, fonts, or data."))
            }
        }
        .task(id: editor.revision) { files = (try? editor.session.bundleFiles()) ?? [] }
    }

    private func row(_ file: BundleFile) -> some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 2) {
                Text(file.path).lineLimit(1).truncationMode(.middle)
                Text(Self.said(file)).font(.caption).foregroundStyle(.secondary).lineLimit(3)
            }
            Spacer()
            if file.named.isEmpty {
                Button("Remove") { remove(file.path) }
                    .help("Take \(file.path) out: nothing names it")
            }
        }
        .help(file.path)
    }

    private func remove(_ path: String) {
        edits.data { try editor.session.removeFile(path) }
    }

    /// What a file is, its size, what names it, and what draws from it.
    private static func said(_ file: BundleFile) -> String {
        let size = ByteCountFormatter.string(fromByteCount: Int64(file.bytes), countStyle: .file)
        let named = file.named.isEmpty ? "nothing names it" : "named by \(file.named.joined(separator: ", "))"
        let used = file.used.isEmpty ? "" : "; drawn in \(file.used.joined(separator: ", "))"
        return "\(size), \(named)\(used)"
    }

    private static func heading(_ type: String) -> String {
        switch type {
        case "image": "Images"
        case "font": "Fonts"
        default: "Data"
        }
    }
}
