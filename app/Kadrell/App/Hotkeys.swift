import AppKit

/// Virtuelle Tastencodes (US-Layout-Position), statt nackter Zahlen in `keyDown`.
enum KeyCode {
    static let returnKey: UInt16 = 36, space: UInt16 = 49, delete: UInt16 = 51, escape: UInt16 = 53, keypadEnter: UInt16 = 76
    static let f1: UInt16 = 122, f3: UInt16 = 99, f10: UInt16 = 109, left: UInt16 = 123, right: UInt16 = 124, down: UInt16 = 125, up: UInt16 = 126
}

/// Frei belegbare Kürzel, Defaults nach Michaels tmux-Config (M-Pfeile, M-z, M-1…9, M-Tab, M-Enter).
/// Der Event-Monitor im AppDelegate fängt sie fensterweit ab, bevor das Terminal sie sieht.
enum HotkeyAction: String, CaseIterable, Sendable {
    case focusLeft, focusRight, focusUp, focusDown
    case swapLeft, swapRight, swapUp, swapDown
    case resizeLeft, resizeRight, resizeUp, resizeDown
    case splitRight, splitDown
    case nextSession, prevSession, lastSession
    case previewNext, previewPrev
    case nextWaiting
    case focus1, focus2, focus3, focus4, focus5, focus6, focus7, focus8, focus9
    case zoom, nextLayout, closeFocused, openEditor, renameSession, syncInput
    case focusSidebar, focusWorkspace, cycleSort, toggleGrouping

    /// 0-basiert für focus1…focus9.
    var tileIndex: Int? { rawValue.hasPrefix("focus") ? Int(rawValue.dropFirst(5)).map { $0 - 1 } : nil }

    var title: String {
        if let i = tileIndex { return String(localized: "Kachel \(i + 1)", bundle: Bundle.app) }
        switch self {
        case .focusLeft: return String(localized: "Fokus links", bundle: Bundle.app)
        case .focusRight: return String(localized: "Fokus rechts", bundle: Bundle.app)
        case .focusUp: return String(localized: "Fokus oben", bundle: Bundle.app)
        case .focusDown: return String(localized: "Fokus unten", bundle: Bundle.app)
        case .swapLeft: return String(localized: "Kachel nach links tauschen", bundle: Bundle.app)
        case .swapRight: return String(localized: "Kachel nach rechts tauschen", bundle: Bundle.app)
        case .swapUp: return String(localized: "Kachel nach oben tauschen", bundle: Bundle.app)
        case .swapDown: return String(localized: "Kachel nach unten tauschen", bundle: Bundle.app)
        case .resizeLeft: return String(localized: "Trennlinie nach links", bundle: Bundle.app)
        case .resizeRight: return String(localized: "Trennlinie nach rechts", bundle: Bundle.app)
        case .resizeUp: return String(localized: "Trennlinie nach oben", bundle: Bundle.app)
        case .resizeDown: return String(localized: "Trennlinie nach unten", bundle: Bundle.app)
        case .splitRight: return String(localized: "Frei: nächste Kachel rechts", bundle: Bundle.app)
        case .splitDown: return String(localized: "Frei: nächste Kachel unten", bundle: Bundle.app)
        case .nextSession: return String(localized: "Nächste Kachel", bundle: Bundle.app)
        case .prevSession: return String(localized: "Vorige Kachel", bundle: Bundle.app)
        case .lastSession: return String(localized: "Zuletzt fokussierte Kachel", bundle: Bundle.app)
        case .previewNext: return String(localized: "Vorschau: nächste Session im Baum", bundle: Bundle.app)
        case .previewPrev: return String(localized: "Vorschau: vorige Session im Baum", bundle: Bundle.app)
        case .nextWaiting: return String(localized: "Nächste wartende Session", bundle: Bundle.app)
        case .zoom: return String(localized: "Zoom: Fokus-Kachel allein", bundle: Bundle.app)
        case .nextLayout: return String(localized: "Layout wechseln: Grid → Haupt + Spalte → Spirale → Frei → Scrollen → Stack", bundle: Bundle.app)
        case .focusSidebar: return String(localized: "Baum: Tastatur hierher, ↑↓ wählt Session", bundle: Bundle.app)
        case .focusWorkspace: return String(localized: "Arbeitsfläche: Tastatur an Claude", bundle: Bundle.app)
        case .openEditor: return String(localized: "Ordner im externen Editor öffnen", bundle: Bundle.app)
        case .renameSession: return String(localized: "Session umbenennen", bundle: Bundle.app)
        case .cycleSort: return String(localized: "Baum sortieren: aus → A–Z → Status", bundle: Bundle.app)
        case .toggleGrouping: return String(localized: "Baum nach Projekt gruppieren", bundle: Bundle.app)
        case .syncInput: return String(localized: "Sync: Eingabe an alle Kacheln", bundle: Bundle.app)
        default: return String(localized: "Fokus-Kachel schließen", bundle: Bundle.app)
        }
    }

    /// Zeile in der Hilfe; Aktionen mit gleichem Text teilen sich eine Zeile.
    var helpText: String {
        if tileIndex != nil { return String(localized: "Kachel 1–9 fokussieren", bundle: Bundle.app) }
        switch self {
        case .focusLeft, .focusRight, .focusUp, .focusDown: return String(localized: "Fokus bewegen · im Stack auf- und zuklappen", bundle: Bundle.app)
        case .swapLeft, .swapRight, .swapUp, .swapDown: return String(localized: "Fokus-Kachel mit Nachbar tauschen", bundle: Bundle.app)
        case .resizeLeft, .resizeRight, .resizeUp, .resizeDown: return String(localized: "Trennlinie an der Fokus-Kachel um 5 % verschieben · Scrollen: Spalte ⅓ ½ ⅔ breit", bundle: Bundle.app)
        case .splitRight, .splitDown: return String(localized: "Layout Frei (wie i3): die Kachel nach der fokussierten entsteht rechts bzw. unten", bundle: Bundle.app)
        case .nextSession, .prevSession: return String(localized: "Nächste / vorige Kachel", bundle: Bundle.app)
        case .previewNext, .previewPrev: return String(localized: "Baum als Vorschau durchblättern, Auswahl bleibt · ⏎ übernimmt, Esc zurück", bundle: Bundle.app)
        case .nextWaiting: return String(localized: "Springt zur nächsten Session, die auf dich wartet", bundle: Bundle.app)
        case .closeFocused: return String(localized: "Fokus-Kachel schließen (Esc selbst geht an Claude)", bundle: Bundle.app)
        case .openEditor: return String(localized: "Ordner der Fokus-Session im Editor aus den Einstellungen öffnen", bundle: Bundle.app)
        case .renameSession: return String(localized: "Fokus-Session umbenennen, auch im Baum · Claude überschreibt den Namen danach nicht mehr", bundle: Bundle.app)
        case .syncInput: return String(localized: "Sync: Tippen und ⌘V gehen an alle offenen Kacheln gleichzeitig · Badge „SYNC“", bundle: Bundle.app)
        case .toggleGrouping: return String(localized: "Gruppen aus: alle Sessions in einer flachen Liste, mit eigener Handreihenfolge", bundle: Bundle.app)
        default: return title
        }
    }

    var defaultKey: Hotkey {
        if let i = tileIndex { return Hotkey(.option, "\(i + 1)") }
        switch self {
        case .focusLeft: return Hotkey(.option, "←")
        case .focusRight: return Hotkey(.option, "→")
        case .focusUp: return Hotkey(.option, "↑")
        case .focusDown: return Hotkey(.option, "↓")
        case .swapLeft: return Hotkey([.option, .shift], "←")
        case .swapRight: return Hotkey([.option, .shift], "→")
        case .swapUp: return Hotkey([.option, .shift], "↑")
        case .swapDown: return Hotkey([.option, .shift], "↓")
        case .resizeLeft: return Hotkey([.control, .option], "←")
        case .resizeRight: return Hotkey([.control, .option], "→")
        case .resizeUp: return Hotkey([.control, .option], "↑")
        case .resizeDown: return Hotkey([.control, .option], "↓")
        case .splitRight: return Hotkey([.control, .option, .shift], "→")
        case .splitDown: return Hotkey([.control, .option, .shift], "↓")
        case .nextSession: return Hotkey(.option, "n")
        case .prevSession: return Hotkey(.option, "p")
        case .lastSession: return Hotkey(.option, "⇥")
        case .previewNext: return Hotkey(.option, "j")
        case .previewPrev: return Hotkey(.option, "k")
        case .nextWaiting: return Hotkey(.option, "w")
        case .zoom: return Hotkey(.option, "z")
        case .nextLayout: return Hotkey(.command, "l")
        case .focusSidebar: return Hotkey(.command, "1")
        case .focusWorkspace: return Hotkey(.command, "2")
        case .openEditor: return Hotkey(.option, "e")
        case .renameSession: return Hotkey([], "F2")
        case .cycleSort: return Hotkey(.option, "o")
        case .toggleGrouping: return Hotkey(.option, "g")
        case .syncInput: return Hotkey(.option, "i")
        default: return Hotkey(.command, "Esc")
        }
    }
}

struct Hotkey: Hashable, Sendable {
    /// `NSEvent.ModifierFlags.rawValue`, nur ⌃⌥⇧⌘.
    let mods: UInt
    /// Zeichen ohne Modifier (klein) oder Name einer Sondertaste („←“, „F5“). Zeichen statt keyCode: ⌥Z liegt auf QWERTZ woanders als auf US.
    let key: String

    static let modMask: NSEvent.ModifierFlags = [.control, .option, .shift, .command]
    /// keyCode, Name, Menü-keyEquivalent
    private static let specials: [(UInt16, String, String)] = {
        func fn(_ c: Int) -> String { String(Character(UnicodeScalar(c)!)) }
        let arrows: [(UInt16, String, String)] = [(123, "←", fn(NSLeftArrowFunctionKey)), (124, "→", fn(NSRightArrowFunctionKey)),
                                                  (125, "↓", fn(NSDownArrowFunctionKey)), (126, "↑", fn(NSUpArrowFunctionKey)),
                                                  (36, "⏎", "\r"), (48, "⇥", "\t"), (53, "Esc", "\u{1b}"), (49, "Space", " "), (51, "⌫", "\u{8}")]
        let fCodes: [UInt16] = [122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111]
        return arrows + fCodes.enumerated().map { ($1, "F\($0 + 1)", fn(NSF1FunctionKey + $0)) }
    }()

    init(_ mods: NSEvent.ModifierFlags, _ key: String) {
        self.mods = mods.intersection(Hotkey.modMask).rawValue
        self.key = key
    }

    init?(event: NSEvent) {
        let mods = event.modifierFlags.intersection(Hotkey.modMask)
        if let s = Hotkey.specials.first(where: { $0.0 == event.keyCode }) { self.init(mods, s.1); return }
        guard let c = event.charactersIgnoringModifiers?.lowercased(), !c.isEmpty else { return nil }
        self.init(mods, c)
    }

    init?(string: String) {
        let parts = string.split(separator: "|", maxSplits: 1)
        guard parts.count == 2, let m = UInt(parts[0]) else { return nil }
        self.init(NSEvent.ModifierFlags(rawValue: m), String(parts[1]))
    }

    var string: String { "\(mods)|\(key)" }
    var flags: NSEvent.ModifierFlags { NSEvent.ModifierFlags(rawValue: mods) }

    var display: String {
        let f = flags
        return (f.contains(.control) ? "⌃" : "") + (f.contains(.option) ? "⌥" : "") + (f.contains(.shift) ? "⇧" : "")
            + (f.contains(.command) ? "⌘" : "") + key.uppercased()
    }

    var menuEquivalent: String { Hotkey.specials.first { $0.1 == key }?.2 ?? key }

    /// Ohne ⌘⌥⌃ würde das Kürzel normales Tippen im Terminal schlucken; F-Tasten sind die Ausnahme.
    var isUsable: Bool {
        !flags.intersection([.command, .option, .control]).isEmpty || (key.hasPrefix("F") && key.count > 1)
    }
}

enum Hotkeys {
    static let defaultsKey = "hotkeys"

    /// Gespeichert werden nur Abweichungen vom Default; "" heißt bewusst ohne Kürzel.
    static var current: [HotkeyAction: Hotkey] {
        get {
            let saved = Profile.defaults.dictionary(forKey: defaultsKey) as? [String: String] ?? [:]
            var out: [HotkeyAction: Hotkey] = [:]
            for a in HotkeyAction.allCases {
                if let s = saved[a.rawValue] { out[a] = Hotkey(string: s) } else { out[a] = a.defaultKey }
            }
            return out
        }
        set {
            var saved: [String: String] = [:]
            for a in HotkeyAction.allCases where newValue[a] != a.defaultKey { saved[a.rawValue] = newValue[a]?.string ?? "" }
            Profile.defaults.set(saved, forKey: defaultsKey)
        }
    }

    static func action(for event: NSEvent) -> HotkeyAction? {
        guard let k = Hotkey(event: event) else { return nil }
        return current.first { $0.value == k }?.key
    }
}
