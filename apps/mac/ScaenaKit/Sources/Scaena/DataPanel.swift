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
                    "No Data", systemImage: "tablecells",
                    description: Text("Paste cells from a spreadsheet, or drop a CSV or JSON file on a slide."))
            } else {
                // Each source as a person names it (PLAN 3.18), as the inspector names a chart's:
                // by its file's name in words, never by the name the deck writes it with.
                Picker("Source", selection: $chosen) {
                    ForEach(sources) { source in
                        Text(Words.source(source.name, file: source.file)).tag(Optional(source.name))
                    }
                }
                .accessibilityIdentifier("data-source")
                .padding(.horizontal, 8)
                if let sheet, let name = chosen {
                    // Where its rows are kept, and how many, as the browser's Data tab says it.
                    Text(Self.kept(sheet))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 8)
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

    /// Where a source's rows are kept, its file by name or the deck itself, and how many there are.
    private static func kept(_ sheet: Sheet) -> String {
        let rows = sheet.rows.count == 1 ? "1 row" : "\(sheet.rows.count) rows"
        guard let file = sheet.file else { return "Written in the deck · \(rows)" }
        return "\(file.split(separator: "/").last.map(String.init) ?? file) · \(rows)"
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
                            Text(Words.column(column.type)).font(.caption2).foregroundStyle(.secondary)
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
            problem = Why.said(error)
        }
    }

    /// `rows` made in source `name`: a file written, the session's undo taking it back; or rows
    /// written inline, the source's.
    private func change(_ name: String, _ rows: [RowEdit], file: String?) {
        let session = editor.session
        let made = { () throws -> Void in
            let edited = try session.dataEdit(name, rows)
            guard edited.edited else {
                let source = Words.source(name, file: file)
                throw Refused("\(source) not changed", edited.refusals, by: editor, otherwise: "it reads so already")
            }
        }
        problem = nil
        if file == nil {
            edits.beside("Data Change") {
                try made()
                return []
            }
        } else {
            edits.data("Data Change", made)
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
