import XCTest
@testable import Kadrell

@MainActor
final class LocalizationTests: XCTestCase {
    override func tearDown() {
        Profile.defaults.removeObject(forKey: Settings.languageKey)
        Localization.apply(.system)
        super.tearDown()
    }

    /// Die Einstellung schaltet die Texte sofort um, ohne Neustart.
    func testLanguageSettingSwitchesTexts() {
        Settings.language = .en
        XCTAssertEqual(String(localized: "Kopieren", bundle: Bundle.app), "Copy")
        XCTAssertEqual(Localization.locale.identifier, "en")
        Settings.language = .de
        XCTAssertEqual(String(localized: "Kopieren", bundle: Bundle.app), "Kopieren")
        Settings.language = .system
        XCTAssertEqual(Bundle.app, Bundle.main, "Sprache des Systems nimmt wieder das Bundle der App")
    }

    /// Jeder Nutzertext muss über das Bundle der eingestellten Sprache gehen, sonst bliebe er beim Umschalten stehen.
    func testEveryLocalizedStringUsesTheSelectedBundle() throws {
        let sources = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("Kadrell")
        var offenders: [String] = []
        for case let file as URL in FileManager.default.enumerator(at: sources, includingPropertiesForKeys: nil)!
        where file.pathExtension == "swift" {
            for (i, line) in try String(contentsOf: file, encoding: .utf8).components(separatedBy: "\n").enumerated()
            where line.contains("String(localized:") && !line.contains("bundle: Bundle.app") {
                offenders.append("\(file.lastPathComponent):\(i + 1)")
            }
        }
        XCTAssertEqual(offenders, [], "String(localized:) ohne bundle: Bundle.app")
    }

    /// Erster Start: die Karte im Leerzustand bietet zwei Flaggen an, nach der Wahl nicht mehr.
    func testEmptyStateOffersFlagsUntilLanguageIsChosen() {
        let ws = WorkspaceView(frame: .zero, defaultsSuffix: ".test-language")
        let w = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 600, height: 400), styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        ws.frame = NSRect(x: 0, y: 0, width: 600, height: 400)
        w.contentView = ws
        defer {
            ws.close(); w.close()
            for k in ["workspace.selected", "workspace.mode", "workspace.auto"] { Profile.defaults.removeObject(forKey: k + ".test-language") }
        }
        var picked: Localization.Language?
        ws.onPickLanguage = { picked = $0 }
        ws.polled = true
        ws.reload(groups: [], sessions: [])
        ws.display()
        XCTAssertEqual(ws.emptyHitRects.count, 3, "zwei Flaggen und der Knopf für die erste Session")

        // Linke Flagge anklicken: Deutsch, danach sind die Flaggen weg.
        ws.emptyHitRects.min { $0.rect.minX < $1.rect.minX }?.action()
        XCTAssertEqual(picked, .de)
        Settings.language = .de
        ws.display()
        XCTAssertEqual(ws.emptyHitRects.count, 1, "nach der Wahl bleibt nur der Knopf für die erste Session")
    }
}
