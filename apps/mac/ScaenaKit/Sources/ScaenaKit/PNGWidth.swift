import CoreGraphics

/// A PNG's width as the export sheet asks it, as the browser's Export… does: a whole number of
/// pixels from 1 to 8192, the height keeping the canvas's aspect. One slide's PNG and every slide's
/// in their zip are made at it.
public enum PNGWidth {
    /// The widths a PNG may be, in pixels, as the browser's field takes them.
    public static let widths = 1...8192

    /// What was typed as a width: a whole number of pixels within `widths`; none for any other.
    public static func width(_ typed: String) -> Int? {
        guard let read = FieldNumber.read(typed), read == read.rounded(), widths.contains(Int(read)) else { return nil }
        return Int(read)
    }

    /// The pixels a PNG `width` wide of a canvas `canvas` is: the height the canvas's proportions
    /// give it, rounded as the browser says it.
    public static func pixels(width: Int, canvas: CGSize) -> (width: Int, height: Int) {
        guard canvas.width > 0 else { return (width, 1) }
        return (width, max(1, Int((canvas.height * Double(width) / canvas.width).rounded())))
    }
}
