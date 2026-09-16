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
    /// Neue Gruppen bekommen reihum eine noch freie Farbe von hier.
    static let palette = palettes[0].colors
    /// Auswahl im Dialog „Gruppe bearbeiten“. Werte aus den offiziellen Paletten übernommen.
    static let palettes: [(name: String, colors: [String])] = [
        ("Catppuccin", ["#fab387", "#cba6f7", "#f5c2e7", "#89b4fa", "#a6e3a1", "#94e2d5", "#f9e2af",
                        "#eba0ac", "#b4befe", "#74c7ec", "#89dceb", "#f38ba8", "#f2cdcd", "#f5e0dc"]),
        ("Tailwind", ["#f87171", "#fb923c", "#fbbf24", "#facc15", "#a3e635", "#4ade80", "#34d399", "#2dd4bf", "#22d3ee",
                      "#38bdf8", "#60a5fa", "#818cf8", "#a78bfa", "#c084fc", "#e879f9", "#f472b6", "#fb7185"]),
        ("Nord", ["#8fbcbb", "#88c0d0", "#81a1c1", "#5e81ac", "#bf616a", "#d08770", "#ebcb8b", "#a3be8c", "#b48ead"]),
        ("Dracula", ["#8be9fd", "#50fa7b", "#ffb86c", "#ff79c6", "#bd93f9", "#ff5555", "#f1fa8c"]),
        ("Gruvbox", ["#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#fe8019",
                     "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#d65d0e"]),
        ("Rosé Pine", ["#eb6f92", "#f6c177", "#ebbcba", "#31748f", "#9ccfd8", "#c4a7e7"]),
        ("Okabe-Ito", ["#e69f00", "#56b4e9", "#009e73", "#f0e442", "#0072b2", "#d55e00", "#cc79a7"]),
    ]
    /// UI-Größe aus den Einstellungen. Gezeichnete Views skalieren per `scaled`, SwiftUI per `ui`, Terminals nicht.
    nonisolated(unsafe) static var scale = CGFloat(Settings.uiScale)
    static var barHeight: CGFloat { (30 * scale).rounded() }

    /// Zeichnet `body` in unskalierten Punkten: Koordinatensystem an `r` verschoben und um `scale` vergrößert.
    @MainActor static func scaled(_ r: CGRect, _ body: (CGRect) -> Void) {
        NSGraphicsContext.saveGraphicsState()
        let t = NSAffineTransform()
        t.translateX(by: r.minX, yBy: r.minY)
        t.scale(by: scale)
        t.concat()
        body(CGRect(x: 0, y: 0, width: r.width / scale, height: r.height / scale))
        NSGraphicsContext.restoreGraphicsState()
    }

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

    /// Home wird zu `~`, sonst bleibt der Pfad wie er ist.
    static func shortPath(_ path: String) -> String {
        let home = NSHomeDirectory()
        return path.hasPrefix(home) ? "~" + path.dropFirst(home.count) : path
    }

    /// `shortPath`, bei Platzmangel von vorn Ordner auf den Anfangsbuchstaben gekürzt (`~/d/p/projekt`).
    /// Der letzte Ordner bleibt ganz, reicht es dann immer noch nicht, schneidet das Zeichnen ab.
    static func fitPath(_ path: String, width: CGFloat, attrs: [NSAttributedString.Key: Any]) -> String {
        var parts = shortPath(path).split(separator: "/", omittingEmptySubsequences: false).map(String.init)
        var s = parts.joined(separator: "/")
        for i in parts.indices.dropLast() where NSAttributedString(string: s, attributes: attrs).size().width > width {
            guard parts[i].count > 1 else { continue }
            parts[i] = String(parts[i].prefix(parts[i].hasPrefix(".") ? 2 : 1))   // .claude wird .c, nicht nur .
            s = parts.joined(separator: "/")
        }
        return s
    }
}

extension CGRect {
    func scaled(_ s: CGFloat) -> CGRect { CGRect(x: minX * s, y: minY * s, width: width * s, height: height * s) }
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
