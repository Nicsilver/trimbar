// macOS has no appbar API, so nothing can shrink the area windows zoom and tile into. Instead
// this watches every app's windows through the Accessibility API and pushes any window that
// ends inside a trimmed strip back above it.

import AppKit
import ApplicationServices

/// A trimmed display in Accessibility coordinates: origin at the top left of the main display, y down.
struct Target {
    let left: CGFloat
    let right: CGFloat
    let top: CGFloat
    let bottom: CGFloat
    /// First dead row; fitted windows end here.
    let cut: CGFloat
    let visibleTop: CGFloat
    let visibleBottom: CGFloat

    init(display: Display, height: Int) {
        let mainTop = NSScreen.screens.first?.frame.maxY ?? 0
        left = display.frame.minX
        right = display.frame.maxX
        top = mainTop - display.frame.maxY
        bottom = mainTop - display.frame.minY
        cut = bottom - CGFloat(height)
        visibleTop = mainTop - display.visibleFrame.maxY
        visibleBottom = mainTop - display.visibleFrame.minY
    }
}

/// A window that snaps back this often within `fightWindow` is left alone, so we never get into
/// a resize war with an app that insists on its size.
private let fightLimit = 5
private let fightWindow: TimeInterval = 10

private let notifications = [
    kAXWindowCreatedNotification,
    kAXWindowMovedNotification,
    kAXWindowResizedNotification,
]

private let onNotification: AXObserverCallback = { _, element, _, refcon in
    guard let refcon else { return }
    Unmanaged<WindowFitter>.fromOpaque(refcon).takeUnretainedValue().changed(element)
}

final class WindowFitter {
    var enabled = true
    private var targets: [Target] = []
    private var observers: [pid_t: AXObserver] = [:]
    private var started = false
    private var pending: [AXUIElement] = []
    private var settleTimer: Timer?
    private var fights: [UInt: (start: Date, count: Int)] = [:]
    private var gaveUp: Set<UInt> = []

    func start() {
        guard !started else { return }
        started = true
        // A hung app must not freeze the menu bar icon.
        AXUIElementSetMessagingTimeout(AXUIElementCreateSystemWide(), 0.5)
        let center = NSWorkspace.shared.notificationCenter
        center.addObserver(forName: NSWorkspace.didLaunchApplicationNotification, object: nil, queue: .main) { [weak self] n in
            if let app = n.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication {
                self?.watch(app)
            }
        }
        center.addObserver(forName: NSWorkspace.didTerminateApplicationNotification, object: nil, queue: .main) { [weak self] n in
            if let app = n.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication {
                self?.unwatch(app.processIdentifier)
            }
        }
        NSWorkspace.shared.runningApplications.forEach { watch($0) }
    }

    func setTargets(_ targets: [Target]) {
        self.targets = targets
        fitAll()
    }

    func fitAll() {
        for pid in observers.keys {
            windows(of: AXUIElementCreateApplication(pid)).forEach(fit)
        }
    }

    private func watch(_ app: NSRunningApplication, attempt: Int = 0) {
        let pid = app.processIdentifier
        guard app.activationPolicy == .regular, !app.isTerminated, pid != getpid(), observers[pid] == nil else { return }
        var created: AXObserver?
        guard AXObserverCreate(pid, onNotification, &created) == .success, let observer = created else { return }
        let element = AXUIElementCreateApplication(pid)
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        let results = notifications.map { AXObserverAddNotification(observer, element, $0 as CFString, refcon) }
        // A freshly launched app only answers accessibility requests once it has finished starting.
        if results.contains(.cannotComplete) {
            if attempt < 10 {
                DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
                    self?.watch(app, attempt: attempt + 1)
                }
            }
            return
        }
        CFRunLoopAddSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(observer), .commonModes)
        observers[pid] = observer
        windows(of: element).forEach(fit)
    }

    private func unwatch(_ pid: pid_t) {
        guard let observer = observers.removeValue(forKey: pid) else { return }
        CFRunLoopRemoveSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(observer), .commonModes)
    }

    fileprivate func changed(_ element: AXUIElement) {
        if !pending.contains(where: { CFEqual($0, element) }) {
            pending.append(element)
        }
        settle(after: 0.25)
    }

    /// Windows move and resize in bursts (drags, zoom and tiling animations). Act once they stop,
    /// and never while a mouse button is held, or we'd yank a window out from under a drag.
    private func settle(after delay: TimeInterval) {
        settleTimer?.invalidate()
        settleTimer = Timer.scheduledTimer(withTimeInterval: delay, repeats: false) { [weak self] _ in
            guard let self else { return }
            if NSEvent.pressedMouseButtons != 0 {
                self.settle(after: 0.25)
                return
            }
            let windows = self.pending
            self.pending.removeAll()
            windows.forEach(self.fit)
        }
    }

    private func fit(_ window: AXUIElement) {
        guard enabled, !targets.isEmpty,
              string(window, kAXRoleAttribute) == kAXWindowRole,
              string(window, kAXSubroleAttribute) == kAXStandardWindowSubrole,
              bool(window, "AXFullScreen") != true,
              bool(window, kAXMinimizedAttribute) != true,
              let frame = windowFrame(window)
        else { return }

        // The display holding the title bar is the one the window lives on.
        let anchor = CGPoint(x: frame.midX, y: frame.minY)
        guard let t = targets.first(where: {
            anchor.x >= $0.left && anchor.x < $0.right && anchor.y >= $0.top && anchor.y < $0.bottom
        }) else {
            forgive(window)
            return
        }
        if frame.maxY < t.cut - 1 {
            // Clear of the strip, and not just sitting where we put it: forgive it, so an app that
            // once fought back still gets fitted next time.
            forgive(window)
            return
        }
        // Already fitted, or deliberately pushed past the bottom edge.
        if frame.maxY <= t.cut + 0.5 || frame.maxY > t.bottom + 1 {
            return
        }
        guard noteAttempt(window) else { return }

        // Zoomed and tiled windows end on the visible frame's bottom edge: shrink them so they keep
        // lining up with their neighbours. A window that was merely dragged low keeps its size.
        let edgeAligned = abs(frame.maxY - t.visibleBottom) <= 1
        if edgeAligned && frame.minY < t.cut {
            setSize(window, CGSize(width: frame.width, height: t.cut - frame.minY))
        } else if frame.height > t.cut - t.visibleTop {
            setSize(window, CGSize(width: frame.width, height: t.cut - t.visibleTop))
        }
        // Moves windows that were only dragged low, and apps with a minimum height that can't shrink enough.
        if let now = windowFrame(window), now.maxY > t.cut + 0.5 {
            setPosition(window, CGPoint(x: now.minX, y: max(t.cut - now.height, t.visibleTop)))
        }
    }

    private func forgive(_ window: AXUIElement) {
        let key = CFHash(window)
        fights.removeValue(forKey: key)
        gaveUp.remove(key)
    }

    /// Returns false once a window has fought back too often.
    private func noteAttempt(_ window: AXUIElement) -> Bool {
        let key = CFHash(window)
        if gaveUp.contains(key) {
            return false
        }
        if fights.count > 256 {
            fights.removeAll()
        }
        let now = Date()
        var entry = fights[key] ?? (now, 0)
        if now.timeIntervalSince(entry.start) > fightWindow {
            entry = (now, 0)
        }
        entry.count += 1
        fights[key] = entry
        if entry.count > fightLimit {
            if gaveUp.count > 256 {
                gaveUp.removeAll()
            }
            gaveUp.insert(key)
            return false
        }
        return true
    }
}

private func windows(of app: AXUIElement) -> [AXUIElement] {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &value) == .success else { return [] }
    return value as? [AXUIElement] ?? []
}

private func string(_ element: AXUIElement, _ name: String) -> String? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else { return nil }
    return value as? String
}

private func bool(_ element: AXUIElement, _ name: String) -> Bool? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else { return nil }
    return (value as? NSNumber)?.boolValue
}

private func windowFrame(_ window: AXUIElement) -> CGRect? {
    var origin = CGPoint.zero
    var size = CGSize.zero
    guard read(window, kAXPositionAttribute, .cgPoint, &origin), read(window, kAXSizeAttribute, .cgSize, &size) else {
        return nil
    }
    return CGRect(origin: origin, size: size)
}

private func read<T: BitwiseCopyable>(_ element: AXUIElement, _ name: String, _ type: AXValueType, _ out: inout T) -> Bool {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success,
          let value, CFGetTypeID(value) == AXValueGetTypeID()
    else { return false }
    return AXValueGetValue(value as! AXValue, type, &out)
}

private func setSize(_ window: AXUIElement, _ size: CGSize) {
    var size = size
    if let value = AXValueCreate(.cgSize, &size) {
        AXUIElementSetAttributeValue(window, kAXSizeAttribute as CFString, value)
    }
}

private func setPosition(_ window: AXUIElement, _ point: CGPoint) {
    var point = point
    if let value = AXValueCreate(.cgPoint, &point) {
        AXUIElementSetAttributeValue(window, kAXPositionAttribute as CFString, value)
    }
}
