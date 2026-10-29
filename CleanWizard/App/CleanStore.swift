import Foundation
import Observation

@MainActor
@Observable
final class CleanStore {
    private(set) var phase: ScanPhase = .idle
    private(set) var selection: Set<CleanCategory> = Set(CleanCategory.scanOrder)
    private(set) var scanResult: ScanResult = .empty
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

    var selectedCategories: [CleanCategory] {
        CleanCategory.scanOrder.filter { selection.contains($0) }
    }

    var selectedSize: Int64 {
        selectedCategories.reduce(0) { $0 + size(for: $1) }
    }

    var lastTotal: Int64 { scanResult.totalSize }

    func items(for category: CleanCategory) -> [ScanItem] {
        scanResult.items.filter { $0.cleanType == category }
    }

    func size(for category: CleanCategory) -> Int64 {
        items(for: category).reduce(0) { $0 + $1.size }
    }

    func isSelected(_ category: CleanCategory) -> Bool { selection.contains(category) }

    func toggle(_ category: CleanCategory) {
        guard !isBusy else { return }
        if selection.contains(category) {
            guard selection.count > 1 else { return }
            selection.remove(category)
        } else {
            selection.insert(category)
        }
        reset()
    }

    func loadRules() {
        let entries = (try? fileManager.contentsOfDirectory(at: rulesDirectory, includingPropertiesForKeys: nil)) ?? []
        let rules = entries
            .filter { $0.pathExtension == "plist" || $0.pathExtension == "json" }
            .map(\.lastPathComponent)
            .sorted()
        rulesVersion = rules.count == 1 ? rules[0] : rules.isEmpty ? "" : "\(rules[0]) +\(rules.count - 1)"
    }

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

        let categories = selectedCategories
        let rulesPath = rulesDirectory.path
        task = Task { [weak self] in
            await self?.runScan(categories: categories, rulesPath: rulesPath)
        }
    }

    func clean() {
        guard !isBusy, !scanResult.items.isEmpty else { return }
        task?.cancel()
        phase = .cleaning
        lastError = nil
        lastTrashed = 0
        lastFailed = 0
        progress = 0
        currentPath = ""

        let targets = scanResult.items.filter { selection.contains($0.cleanType) }
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
        phase = .idle
        progress = 0
        currentPath = ""
    }

    private func runScan(categories: [CleanCategory], rulesPath: String) async {
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

            let result = try await Engine.scan(rulesPath: rulesPath, categories: categories.map(\.rawValue))
            ticker.cancel()
            guard !Task.isCancelled else { throw CancellationError() }
            scanResult = result
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
        var trashed: [String] = []
        var failures: [(path: String, reason: String)] = []

        for (index, path) in paths.enumerated() {
            if Task.isCancelled { break }
            currentPath = path
            do {
                try fileManager.trashItem(at: URL(fileURLWithPath: path), resultingItemURL: nil)
                trashed.append(path)
            } catch {
                failures.append((path, error.localizedDescription))
            }
            progress = Double(index + 1) / Double(total)
        }

        guard !Task.isCancelled else {
            phase = .ready
            return
        }

        lastTrashed = trashed.count
        lastFailed = failures.count
        if let failure = failures.first {
            lastError = "\(failure.path): \(failure.reason)"
        }
        let removed = Set(items.map(\.id))
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
