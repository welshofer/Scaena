import CoreGraphics
import Foundation

/// Where a map goes (ADR-0030, PLAN O.1d), as the engine says (`scaena_ops::maps::Target`): the
/// image `node`, which shows it in place of what it showed; the empty place for a picture `slot`;
/// or neither, a new image at `rect`. The map is asked for at the box's size, in the `style` the
/// theme's surface asks (`light` or `dark`), its places named in the deck's language.
public struct MapTarget: Codable, Sendable, Equatable, Identifiable {
    public let state: String
    public let node: String?
    public let slot: String?
    public let rect: [Double]
    public let style: String
    public let lang: String?

    /// What it is to a sheet presented for it: the image, the place, or the box.
    public var id: String { node ?? slot ?? rect.map { "\($0)" }.joined(separator: ",") }

    /// The box's size, canvas units.
    public var size: CGSize { rect.count == 4 ? CGSize(width: rect[2], height: rect[3]) : .zero }
}

/// How much of the world a map shows, as the sheet offers it: a street's block, a neighborhood, a
/// city, or a region.
public enum MapShow: String, CaseIterable, Sendable, Identifiable {
    case block, neighborhood, city, region

    public var id: String { rawValue }

    /// Its name, as the sheet says it.
    public var name: String {
        switch self {
        case .block: "A Block"
        case .neighborhood: "A Neighborhood"
        case .city: "A City"
        case .region: "A Region"
        }
    }
}

/// A place a name matched, to pick one of: its name, its address in words, where it is, and what
/// it is (a city, a street, an amenity), as the geocoder says.
public struct MapPlace: Codable, Sendable, Equatable, Identifiable {
    public let name: String
    public let address: String
    public let lat: Double
    public let lon: Double
    public let kind: String?

    public var id: String { "\(address) \(lat),\(lon)" }
}

/// A map fetched: its record, where its place stands in the picture (fractions of it), the place's
/// name, and the picture, a PNG.
public struct FetchedMap: Sendable, Equatable {
    public let fetched: JSONValue
    public let pin: [Double]
    public let place: String
    public let png: Data
}

/// What a map asked for came to: the places a name matched, to pick one of, or the map.
public enum MapAnswer: Sendable, Equatable {
    case places([MapPlace])
    case map(FetchedMap)
}

/// What a map is asked for, as the browser's sheet asks (`scaena_objects::map::Asked`): what the
/// person typed, or a refresh's place known already and its center; how much it shows, or a zoom;
/// the style; the box's size; the deck's language; and which of several places found.
public struct MapAsk: Sendable, Equatable {
    public var place: String
    public var known: JSONValue = nil
    public var center: JSONValue = nil
    public var zoom: Double?
    public var show: MapShow?
    public var style: String
    public var size: CGSize
    public var lang: String?
    public var pick: Int?
    /// Only the places the name matches, to choose from: no picture is asked for.
    public var list = false

    public init(place: String, show: MapShow? = .neighborhood, target: MapTarget, pick: Int? = nil) {
        self.place = place
        self.show = show
        self.style = target.style
        self.size = target.size
        self.lang = target.lang
        self.pick = pick
    }

    /// A refresh's ask, as `mapRefreshing` says it: what was asked, the place known, `center` and
    /// `zoom` as the person moved them.
    public init(again: MapRefresh, zoom: Double, center: JSONValue) {
        self.place = again.asked
        self.known = again.place
        self.center = center
        self.zoom = zoom
        self.show = nil
        self.style = again.style
        self.size = again.target.size
        self.lang = again.target.lang
    }

    var json: JSONValue {
        var o: [String: JSONValue] = [
            "place": .string(place), "style": .string(style),
            "size": .array([.number(Double(size.width)), .number(Double(size.height))]),
        ]
        if known != .null { o["known"] = known }
        if center != .null { o["center"] = center }
        if let zoom { o["zoom"] = .number(zoom) }
        if let show, zoom == nil { o["show"] = .string(show.rawValue) }
        if let lang { o["lang"] = .string(lang) }
        if let pick { o["pick"] = .number(Double(pick)) }
        if list { o["list"] = .bool(true) }
        return .object(o)
    }
}

/// What fetching an image's map again asks for (`scaena_ops::maps::Refresh`): its target, what was
/// asked first, its place, its center and zoom, its style, and its pin's label.
public struct MapRefresh: Decodable, Sendable, Equatable {
    public let target: MapTarget
    public let asked: String
    public let place: JSONValue
    public let center: JSONValue
    public let zoom: Double
    public let style: String
    public let label: String?

    /// The place's latitude and longitude, where its pin stands, as a center.
    public var pinned: JSONValue {
        .object(["lat": place["lat"] ?? .null, "lon": place["lon"] ?? .null])
    }
}

/// A map put where it was asked for: the patch, one step to undo, the image it went to, the place,
/// and what the window says of it.
public struct MapPut: Sendable {
    public let patch: [JSONValue]
    public let node: String
    /// Its picture's path in the bundle.
    public let src: String
    public let place: String
    public let said: String
}

extension Generator {
    private struct MapStep: Decodable {
        struct Done: Decodable {
            let fetched: JSONValue
            let pin: [Double]
            let place: String
            let png: Data
        }

        let request: GeneratorRequest?
        let choose: [MapPlace]?
        let map: Done?
    }

    /// The map `ask` asks for, fetched from Geoapify (ADR-0030, PLAN O.1d): the engine builds each
    /// request with `key`, the user's Geoapify key, in its query alone, and reads each answer; the
    /// places a name matched where it matched several, else the map. Why none came, in a person's
    /// words (PLAN 3.18): the key not taken, Geoapify busy, no way to it.
    public func map(_ ask: MapAsk, key: String, now: @Sendable () -> Date = { Date() }) async throws -> MapAnswer {
        var next: MapStep = try step("map", ["ask": ask.json, "key": .string(key)])
        while let request = next.request {
            let reply = await send(request)
            guard reply.status != 0 else {
                throw ScaenaError(message: "Geoapify could not be reached: \(reply.text)")
            }
            next = try step(
                "mapAnswered",
                [
                    "status": .number(Double(reply.status)), "kind": .string(reply.kind), "key": .string(key),
                    "at": .string(now().formatted(.iso8601)),
                ], bytes: reply.body)
        }
        if let places = next.choose { return .places(places) }
        guard let done = next.map else { throw ScaenaError(message: "no map came back") }
        return .map(FetchedMap(fetched: done.fetched, pin: done.pin, place: done.place, png: done.png))
    }
}

extension ScaenaSession {
    /// Where a map for `state` goes (PLAN O.1d), as the browser's canvas finds it, where a picture
    /// made goes: the image selected, alone, which it takes the place of; the slide's empty place
    /// for a picture; or about `pressed`, a box a third of the canvas wide, at 16:9.
    public func mapTarget(state: String, selected: [String], pressed: CGPoint?) throws -> MapTarget {
        let place = try pictureTarget(state: state, selected: selected, pressed: pressed)
        let rect: [Double]
        if place.node != nil || place.slot != nil {
            rect = [
                place.point.x - place.size.width / 2, place.point.y - place.size.height / 2, place.size.width,
                place.size.height,
            ].map(Double.init)
        } else {
            let width = Double(try canvasSize().width) / 3
            let height = width * 9 / 16
            rect = [
                (Double(place.point.x) - width / 2).rounded(), (Double(place.point.y) - height / 2).rounded(),
                width.rounded(), height.rounded(),
            ]
        }
        var args: [String: JSONValue] = ["state": .string(state), "rect": .array(rect.map(JSONValue.number))]
        if let node = place.node { args["node"] = .string(node) }
        if let slot = place.slot { args["slot"] = .string(slot) }
        return try call("mapTarget", .object(args))
    }

    private struct Mapped: Decodable {
        let node: String
        let src: String
        let place: String
        let said: String
        let ops: [JSONValue]
    }

    /// The map `map` put where `target` says (PLAN O.1d): its picture joins the bundle, named by its
    /// content, and the image shows it with its record and its pin, labeled `label` (the place's
    /// name where none is given, none where it is empty), or a new image does, entering in the
    /// state and kept to its slide. One patch, one step to undo.
    public func mapping(_ map: FetchedMap, at target: MapTarget, label: String?) throws -> MapPut {
        var args: [String: JSONValue] = [
            "target": try JSONValue.encoded(target), "fetched": map.fetched,
            "pin": .array(map.pin.map(JSONValue.number)), "png": .string(map.png.base64EncodedString()),
        ]
        if let label { args["label"] = .string(label) }
        let mapped: Mapped = try call("mapping", .object(args))
        return MapPut(patch: mapped.ops, node: mapped.node, src: mapped.src, place: mapped.place, said: mapped.said)
    }

    /// What fetching image `node`'s map again in `state` asks for (PLAN O.1d), at `zoom`, or
    /// centered on `lat`, `lon`, where they are given.
    public func mapRefreshing(state: String, node: String, zoom: Double? = nil, lat: Double? = nil, lon: Double? = nil)
        throws -> MapRefresh
    {
        var args: [String: JSONValue] = ["state": .string(state), "node": .string(node)]
        if let zoom { args["zoom"] = .number(zoom) }
        if let lat, let lon {
            args["lat"] = .number(lat)
            args["lon"] = .number(lon)
        }
        return try call("mapRefreshing", .object(args))
    }
}

extension JSONValue {
    /// `value` as JSON.
    static func encoded<T: Encodable>(_ value: T) throws -> JSONValue {
        try JSONDecoder().decode(JSONValue.self, from: JSONEncoder().encode(value))
    }
}
