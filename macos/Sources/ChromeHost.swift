// Browser fullscreen gets its own space that no other app can resize, so the Trimbar extension
// shrinks fullscreen pages from inside the browser instead. Chromium browsers launch this same
// binary as the extension's native messaging host to learn the trims.

import AppKit

enum ChromeHost {
    private static let name = "io.github.nicsilver.trimbar"
    /// Fixed by the `key` in chrome-extension/manifest.json.
    private static let origin = "chrome-extension://gjobilhlpleapalpehhpmaglijlfcmhe/"
    private static let browserDirs = [
        "Google/Chrome",
        "Google/Chrome Beta",
        "Google/Chrome Canary",
        "Chromium",
        "Microsoft Edge",
        "BraveSoftware/Brave-Browser",
        "Vivaldi",
        "Arc/User Data",
    ]

    private static var support: URL {
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support")
    }

    static var extensionFolder: URL {
        support.appendingPathComponent("Trimbar/chrome-extension")
    }

    static var isInvoked: Bool {
        CommandLine.arguments.dropFirst().contains { $0.hasPrefix("chrome-extension://") }
    }

    /// Answers the extension's single request with the trimmed displays, in the browser's screen
    /// coordinates (points, origin at the top left of the main display), and exits.
    static func serve() -> Never {
        let input = FileHandle.standardInput
        let header = input.readData(ofLength: 4)
        if header.count == 4 {
            _ = input.readData(ofLength: Int(header.withUnsafeBytes { $0.loadUnaligned(as: UInt32.self) }))
        }

        let saved = Config.load() ?? [:]
        var targets: [[String: Double]] = []
        if (saved[Config.fitKey] ?? 1) != 0 {
            for display in Display.all() {
                guard let height = saved[display.id], height > 0 else { continue }
                let t = Target(display: display, height: height)
                targets.append([
                    "left": Double(t.left),
                    "top": Double(t.top),
                    "width": Double(t.right - t.left),
                    "height": Double(t.bottom - t.top),
                    "trim": Double(height),
                ])
            }
        }

        let body = (try? JSONSerialization.data(withJSONObject: ["targets": targets])) ?? Data("{}".utf8)
        var length = UInt32(body.count)
        let output = FileHandle.standardOutput
        output.write(Data(bytes: &length, count: 4))
        output.write(body)
        exit(0)
    }

    /// Chromium browsers only launch hosts listed in their own NativeMessagingHosts folder.
    /// Rewritten on every start so the entry follows the app if it is moved.
    static func register() {
        guard let executable = Bundle.main.executablePath else { return }
        let manifest: [String: Any] = [
            "name": name,
            "description": "Trimbar",
            "path": executable,
            "type": "stdio",
            "allowed_origins": [origin],
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: manifest, options: [.prettyPrinted, .withoutEscapingSlashes]) else {
            return
        }
        let files = FileManager.default
        for dir in browserDirs {
            let browser = support.appendingPathComponent(dir)
            guard files.fileExists(atPath: browser.path) else { continue }
            let hosts = browser.appendingPathComponent("NativeMessagingHosts")
            try? files.createDirectory(at: hosts, withIntermediateDirectories: true)
            try? data.write(to: hosts.appendingPathComponent("\(name).json"))
        }
    }

    /// Keeps a copy of the bundled extension at a fixed path that is easy to pick in Chrome's
    /// "Load unpacked" dialog, refreshed so it updates along with the app.
    static func syncExtension() {
        guard let bundled = Bundle.main.resourceURL?.appendingPathComponent("chrome-extension"),
              FileManager.default.fileExists(atPath: bundled.path)
        else { return }
        let files = FileManager.default
        try? files.createDirectory(at: extensionFolder.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? files.removeItem(at: extensionFolder)
        try? files.copyItem(at: bundled, to: extensionFolder)
    }

    static func showInstallSteps() {
        NSApp.activate(ignoringOtherApps: true)
        let alert = NSAlert()
        alert.messageText = "Install the Trimbar extension"
        alert.informativeText = """
            Fullscreen video in Chrome (and Edge, Brave, Vivaldi, Arc) needs a small extension to stay above the trim.

            1. On the extensions page, turn on Developer mode (top right).
            2. Click Load unpacked.
            3. Pick the chrome-extension folder that Finder shows.
            """
        alert.addButton(withTitle: "Open Chrome")
        alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        NSWorkspace.shared.activateFileViewerSelecting([extensionFolder])
        let chrome = URL(fileURLWithPath: "/Applications/Google Chrome.app")
        NSWorkspace.shared.open(
            [URL(string: "chrome://extensions")!],
            withApplicationAt: chrome,
            configuration: NSWorkspace.OpenConfiguration()
        )
    }
}
