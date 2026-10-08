import Foundation
import ServiceManagement

enum Config {
    /// Stored alongside the per-display heights; display UUIDs never collide with it.
    static let fitKey = "fit_windows"

    private static var url: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/Trimbar/config.txt")
    }

    /// `nil` means Trimbar has never been set up on this Mac.
    static func load() -> [String: Int]? {
        guard let text = try? String(contentsOf: url, encoding: .utf8) else { return nil }
        var map: [String: Int] = [:]
        for raw in text.split(whereSeparator: \.isNewline) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            guard !line.isEmpty, !line.hasPrefix("#"), let eq = line.lastIndex(of: "=") else { continue }
            guard let value = Int(line[line.index(after: eq)...].trimmingCharacters(in: .whitespaces)) else { continue }
            map[line[..<eq].trimmingCharacters(in: .whitespaces)] = value
        }
        return map
    }

    static func save(_ map: [String: Int]) {
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        var text = "# Trimbar: points hidden at the bottom of each display\n"
        for key in map.keys.sorted() {
            text += "\(key)=\(map[key]!)\n"
        }
        try? text.write(to: url, atomically: true, encoding: .utf8)
    }

    static var autostartEnabled: Bool {
        SMAppService.mainApp.status == .enabled
    }

    static func setAutostart(_ on: Bool) {
        if on {
            try? SMAppService.mainApp.register()
        } else {
            try? SMAppService.mainApp.unregister()
        }
    }
}
