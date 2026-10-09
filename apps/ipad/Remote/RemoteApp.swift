import ScaenaRemote
import SwiftUI

/// The remote on an iPhone or an iPad (PLAN 4.10, SPEC §9.5): it plays a show a Mac or an iPad
/// presents, over the local network, and shows the notes and what comes next. It holds no deck and
/// no engine: what it draws, the presenter sends it.
@main
struct RemoteApp: App {
    var body: some Scene {
        WindowGroup {
            RemoteView()
        }
    }
}
