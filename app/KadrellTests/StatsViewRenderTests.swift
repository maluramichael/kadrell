import XCTest
import SwiftUI
@testable import Kadrell

/// Das F3-Panel muss in beiden Sprachen rendern, ohne dass eine Zeile umbricht. Mit
/// `KADRELL_RENDER_OUT=<pfad>.png` legt der Test zusätzlich je ein Bild ab, für die Sichtprüfung ohne laufende App.
@MainActor
final class StatsViewRenderTests: XCTestCase {
    private let suite = "de.malura.kadrell.tests.statsrender"

    private static func demoModel() async -> StatsModel {
        let now = Date()
        let source = LimitSource(id: "a", title: "michael@example.org", detail: "5h 45% · 7d 56% · vor 3 min", kinds: LimitKind.allCases, current: nil)
        let ultra = LimitSource(id: "b", title: "Ultra", detail: "7d 12% · vor 12 min", kinds: [.weekly, .fable], current: nil)
        let samples = stride(from: 0.0, to: 6 * 24 * 3600, by: 1800).flatMap { ago -> [UsageSample] in
            let t = now.timeIntervalSince1970 - ago
            let session = Int(ago / 1800) % 10 * 10, weekly = 80 - Int(ago / (24 * 3600)) * 12
            return [UsageSample(t: t, account: "a", session: session, weekly: weekly, fable: weekly / 2),
                    UsageSample(t: t, account: "b", weekly: weekly / 3, fable: weekly / 5)]
        }
        var kadrell = HistorySession(sessionId: "1", path: "/1", cwd: "/x/kadrell")
        kadrell.promptTimes = stride(from: 0.0, to: 6 * 24 * 3600, by: 900).map { now.timeIntervalSince1970 - $0 }
        var other = HistorySession(sessionId: "2", path: "/2", cwd: "/x/homelab")
        other.promptTimes = stride(from: 100.0, to: 3 * 24 * 3600, by: 2700).map { now.timeIntervalSince1970 - $0 }
        let model = StatsModel(sources: { [source, ultra] }, samples: { samples }, sessions: { [kadrell, other] })
        await model.reload()
        return model
    }

    /// Sichtprüfung der fensterfüllenden Seite mit Beispieldaten, nur mit `KADRELL_RENDER_OUT`.
    func testFullPageRender() async throws {
        let model = await Self.demoModel()
        XCTAssertEqual(model.snapshot?.limits.count, 2)
        XCTAssertGreaterThan(model.snapshot?.promptTotal ?? 0, 0)
        guard let out = ProcessInfo.processInfo.environment["KADRELL_RENDER_OUT"] else { return }
        let host = NSHostingView(rootView: StatsView(model: model).background(Theme.raisedColor).environment(\.locale, Locale(identifier: "de")))
        host.frame = NSRect(x: 0, y: 0, width: 1280, height: 860)
        host.layoutSubtreeIfNeeded()
        try await Task.sleep(for: .milliseconds(300))
        guard let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) else { return XCTFail("kein Bitmap") }
        host.cacheDisplay(in: host.bounds, to: rep)
        try rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: out.replacingOccurrences(of: ".png", with: "-page.png")))
    }

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
