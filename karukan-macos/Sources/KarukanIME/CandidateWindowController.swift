import Cocoa

/// Custom candidate window (borderless non-activating NSPanel).
///
/// The engine pre-paginates: `show` receives only the visible page plus
/// page metadata, so this controller just renders rows. An optional aux
/// line (reading hint / model info from the engine) is shown as a footer.
class CandidateWindowController {
    // Visual scale of the panel. Candidate rows use a larger type size
    // than the footers (page indicator / aux line), matching the system
    // Japanese IME's proportions.
    private static let candidateFontSize: CGFloat = 18
    private static let footerFontSize: CGFloat = 13
    private static let minPanelWidth: CGFloat = 160

    private let panel: NSPanel
    private let stackView: NSStackView
    private var rowViews: [NSView] = []
    private var auxText: String?

    private struct PageState {
        let candidates: [CandidateItem]
        let cursor: Int
        let page: Int
        let totalPages: Int
    }
    private var pageState: PageState?

    init() {
        panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 200, height: 100),
            styleMask: [.nonactivatingPanel, .borderless],
            backing: .buffered,
            defer: true
        )
        panel.level = .popUpMenu
        panel.hidesOnDeactivate = false
        panel.isOpaque = false
        panel.backgroundColor = NSColor.windowBackgroundColor
        panel.ignoresMouseEvents = true

        stackView = NSStackView()
        stackView.orientation = .vertical
        stackView.alignment = .leading
        stackView.spacing = 4
        stackView.edgeInsets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
        stackView.translatesAutoresizingMaskIntoConstraints = false

        panel.contentView?.addSubview(stackView)
        if let contentView = panel.contentView {
            NSLayoutConstraint.activate([
                stackView.topAnchor.constraint(equalTo: contentView.topAnchor),
                stackView.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
                stackView.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
                stackView.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
            ])
        }
    }

    var isVisible: Bool { panel.isVisible }

    /// IMK client that owns the current composition anchor. The panel is
    /// shared across controller instances, so we must drop stale geometry
    /// when focus moves to another window.
    private var anchorClientID: ObjectIdentifier?

    /// Bind the panel to `client`, clearing any state left by a previous client.
    func claimClient(_ client: AnyObject) {
        let id = ObjectIdentifier(client)
        if let current = anchorClientID, current != id {
            hide()
        }
        anchorClientID = id
    }

    /// Hide the panel when the given client loses focus.
    func releaseClient(_ client: AnyObject) {
        if anchorClientID == ObjectIdentifier(client) {
            hide()
        }
    }

    func ownsClient(_ client: AnyObject) -> Bool {
        anchorClientID == ObjectIdentifier(client)
    }

    /// `cursorRect: nil` reuses the rect from the previous update for the
    /// same client only — never from another window's composition.
    func show(
        candidates: [CandidateItem], cursor: Int, page: Int, totalPages: Int,
        cursorRect: NSRect?, client: AnyObject
    ) {
        claimClient(client)
        pageState = PageState(
            candidates: candidates, cursor: cursor, page: page, totalPages: totalPages)
        render(cursorRect: cursorRect)
    }

    /// Update the aux footer; re-renders in place if the window is visible.
    /// Pass `deferRender: true` when a `show`/`hide` follows in the same
    /// action batch, so the panel is rendered once per batch instead of
    /// once for the aux change and again for the candidates.
    func setAux(_ text: String?, client: AnyObject, deferRender: Bool = false) {
        guard ownsClient(client) else { return }
        auxText = text
        if !deferRender, panel.isVisible, pageState != nil {
            render(cursorRect: nil)
        }
    }

    func hide() {
        pageState = nil
        lastCursorRect = .zero
        anchorClientID = nil
        panel.orderOut(nil)
    }

    /// Move the panel to follow a new composition anchor without rebuilding rows.
    func reposition(cursorRect: NSRect, client: AnyObject) {
        guard ownsClient(client), panel.isVisible, pageState != nil else { return }
        positionPanel(cursorRect: cursorRect)
    }

    private func render(cursorRect: NSRect?) {
        clearRows()
        guard let state = pageState, !state.candidates.isEmpty else {
            hide()
            return
        }

        for (index, candidate) in state.candidates.enumerated() {
            addCandidateRow(candidate, number: index + 1, selected: index == state.cursor)
        }
        if state.totalPages > 1 {
            addFooterLabel("[\(state.page + 1)/\(state.totalPages)]")
        }
        if let aux = auxText, !aux.isEmpty {
            addFooterLabel(aux)
        }

        positionPanel(cursorRect: cursorRect)
    }

    private func clearRows() {
        for view in rowViews {
            stackView.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        rowViews.removeAll()
    }

    private func addCandidateRow(_ candidate: CandidateItem, number: Int, selected: Bool) {
        let text = NSMutableAttributedString(
            string: "\(number). \(candidate.text)",
            attributes: [
                .font: NSFont.systemFont(ofSize: Self.candidateFontSize),
                .foregroundColor: selected ? NSColor.white : NSColor.labelColor,
            ]
        )
        if let description = candidate.description {
            text.append(
                NSAttributedString(
                    string: "  \(description)",
                    attributes: [
                        .font: NSFont.systemFont(ofSize: Self.footerFontSize),
                        .foregroundColor: selected
                            ? NSColor.white.withAlphaComponent(0.8)
                            : NSColor.secondaryLabelColor,
                    ]
                ))
        }

        let label = NSTextField(labelWithAttributedString: text)
        label.translatesAutoresizingMaskIntoConstraints = false
        if selected {
            label.backgroundColor = NSColor.selectedContentBackgroundColor
            label.drawsBackground = true
        } else {
            label.backgroundColor = .clear
            label.drawsBackground = false
        }
        stackView.addArrangedSubview(label)
        rowViews.append(label)
    }

    private func addFooterLabel(_ text: String) {
        let label = NSTextField(labelWithString: text)
        label.font = NSFont.systemFont(ofSize: Self.footerFontSize)
        label.textColor = NSColor.secondaryLabelColor
        label.translatesAutoresizingMaskIntoConstraints = false
        stackView.addArrangedSubview(label)
        rowViews.append(label)
    }

    private var lastCursorRect: NSRect = .zero

    private func positionPanel(cursorRect: NSRect?) {
        if let rect = cursorRect {
            lastCursorRect = rect
        }
        let cursorRect = lastCursorRect

        stackView.layoutSubtreeIfNeeded()
        let contentSize = stackView.fittingSize
        let panelWidth = max(contentSize.width + 16, Self.minPanelWidth)
        let panelHeight = contentSize.height + 8

        guard cursorRect != .zero else {
            // No trustworthy anchor yet — keep hidden rather than flashing at
            // a stale coordinate from another window.
            panel.orderOut(nil)
            return
        }

        let visibleFrame =
            CandidatePanelPlacement.screen(for: cursorRect)?.visibleFrame ?? NSScreen.main?
            .visibleFrame ?? .zero
        let frame = CandidatePanelPlacement.frame(
            cursorRect: cursorRect,
            panelSize: NSSize(width: panelWidth, height: panelHeight),
            visibleFrame: visibleFrame
        )
        panel.setFrame(frame, display: true)
        panel.orderFront(nil)
    }
}
