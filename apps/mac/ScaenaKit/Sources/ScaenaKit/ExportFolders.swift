import Foundation

/// The folders an export is written into for the Share sheet and Quick Look (PLAN 3.8), each of its
/// own under the temporary directory, and taken away once the export has been shared or seen
/// (lift-up REL-5): a pictured deck's PDF or slides zip is tens of megabytes, and on the iPad the
/// app's container would grow with every export. What a quit or a crash left is swept as the app
/// starts.
public enum ExportFolders {
    /// What each folder's name begins with, so a sweep takes away no folder but these.
    public static let prefix = "Scaena-"

    /// `data` written as `name` in a folder of its own under `directory`; the file's URL.
    public static func write(
        _ data: Data, named name: String, in directory: URL = FileManager.default.temporaryDirectory
    ) throws -> URL {
        let folder = directory.appending(path: "\(prefix)\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = folder.appending(path: name)
        try data.write(to: url)
        return url
    }

    /// The folder the export at `url` was written into, taken away: none other, where `url` lies in
    /// no folder of these.
    public static func remove(_ url: URL) {
        let folder = url.deletingLastPathComponent()
        guard folder.lastPathComponent.hasPrefix(prefix) else { return }
        try? FileManager.default.removeItem(at: folder)
    }

    /// Each folder of these under `directory` made longer ago than `age` before `now`, taken away;
    /// how many were.
    @discardableResult
    public static func sweep(
        olderThan age: TimeInterval = 24 * 60 * 60, now: Date = Date(),
        in directory: URL = FileManager.default.temporaryDirectory
    ) -> Int {
        let keys: Set<URLResourceKey> = [.isDirectoryKey, .creationDateKey]
        guard let found = try? FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: Array(keys), options: [.skipsHiddenFiles])
        else { return 0 }
        var swept = 0
        for folder in found where folder.lastPathComponent.hasPrefix(prefix) {
            guard let values = try? folder.resourceValues(forKeys: keys), values.isDirectory == true,
                let made = values.creationDate, now.timeIntervalSince(made) > age
            else { continue }
            if (try? FileManager.default.removeItem(at: folder)) != nil { swept += 1 }
        }
        return swept
    }
}
