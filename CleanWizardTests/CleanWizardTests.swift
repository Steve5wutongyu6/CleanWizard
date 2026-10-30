import XCTest
@testable import CleanWizard

final class CleanWizardTests: XCTestCase {
    func testCleanCategoryRoundTripsRawValue() {
        for category in CleanCategory.allCases {
            XCTAssertEqual(CleanCategory(rawValue: category.rawValue), category)
        }
    }

    func testEngineTypesAreCovered() {
        let raw = Set(CleanCategory.scanOrder.map(\.rawValue))
        XCTAssertTrue(raw.contains("sys"))
        XCTAssertTrue(raw.contains("app"))
        XCTAssertTrue(raw.contains("int"))
    }

    func testFormatSizeIsNonEmpty() {
        XCTAssertFalse(Format.size(-1).isEmpty)
        XCTAssertFalse(Format.size(1024 * 1024).isEmpty)
    }

    func testFormatFilesPluralises() {
        XCTAssertEqual(Format.files(1), "1 file")
        XCTAssertEqual(Format.files(3), "3 files")
    }

    func testScanResultEmptyIsZeroed() {
        let empty = ScanResult.empty
        XCTAssertTrue(empty.items.isEmpty)
        XCTAssertEqual(empty.totalSize, 0)
        XCTAssertEqual(empty.totalFiles, 0)
    }

    func testEngineReportsVersion() {
        XCTAssertNotNil(Engine.version)
    }

    @MainActor
    func testItemToggleTracksEachItemSeparately() {
        let store = CleanStore(root: URL(fileURLWithPath: NSTemporaryDirectory()))
        let logs = ScanItem(
            id: "logs", title: "User Logs", cleanType: .system, size: 100,
            fileCount: 2, paths: ["/tmp/a"], isCaution: false, isSelected: false
        )
        let caches = ScanItem(
            id: "caches", title: "User Caches", cleanType: .app, size: 300,
            fileCount: 5, paths: ["/tmp/b"], isCaution: false, isSelected: false
        )

        XCTAssertTrue(store.selection.isEmpty)
        store.toggle(logs)
        XCTAssertTrue(store.isSelected(logs))
        XCTAssertFalse(store.isSelected(caches), "checking one item must not check another")

        store.toggle(caches)
        XCTAssertEqual(store.selection, ["logs", "caches"])

        store.toggle(logs)
        XCTAssertEqual(store.selection, ["caches"], "unchecking one item keeps the rest")
    }
}
