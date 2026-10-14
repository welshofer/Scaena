import Foundation

/// A step of undo by what its patch does (PLAN 3.28, 4.11), as the Edit menu names it after Undo
/// and Redo and the iPad's Undo reads it: Undo Move, Redo Duplicate, as a presentation app names
/// every step. Where a gesture or a command knows better than its patch says (a handle dragged is
/// a resize, words pasted are a paste), the window names the step itself.
public enum StepName {
    /// What `ops` does, in a person's words; `label` says a property as the inspector labels it,
    /// `style/size` as Size. A patch of nothing it knows is a change.
    public static func of(_ ops: [JSONValue], label: (String) -> String) -> String {
        let kinds = ops.compactMap { $0["op"]?.string }.filter { $0 != "test" }
        let has = { (kind: String) in kinds.contains(kind) }
        // What a step adds is what it is: a patch that inserts also keeps what it inserted on its
        // slide (PLAN 3.24), and one that duplicates adds the copy.
        if has("add_node") { return "Insert" }
        if has("group") { return "Group" }
        if has("ungroup") { return "Ungroup" }
        if has("duplicate_slide") { return "Duplicate Slide" }
        if has("remove_slide") { return "Delete Slide" }
        if has("move_slide") || has("move_state") { return "Move Slide" }
        if has("add_state") { return "New Slide" }
        if has("remove_state") { return "Delete Step" }
        if has("rename_state") || has("rename_node") { return "Rename" }
        if has("remove_node") || has("hide_node") { return "Delete" }
        if has("show_node") { return "Show" }
        if has("replace_text") || has("set_text") { return "Typing" }
        if has("style_text") { return "Text Style" }
        if has("list") { return "List" }
        if has("time_motion") { return "Timing" }
        if has("apply_preset") { return "Add Build" }
        if has("retheme") { return "Theme" }
        if has("place") { return "Move" }
        let props = ops.filter { ["choose", "set_prop"].contains($0["op"]?.string ?? "") }.compactMap { $0["prop"]?.string }
        if let prop = props.first {
            return Set(props).count == 1 ? choosing(prop, label: label) : "Change Style"
        }
        if let prop = ops.first(where: { $0["op"]?.string == "set_state" })?["prop"]?.string {
            return "Change \(label(prop))"
        }
        return "Change"
    }

    /// A property chosen, as the gesture that sets it is called: a turn, a shape's points, a
    /// picture's crop; else the property changed.
    private static func choosing(_ prop: String, label: (String) -> String) -> String {
        switch prop {
        case "transform/rotate": "Rotate"
        case "points": "Reshape"
        case "radius": "Round Corners"
        case "crop": "Crop"
        case "focal": "Move Focal Point"
        case "src": "Replace Picture"
        case "z": "Arrange"
        default: "Change \(label(prop))"
        }
    }
}
