import Cocoa
@testable import KarukanIME
import XCTest

final class CandidatePanelPlacementTests: XCTestCase {
    private let visibleFrame = NSRect(x: 0, y: 0, width: 1440, height: 900)

    func testPrefersBelowWhenEnoughSpace() {
        let cursor = NSRect(x: 200, y: 500, width: 12, height: 18)
        let panel = NSSize(width: 220, height: 120)

        let frame = CandidatePanelPlacement.frame(
            cursorRect: cursor, panelSize: panel, visibleFrame: visibleFrame)

        XCTAssertLessThan(frame.maxY, cursor.origin.y)
        XCTAssertGreaterThanOrEqual(frame.origin.y, visibleFrame.origin.y)
    }

    func testFlipsAboveNearScreenBottom() {
        let cursor = NSRect(x: 200, y: 80, width: 12, height: 18)
        let panel = NSSize(width: 220, height: 120)

        let frame = CandidatePanelPlacement.frame(
            cursorRect: cursor, panelSize: panel, visibleFrame: visibleFrame)

        let clearance = CandidatePanelPlacement.preeditClearance(lineHeight: cursor.height)
        XCTAssertGreaterThanOrEqual(frame.origin.y, cursor.maxY + clearance - 0.5)
        XCTAssertLessThanOrEqual(frame.maxY, visibleFrame.maxY)
    }

    func testAboveLeavesPreeditLineVisible() {
        let cursor = NSRect(x: 200, y: 60, width: 12, height: 20)
        let panel = NSSize(width: 220, height: 100)

        let frame = CandidatePanelPlacement.frame(
            cursorRect: cursor, panelSize: panel, visibleFrame: visibleFrame)

        XCTAssertGreaterThanOrEqual(frame.origin.y, cursor.maxY + CandidatePanelPlacement.gap)
    }

    func testPicksSideWithMoreSpaceWhenNeitherFitsFully() {
        let cursor = NSRect(x: 200, y: 450, width: 12, height: 18)
        let panel = NSSize(width: 220, height: 800)

        let showAbove = CandidatePanelPlacement.shouldShowAbove(
            cursorRect: cursor, panelHeight: panel.height, visibleFrame: visibleFrame)

        XCTAssertTrue(showAbove)
    }

    func testClampsHorizontally() {
        let cursor = NSRect(x: 1300, y: 500, width: 12, height: 18)
        let panel = NSSize(width: 220, height: 120)

        let frame = CandidatePanelPlacement.frame(
            cursorRect: cursor, panelSize: panel, visibleFrame: visibleFrame)

        XCTAssertLessThanOrEqual(frame.maxX, visibleFrame.maxX)
        XCTAssertGreaterThanOrEqual(frame.origin.x, visibleFrame.origin.x)
    }

    func testClampsVerticallyWhenAboveWouldLeaveScreen() {
        let cursor = NSRect(x: 200, y: 850, width: 12, height: 18)
        let panel = NSSize(width: 220, height: 120)

        let frame = CandidatePanelPlacement.frame(
            cursorRect: cursor, panelSize: panel, visibleFrame: visibleFrame)

        XCTAssertLessThanOrEqual(frame.maxY, visibleFrame.maxY)
        XCTAssertGreaterThanOrEqual(frame.origin.y, visibleFrame.origin.y)
    }
}
