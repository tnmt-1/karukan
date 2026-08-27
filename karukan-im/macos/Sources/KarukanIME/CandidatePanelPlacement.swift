import Cocoa

/// Pure placement math for the candidate panel relative to the composition
/// anchor. Extracted so flip/clamp rules can be unit-tested without AppKit
/// window state.
enum CandidatePanelPlacement {
    /// Minimum gap between the panel edge and the composition line.
    static let gap: CGFloat = 10

    /// Extra clearance above the preedit line when the panel is flipped upward,
    /// so the underlined composition stays readable.
    static func preeditClearance(lineHeight: CGFloat) -> CGFloat {
        max(gap, lineHeight)
    }

    /// Screen whose frame contains the anchor rect's midpoint, falling back to
    /// the nearest screen then `NSScreen.main`.
    static func screen(for cursorRect: NSRect) -> NSScreen? {
        let anchor = NSPoint(x: cursorRect.midX, y: cursorRect.midY)
        if let containing = NSScreen.screens.first(where: { $0.frame.contains(anchor) }) {
            return containing
        }
        return NSScreen.screens.min(by: { lhs, rhs in
            distance(from: anchor, to: lhs.frame) < distance(from: anchor, to: rhs.frame)
        }) ?? NSScreen.main
    }

    /// Whether the panel should appear above the composition line.
    static func shouldShowAbove(
        cursorRect: NSRect,
        panelHeight: CGFloat,
        visibleFrame: NSRect,
        gap: CGFloat = gap
    ) -> Bool {
        let lineHeight = max(cursorRect.height, 1)
        let clearance = preeditClearance(lineHeight: lineHeight)
        let spaceBelow = cursorRect.origin.y - visibleFrame.origin.y
        let spaceAbove = visibleFrame.maxY - cursorRect.maxY
        // Below the line the panel covers following content; above it we need
        // room for the preedit line itself to stay visible.
        let neededBelow = panelHeight + gap
        let neededAbove = panelHeight + clearance

        if spaceBelow >= neededBelow { return false }
        if spaceAbove >= neededAbove { return true }
        return spaceAbove > spaceBelow
    }

    /// Frame for the candidate panel in screen coordinates.
    static func frame(
        cursorRect: NSRect,
        panelSize: NSSize,
        visibleFrame: NSRect,
        gap: CGFloat = gap
    ) -> NSRect {
        let lineHeight = max(cursorRect.height, 1)
        let clearance = preeditClearance(lineHeight: lineHeight)
        let showAbove = shouldShowAbove(
            cursorRect: cursorRect,
            panelHeight: panelSize.height,
            visibleFrame: visibleFrame,
            gap: gap
        )

        var originY: CGFloat
        if showAbove {
            originY = cursorRect.maxY + clearance
        } else {
            originY = cursorRect.origin.y - panelSize.height - gap
        }

        var originX = cursorRect.origin.x
        let minX = visibleFrame.origin.x
        let maxX = visibleFrame.maxX - panelSize.width
        if maxX >= minX {
            originX = min(max(originX, minX), maxX)
        } else {
            originX = minX
        }

        var frame = NSRect(x: originX, y: originY, width: panelSize.width, height: panelSize.height)

        // Keep the panel inside the visible frame when possible.
        if frame.maxY > visibleFrame.maxY {
            frame.origin.y = visibleFrame.maxY - frame.height
        }
        if frame.origin.y < visibleFrame.origin.y {
            frame.origin.y = visibleFrame.origin.y
        }

        // After clamping, never let an above-panel overlap the preedit line.
        if showAbove, frame.origin.y < cursorRect.maxY + gap {
            frame.origin.y = cursorRect.maxY + clearance
            if frame.maxY > visibleFrame.maxY {
                frame.origin.y = visibleFrame.maxY - frame.height
            }
        }

        return frame
    }

    private static func distance(from point: NSPoint, to rect: NSRect) -> CGFloat {
        let dx: CGFloat
        if point.x < rect.minX {
            dx = rect.minX - point.x
        } else if point.x > rect.maxX {
            dx = point.x - rect.maxX
        } else {
            dx = 0
        }

        let dy: CGFloat
        if point.y < rect.minY {
            dy = rect.minY - point.y
        } else if point.y > rect.maxY {
            dy = point.y - rect.maxY
        } else {
            dy = 0
        }

        return hypot(dx, dy)
    }
}
