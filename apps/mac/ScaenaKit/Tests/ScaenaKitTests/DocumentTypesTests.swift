import Foundation
import Testing

/// An app's document types, as Launch Services and Files read them from its Info.plist: each type's
/// rank and the content types it lists, and the types the app declares as its own.
private struct DocumentTypes {
    /// Each `CFBundleDocumentTypes` entry: its name, its `LSHandlerRank`, and its `LSItemContentTypes`.
    let claims: [(name: String, rank: String, types: [String])]
    /// The identifiers of `UTExportedTypeDeclarations`: the types the app defines.
    let defined: Set<String>

    init(_ path: String) throws {
        let data = try Data(contentsOf: repository.appending(path: path))
        let plist = try PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any] ?? [:]
        claims = (plist["CFBundleDocumentTypes"] as? [[String: Any]] ?? []).map { type in
            (
                name: type["CFBundleTypeName"] as? String ?? "?", rank: type["LSHandlerRank"] as? String ?? "Default",
                types: type["LSItemContentTypes"] as? [String] ?? []
            )
        }
        defined = Set((plist["UTExportedTypeDeclarations"] as? [[String: Any]] ?? []).compactMap { $0["UTTypeIdentifier"] as? String })
    }
}

/// The Mac's and the iPad's apps own only the types they define (lift-up USE-1). Ranked Owner of
/// `public.zip-archive`, an app takes every zip's double click in the Finder and in Files from the
/// app that owns zips; Apple reserves Owner for the types an app defines. A `.scaena` zip is still
/// the app's own, and a zip that is not named so still opens from Open With, the Open dialog, and a
/// drop, as an alternate for it.
@Test(arguments: ["apps/mac/Scaena/Info.plist", "apps/ipad/Info.plist"])
func theAppOwnsOnlyTheTypesItDefines(plist: String) throws {
    let types = try DocumentTypes(plist)
    #expect(types.defined == ["com.scaena.deck", "com.scaena.deck.zip"], "\(plist) defines \(types.defined.sorted())")
    for claim in types.claims where claim.rank == "Owner" {
        let others = claim.types.filter { !types.defined.contains($0) }
        #expect(others.isEmpty, "\(plist): \(claim.name) is ranked Owner of \(others), which the app does not define")
    }
    let owned = Set(types.claims.filter { $0.rank == "Owner" }.flatMap(\.types))
    #expect(owned == types.defined, "\(plist): the app owns \(owned.sorted()), and defines \(types.defined.sorted())")
    let zips = types.claims.filter { $0.types.contains("public.zip-archive") }
    #expect(zips.count == 1 && zips.first?.rank == "Alternate", "\(plist): a zip opens as an alternate: \(zips.map { "\($0.name) \($0.rank)" })")
}
