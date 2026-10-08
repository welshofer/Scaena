import ScaenaKit
import SwiftUI

/// A data source as a table, edited (PLAN 3.15, as the browser's Data tab, PLAN 2.55): each cell set
/// when typed, a row added, or one taken away, as `scaena data` edits them, each value read by its
/// column's type, all or none. An edit writes the source's file, which the window's undo puts back
/// (the session keeps each version), or the deck's rows written inline, which the source's undo
/// takes back. A cell its column does not read is marked, and says why.
struct DataPanel: View {
    let editor: DeckEditor
    let edits: PanelEdits
    @State private var sources: [DataSource] = []
    @State private var chosen: String?
    @State private var sheet: Sheet?
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if sources.isEmpty {
                ContentUnavailableView(
                    "No data", systemImage: "tablecells", description: Text("The deck declares no data source."))
            } else {
                Picker("Source", selection: $chosen) {
                    ForEach(sources) { source in
                        Text("\(source.name) · \(source.file ?? "inline")").tag(Optional(source.name))
                    }
                }
                .padding(.horizontal, 8)
                if let sheet, let name = chosen {
                    table(sheet, name: name)
                    HStack {
                        Button("Add Row") { change(name, [.add(row: nil, values: [:])], file: sheet.file) }
                        Spacer()
                        if let problem { Text(problem).font(.caption).foregroundStyle(.red).lineLimit(2) }
                    }
                    .padding(8)
                } else if let problem {
                    Text(problem).foregroundStyle(.secondary).padding(8)
                }
            }
        }
        .task(id: "\(editor.revision)\u{1f}\(chosen ?? "")") { read() }
    }

    /// The sheet as a grid: its columns and their types, then each row's cells, each a field.
    private func table(_ sheet: Sheet, name: String) -> some View {
        ScrollView([.horizontal, .vertical]) {
            Grid(alignment: .leading, horizontalSpacing: 6, verticalSpacing: 3) {
                GridRow {
                    Text("")
                    ForEach(sheet.columns, id: \.name) { column in
                        VStack(alignment: .leading, spacing: 0) {
                            Text(column.name).bold()
                            Text(column.type).font(.caption2).foregroundStyle(.secondary)
                        }
                    }
                    Text("")
                }
                ForEach(sheet.rows.indices, id: \.self) { row in
                    GridRow {
                        Text("\(row + 1)").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                        ForEach(sheet.columns.indices, id: \.self) { c in
                            let column = sheet.columns[c].name
                            let value = c < sheet.rows[row].count ? sheet.rows[row][c] : ""
                            Cell(value: value, why: sheet.problem(row: row, column: column)) {
                                change(name, [.set(row: row, column: column, value: $0)], file: sheet.file)
                            }
                        }
                        Button {
                            change(name, [.remove(row: row)], file: sheet.file)
                        } label: {
                            Image(systemName: "minus.circle")
                        }
                        .buttonStyle(.borderless)
                        .help("Take row \(row + 1) away")
                    }
                }
            }
            .padding(8)
        }
    }

    private func read() {
        let session = editor.session
        sources = (try? session.dataSources()) ?? []
        if chosen == nil || !sources.contains(where: { $0.name == chosen }) { chosen = sources.first?.name }
        guard let chosen else {
            sheet = nil
            return
        }
        do {
            sheet = try session.dataSheet(chosen)
            problem = nil
        } catch {
            sheet = nil
            problem = "\(error)"
        }
    }

    /// `rows` made in source `name`: a file written, the session's undo taking it back; or rows
    /// written inline, the source's.
    private func change(_ name: String, _ rows: [RowEdit], file: String?) {
        let session = editor.session
        let made = { () throws -> Void in
            let edited = try session.dataEdit(name, rows)
            guard edited.edited else {
                throw Refused(description: "\(name): \(edited.why.first ?? "nothing changed")")
            }
        }
        problem = nil
        if file == nil {
            edits.beside {
                try made()
                return []
            }
        } else {
            edits.data(made)
        }
    }
}

/// A cell: its value as written, set when typed; marked where its column does not read it.
private struct Cell: View {
    let value: String
    let why: String?
    let set: (String) -> Void
    @State private var typed = ""

    var body: some View {
        TextField("", text: $typed)
            .frame(minWidth: 70, idealWidth: 100)
            .overlay(RoundedRectangle(cornerRadius: 3).strokeBorder(why == nil ? Color.clear : Color.red))
            .help(why ?? value)
            .onSubmit { if typed != value { set(typed) } }
            .onAppear { typed = value }
            .onChange(of: value) { _, now in typed = now }
    }
}
