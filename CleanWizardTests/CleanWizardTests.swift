import XCTest
@testable import CleanWizard

final class CleanWizardTests: XCTestCase {
    func testCleanCategoryRoundTripsRawValue() {
        for category in CleanCategory.allCases {
            XCTAssertEqual(CleanCategory(rawValue: category.rawValue), category)
        }
    }

    func testCleanCategoriesCoverEngineTypes() {
        let raw = Set(CleanCategory.scanOrder.map(\.rawValue))
        XCTAssertTrue(raw.contains("sys"))
        XCTAssertTrue(raw.contains("app"))
        XCTAssertTrue(raw.contains("int"))
    }

    func testFormatSizeNeverNegative() {
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
}
