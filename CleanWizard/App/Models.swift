import SwiftUI

enum CleanCategory: String, CaseIterable, Identifiable, Sendable {
    case system = "sys"
    case app = "app"
    case internet = "int"
    case other = "other"

    static let scanOrder: [CleanCategory] = [.system, .app, .internet, .other]

    var id: String { rawValue }

    var title: String {
        switch self {
        case .system: "System Junk"
        case .app: "App Junk"
        case .internet: "Internet Junk"
        case .other: "Other"
        }
    }

    var symbol: String {
        switch self {
        case .system: "trash.fill"
        case .app: "internaldrive"
        case .internet: "globe"
        case .other: "clock.arrow.circlepath"
        }
    }
}

enum RuleSource: String, CaseIterable, Identifiable, Sendable {
    case seed
    case lemon

    var id: String { rawValue }

    static let directoryName = "Rules"

    var title: String {
        switch self {
        case .seed: "Built-in Rules"
        case .lemon: "Exported from Lemon"
        }
    }

    var symbol: String {
        switch self {
        case .seed: "shippingbox"
        case .lemon: "tray.and.arrow.down"
        }
    }

    func rulesDirectory(root: URL) -> URL {
        root.appendingPathComponent(Self.directoryName, isDirectory: true)
            .appendingPathComponent(rawValue, isDirectory: true)
    }
}

enum SelectionState: Equatable, Sendable {
    case none
    case partial
    case all
}

enum ScanPhase: Equatable, Sendable {
    case idle
    case scanning
    case ready
    case cleaning
    case finished
}

enum Format {
    static func size(_ bytes: Int64) -> String {
        ByteCountFormatStyle(style: .file, allowedUnits: [.kb, .mb, .gb])
            .format(max(0, bytes))
    }

    static func files(_ count: Int) -> String {
        count == 1 ? "1 file" : "\(count) files"
    }
}
