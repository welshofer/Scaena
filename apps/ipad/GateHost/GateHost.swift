import SwiftUI

/// What hosts gate 4's tests on an iPad, which runs no test bundle without an app (`ScaenaGateTests`
/// in `apps/ipad/project.yml`, `docs/gate-4.md`): an app that does nothing, so that the tests time
/// the engine alone. It has no engine of its own; the tests bring theirs.
@main
struct GateHost: App {
    var body: some Scene {
        WindowGroup {
            Color.clear
        }
    }
}
