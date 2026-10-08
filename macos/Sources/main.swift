import AppKit
import ApplicationServices

private struct Setup {
    var draft: [String: Int]
    var panels: [SetupPanel]
    var overlays: [NSWindow]
}

final class App: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem!
    private var displays: [Display] = []
    private var saved: [String: Int] = [:]
    private var setup: Setup?
    private let fitter = WindowFitter()
    private var rebuildTimer: Timer?
    private var trustTimer: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        let loaded = Config.load()
        let firstRun = loaded == nil
        saved = loaded ?? [:]
        if let fit = saved.removeValue(forKey: Config.fitKey) {
            fitter.enabled = fit != 0
        }
        if firstRun {
            Config.setAutostart(true)
        }

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        if let button = statusItem.button {
            button.image = statusIcon()
            button.toolTip = "Trimbar"
            button.target = self
            button.action = #selector(statusClicked)
            button.sendAction(on: [.leftMouseUp, .rightMouseUp])
        }

        displays = Display.all()
        applyTargets()
        startFitterWhenTrusted(prompt: true)

        NotificationCenter.default.addObserver(
            self,
            selector: #selector(screensChanged),
            name: NSApplication.didChangeScreenParametersNotification,
            object: nil
        )
        if firstRun {
            openSetup()
        }
    }

    /// Launching the app again is the obvious way to get the setup back.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        openSetup()
        return false
    }

    private func startFitterWhenTrusted(prompt: Bool) {
        let options = ["AXTrustedCheckOptionPrompt": prompt] as CFDictionary
        if AXIsProcessTrustedWithOptions(options) {
            fitter.start()
            fitter.setTargets(targets())
            return
        }
        // The grant takes effect without a restart, but nothing announces it.
        trustTimer?.invalidate()
        trustTimer = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] timer in
            guard AXIsProcessTrusted() else { return }
            timer.invalidate()
            self?.startFitterWhenTrusted(prompt: false)
        }
    }

    private func height(for display: Display) -> Int {
        saved[display.id] ?? 0
    }

    private func targets() -> [Target] {
        displays.compactMap { d in
            let h = height(for: d)
            return h > 0 ? Target(display: d, height: h) : nil
        }
    }

    private func applyTargets() {
        fitter.setTargets(targets())
    }

    private func persist() {
        var map = saved
        map[Config.fitKey] = fitter.enabled ? 1 : 0
        Config.save(map)
    }

    // Displays were added, removed, rearranged or changed resolution, or the Dock moved. Wait for
    // the layout to settle; macOS sends several of these in a row.
    @objc private func screensChanged() {
        rebuildTimer?.invalidate()
        rebuildTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: false) { [weak self] _ in
            self?.rebuild()
        }
    }

    private func rebuild() {
        let draft = setup?.draft
        if draft != nil {
            closeSetup()
        }
        displays = Display.all()
        applyTargets()
        if let draft {
            openSetup(draft: draft)
        }
    }

    private func openSetup(draft initial: [String: Int] = [:]) {
        NSApp.activate(ignoringOtherApps: true)
        if let setup {
            setup.panels.first?.makeKeyAndOrderFront(nil)
            return
        }
        var draft = initial
        for d in displays where draft[d.id] == nil {
            draft[d.id] = height(for: d)
        }

        var panels: [SetupPanel] = []
        var overlays: [NSWindow] = []
        for (i, d) in displays.enumerated() {
            let overlay = makeOverlay()
            placeOverlay(overlay, on: d, height: draft[d.id] ?? 0)
            overlays.append(overlay)

            let panel = SetupPanel(index: i, display: d)
            panel.show(height: draft[d.id] ?? 0)
            panel.onAdjust = { [weak self] delta in self?.adjust(i, by: delta) }
            panel.onFinish = { [weak self] save in self?.finishSetup(save: save) }
            panel.orderFrontRegardless()
            panels.append(panel)
        }
        // Start on a side display: the main screen is rarely the broken one.
        let first = displays.firstIndex { !$0.primary } ?? 0
        if panels.indices.contains(first) {
            panels[first].makeKeyAndOrderFront(nil)
        }
        setup = Setup(draft: draft, panels: panels, overlays: overlays)
    }

    private func adjust(_ index: Int, by delta: Int) {
        guard var current = setup, displays.indices.contains(index) else { return }
        let d = displays[index]
        let value = min(max((current.draft[d.id] ?? 0) + delta, 0), d.height / 3)
        current.draft[d.id] = value
        setup = current
        placeOverlay(current.overlays[index], on: d, height: value)
        current.panels[index].show(height: value)
    }

    @discardableResult
    private func closeSetup() -> Setup? {
        guard let current = setup else { return nil }
        setup = nil
        current.panels.forEach { $0.close() }
        current.overlays.forEach { $0.close() }
        return current
    }

    private func finishSetup(save: Bool) {
        // Deferred: this runs from inside a panel's event handling, and closing it there is unsafe.
        DispatchQueue.main.async { [self] in
            guard let finished = closeSetup(), save else { return }
            // Merge rather than replace: keeps trims for displays that are disconnected right now.
            saved.merge(finished.draft) { _, new in new }
            persist()
            applyTargets()
        }
    }

    @objc private func statusClicked() {
        let event = NSApp.currentEvent
        if event?.type == .rightMouseUp || event?.modifierFlags.contains(.control) == true {
            showMenu()
        } else {
            openSetup()
        }
    }

    private func showMenu() {
        let menu = NSMenu()
        menu.addItem(item("Adjust trim…", #selector(menuAdjust)))
        menu.addItem(item("Start at login", #selector(menuAutostart), checked: Config.autostartEnabled))
        menu.addItem(item("Keep windows above the trim", #selector(menuFit), checked: fitter.enabled))
        if !AXIsProcessTrusted() {
            menu.addItem(item("Allow window control…", #selector(menuAccessibility)))
        }
        menu.addItem(.separator())
        menu.addItem(item("Quit Trimbar", #selector(menuQuit)))
        guard let button = statusItem.button else { return }
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: button.bounds.height + 4), in: button)
    }

    private func item(_ title: String, _ action: Selector, checked: Bool = false) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
        item.target = self
        item.state = checked ? .on : .off
        return item
    }

    @objc private func menuAdjust() { openSetup() }

    @objc private func menuAutostart() {
        Config.setAutostart(!Config.autostartEnabled)
    }

    @objc private func menuFit() {
        fitter.enabled.toggle()
        persist()
        fitter.fitAll()
    }

    @objc private func menuAccessibility() {
        let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        NSWorkspace.shared.open(URL(string: url)!)
    }

    @objc private func menuQuit() {
        NSApp.terminate(nil)
    }
}

/// A monitor with its bottom rows blacked out, drawn as a template so it follows the menu bar's colour.
private func statusIcon() -> NSImage {
    let image = NSImage(size: NSSize(width: 18, height: 18), flipped: true) { _ in
        NSColor.black.set()
        let screen = NSBezierPath(roundedRect: NSRect(x: 1.75, y: 2.75, width: 14.5, height: 10.5), xRadius: 1.5, yRadius: 1.5)
        screen.lineWidth = 1.5
        screen.stroke()
        NSRect(x: 2.5, y: 9.5, width: 13, height: 3.5).fill()
        NSRect(x: 8, y: 13.5, width: 2, height: 1.5).fill()
        NSBezierPath(roundedRect: NSRect(x: 5, y: 14.5, width: 8, height: 1.5), xRadius: 0.75, yRadius: 0.75).fill()
        return true
    }
    image.isTemplate = true
    return image
}

let app = NSApplication.shared
let delegate = App()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
