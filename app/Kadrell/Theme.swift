import AppKit

enum Theme {
    static let bg = NSColor(hex: 0x11111b)
    static let panel = NSColor(hex: 0x181825)
    static let surface = NSColor(hex: 0x1e1e2e)
    static let line = NSColor(hex: 0x313244)
    static let fg = NSColor(hex: 0xcdd6f4)
    static let sub = NSColor(hex: 0xa6adc8)
    static let muted = NSColor(hex: 0x6c7086)
    static let running = NSColor(hex: 0x89b4fa)
    static let waiting = NSColor(hex: 0xf9e2af)
    static let idle = NSColor(hex: 0xa6e3a1)
    static let error = NSColor(hex: 0xf38ba8)
    static let detached = NSColor(hex: 0x45475a)
    static let palette = ["#fab387", "#cba6f7", "#f5c2e7", "#89b4fa", "#a6e3a1",
                          "#94e2d5", "#f9e2af", "#eba0ac", "#b4befe", "#74c7ec"]
    static let barHeight: CGFloat = 30

    static func color(for status: SessionStatus) -> NSColor {
        switch status {
        case .running: running
        case .waiting: waiting
        case .idle: idle
        case .error: error
        }
    }

    static func font(_ size: CGFloat, bold: Bool = false) -> NSFont {
        NSFont(name: bold ? "JetBrainsMonoNF-Bold" : "JetBrainsMonoNF-Regular", size: size)
            ?? NSFont.monospacedSystemFont(ofSize: size, weight: bold ? .bold : .regular)
    }

    static func attrs(_ size: CGFloat, _ color: NSColor, bold: Bool = false, truncate: Bool = true) -> [NSAttributedString.Key: Any] {
        let p = NSMutableParagraphStyle()
        p.lineBreakMode = truncate ? .byTruncatingTail : .byWordWrapping
        return [.font: font(size, bold: bold), .foregroundColor: color, .paragraphStyle: p]
    }

    /// `/Users/dev/development/x` wird zu `~/dev/x`, wie im Prototyp.
    static func shortPath(_ path: String) -> String {
        let home = NSHomeDirectory()
        var p = path
        if p.hasPrefix(home) { p = "~" + p.dropFirst(home.count) }
        if p.hasPrefix("~/development/") { p = "~/dev/" + p.dropFirst("~/development/".count) }
        return p
    }
}

extension NSColor {
    convenience init(hex: UInt32) {
        self.init(srgbRed: CGFloat((hex >> 16) & 0xff) / 255, green: CGFloat((hex >> 8) & 0xff) / 255,
                  blue: CGFloat(hex & 0xff) / 255, alpha: 1)
    }

    convenience init(hexString: String) {
        var s = hexString.trimmingCharacters(in: .whitespaces)
        if s.hasPrefix("#") { s.removeFirst() }
        self.init(hex: UInt32(s, radix: 16) ?? 0x6c7086)
    }

    var hexString: String {
        let c = usingColorSpace(.sRGB) ?? self
        return String(format: "#%02x%02x%02x", Int(round(c.redComponent * 255)), Int(round(c.greenComponent * 255)), Int(round(c.blueComponent * 255)))
    }

    /// Entspricht `color-mix(in srgb, self fraction, other)`.
    func mixed(_ fraction: CGFloat, into other: NSColor) -> NSColor {
        let a = usingColorSpace(.sRGB) ?? self, b = other.usingColorSpace(.sRGB) ?? other
        return NSColor(srgbRed: a.redComponent * fraction + b.redComponent * (1 - fraction),
                       green: a.greenComponent * fraction + b.greenComponent * (1 - fraction),
                       blue: a.blueComponent * fraction + b.blueComponent * (1 - fraction), alpha: 1)
    }
}
