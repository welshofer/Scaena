import ScaenaKit
import SwiftUI

/// The window's words (PLAN 3.18): what a person reads where the deck's format has names of its
/// own. A slide is numbered and a step is its slide's; a node is what it shows; a property is what
/// a presentation app calls it; a value is said, a time in seconds. No id, path, code, or
/// millisecond reaches the window through here. The browser says the same through
/// `scaena_core::words`, and `crates/scaena-core/tests/words.rs` holds these tables to its.
enum Words {
    /// The device the window is on, as a person calls it: what the on-device model answers on
    /// (PLAN 3.6, 4.9).
    static var device: String {
        #if os(macOS)
        "Mac"
        #else
        "iPad"
        #endif
    }

    /// A property of a node, of characters, or of the state, as the inspector labels it.
    static func label(_ prop: String) -> String {
        labels[prop] ?? phrase(prop.split(separator: "/").last.map(String.init) ?? prop)
    }

    private static let labels: [String: String] = [
        "role": "Text Style", "emphasis": "Emphasis", "style/family": "Font", "style/weight": "Weight",
        "style/italic": "Italic", "style/size": "Size", "style/color": "Color", "style/case": "Capitalization",
        "style/tracking": "Character Spacing", "align/x": "Alignment", "fit": "Fit", "wrap": "Line Breaks",
        "maxLines": "Most Lines", "mask": "Mask", "shadow": "Shadow",
        "adjust/exposure": "Exposure", "adjust/contrast": "Contrast", "adjust/saturation": "Saturation",
        "adjust/warmth": "Warmth",
        "opacity": "Opacity", "transform/rotate": "Rotation", "enter": "Build In", "exit": "Build Out",
        "alt": "Description", "semantic": "Part in the Story", "focal": "Focal Point", "crop": "Crop",
        "radius": "Corners", "fill": "Fill", "stroke/paint": "Border", "stroke/width": "Border Width",
        "connect/route": "Connection", "ends/start": "Line Start", "ends/end": "Line End",
        "data": "Data", "kind": "Chart", "orient": "Bars Run", "x/field": "Across", "x/type": "Across Are",
        "y/field": "Up", "y/type": "Up Are", "series/field": "Series", "color/field": "Color By",
        "sizeEncoding/field": "Size By", "interval/low": "Low End", "interval/high": "High End",
        "facet/field": "Small Multiples By", "key": "Rows Match By",
        "labels/show": "Values Shown", "labels/role": "Value Style", "header": "Header Row", "preset": "Effect",
        "palette": "Colors", "axis": "Direction", "gap": "Spacing", "layout": "Layout",
        "transition/duration": "Duration", "transition/ease": "Easing", "transition/spring": "Spring",
        "transition/match": "Magic Move", "transition/style": "Transition", "transition/direction": "Direction",
        "hold": "Advance", "notes": "Presenter Notes",
    ]

    /// A chart's property as its kind of chart reads it (`kind`, the chart's): a donut's `x` names
    /// its slices and its `y` sizes them, where a chart with axes reads them across and up; bars
    /// that run across (`horizontal`, PLAN 1.29) name their categories down the side and their
    /// values across, as the browser says them.
    static func label(_ prop: String, chart kind: String?, horizontal: Bool = false) -> String {
        if kind == "donut", let said = donut[prop] { return said }
        if horizontal, kind == "bar" || kind == "stackedBar", let said = across[prop] { return said }
        return label(prop)
    }

    /// A donut's channels, which no axis runs along.
    private static let donut: [String: String] = [
        "x/field": "Slices", "x/type": "Slices Are", "y/field": "Values", "y/type": "Values Are",
    ]

    /// The channels of bars that run across: the categories down the side, the values across.
    private static let across: [String: String] = [
        "x/field": "Down the Side", "x/type": "Down the Side Are", "y/field": "Across", "y/type": "Across Are",
    ]

    /// A font's weights by their names, Thin to Black, as a presentation app names them.
    static let weights: [(value: Int, name: String)] = [
        (100, "Thin"), (200, "Extra Light"), (300, "Light"), (400, "Regular"), (500, "Medium"), (600, "Semibold"),
        (700, "Bold"), (800, "Extra Bold"), (900, "Black"),
    ]

    /// A weight by its name, or by its number where it falls between two.
    static func weight(_ value: Int) -> String {
        weights.first { $0.value == value }?.name ?? "\(value)"
    }

    /// One of the words a property takes, said: `cover` as an image's Fill; a data source, which the
    /// deck writes `@name`, by the file `sources` say it is in.
    static func word(_ word: String, of prop: String, type: String?, sources: [DataSource] = []) -> String {
        switch (prop, word) {
        case ("fit", "cover"): return "Fill"
        case ("mask", "ellipse"): return "Oval"
        case ("fit", "contain"): return "Fit"
        case ("fit", "fill"): return "Stretch"
        case ("fit", "wrap"): return "Wrap"
        case ("fit", "shrink"): return "Shrink to Fit"
        case ("fit", "grow"): return "Grow to Fill"
        case ("fit", "clip"): return "Cut Off"
        case ("fit", "error"): return "Flag It"
        case ("wrap", "greedy"): return "Plain"
        case ("wrap", "pretty"): return "Even"
        case ("wrap", "balance"): return "Balanced"
        case ("align/x", "start"): return "Left"
        case ("align/x", "center"): return "Center"
        case ("align/x", "end"): return "Right"
        case ("style/case", "none"): return "As Typed"
        case ("style/case", "upper"): return "All Caps"
        case ("style/case", "lower"): return "Lowercase"
        case ("style/case", "title"): return "Title Case"
        case ("style/case", "smallcaps"): return "Small Caps"
        case ("transition/match", "id"): return "On"
        case ("transition/match", "none"): return "Off"
        case ("transition/style", "dissolve"): return "Dissolve"
        case ("transition/style", "push"): return "Push"
        case ("transition/style", "cover"): return "Move In"
        case ("transition/style", "reveal"): return "Reveal"
        case ("transition/style", "wipe"): return "Wipe"
        case ("transition/direction", "left"): return "Right to Left"
        case ("transition/direction", "right"): return "Left to Right"
        case ("transition/direction", "up"): return "Bottom to Top"
        case ("transition/direction", "down"): return "Top to Bottom"
        case ("axis", "x"): return "Across"
        case ("axis", "y"): return "Down"
        case ("labels/show", "auto"): return "Automatic"
        case ("labels/show", "ends"): return "First and Last"
        case ("connect/route", "straight"): return "Straight"
        case ("connect/route", "elbow"): return "Corner"
        case ("connect/route", "curved"): return "Curved"
        case ("ends/start", "none"): return "None"
        case ("ends/start", "arrow"): return "Arrow"
        case ("ends/start", "open"): return "Open Arrow"
        case ("ends/start", "circle"): return "Circle"
        case ("ends/start", "square"): return "Square"
        case ("ends/start", "diamond"): return "Diamond"
        case ("ends/end", "none"): return "None"
        case ("ends/end", "arrow"): return "Arrow"
        case ("ends/end", "open"): return "Open Arrow"
        case ("ends/end", "circle"): return "Circle"
        case ("ends/end", "square"): return "Square"
        case ("ends/end", "diamond"): return "Diamond"
        case (_, "quantitative"): return "Numbers"
        case (_, "ordinal"): return "In Order"
        case (_, "nominal"): return "Names"
        case (_, "temporal"): return "Dates"
        default:
            if word.hasPrefix("@") {
                let name = String(word.dropFirst())
                return source(name, file: sources.first(where: { $0.name == name })?.file)
            }
            return phrase(word)
        }
    }

    /// Data source `name` as a person names it (PLAN 3.18), never by the id the deck writes it with:
    /// by the name of its `file` in words, `data/trails-funding.csv` as Trails Funding; rows written
    /// in the deck, by its own name in words. The browser says it so (`scaena_core::words::source`).
    static func source(_ name: String, file: String?) -> String {
        if let file {
            let leaf = file.split(separator: "/", omittingEmptySubsequences: false).last.map(String.init) ?? file
            let stem = leaf.lastIndex(of: ".").map { String(leaf[..<$0]) } ?? leaf
            let said = phrase(stem)
            if !said.isEmpty { return said }
        }
        return phrase(name)
    }

    /// One of the theme's names of a kind (`of`), said: a color by what it is for, a font by its
    /// family, a duration with its seconds, a step of a scale by its size.
    static func name(_ name: String, of kind: String, theme: ThemeLook?) -> String {
        switch kind {
        case "color":
            return colors[name] ?? phrase(name)
        case "font-family":
            return theme?.family(name) ?? phrase(name)
        case "duration":
            guard let ms = theme?.duration(name) else { return phrase(name) }
            return "\(phrase(name)) (\(seconds(ms)))"
        case "easing":
            return easings[name] ?? phrase(name)
        case "motion-preset":
            return presets[name] ?? phrase(name)
        case "radius", "space":
            guard let size = theme?.step(name) else { return phrase(name) }
            return size == 0 ? "None" : "\(number(size)) pt"
        case "layout":
            return theme?.layout(name) ?? phrase(name)
        default:
            return phrase(name)
        }
    }

    private static let colors: [String: String] = [
        "surface": "Background", "surface-2": "Background 2", "onSurface": "Text", "onSurfaceMuted": "Muted Text",
        "onAccent": "Text on Accent",
    ]

    private static let easings: [String: String] = ["in": "Ease In", "out": "Ease Out"]

    /// The shipped themes' builds whose names say them in fewer words than a person would (PLAN 2.112).
    private static let presets: [String: String] = [
        "words": "Word by Word", "bullets": "By Bullet", "fadeScale": "Fade and Scale", "scaleBig": "Scale Big",
        "moveInLeft": "Move In from Left", "moveInRight": "Move In from Right", "moveInTop": "Move In from Top",
        "moveInBottom": "Move In from Bottom",
    ]

    /// A build as an object takes it (`motion`: `enter`, `emphasis`, or `exit`), as the browser says
    /// it: by its name, and as the object leaves by Build Out's (PLAN 2.116), the entrance played
    /// back, `moveInLeft` as Move Out to Left and `appear` as Disappear.
    static func build(_ preset: String, as motion: String) -> String {
        if motion == "exit", let said = exits[preset] { return said }
        return name(preset, of: "motion-preset", theme: nil)
    }

    /// The builds Build Out names otherwise than Build In, each as it leaves; one not here, Pop or
    /// Spin, is said as it enters.
    private static let exits: [String: String] = [
        "appear": "Disappear", "fade": "Fade Out", "rise": "Sink", "words": "Word by Word Out",
        "typewriter": "Typewriter Out", "scale": "Scale Down", "grow": "Shrink", "draw": "Erase",
        "moveInLeft": "Move Out to Left", "moveInRight": "Move Out to Right", "moveInTop": "Move Out to Top",
        "moveInBottom": "Move Out to Bottom",
    ]

    /// A node's kind, as a person names it.
    static func kind(_ type: String?) -> String {
        switch type {
        case "text": "Text"
        case "image": "Image"
        case "chart": "Chart"
        case "table": "Table"
        case "shape": "Shape"
        case "shader": "Background Effect"
        case nil: "Object"
        default: "Group"
        }
    }

    /// A node as a person names it: its words where it has any, the first line, else its kind:
    /// `named`, as the engine names it where it is known (a Star), else its type's.
    static func node(_ type: String?, words: String?, kind named: String? = nil) -> String {
        let line = words?.split(whereSeparator: \.isNewline).first.map(String.init)?.trimmingCharacters(in: .whitespaces)
        guard let line, !line.isEmpty else { return named ?? kind(type) }
        return line.count > 40 ? String(line.prefix(39)) + "…" : line
    }

    /// What Insert adds, as a menu offers it: a text by its style, a shape by its name, a chart or
    /// a table by its data, a picture by its place among the bundle's.
    static func insertion(_ insert: Insert, among all: [Insert]) -> String {
        switch insert.kind {
        case "Shape": return shapes[insert.name] ?? phrase(insert.name)
        case "Image":
            let pictures = all.filter { $0.kind == "Image" }
            let n = (pictures.firstIndex { $0.id == insert.id } ?? 0) + 1
            return pictures.count > 1 ? "Picture \(n)" : "Picture"
        case "Chart", "Table": return "\(insert.kind) of \(insert.name)"
        default: return phrase(insert.name)
        }
    }

    /// Each shape Insert offers, as a presentation app's shape library names it (PLAN 2.106).
    private static let shapes = [
        "rect": "Rectangle", "ellipse": "Oval", "line": "Line", "arrow": "Arrow", "polygon": "Polygon",
        "triangle": "Triangle", "right-triangle": "Right Triangle", "diamond": "Diamond",
        "parallelogram": "Parallelogram", "trapezoid": "Trapezoid", "pentagon": "Pentagon", "hexagon": "Hexagon",
        "octagon": "Octagon", "star": "Star", "chevron": "Chevron", "right-arrow": "Right Arrow",
        "left-arrow": "Left Arrow", "up-arrow": "Up Arrow", "down-arrow": "Down Arrow", "plus": "Plus",
        "callout": "Callout",
    ]

    /// An icon as a person names it (PLAN 2.104): `airplane-tilt` as `Airplane tilt`.
    static func icon(_ name: String) -> String {
        let words = name.replacingOccurrences(of: "-", with: " ")
        return words.prefix(1).uppercased() + words.dropFirst()
    }

    /// What an insert added, as the window says it once it is in: `Title text`, `Rectangle`.
    static func inserted(_ insert: Insert) -> String {
        switch insert.kind {
        case "Text": return "\(phrase(insert.name)) text"
        case "Shape": return shapes[insert.name] ?? phrase(insert.name)
        case "Shader": return "Background effect"
        default: return insert.kind
        }
    }

    /// Where `state` stands among the deck's slides, as a person counts them: `Slide 3`, or
    /// `Slide 3, step 2` for a step after a slide's first.
    static func slide(_ state: String, in slots: [ScaenaSession.Slot]) -> String {
        var number = 0
        var step = 0
        var slide: String?
        for slot in slots {
            if slot.slide != slide {
                number += 1
                step = 1
                slide = slot.slide
            } else {
                step += 1
            }
            if slot.state == state { return step > 1 ? "Slide \(number), step \(step)" : "Slide \(number)" }
        }
        return "Slide"
    }

    /// A theme file as a person names its theme: `themes/dusk.theme.json` as `Dusk`.
    static func theme(_ file: String) -> String {
        var name = (file as NSString).lastPathComponent
        for suffix in [".json", ".theme"] where name.hasSuffix(suffix) {
            name.removeLast(suffix.count)
        }
        return phrase(name)
    }

    /// A bundle's file as a person names it: a font by its family and face, as
    /// `fonts/Inter-Italic-5a81e633895bf83d.ttf` is Inter Italic, without the hash a save adds;
    /// a data file by its name. None for a file a save names by its content alone, as it names a
    /// picture, whose name says nothing to a person.
    static func file(_ path: String, type: String) -> String? {
        let name = (path as NSString).lastPathComponent
        var stem = (name as NSString).deletingPathExtension
        let hex = { (part: Substring) in part.allSatisfy(\.isHexDigit) }
        if stem.count == 64, hex(stem[...]) { return nil }
        guard type == "font" else { return name }
        if let dash = stem.lastIndex(of: "-"), stem[stem.index(after: dash)...].count == 16,
            hex(stem[stem.index(after: dash)...])
        {
            stem = String(stem[..<dash])
        }
        if stem.hasSuffix("-VF") { stem.removeLast(3) }
        return stem.replacingOccurrences(of: "-", with: " ")
    }

    /// What a motion does, as a person says it: `enter` as Enters, and one split by words as Enters
    /// Word by Word.
    static func motion(_ kind: String?, split: String? = nil) -> String {
        let does = ["enter": "Enters", "exit": "Leaves", "emphasis": "Emphasis", "anim": "Moves"][kind ?? ""] ?? phrase(kind ?? "")
        let by = [
            "lines": "Line by Line", "words": "Word by Word", "glyphs": "Letter by Letter",
            "paragraphs": "Bullet by Bullet", "children": "One by One", "marks": "Mark by Mark",
        ][split ?? ""]
        return by.map { "\(does) \($0)" } ?? does
    }

    /// Milliseconds said in seconds: `5.5 s`, `0.18 s`.
    static func seconds(_ ms: Double) -> String {
        "\(number(ms / 1000)) s"
    }

    /// A number as a person writes it: whole where it is, else to two places, no trailing zeros.
    static func number(_ value: Double) -> String {
        if value.rounded() == value, abs(value) < 1e12 { return String(Int64(value)) }
        var text = String(format: "%.2f", value)
        while text.hasSuffix("0") { text.removeLast() }
        if text.hasSuffix(".") { text.removeLast() }
        return text
    }

    /// A name as words: `onSurfaceMuted` as `On Surface Muted`, `art-left` as `Art Left`,
    /// `stackedBar` as `Stacked Bar`.
    static func phrase(_ name: String) -> String {
        var words: [String] = []
        var word = ""
        for ch in name {
            if ch == "-" || ch == "_" || ch == "." || ch == " " {
                if !word.isEmpty { words.append(word) }
                word = ""
            } else if ch.isUppercase, let last = word.last, last.isLowercase || last.isNumber {
                words.append(word)
                word = String(ch)
            } else {
                word.append(ch)
            }
        }
        if !word.isEmpty { words.append(word) }
        return words.map { $0.prefix(1).uppercased() + $0.dropFirst() }.joined(separator: " ")
    }
}

/// What the theme the deck names says of its names (PLAN 3.18): each color's swatch, each font's
/// family, each duration's milliseconds, and each step of its radius and space scales: what the
/// inspector shows beside a name.
struct ThemeLook {
    private let theme: JSONValue

    init?(_ text: ThemeText?) {
        guard let theme = text?.json else { return nil }
        self.theme = theme
    }

    /// A color by its name, or by the role that names one: `accent`, or `onSurface`.
    func color(_ name: String) -> Color? {
        guard let colors = theme["tokens"]?["color"] else { return nil }
        if let hex = colors[name]?.string { return Self.hex(hex) }
        guard let named = theme["tokens"]?["roles"]?[name]?.string, let hex = colors[named]?.string else { return nil }
        return Self.hex(hex)
    }

    /// A color as the theme writes it, by its name or by the role that names one, lowercased: two
    /// names that give one color give the same words.
    func written(_ name: String) -> String? {
        guard let colors = theme["tokens"]?["color"] else { return nil }
        if let value = colors[name]?.string { return value.lowercased() }
        guard let named = theme["tokens"]?["roles"]?[name]?.string else { return nil }
        return colors[named]?.string?.lowercased()
    }

    /// Each role the theme gives a color, by its name, and the name of the color it gives it.
    var roles: [String: String] {
        (theme["tokens"]?["roles"]?.object ?? [:]).compactMapValues(\.string)
    }

    /// A layout by what a person calls it, as the theme names it: `swot` as `SWOT`.
    func layout(_ name: String) -> String? {
        theme["layouts"]?[name]?["name"]?.string
    }

    /// A font by its name in the theme: `display` as `Fraunces`.
    func family(_ name: String) -> String? {
        theme["type"]?["families"]?[name]?["family"]?.string
    }

    /// A duration by its name, milliseconds.
    func duration(_ name: String) -> Double? {
        theme["motion"]?["durations"]?[name]?.number
    }

    /// A step of the radius or the space scale by its name, `radius.2`, its size in canvas units.
    func step(_ name: String) -> Double? {
        let parts = name.split(separator: ".")
        guard parts.count == 2, let i = Int(parts[1]), let scale = theme["tokens"]?[String(parts[0])]?["scale"]?.array,
            scale.indices.contains(i)
        else { return nil }
        return scale[i].number
    }

    /// `color` as `#RRGGBB`, or `#RRGGBBAA` where it is not opaque.
    static func hex(_ color: Color) -> String? {
        guard let srgb = color.srgb else { return nil }
        let byte = { (v: Double) in Int((min(max(v, 0), 1) * 255).rounded()) }
        let rgb = String(format: "#%02X%02X%02X", byte(srgb.red), byte(srgb.green), byte(srgb.blue))
        let alpha = byte(srgb.alpha)
        return alpha == 255 ? rgb : rgb + String(format: "%02X", alpha)
    }

    /// `#rrggbb` or `#rrggbbaa` as a color; none for another kind.
    static func hex(_ value: String) -> Color? {
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
}
