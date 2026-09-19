import XCTest

@testable import KarukanIME

final class CandidateWindowPlacementTests: XCTestCase {
    let screenVisibleFrame = NSRect(x: 0, y: 0, width: 1440, height: 900)
    let maxPanelHeight: CGFloat = 300

    func testFitsBelowNearTop() {
        let cursorRect = NSRect(x: 100, y: 800, width: 2, height: 16)
        let placement = CandidateWindowController.placement(
            cursorRect: cursorRect, maxPanelHeight: maxPanelHeight,
            screenVisibleFrame: screenVisibleFrame)
        XCTAssertEqual(placement, .below)
    }

    func testFitsAboveNearBottom() {
        let cursorRect = NSRect(x: 100, y: 50, width: 2, height: 16)
        let placement = CandidateWindowController.placement(
            cursorRect: cursorRect, maxPanelHeight: maxPanelHeight,
            screenVisibleFrame: screenVisibleFrame)
        XCTAssertEqual(placement, .above)
    }

    func testMaxHeightPreventsFlickerNearBottom() {
        let cursorRect = NSRect(x: 100, y: 50, width: 2, height: 16)
        let smallHeight: CGFloat = 20
        let placementWithSmallHeight = CandidateWindowController.placement(
            cursorRect: cursorRect, maxPanelHeight: smallHeight,
            screenVisibleFrame: screenVisibleFrame)
        XCTAssertEqual(placementWithSmallHeight, .below)

        let placementWithMaxHeight = CandidateWindowController.placement(
            cursorRect: cursorRect, maxPanelHeight: maxPanelHeight,
            screenVisibleFrame: screenVisibleFrame)
        XCTAssertEqual(placementWithMaxHeight, .above)
    }

    func testNeitherFitsPrefersRoomierAbove() {
        let cursorRect = NSRect(x: 100, y: 100, width: 2, height: 16)
        let hugeHeight: CGFloat = 850
        let placement = CandidateWindowController.placement(
            cursorRect: cursorRect, maxPanelHeight: hugeHeight,
            screenVisibleFrame: screenVisibleFrame)
        XCTAssertEqual(placement, .above)
    }

    func testNeitherFitsPrefersRoomierBelow() {
        let cursorRect = NSRect(x: 100, y: 800, width: 2, height: 16)
        let hugeHeight: CGFloat = 850
        let placement = CandidateWindowController.placement(
            cursorRect: cursorRect, maxPanelHeight: hugeHeight,
            screenVisibleFrame: screenVisibleFrame)
        XCTAssertEqual(placement, .below)
    }

    func testNeitherFitsTieBreaksBelow() {
        // spaceBelow == spaceAbove == 442 for this cursorRect on a 900pt-tall screen.
        let cursorRect = NSRect(x: 100, y: 442, width: 2, height: 16)
        let hugeHeight: CGFloat = 1000
        let placement = CandidateWindowController.placement(
            cursorRect: cursorRect, maxPanelHeight: hugeHeight,
            screenVisibleFrame: screenVisibleFrame)
        XCTAssertEqual(placement, .below)
    }
}
