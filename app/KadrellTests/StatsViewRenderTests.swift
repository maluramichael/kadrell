import XCTest
import SwiftUI
@testable import Kadrell

/// Das F3-Panel muss in beiden Sprachen rendern, ohne dass eine Zeile umbricht. Mit
/// `KADRELL_RENDER_OUT=<pfad>.png` legt der Test zusätzlich je ein Bild ab, für die Sichtprüfung ohne laufende App.
@MainActor
final class StatsViewRenderTests: XCTestCase {
    private let suite = "de.malura.kadrell.tests.statsrender"

    func testPanelRendersInBothLanguages() throws {
        UserDefaults.standard.removePersistentDomain(forName: suite)
        Stats.defaults = UserDefaults(suiteName: suite)!
        defer {
            UserDefaults.standard.removePersistentDomain(forName: suite)
            Stats.defaults = Profile.defaults
        }
        Stats.bump(.messages, by: 12843)
        Stats.bump(.sessions, by: 417)
        Stats.bump(.terminals, by: 63)
        Stats.bump(.focusSwitches, by: 9102)
        Stats.noteConcurrent(23)

        let out = ProcessInfo.processInfo.environment["KADRELL_RENDER_OUT"]
        var heights: [CGFloat] = []
        for language in ["de", "en"] {
            let host = NSHostingView(rootView: StatsView().environment(\.locale, Locale(identifier: language)))
            host.layout()
            let size = host.fittingSize
            XCTAssertGreaterThan(size.width, 300, "\(language): breit genug für Zahl und Beschriftung")
            XCTAssertGreaterThan(size.height, 200, "\(language): alle fünf Zeilen sind da")
            heights.append(size.height)
            guard let out else { continue }
            host.frame = NSRect(origin: .zero, size: size)
            guard let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) else { return XCTFail("kein Bitmap") }
            host.cacheDisplay(in: host.bounds, to: rep)
            guard let png = rep.representation(using: .png, properties: [:]) else { return XCTFail("kein PNG") }
            try png.write(to: URL(fileURLWithPath: out.replacingOccurrences(of: ".png", with: "-\(language).png")))
        }
        // Bricht eine Beschriftung in einer Sprache um, wird das Panel dort höher.
        XCTAssertEqual(heights[0], heights[1], accuracy: 1, "beide Sprachen bleiben gleich hoch")
    }
}
