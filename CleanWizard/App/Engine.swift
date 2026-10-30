import Foundation

enum EngineError: LocalizedError {
    case unavailable
    case invalidResponse
    case engine(String)

    var errorDescription: String? {
        switch self {
        case .unavailable: "Engine unavailable"
        case .invalidResponse: "Malformed engine response"
        case .engine(let message): message
        }
    }
}

struct ScanItem: Identifiable, Sendable, Hashable {
    let id: String
    let title: String
    let cleanType: CleanCategory
    let size: Int64
    let fileCount: Int
    let paths: [String]
    let isCaution: Bool
    /// Default state from the rule set; the UI may change it.
    let isSelected: Bool
}

struct ScanResult: Sendable {
    let items: [ScanItem]
    let totalSize: Int64
    let totalFiles: Int
    let durationMs: Double

    static let empty = ScanResult(items: [], totalSize: 0, totalFiles: 0, durationMs: 0)
}

struct TrashResult: Sendable {
    let trashed: [String]
    let failed: [(path: String, reason: String)]
}

struct ParsedRules: Sendable {
    let version: String
    let categories: [String]
}

enum Engine {
    static var version: String? {
        guard let raw = cw_engine_version() else { return nil }
        return String(validatingCString: raw)
    }

    private struct ScopePayload: Encodable {
        struct Options: Encodable {
            let hidden = false
            let emptyFolder = false
        }

        let options = Options()
    }

    /// Scans every rule root; the UI decides what to keep afterwards.
    @concurrent
    static func scan(rulesPath: String) async throws -> ScanResult {
        let payload = ScopePayload()
        let scopeJSON = String(decoding: try JSONEncoder().encode(payload), as: UTF8.self)
        let data = try invoke { cw_scan(rulesPath, scopeJSON) }
        return try decodeScan(data)
    }

    @concurrent
    static func parseRules(at path: String) async throws -> ParsedRules {
        let data = try invoke { cw_parse_rules(path) }
        guard
            let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            let version = object["version"] as? String
        else {
            throw EngineError.invalidResponse
        }
        let categories = (object["categories"] as? [[String: Any]] ?? [])
            .compactMap { $0["name"] as? String ?? $0["id"] as? String }
        return ParsedRules(version: version, categories: categories)
    }

    @concurrent
    static func trash(paths: [String]) async throws -> TrashResult {
        struct Payload: Encodable { let paths: [String] }
        let payload = String(decoding: try JSONEncoder().encode(Payload(paths: paths)), as: UTF8.self)
        let data = try invoke { cw_trash(payload) }
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw EngineError.invalidResponse
        }
        let trashed = object["trashed"] as? [String] ?? []
        let failed = (object["failed"] as? [[String: Any]] ?? []).map {
            (path: $0["path"] as? String ?? "", reason: $0["reason"] as? String ?? "")
        }
        return TrashResult(trashed: trashed, failed: failed)
    }

    private static func decodeScan(_ data: Data) throws -> ScanResult {
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw EngineError.invalidResponse
        }
        let items = (object["items"] as? [[String: Any]] ?? []).compactMap { entry -> ScanItem? in
            guard let id = entry["id"] as? String else { return nil }
            let type = (entry["cleanType"] as? String).flatMap(CleanCategory.init(rawValue:)) ?? .other
            return ScanItem(
                id: id,
                title: entry["title"] as? String ?? id,
                cleanType: type,
                size: (entry["size"] as? NSNumber)?.int64Value ?? 0,
                fileCount: (entry["fileCount"] as? NSNumber)?.intValue ?? 0,
                paths: entry["paths"] as? [String] ?? [],
                isCaution: entry["isCaution"] as? Bool ?? false,
                isSelected: entry["selected"] as? Bool ?? true
            )
        }
        return ScanResult(
            items: items,
            totalSize: (object["totalSize"] as? NSNumber)?.int64Value ?? items.reduce(0) { $0 + $1.size },
            totalFiles: (object["totalFiles"] as? NSNumber)?.intValue ?? items.reduce(0) { $0 + $1.fileCount },
            durationMs: (object["durationMs"] as? NSNumber)?.doubleValue ?? 0
        )
    }

    private static func invoke(_ body: () -> UnsafeMutablePointer<CChar>?) throws -> Data {
        guard let pointer = body() else {
            throw EngineError.unavailable
        }
        defer { cw_engine_free(pointer) }
        let text: String = String(cString: pointer)
        let data = Data(text.utf8)
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw EngineError.invalidResponse
        }
        if let error = object["error"] as? String {
            throw EngineError.engine(error)
        }
        return data
    }
}
