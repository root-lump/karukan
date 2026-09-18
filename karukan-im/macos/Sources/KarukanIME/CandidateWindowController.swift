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
    private static let rowSpacing: CGFloat = 4
    private static let contentInsets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
    private static let panelVerticalPadding: CGFloat = 8
    private static let panelHorizontalPadding: CGFloat = 16

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
        stackView.spacing = Self.rowSpacing
        stackView.edgeInsets = Self.contentInsets
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

    /// `cursorRect: nil` reuses the rect from the previous `show` — the
    /// caller can skip its (synchronous, per-keystroke) client IPC while
    /// the panel is already on screen, since the composition anchor
    /// doesn't move mid-composition.
    func show(
        candidates: [CandidateItem], cursor: Int, page: Int, totalPages: Int, cursorRect: NSRect?
    ) {
        pageState = PageState(
            candidates: candidates, cursor: cursor, page: page, totalPages: totalPages)
        render(cursorRect: cursorRect)
    }

    /// Update the aux footer; re-renders in place if the window is visible.
    /// Pass `deferRender: true` when a `show`/`hide` follows in the same
    /// action batch, so the panel is rendered once per batch instead of
    /// once for the aux change and again for the candidates.
    func setAux(_ text: String?, deferRender: Bool = false) {
        auxText = text
        if !deferRender, panel.isVisible, pageState != nil {
            render(cursorRect: nil)
        }
    }

    func hide() {
        pageState = nil
        panel.orderOut(nil)
    }

    private func render(cursorRect: NSRect?) {
        clearRows()
        guard let state = pageState else {
            hide()
            return
        }
        // An empty list still renders while the aux footer has text: a
        // source-filtered view narrowed to an empty source must show its
        // 「候補なし」 footer, not a silently vanishing panel.
        if state.candidates.isEmpty && (auxText ?? "").isEmpty {
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
        let panelWidth = max(contentSize.width + Self.panelHorizontalPadding, Self.minPanelWidth)
        let panelHeight = contentSize.height + Self.panelVerticalPadding

        guard cursorRect != .zero else {
            panel.setFrame(
                NSRect(x: 100, y: 100, width: panelWidth, height: panelHeight), display: true)
            panel.orderFront(nil)
            return
        }

        let placement: PanelPlacement
        if let screen = Self.screen(containing: cursorRect) {
            placement = Self.placement(
                cursorRect: cursorRect,
                maxPanelHeight: Self.maxPanelHeight,
                screenVisibleFrame: screen.visibleFrame
            )
        } else {
            placement = .below
        }

        let originY: CGFloat
        switch placement {
        case .above:
            originY = cursorRect.maxY
        case .below:
            originY = cursorRect.minY - panelHeight
        }

        panel.setFrame(
            NSRect(x: cursorRect.origin.x, y: originY, width: panelWidth, height: panelHeight),
            display: true)
        panel.orderFront(nil)
    }

    /// Upper bound on the number of candidate rows: the engine sends one
    /// pre-paginated page, and the page size is `CandidateList::DEFAULT_PAGE_SIZE`
    /// (karukan-im/core/src/core/candidate.rs).
    private static let maxCandidateRows = 9
    /// Page indicator + aux line.
    private static let maxFooterRows = 2

    /// Height of the tallest panel this controller can render. Measured once:
    /// the row heights depend only on the fonts, which are constants.
    private static let maxPanelHeight: CGFloat = {
        let candidateRow = measuredRowHeight(font: .systemFont(ofSize: candidateFontSize))
        let footerRow = measuredRowHeight(font: .systemFont(ofSize: footerFontSize))
        let rows = maxCandidateRows + maxFooterRows
        return CGFloat(maxCandidateRows) * candidateRow
            + CGFloat(maxFooterRows) * footerRow
            + CGFloat(rows - 1) * rowSpacing
            + contentInsets.top + contentInsets.bottom
            + panelVerticalPadding
    }()

    private static func measuredRowHeight(font: NSFont) -> CGFloat {
        let label = NSTextField(labelWithString: "あ")
        label.font = font
        return label.fittingSize.height
    }

    /// Which side of the composition anchor the panel is placed on.
    enum PanelPlacement: Equatable {
        case below
        case above
    }

    /// Decide the side from the *maximum* panel height rather than the current
    /// one: the candidate count changes on every keystroke, and deciding from
    /// the current height makes the panel flip sides mid-composition.
    static func placement(
        cursorRect: NSRect, maxPanelHeight: CGFloat, screenVisibleFrame: NSRect
    ) -> PanelPlacement {
        let spaceBelow = cursorRect.minY - screenVisibleFrame.minY
        let spaceAbove = screenVisibleFrame.maxY - cursorRect.maxY
        if spaceBelow >= maxPanelHeight {
            return .below
        }
        if spaceAbove >= maxPanelHeight {
            return .above
        }
        // Neither side fits: take the roomier one, so the most candidates stay
        // visible. Ties keep the historical default.
        return spaceAbove > spaceBelow ? .above : .below
    }

    // `frame`, not `visibleFrame`: the cursor may sit under the menu bar or
    // Dock, and `visibleFrame` excludes those strips from every screen.
    private static func screen(containing rect: NSRect) -> NSScreen? {
        NSScreen.screens.first { $0.frame.intersects(rect) } ?? NSScreen.main
    }
}
