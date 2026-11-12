import SwiftUI

/// An export's width, asked in the browser's words: "Width", a field of whole pixels, "pixels"
/// beside it. The movie's sheet and the PNGs' ask it alike (lift-up 20261031 FUNC-2).
struct WidthRow: View {
    @Binding var typed: String
    var focused: FocusState<Bool>.Binding
    /// What a test finds the field by.
    let id: String
    /// Return in the field: what the sheet's default button does.
    let submit: () -> Void

    var body: some View {
        LabeledContent("Width") {
            HStack(spacing: 6) {
                TextField("Width", text: $typed)
                    .textFieldStyle(.roundedBorder)
                    .monospacedDigit()
                    .frame(width: 88)
                    .focused(focused)
                    .onSubmit(submit)
                    #if os(iOS)
                        .keyboardType(.numberPad)
                    #endif
                    .accessibilityLabel("Width in pixels")
                    .accessibilityIdentifier(id)
                Text("pixels")
                    .foregroundStyle(.secondary)
            }
        }
    }
}
