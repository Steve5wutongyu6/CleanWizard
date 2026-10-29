import SwiftUI

@main
struct CleanWizardApp: App {
    @State private var store: CleanStore

    init() {
        let root = AppPaths.rulesRoot()
        AppPaths.seedIfNeeded(into: root)
        _store = State(initialValue: CleanStore(root: root))
    }

    var body: some Scene {
        WindowGroup {
            MainWindowView(store: store)
        }
        .windowResizability(.contentMinSize)
        .commands {
            CommandGroup(after: .newItem) {
                Button("Scan") { store.scan() }
                    .keyboardShortcut("r", modifiers: .command)
                    .disabled(store.isBusy)
                Button("Clean") { store.clean() }
                    .keyboardShortcut(.delete, modifiers: .command)
                    .disabled(store.phase != .ready || store.scanResult.items.isEmpty)
            }
        }

        Settings {
            SettingsView(store: store)
        }
    }
}

enum AppPaths {
    /// Writable home for rule sets; the bundle stays read-only.
    static func rulesRoot() -> URL {
        if let override = ProcessInfo.processInfo.environment["CLEANWIZARD_RESOURCES"],
           let url = URL(string: override) {
            return url
        }
        let base = FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first ?? URL(fileURLWithPath: NSHomeDirectory())
        return base.appendingPathComponent("CleanWizard", isDirectory: true)
    }

    /// Copies the bundled seed rules into the writable root the first time the
    /// app runs, so the seed set is always available offline.
    static func seedIfNeeded(into root: URL) {
        let manager = FileManager.default
        let target = root
            .appendingPathComponent(RuleSource.directoryName, isDirectory: true)
            .appendingPathComponent(RuleSource.seed.rawValue, isDirectory: true)

        let existing = (try? manager.contentsOfDirectory(at: target, includingPropertiesForKeys: nil)) ?? []
        guard existing.isEmpty else { return }

        guard let bundled = Bundle.main.resourceURL?
            .appendingPathComponent(RuleSource.directoryName, isDirectory: true)
            .appendingPathComponent(RuleSource.seed.rawValue, isDirectory: true),
            manager.fileExists(atPath: bundled.path)
        else { return }

        try? manager.createDirectory(at: target, withIntermediateDirectories: true)
        for file in (try? manager.contentsOfDirectory(at: bundled, includingPropertiesForKeys: nil)) ?? [] {
            try? manager.copyItem(at: file, to: target.appendingPathComponent(file.lastPathComponent))
        }
    }
}
