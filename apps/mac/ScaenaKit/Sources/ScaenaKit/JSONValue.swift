import Foundation

/// A JSON value: a call's result before a type is given it, and a call's arguments.
public enum JSONValue: Codable, Equatable, Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    public init(from decoder: any Decoder) throws {
        let value = try decoder.singleValueContainer()
        if value.decodeNil() {
            self = .null
        } else if let b = try? value.decode(Bool.self) {
            self = .bool(b)
        } else if let n = try? value.decode(Double.self) {
            self = .number(n)
        } else if let s = try? value.decode(String.self) {
            self = .string(s)
        } else if let a = try? value.decode([JSONValue].self) {
            self = .array(a)
        } else {
            self = .object(try value.decode([String: JSONValue].self))
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var value = encoder.singleValueContainer()
        switch self {
        case .null: try value.encodeNil()
        case .bool(let b): try value.encode(b)
        // A whole number is written as one: the engine reads offsets and counts as integers.
        case .number(let n) where n.rounded() == n && abs(n) < 9.0e15: try value.encode(Int64(n))
        case .number(let n): try value.encode(n)
        case .string(let s): try value.encode(s)
        case .array(let a): try value.encode(a)
        case .object(let o): try value.encode(o)
        }
    }

    public subscript(key: String) -> JSONValue? {
        if case .object(let o) = self { o[key] } else { nil }
    }

    public subscript(index: Int) -> JSONValue? {
        if case .array(let a) = self, a.indices.contains(index) { a[index] } else { nil }
    }

    public var bool: Bool? {
        if case .bool(let b) = self { b } else { nil }
    }

    public var number: Double? {
        if case .number(let n) = self { n } else { nil }
    }

    public var string: String? {
        if case .string(let s) = self { s } else { nil }
    }

    public var array: [JSONValue]? {
        if case .array(let a) = self { a } else { nil }
    }

    public var object: [String: JSONValue]? {
        if case .object(let o) = self { o } else { nil }
    }
}

extension JSONValue: ExpressibleByNilLiteral {
    public init(nilLiteral: ()) { self = .null }
}

extension JSONValue: ExpressibleByBooleanLiteral {
    public init(booleanLiteral value: Bool) { self = .bool(value) }
}

extension JSONValue: ExpressibleByIntegerLiteral {
    public init(integerLiteral value: Int) { self = .number(Double(value)) }
}

extension JSONValue: ExpressibleByFloatLiteral {
    public init(floatLiteral value: Double) { self = .number(value) }
}

extension JSONValue: ExpressibleByStringLiteral {
    public init(stringLiteral value: String) { self = .string(value) }
}

extension JSONValue: ExpressibleByArrayLiteral {
    public init(arrayLiteral elements: JSONValue...) { self = .array(elements) }
}

extension JSONValue: ExpressibleByDictionaryLiteral {
    public init(dictionaryLiteral elements: (String, JSONValue)...) {
        self = .object(Dictionary(elements, uniquingKeysWith: { _, last in last }))
    }
}
