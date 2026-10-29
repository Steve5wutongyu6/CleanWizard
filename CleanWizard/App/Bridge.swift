import Foundation

enum DumpStatus: Sendable {
    case success
    case lemonNotFound
    case noKnowledgeRules
}

struct DumpReport: Sendable {
    let status: DumpStatus
    let written: [String]
    let knowledgeDirectories: [String]
    let ruleFileCount: Int
}

enum Bridge {
    static var isLemonInstalled: Bool { CWBridge.isLemonInstalled() }

    static var knowledgeSummary: String? { CWBridge.lemonKnowledgeDirectoriesDescription() }

    static func dumpLemonRules(to directory: URL) -> DumpReport {
        let result = CWBridge.dumpLocalLemonRules(toDirectory: directory)
        let status: DumpStatus = switch result.status {
        case .success: .success
        case .lemonNotFound: .lemonNotFound
        default: .noKnowledgeRules
        }
        return DumpReport(
            status: status,
            written: result.writtenPaths,
            knowledgeDirectories: result.knowledgeDirectories,
            ruleFileCount: result.ruleFileCount
        )
    }

    static func isSigned(_ path: String) -> Bool { CWBridge.isCodeSigned(atPath: path) }

    static func architecture(_ path: String) -> String? { CWBridge.architecture(atPath: path) }
}
