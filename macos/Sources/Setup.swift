// The per-display setup panel and red preview bar shown while adjusting the trim.

import AppKit

let overlayColor = NSColor(srgbRed: 1, green: 45 / 255, blue: 85 / 255, alpha: 1)

private let panelSize = NSSize(width: 480, height: 252)

/// Flipped so the layout reads top to bottom.
private final class FlippedView: NSView {
    override var isFlipped: Bool { true }
}

final class SetupPanel: NSPanel {
    let index: Int
    var onAdjust: (Int) -> Void = { _ in }
    var onFinish: (Bool) -> Void = { _ in }

    private let titleLabel = NSTextField(labelWithString: "")
    private let valueLabel = NSTextField(labelWithString: "")
    private var scrollAccumulator: CGFloat = 0

    init(index: Int, display: Display) {
        self.index = index
        let origin = NSPoint(
            x: display.frame.minX + (display.frame.width - panelSize.width) / 2,
            y: display.frame.maxY - (display.frame.height - panelSize.height) * 2 / 5 - panelSize.height
        )
        super.init(
            contentRect: NSRect(origin: origin, size: panelSize),
            styleMask: [.titled, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        titlebarAppearsTransparent = true
        titleVisibility = .hidden
        for button in [NSWindow.ButtonType.closeButton, .miniaturizeButton, .zoomButton] {
            standardWindowButton(button)?.isHidden = true
        }
        appearance = NSAppearance(named: .darkAqua)
        level = .floating
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        isReleasedWhenClosed = false
        hidesOnDeactivate = false
        isMovableByWindowBackground = true

        let background = NSVisualEffectView()
        background.material = .hudWindow
        background.state = .active
        background.blendingMode = .behindWindow
        contentView = background

        let root = FlippedView(frame: NSRect(origin: .zero, size: panelSize))
        root.autoresizingMask = [.width, .height]
        background.addSubview(root)

        titleLabel.stringValue = display.title
        titleLabel.font = .systemFont(ofSize: 13, weight: .semibold)
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.frame = NSRect(x: 20, y: 16, width: panelSize.width - 40, height: 20)
        root.addSubview(titleLabel)

        valueLabel.font = .systemFont(ofSize: 44, weight: .semibold)
        valueLabel.frame = NSRect(x: 18, y: 40, width: panelSize.width - 40, height: 58)
        root.addSubview(valueLabel)

        let help = NSTextField(wrappingLabelWithString:
            "Raise it until a red bar shows at the bottom of this screen, then lower it until the red is just gone.")
        help.font = .systemFont(ofSize: 13)
        help.frame = NSRect(x: 20, y: 106, width: panelSize.width - 40, height: 40)
        root.addSubview(help)

        let steps: [(String, Int)] = [("−10", -10), ("−1", -1), ("+1", 1), ("+10", 10)]
        for (i, (title, delta)) in steps.enumerated() {
            let button = makeButton(title, NSRect(x: 16 + i * 60, y: 160, width: 58, height: 32), #selector(step(_:)))
            button.tag = delta
            root.addSubview(button)
        }
        let cancel = makeButton("Cancel", NSRect(x: Int(panelSize.width) - 208, y: 160, width: 96, height: 32), #selector(cancel))
        cancel.keyEquivalent = "\u{1b}"
        root.addSubview(cancel)
        let save = makeButton("Save", NSRect(x: Int(panelSize.width) - 110, y: 160, width: 94, height: 32), #selector(save))
        save.keyEquivalent = "\r"
        root.addSubview(save)

        let hint = NSTextField(labelWithString: "↑ ↓ or scroll: 1 pt  ·  Shift: 10 pt  ·  Return: save  ·  Esc: cancel")
        hint.font = .systemFont(ofSize: 11)
        hint.textColor = .secondaryLabelColor
        hint.frame = NSRect(x: 20, y: 214, width: panelSize.width - 40, height: 18)
        root.addSubview(hint)

        setFrameOrigin(origin)
        refreshFocus()
    }

    private func makeButton(_ title: String, _ frame: NSRect, _ action: Selector) -> NSButton {
        let button = NSButton(title: title, target: self, action: action)
        button.bezelStyle = .rounded
        button.controlSize = .large
        button.frame = frame
        return button
    }

    override var canBecomeKey: Bool { true }

    // AppKit would otherwise pull every panel onto the screen it was created on.
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }

    func show(height: Int) {
        valueLabel.stringValue = height == 0 ? "Off" : "\(height) pt"
    }

    private func refreshFocus() {
        titleLabel.textColor = isKeyWindow ? .controlAccentColor : .secondaryLabelColor
        valueLabel.textColor = isKeyWindow ? .labelColor : .secondaryLabelColor
    }

    override func becomeKey() {
        super.becomeKey()
        refreshFocus()
    }

    override func resignKey() {
        super.resignKey()
        refreshFocus()
    }

    private var stepSize: Int {
        NSEvent.modifierFlags.contains(.shift) ? 10 : 1
    }

    @objc private func step(_ sender: NSButton) { onAdjust(sender.tag) }
    @objc private func cancel() { onFinish(false) }
    @objc private func save() { onFinish(true) }

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case 126, 124: onAdjust(stepSize) // up, right
        case 125, 123: onAdjust(-stepSize) // down, left
        case 116: onAdjust(10) // page up
        case 121: onAdjust(-10) // page down
        case 36, 76: onFinish(true) // return, enter
        case 53: onFinish(false) // esc
        default: super.keyDown(with: event)
        }
    }

    override func scrollWheel(with event: NSEvent) {
        // Shift turns a mouse wheel into horizontal scrolling.
        var delta = event.scrollingDeltaY != 0 ? event.scrollingDeltaY : event.scrollingDeltaX
        if event.isDirectionInvertedFromDevice {
            delta = -delta
        }
        if !event.hasPreciseScrollingDeltas {
            if delta != 0 { onAdjust(delta > 0 ? stepSize : -stepSize) }
            return
        }
        // Trackpads send a stream of small deltas; one step per few points of travel.
        scrollAccumulator += delta
        while abs(scrollAccumulator) >= 6 {
            onAdjust(scrollAccumulator > 0 ? stepSize : -stepSize)
            scrollAccumulator -= scrollAccumulator > 0 ? 6 : -6
        }
    }
}

func makeOverlay() -> NSWindow {
    let window = NSWindow(contentRect: .zero, styleMask: .borderless, backing: .buffered, defer: false)
    window.backgroundColor = overlayColor
    // Above the Dock and menu bar, so the bar shows even where they sit in the dead rows.
    window.level = .screenSaver
    window.ignoresMouseEvents = true
    window.hasShadow = false
    window.isReleasedWhenClosed = false
    window.collectionBehavior = [.canJoinAllSpaces, .stationary, .fullScreenAuxiliary, .ignoresCycle]
    return window
}

func placeOverlay(_ window: NSWindow, on display: Display, height: Int) {
    if height <= 0 {
        window.orderOut(nil)
        return
    }
    let f = display.frame
    window.setFrame(NSRect(x: f.minX, y: f.minY, width: f.width, height: CGFloat(height)), display: true)
    window.orderFrontRegardless()
}
