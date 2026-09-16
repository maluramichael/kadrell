import AppKit

enum Theme {
    /// Aktives Farbschema aus den Einstellungen, `applyAppearance` tauscht es zur Laufzeit.
    nonisolated(unsafe) static var current = ColorTheme.named(Settings.colorTheme)
    static var bg: NSColor { current.bg }
    static var panel: NSColor { current.panel }
    static var surface: NSColor { current.surface }
    static var line: NSColor { current.line }
    static var fg: NSColor { current.fg }
    static var sub: NSColor { current.sub }
    static var muted: NSColor { current.muted }
    static var running: NSColor { current.running }
    static var waiting: NSColor { current.waiting }
    static var idle: NSColor { current.idle }
    static var error: NSColor { current.error }
    static var detached: NSColor { current.detached }
    static var appearance: NSAppearance? { NSAppearance(named: current.dark ? .darkAqua : .aqua) }

    /// Gruppenfarbe. Die Paletten sind für dunklen Grund gemacht, auf hellem werden sie abgedunkelt, sonst ist der Name unlesbar.
    static func group(_ hex: String) -> NSColor {
        let c = NSColor(hexString: hex)
        return current.dark ? c : c.mixed(0.6, into: NSColor(hex: 0))
    }
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

/// Farbschema der ganzen Oberfläche. `ansi` sind die acht Terminal-Grundfarben, die hellen Varianten sind dieselben.
struct ColorTheme {
    let id: String, name: String, dark: Bool
    let bg, panel, surface, line, fg, sub, muted, detached, running, waiting, idle, error: NSColor
    let ansi: [NSColor]

    init(_ id: String, _ name: String, dark: Bool, _ c: [UInt32], ansi: [UInt32]) {
        self.id = id; self.name = name; self.dark = dark
        let n = c.map { NSColor(hex: $0) }
        (bg, panel, surface, line, fg, sub, muted, detached) = (n[0], n[1], n[2], n[3], n[4], n[5], n[6], n[7])
        (running, waiting, idle, error) = (n[8], n[9], n[10], n[11])
        self.ansi = ansi.map { NSColor(hex: $0) }
    }

    static func named(_ id: String) -> ColorTheme { all.first { $0.id == id } ?? all[0] }

    // Reihenfolge: bg, panel, surface, line, fg, sub, muted, detached, running, waiting, idle, error.
    static let all: [ColorTheme] = [
        ColorTheme("mocha", "Catppuccin Mocha", dark: true,
                   [0x11111b, 0x181825, 0x1e1e2e, 0x313244, 0xcdd6f4, 0xa6adc8, 0x6c7086, 0x45475a, 0x89b4fa, 0xf9e2af, 0xa6e3a1, 0xf38ba8],
                   ansi: [0x45475a, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xbac2de]),
        ColorTheme("latte", "Catppuccin Latte", dark: false,
                   [0xeff1f5, 0xe6e9ef, 0xdce0e8, 0xccd0da, 0x4c4f69, 0x6c6f85, 0x8c8fa1, 0xacb0be, 0x1e66f5, 0xdf8e1d, 0x40a02b, 0xd20f39],
                   ansi: [0x5c5f77, 0xd20f39, 0x40a02b, 0xdf8e1d, 0x1e66f5, 0xea76cb, 0x179299, 0xacb0be]),
        ColorTheme("monokai", "Monokai", dark: true,
                   [0x1e1f1c, 0x272822, 0x3e3d32, 0x49483e, 0xf8f8f2, 0xcfcfc2, 0x75715e, 0x5b5a4e, 0x66d9ef, 0xe6db74, 0xa6e22e, 0xf92672],
                   ansi: [0x272822, 0xf92672, 0xa6e22e, 0xf4bf75, 0x66d9ef, 0xae81ff, 0xa1efe4, 0xf8f8f2]),
        ColorTheme("dracula", "Dracula", dark: true,
                   [0x191a21, 0x21222c, 0x282a36, 0x44475a, 0xf8f8f2, 0xc6c8d1, 0x6272a4, 0x565970, 0xbd93f9, 0xf1fa8c, 0x50fa7b, 0xff5555],
                   ansi: [0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2]),
        ColorTheme("nord", "Nord", dark: true,
                   [0x2e3440, 0x3b4252, 0x434c5e, 0x4c566a, 0xeceff4, 0xd8dee9, 0x7b88a1, 0x4c566a, 0x88c0d0, 0xebcb8b, 0xa3be8c, 0xbf616a],
                   ansi: [0x3b4252, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xe5e9f0]),
        ColorTheme("gruvbox", "Gruvbox Dark", dark: true,
                   [0x1d2021, 0x282828, 0x32302f, 0x3c3836, 0xebdbb2, 0xd5c4a1, 0x928374, 0x504945, 0x83a598, 0xfabd2f, 0xb8bb26, 0xfb4934],
                   ansi: [0x282828, 0xcc241d, 0x98971a, 0xd79921, 0x458588, 0xb16286, 0x689d6a, 0xa89984]),
        ColorTheme("one-dark", "One Dark", dark: true,
                   [0x21252b, 0x282c34, 0x2c313a, 0x3e4451, 0xabb2bf, 0x9da5b4, 0x5c6370, 0x4b5263, 0x61afef, 0xe5c07b, 0x98c379, 0xe06c75],
                   ansi: [0x3f4451, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xd7dae0]),
        ColorTheme("solarized-dark", "Solarized Dark", dark: true,
                   [0x002b36, 0x00212b, 0x073642, 0x0e4552, 0x93a1a1, 0x839496, 0x586e75, 0x3b5560, 0x268bd2, 0xb58900, 0x859900, 0xdc322f],
                   ansi: [0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5]),
        ColorTheme("solarized-light", "Solarized Light", dark: false,
                   [0xfdf6e3, 0xeee8d5, 0xe4ddc8, 0xd9d2bd, 0x586e75, 0x657b83, 0x93a1a1, 0xc9c2ae, 0x268bd2, 0xb58900, 0x859900, 0xdc322f],
                   ansi: [0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5]),
        ColorTheme("github-light", "GitHub Light", dark: false,
                   [0xffffff, 0xf6f8fa, 0xeaeef2, 0xd0d7de, 0x1f2328, 0x424a53, 0x6e7781, 0xafb8c1, 0x0969da, 0x9a6700, 0x1a7f37, 0xcf222e],
                   ansi: [0x24292f, 0xcf222e, 0x116329, 0x4d2d00, 0x0550ae, 0x8250df, 0x1b7c83, 0x6e7781]),
    ]
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
