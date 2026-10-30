import Foundation
import Observation

@MainActor
@Observable
final class CleanStore {
    private(set) var phase: ScanPhase = .idle
    /// Every scanned item, in scan order.
    private(set) var scanResult: ScanResult = .empty
    /// Item ids the user wants removed.
    private(set) var selection: Set<String> = []
    private(set) var progress: Double = 0
    private(set) var currentPath: String = ""
    private(set) var lastError: String?
    private(set) var lastTrashed: Int = 0
    private(set) var lastFailed: Int = 0
    private(set) var rulesVersion: String = ""

    var source: RuleSource {
        didSet {
            guard source != oldValue else { return }
            defaults.set(source.rawValue, forKey: Keys.source)
            reset()
            loadRules()
        }
    }

    let root: URL

    private let defaults = UserDefaults.standard
    private let fileManager = FileManager.default
    private var task: Task<Void, Never>?

    private enum Keys {
        static let source = "ruleSource"
    }

    init(root: URL) {
        self.root = root
        let stored = UserDefaults.standard.string(forKey: Keys.source).flatMap(RuleSource.init(rawValue:))
        self.source = stored ?? .seed
        loadRules()
    }

    var rulesDirectory: URL { source.rulesDirectory(root: root) }

    var isBusy: Bool { phase == .scanning || phase == .cleaning }

    // MARK: Items

    func items(for category: CleanCategory) -> [ScanItem] {
        scanResult.items.filter { $0.cleanType == category }
    }

    /// Items that the engine could not classify fall under Other.
    func otherItems() -> [ScanItem] {
        scanResult.items.filter { $0.cleanType == .other }
    }

    func size(for category: CleanCategory) -> Int64 {
        items(for: category).reduce(0) { $0 + $1.size }
    }

    func isSelected(_ item: ScanItem) -> Bool { selection.contains(item.id) }

    func toggle(_ item: ScanItem) {
        guard !isBusy else { return }
        if selection.contains(item.id) {
            selection.remove(item.id)
        } else {
            selection.insert(item.id)
        }
    }

    /// Category-level state: all, some or none of its items are checked.
    func selectionState(for category: CleanCategory) -> SelectionState {
        let items = items(for: category)
        guard !items.isEmpty else { return .none }
        let checked = items.filter { selection.contains($0.id) }.count
        if checked == 0 { return .none }
        return checked == items.count ? .all : .partial
    }

    func toggle(_ category: CleanCategory) {
        guard !isBusy else { return }
        let items = items(for: category)
        guard !items.isEmpty else { return }
        if selectionState(for: category) == .all {
            selection.subtract(items.map(\.id))
        } else {
            selection.formUnion(items.map(\.id))
        }
    }

    var selectedItems: [ScanItem] {
        scanResult.items.filter { selection.contains($0.id) }
    }

    var selectedSize: Int64 { selectedItems.reduce(0) { $0 + $1.size } }
    var selectedFileCount: Int { selectedItems.reduce(0) { $0 + $1.fileCount } }
    var lastTotal: Int64 { scanResult.totalSize }

    // MARK: Rules

    func loadRules() {
        let entries = (try? fileManager.contentsOfDirectory(at: rulesDirectory, includingPropertiesForKeys: nil)) ?? []
        // Only rule documents count; the dump also drops metadata JSON here.
        let rules = entries
            .filter { ["xml", "plist"].contains($0.pathExtension) }
            .map(\.lastPathComponent)
            .sorted()
        rulesVersion = rules.count == 1 ? rules[0] : rules.isEmpty ? "" : "\(rules[0]) +\(rules.count - 1)"
    }

    // MARK: Scanning

    func scan() {
        guard !isBusy else { return }
        task?.cancel()
        phase = .scanning
        progress = 0
        lastError = nil
        lastTrashed = 0
        lastFailed = 0
        currentPath = ""
        scanResult = .empty
        selection = []

        let rulesPath = rulesDirectory.path
        task = Task { [weak self] in
            await self?.runScan(rulesPath: rulesPath)
        }
    }

    func clean() {
        guard !isBusy, !selectedItems.isEmpty else { return }
        task?.cancel()
        phase = .cleaning
        lastError = nil
        lastTrashed = 0
        lastFailed = 0
        progress = 0
        currentPath = ""

        let targets = selectedItems
        task = Task { [weak self] in
            await self?.runClean(items: targets)
        }
    }

    func cancel() {
        guard phase == .scanning else { return }
        task?.cancel()
        task = nil
        phase = .idle
        progress = 0
        currentPath = ""
    }

    func reset() {
        task?.cancel()
        task = nil
        scanResult = .empty
        selection = []
        phase = .idle
        progress = 0
        currentPath = ""
        lastError = nil
        lastTrashed = 0
        lastFailed = 0
    }

    private func runScan(rulesPath: String) async {
        do {
            phase = .scanning
            currentPath = rulesPath
            progress = 0.05

            let ticker = Task { @MainActor [weak self] in
                var value = 0.05
                while !Task.isCancelled, value < 0.9 {
                    try? await Task.sleep(for: .milliseconds(120))
                    value = min(value + 0.02, 0.9)
                    self?.progress = value
                }
            }

            let result = try await Engine.scan(rulesPath: rulesPath)
            ticker.cancel()
            guard !Task.isCancelled else { throw CancellationError() }
            scanResult = result
            // Everything the scan found is checked after a scan; risky items
            // stay off so the user opts into them.
            selection = Set(result.items.filter { !$0.isCaution }.map { $0.id })
            progress = 1
            currentPath = ""
            phase = .ready
        } catch is CancellationError {
            phase = .idle
            progress = 0
            currentPath = ""
        } catch {
            lastError = error.localizedDescription
            phase = .idle
            progress = 0
            currentPath = ""
        }
    }

    private func runClean(items: [ScanItem]) async {
        let paths = Array(Set(items.flatMap(\.paths))).sorted()
        let total = max(paths.count, 1)
        var trashed = 0
        var failures = 0
        var firstFailure: (path: String, reason: String)?

        for (index, path) in paths.enumerated() {
            if Task.isCancelled { break }
            currentPath = path
            do {
                try fileManager.trashItem(at: URL(fileURLWithPath: path), resultingItemURL: nil)
                trashed += 1
            } catch {
                failures += 1
                if firstFailure == nil { firstFailure = (path, error.localizedDescription) }
            }
            progress = Double(index + 1) / Double(total)
        }

        guard !Task.isCancelled else {
            phase = .ready
            return
        }

        lastTrashed = trashed
        lastFailed = failures
        if let failure = firstFailure {
            lastError = "\(failure.path): \(failure.reason)"
        }
        let removed = Set(items.map(\.id))
        selection.subtract(removed)
        let remaining = scanResult.items.filter { !removed.contains($0.id) }
        scanResult = ScanResult(
            items: remaining,
            totalSize: remaining.reduce(0) { $0 + $1.size },
            totalFiles: remaining.reduce(0) { $0 + $1.fileCount },
            durationMs: scanResult.durationMs
        )
        currentPath = ""
        phase = .finished
    }
}
