import AppKit

struct Display {
    /// CoreGraphics display UUID. Unlike the display number it survives reconnects and
    /// rearranging, and two identical monitors with serial numbers still get different ids.
    let id: String
    let label: String
    /// Cocoa coordinates, in points.
    let frame: NSRect
    let visibleFrame: NSRect
    let primary: Bool

    var width: Int { Int(frame.width) }
    var height: Int { Int(frame.height) }

    static func all() -> [Display] {
        NSScreen.screens.compactMap { screen in
            guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber else {
                return nil
            }
            let displayID = CGDirectDisplayID(number.uint32Value)
            let uuid = CGDisplayCreateUUIDFromDisplayID(displayID)?.takeRetainedValue()
            return Display(
                id: uuid.map { CFUUIDCreateString(nil, $0) as String } ?? "display-\(displayID)",
                label: screen.localizedName,
                frame: screen.frame,
                visibleFrame: screen.visibleFrame,
                primary: CGDisplayIsMain(displayID) != 0
            )
        }
        .sorted { ($0.frame.minX, $0.frame.minY) < ($1.frame.minX, $1.frame.minY) }
    }

    var title: String {
        "\(label)  ·  \(width)×\(height)\(primary ? "  ·  main" : "")"
    }
}
