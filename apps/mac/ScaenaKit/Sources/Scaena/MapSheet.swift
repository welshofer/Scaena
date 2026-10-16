import ScaenaKit
import SwiftUI

/// What fetches a map again from the Format tab's Map section (PLAN O.1d), as the browser's does:
/// each one fetch, one step to undo.
enum MapRemap: String, CaseIterable, Identifiable {
    case nearer = "in"
    case further = "out"
    case pin
    case again

    var id: String { rawValue }

    var title: String {
        switch self {
        case .nearer: "Zoom In"
        case .further: "Zoom Out"
        case .pin: "Center on the Pin"
        case .again: "Fetch Again"
        }
    }

    var help: String {
        switch self {
        case .nearer: "Fetched again a step nearer, about where it is centered"
        case .further: "Fetched again a step further, about where it is centered"
        case .pin: "Fetched again centered on the place its pin marks"
        case .again: "Fetched again as it was asked"
        }
    }

    /// What the window says once it is fetched so.
    var did: String {
        switch self {
        case .nearer: "a step nearer"
        case .further: "a step further"
        case .pin: "centered on its pin"
        case .again: "again"
        }
    }
}

/// Insert › Map… (ADR-0030, PLAN O.1d), as the browser's sheet: Where (an address, a place, or a
/// latitude and longitude, searched on Return), how much it shows, and the pin's label. A name that
/// matches several places offers up to five to pick from. The map is fetched from Geoapify with the
/// user's own key, kept in the Keychain: without one, the sheet asks for one and keeps it. The
/// map's picture joins the bundle and goes where it was asked for, its pin and its credit with it,
/// one step to undo. Cancel, or the sheet put away, drops a map still coming.
struct MapSheet: View {
    let target: MapTarget
    /// The map fetched, put where it was asked for with the pin's label typed, if one was.
    let put: (FetchedMap, String?) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var place = ""
    @State private var show: MapShow = .neighborhood
    @State private var label = ""
    @State private var places: [MapPlace] = []
    @State private var status = ""
    @State private var typedKey = ""
    @State private var kept: String?
    @State private var fetching: Task<Void, Never>?
    @FocusState private var focused: Bool

    private let keychain = Keychain()

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Insert a Map")
                .font(.headline)
            if kept == nil {
                SecureField("Your Geoapify key", text: $typedKey)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityLabel("Geoapify key")
                    .accessibilityIdentifier("map-key")
                Link("Get a Geoapify key", destination: URL(string: "https://myprojects.geoapify.com/")!)
                    .font(.caption)
            }
            TextField("An address, a place, or a latitude and longitude", text: $place)
                .textFieldStyle(.roundedBorder)
                .focused($focused)
                .onSubmit { fetch() }
                .onChange(of: place) { places = [] }
                .accessibilityLabel("Where")
                .accessibilityIdentifier("map-where")
            Picker("Show", selection: $show) {
                ForEach(MapShow.allCases) { show in Text(show.name).tag(show) }
            }
            TextField("The place's name", text: $label)
                .textFieldStyle(.roundedBorder)
                .accessibilityLabel("Label")
                .accessibilityIdentifier("map-label")
            if !places.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(Array(places.prefix(5).enumerated()), id: \.element.id) { n, found in
                        Button(found.address) { fetch(pick: n) }
                            .help(found.kind ?? found.address)
                            .accessibilityIdentifier("map-place-\(n)")
                    }
                }
            }
            Text(goes)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text("What you type, and the map's area, go to Geoapify under its terms. The map's credit is drawn with it.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if !status.isEmpty {
                Text(status)
                    .font(.callout)
                    .accessibilityIdentifier("map-status")
            }
            HStack {
                Spacer()
                Button("Cancel") {
                    fetching?.cancel()
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)
                Button("Insert") { fetch() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(fetching != nil || place.trimmingCharacters(in: .whitespaces).isEmpty)
                    .accessibilityIdentifier("map-insert")
            }
        }
        .padding(20)
        .frame(minWidth: 420)
        .onAppear {
            kept = keychain.geoapify
            focused = true
        }
        // Put away while a map is coming: it is dropped, and nothing is put anywhere.
        .onDisappear { fetching?.cancel() }
    }

    /// Where the map goes.
    private var goes: String {
        if target.node != nil { return "It takes the place of the picture selected." }
        if target.slot != nil { return "It fills the empty place for a picture on this slide." }
        return "It goes on this slide where you last clicked."
    }

    /// Fetch the map: of the place picked from those offered, `pick`, or of what Where says.
    private func fetch(pick: Int? = nil) {
        let asked = place.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !asked.isEmpty, fetching == nil else { return }
        var key = kept
        if key == nil {
            let typed = typedKey.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !typed.isEmpty else {
                status = "Geoapify needs a key of your own, from geoapify.com: paste it above."
                return
            }
            do {
                try keychain.setGeoapify(typed)
            } catch {
                status = Why.said(error)
                return
            }
            kept = typed
            key = typed
        }
        guard let key else { return }
        let ask = MapAsk(place: asked, show: show, target: target, pick: pick)
        let typedLabel = label.trimmingCharacters(in: .whitespacesAndNewlines)
        status = pick == nil && asked.first?.isNumber != true ? "Finding the place…" : "Fetching the map…"
        fetching = Task { @MainActor in
            defer { fetching = nil }
            do {
                let answer = try await Generator().map(ask, key: key)
                guard !Task.isCancelled else { return }
                switch answer {
                case .places(let found):
                    places = found
                    status = "“\(asked)” matches \(found.count) places: pick one."
                case .map(let map):
                    put(map, typedLabel.isEmpty ? nil : typedLabel)
                    dismiss()
                }
            } catch {
                guard !Task.isCancelled else { return }
                let why = Why.said(error)
                // A key Geoapify did not take is forgotten here, so the next try asks for another.
                if why.contains("401") || why.contains("403") || why.localizedCaseInsensitiveContains("key") {
                    kept = nil
                    typedKey = ""
                }
                status = "No map was fetched. \(why)"
            }
        }
    }
}
