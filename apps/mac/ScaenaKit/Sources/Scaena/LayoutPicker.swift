import ScaenaKit
import SwiftUI

/// The theme's layouts to choose from, as a presentation app's layout chooser shows them (PLAN
/// 3.31): the sections down the side, and each layout of the one chosen drawn as it is used, a
/// picture where a picture goes and a chart or a table where a figure goes, under the name a person
/// calls it; and a field that finds layouts in every section by their names, what they are for,
/// and their sections. The layouts shown are drawn first, a few at a time, then the rest, each
/// kept for the next time the picker opens. Add Slide starts a slide in the layout chosen, or a
/// blank one, or a step; the inspector gives the slide shown the layout chosen, of those it may
/// take, the one it takes outlined.
struct LayoutPicker: View {
    enum Purpose: Equatable {
        /// A slide to start after the slide shown: in any layout, or blank.
        case start
        /// The slide shown's layout: one of `names`, the layouts it may take; `current` the one it
        /// takes now.
        case change(names: [String], current: String?)
    }

    let editor: DeckEditor
    /// The state shown.
    let state: String
    let purpose: Purpose
    /// A layout chosen; nil a blank slide.
    let choose: (String?) -> Void
    /// A step of the slide shown, where a slide is started.
    var step: (() -> Void)? = nil

    /// Every layout the picker offers, in the theme's order, listed before any is drawn.
    @State private var offered: [Starter] = []
    /// The section shown, where nothing is typed to find.
    @State private var section: String?
    @State private var query = ""
    /// Each layout's picture drawn so far, by its layout.
    @State private var pictures: [String: CGImage] = [:]

    /// The section shown last, which a picker opened to start a slide shows again.
    @MainActor private static var shownLast: String?
    /// How high each slide is painted, pixels: a tile's height on a Retina screen.
    private static let painted = 180
    /// How many slides are drawn between two answers to a person.
    private static let batch = 3

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            HStack(spacing: 0) {
                sidebar
                Divider()
                grid
            }
            if purpose == .start {
                Divider()
                footer
            }
        }
        .frame(minWidth: 560, idealWidth: 780, maxWidth: 780, minHeight: 420, idealHeight: 560, maxHeight: 560)
        .task(id: "\(editor.revision)\u{1f}\(editor.format ?? "")") { list() }
        .task(id: "\(shown.map(\.id))\u{1f}\(editor.format ?? "")\u{1f}\(offered.count)") { await draw() }
    }

    private var title: String {
        purpose == .start ? "Add a Slide" : "Layout"
    }

    private var header: some View {
        HStack(spacing: 12) {
            Text(title)
                .font(.headline)
            Spacer()
            HStack(spacing: 4) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                TextField("Find a Layout", text: $query)
                    .textFieldStyle(.plain)
                    .accessibilityLabel("Find a layout")
                    .accessibilityIdentifier("layouts-find")
                if !query.isEmpty {
                    Button {
                        query = ""
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Clear")
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background(Color.secondary.opacity(0.12), in: RoundedRectangle(cornerRadius: 6))
            .frame(width: 240)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
    }

    /// The sections, each with how many layouts it offers: a click shows it, and the arrow keys go
    /// up and down them.
    private var sidebar: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 2) {
                ForEach(sections, id: \.self) { title in
                    let chosen = query.isEmpty && title == section
                    Button {
                        show(title)
                    } label: {
                        HStack {
                            Text(title)
                            Spacer()
                            Text("\(count(title))")
                                .monospacedDigit()
                                .foregroundStyle(.secondary)
                        }
                        .padding(.horizontal, 8)
                        .padding(.vertical, 5)
                        .background(
                            chosen ? Color.accentColor.opacity(0.22) : Color.clear,
                            in: RoundedRectangle(cornerRadius: 6)
                        )
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(title)
                    .accessibilityValue(count(title) == 1 ? "1 layout" : "\(count(title)) layouts")
                    .accessibilityAddTraits(chosen ? .isSelected : [])
                    .accessibilityIdentifier("section-\(Self.slug(title))")
                }
            }
            .padding(8)
        }
        .frame(width: 190)
        .focusable()
        .onKeyPress(.upArrow) { move(by: -1) }
        .onKeyPress(.downArrow) { move(by: 1) }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Sections")
    }

    private var grid: some View {
        ScrollView {
            if shown.isEmpty {
                Text(offered.isEmpty ? "" : query.isEmpty ? "No layouts here" : "No layout matches “\(query)”")
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity)
                    .padding(.top, 40)
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 12)], spacing: 14) {
                ForEach(shown) { one in tile(one) }
            }
            .padding(12)
        }
        .overlay {
            if offered.isEmpty { ProgressView() }
        }
    }

    /// A blank slide, and a step of the slide shown.
    private var footer: some View {
        HStack {
            Button("Blank Slide") { choose(nil) }
                .help("A slide in no layout, with nothing on it")
                .accessibilityIdentifier("start-blank")
            Spacer()
            if let step {
                Button("New Step on This Slide", action: step)
                    .accessibilityIdentifier("add-step")
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
    }

    private func tile(_ one: Starter) -> some View {
        let name = one.name ?? one.layout.map(Words.phrase) ?? "Blank"
        let current = one.layout != nil && one.layout == taken
        return Button {
            choose(one.layout)
        } label: {
            VStack(spacing: 4) {
                ZStack {
                    Rectangle().fill(Color.secondary.opacity(0.08))
                    if let image = pictures[one.id] {
                        Image(decorative: image, scale: 1)
                            .resizable()
                            .aspectRatio(contentMode: .fit)
                    } else {
                        ProgressView().controlSize(.small)
                    }
                }
                .aspectRatio(aspect, contentMode: .fit)
                .clipShape(RoundedRectangle(cornerRadius: 4))
                .overlay {
                    RoundedRectangle(cornerRadius: 4)
                        .strokeBorder(
                            current ? Color.accentColor : Color.secondary.opacity(0.3), lineWidth: current ? 2.5 : 1)
                }
                Text(name)
                    .font(.caption)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .foregroundStyle(current ? .primary : .secondary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(one.description ?? name)
        .accessibilityLabel(name)
        .accessibilityHint(one.description ?? "")
        .accessibilityAddTraits(current ? .isSelected : [])
        .accessibilityIdentifier(purpose == .start ? "start-\(one.layout ?? "blank")" : "layout-\(one.layout ?? "")")
    }

    // MARK: What it offers

    /// The layout the slide shown takes, where the picker changes it.
    private var taken: String? {
        if case .change(_, let current) = purpose { return current }
        return nil
    }

    /// The sections, in the order the theme first names each; a layout in none in `Other`.
    private var sections: [String] {
        var out: [String] = []
        for one in offered where one.layout != nil {
            let title = one.group ?? "Other"
            if !out.contains(title) { out.append(title) }
        }
        return out
    }

    private func count(_ title: String) -> Int {
        offered.count(where: { $0.layout != nil && ($0.group ?? "Other") == title })
    }

    /// The layouts shown: those found by what is typed, in every section, or the section's.
    private var shown: [Starter] {
        let named = offered.filter { $0.layout != nil }
        let words = query.trimmingCharacters(in: .whitespaces)
        if !words.isEmpty { return named.filter { $0.matches(words) } }
        return named.filter { ($0.group ?? "Other") == section }
    }

    /// How wide a slide is for how high: the deck's, as a picture drawn says; until one is, 16:9.
    private var aspect: CGFloat {
        guard let one = pictures.values.first, one.height > 0 else { return 16 / 9 }
        return CGFloat(one.width) / CGFloat(one.height)
    }

    private func show(_ title: String) {
        section = title
        query = ""
        if purpose == .start { Self.shownLast = title }
    }

    private func move(by step: Int) -> KeyPress.Result {
        guard let at = sections.firstIndex(where: { $0 == section }) else { return .ignored }
        let next = at + step
        guard sections.indices.contains(next) else { return .handled }
        show(sections[next])
        return .handled
    }

    /// The layouts the theme offers, as the session lists them, and the section to show first: the
    /// one the slide's layout is in, or the one shown last, or the first.
    private func list() {
        var all = (try? editor.session.starters()) ?? []
        if case .change(let names, _) = purpose {
            all = all.filter { one in one.layout.map { names.contains($0) } ?? false }
        }
        offered = all
        let titles = sections
        if let section, titles.contains(section) { return }
        let mine = all.first { $0.layout != nil && $0.layout == taken }.map { $0.group ?? "Other" }
        section = mine ?? Self.shownLast.flatMap { titles.contains($0) ? $0 : nil } ?? titles.first
    }

    /// Each picture shown: those kept at once, then the rest drawn a few at a time, then those of
    /// the other sections, so another shows at once when it is chosen. Cancelled, it stops where it
    /// is, the next round going on from there.
    private func draw() async {
        let wanted = shown.map(\.layout)
        for one in editor.startersKept(wanted, height: Self.painted) { pictures[one.starter.id] = one.image }
        let rest = offered.map(\.layout).filter { layout in layout != nil && !wanted.contains(layout) }
        let todo = (wanted + rest).filter { pictures[$0 ?? ""] == nil }
        var at = 0
        while at < todo.count {
            try? await Task.sleep(for: .milliseconds(1))
            if Task.isCancelled { return }
            let batch = Array(todo[at..<min(at + Self.batch, todo.count)])
            at += batch.count
            for one in editor.starters(batch, after: state, height: Self.painted) {
                pictures[one.starter.id] = one.image
            }
        }
    }

    /// A section's name as an identifier's part: `Titles` as `titles`.
    static func slug(_ title: String) -> String {
        String(title.lowercased().map { $0.isLetter || $0.isNumber ? $0 : "-" })
    }
}
