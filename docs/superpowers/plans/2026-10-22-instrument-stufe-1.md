# Instrument Stufe 1: Bauteile, Leiste, Schema Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kadrell bekommt die Instrument-Bauteile (Fuge, Taste, Display), das Farbschema Instrument Dark/Light mit eigener Akzentfarbe, und die Leiste wird als erstes Teil darauf umgebaut (Fugen, Tasten, Status-Display, kein Breadcrumb).

**Architecture:** Farben und Zeichenfunktionen der Bauteile liegen zentral in `Theme+Instrument.swift` und leiten sich aus dem aktiven `ColorTheme` ab, die zehn vorhandenen Schemata bleiben unverändert. Der Text des Status-Displays ist reine Logik (`StatusDisplay`) und getrennt testbar. `StatusBarView` zeichnet nur noch mit den Bauteilen.

**Tech Stack:** Swift 6, AppKit (handgezeichnete NSViews), SwiftUI für Dialoge, XCTest, String Catalog (`Localizable.xcstrings`).

**Spec:** `docs/superpowers/specs/2026-10-22-instrument-fugen-design.md` (Stufe 1). Stufen 2 bis 4 bekommen eigene Pläne, nachdem Michael Stufe 1 in echt gesehen hat. Nicht in Stufe 1: Modul mit runder Ecke, Strich-Anzeige, SwiftUI-Gegenstücke der Bauteile, Shell-Farbe für Instrument (alles Stufe 2 bis 4).

**Branch:** Arbeit in einem Worktree auf `instrument-stufe-1` (Skill using-git-worktrees). Commits pro Task dort, Changelog und Version erst beim Merge nach `master` in Task 6 (`CLAUDE.md`: Version bei jedem Merge).

## Global Constraints

- Clean Build ohne eigene Warnungen. Einzige erlaubte Meldung: `appintentsmetadataprocessor: Metadata extraction skipped`.
- `cd app && uv tool run lizard Kadrell -w` liefert nichts (CCN ≤ 15). Verzweigte Funktionen in kleine benannte Funktionen zerlegen.
- Jeder neue Nutzertext in AppKit als `String(localized: "…", bundle: Bundle.app)`, Deutsch und Englisch im selben Arbeitsgang in `app/Kadrell/Localization/Localizable.xcstrings`.
- Fugen: 1 pt dunkle + 1 pt helle Linie, bei jeder UI-Größe (90 bis 130 %) genau 1 pt, also in skaliertem Zeichnen `1 / Theme.scale`.
- Fugenfarben: dunkle Schemata Schwarz α 0,6 + Weiß α 0,05; helle Schemata Schwarz α 0,13 + Weiß α 0,65.
- Display: Grund `#050505` (dunkel) bzw. `#0d0d0c` (hell), Text `#efe9dc`, gedämpft `#8f8b81`, in allen Schemata.
- Leistenhöhe höchstens 4 pt höher als heute (`Theme.barHeight` = 30 × scale). Ziel: unverändert 30.
- Keine Änderung an den Farbwerten der zehn vorhandenen Schemata.
- Tests laufen im Temp-Profil: `TEST="cd app && /opt/homebrew/bin/xcodegen generate && TEST_RUNNER_KADRELL_PROFILE=tmp xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug -derivedDataPath build -skipPackagePluginValidation -skipMacroValidation test -only-testing:KadrellTests/<Klasse>"`.
- Laufende App nur mit `open -n app/build/Build/Products/Debug/Kadrell.app --args --profile tmp`, nie das Standardprofil anfassen.
- Keine Em-Dashes in Texten, Changelog, Commits. Commits nur per Pfad (`git add <dateien>`), kein Co-Author.

## Review Focus

1. **UI-Größe 130 %:** Fugen bleiben scharf 1 pt + 1 pt, nicht 1,3 pt verwaschen. Test in Task 2 (`testSeamStaysOnePointAtScale`).
2. **Helle Schemata (Latte, Solarized Light, GitHub Light, Instrument Light):** die helle Fugenlinie verschwindet nicht auf `panel`. Test in Task 2 (`testSeamVisibleInEveryTheme`).
3. **Schmales Fenster (900 pt) mit Extension-Einträgen, Warte- und Trennen-Taste:** nichts überlappt, das Display kürzt auf die Kurzform statt abgeschnitten zu werden. Test in Task 4 (`testNarrowBarUsesCompactDisplayWithoutOverlap`).
4. **Sprachwechsel zur Laufzeit:** Display-Wörter wechseln sofort (kein Cache der Strings). Test in Task 3 (`testDisplaySwitchesLanguage`).
5. **Bestehendes Profil ohne gespeichertes Schema:** bleibt nach dem Update auf Mocha, statt auf Instrument zu springen. Test in Task 6 (`testExistingProfileKeepsMocha`).

---

### Task 1: Akzentrolle und Schema Instrument Dark/Light

**Files:**
- Modify: `app/Kadrell/Theme.swift` (`ColorTheme`, `Theme`)
- Modify: `app/Kadrell/UI/Theme+SwiftUI.swift`
- Test: `app/KadrellTests/ContrastTests.swift`

**Interfaces:**
- Produces: `ColorTheme.accent: NSColor`, `Theme.accent: NSColor`, `Theme.accentColor: Color`, Schema-IDs `"instrument-dark"` und `"instrument-light"` in `ColorTheme.all` (am Ende der Liste anhängen, `all[0]` bleibt Mocha).

- [ ] **Step 1: Tests schreiben**

```swift
func testAccentDefaultsToRunning() {
    for t in ColorTheme.all where !t.id.hasPrefix("instrument") {
        XCTAssertEqual(t.accent, t.running, t.id)
    }
}

func testInstrumentThemes() throws {
    let dark = ColorTheme.named("instrument-dark"), light = ColorTheme.named("instrument-light")
    XCTAssertEqual(dark.id, "instrument-dark"); XCTAssertTrue(dark.dark)
    XCTAssertEqual(light.id, "instrument-light"); XCTAssertFalse(light.dark)
    XCTAssertEqual(dark.panel.hexString, "#151514"); XCTAssertEqual(dark.bg.hexString, "#1c1c1a")
    XCTAssertEqual(dark.waiting.hexString, "#ff5e14"); XCTAssertEqual(dark.accent.hexString, "#ff8a4c")
    XCTAssertEqual(light.panel.hexString, "#e3e1db"); XCTAssertEqual(light.bg.hexString, "#f2f1ec")
    XCTAssertEqual(light.waiting.hexString, "#ff4f00"); XCTAssertEqual(light.accent.hexString, "#a83500")
    XCTAssertEqual(ColorTheme.all[0].id, "mocha")
}

/// Akzent als Text (Links, Abschnittstitel) mindestens 4,5:1 auf panel und surface.
func testAccentTextMeetsAA() {
    for t in ColorTheme.all where t.id.hasPrefix("instrument") {
        for bg in [t.panel, t.surface, t.bg] {
            XCTAssertGreaterThanOrEqual(t.accent.contrastRatio(with: bg), 4.5, "\(t.id): accent gegen \(bg.hexString)")
        }
    }
}
```

Die bestehenden `testSecondaryTextMeetsAA` und `testPillTextMeetsAA` decken die neuen Schemata automatisch mit ab.

- [ ] **Step 2: Laufen lassen, muss fehlschlagen**

Run: `$TEST ContrastTests` · Expected: FAIL, `accent` unbekannt.

- [ ] **Step 3: Implementieren**

`ColorTheme.init(_ id:, _ name:, dark:, _ c:, ansi:, accent: UInt32? = nil)`, `accent = accent.map(NSColor.init(hex:)) ?? running`. `Theme.accent { current.accent }`, `Theme.accentColor` analog zu `runningColor`.

Werte (Reihenfolge bg, panel, surface, line, fg, sub, muted, detached, running, waiting, idle, error):

| | Dark (`"Instrument Dark"`) | Light (`"Instrument Light"`) |
|---|---|---|
| c | `0x1c1c1a, 0x151514, 0x252523, 0x2e2e2b, 0xecebe5, 0xa3a19a, 0x8d8b85, 0x5c5a54, 0xecebe5, 0xff5e14, 0x4fd98a, 0xff3b2f` | `0xf2f1ec, 0xe3e1db, 0xfbfaf7, 0xcfccc4, 0x1a1a18, 0x5c5a54, 0x6b6963, 0xb5b2a9, 0x1a1a18, 0xff4f00, 0x1f8f4e, 0xd8231a` |
| ansi (Startwerte, bei Kontrastfehlern anpassen) | `0x252523, 0xff5a4e, 0x4fd98a, 0xf2c14e, 0x6ea8fe, 0xc792ea, 0x5fd4d0, 0xecebe5` | `0x1a1a18, 0xb51c14, 0x1f8f4e, 0x8a6100, 0x1f5fbf, 0x8a3fb0, 0x0f7a78, 0x5c5a54` |
| accent | `0xff8a4c` | `0xa83500` |

Kommentar über den beiden Einträgen: Werte aus Monsters (github.com/Saturday-Company/monsters, Apache-2.0, `docs/design.md`).

- [ ] **Step 4: Tests grün**

Run: `$TEST ContrastTests` · Expected: PASS.

- [ ] **Step 5: Commit**

`git add app/Kadrell/Theme.swift app/Kadrell/UI/Theme+SwiftUI.swift app/KadrellTests/ContrastTests.swift && git commit -m "Farbschema Instrument Dark/Light und Akzentrolle"`

---

### Task 2: Bauteile Fuge, Taste, Display

**Files:**
- Create: `app/Kadrell/Theme+Instrument.swift`
- Test: `app/KadrellTests/InstrumentTests.swift`

**Interfaces:**
- Consumes: `Theme.current`, `Theme.scale`, `Theme.panel`, `Theme.surface`, `Theme.bg`, `Theme.line`.
- Produces (alle `@MainActor` wo gezeichnet wird, Koordinaten in unskalierten Punkten innerhalb von `Theme.scaled`, gespiegelte Views):
  - `enum Instrument` mit Farben: `static var seamDark: NSColor`, `seamLight: NSColor`, `keyCap: NSColor` (= `surface`), `keyEdge: NSColor`, `screen: NSColor`, `screenInk: NSColor`, `screenMuted: NSColor`.
  - `static var hairline: CGFloat { 1 / Theme.scale }`
  - `static func hSeam(x: CGFloat, y: CGFloat, width: CGFloat)`: dunkle Linie bei `y`, helle bei `y + hairline`.
  - `static func vSeam(x: CGFloat, y: CGFloat, height: CGFloat)`: dunkle Linie bei `x`, helle bei `x + hairline`.
  - `static func key(_ r: CGRect, pressed: Bool, fill: NSColor?) -> CGRect`: zeichnet Taste (Kappe `fill ?? keyCap`, 1 pt Kante `keyEdge`, Dicke 2 pt unten in `keyEdge`, Radius 5), gibt das Rechteck der Kappe zurück (für den Text). Gedrückt: Kappe 1 pt tiefer, Dicke 1 pt.
  - `static func screen(_ r: CGRect)`: Radius 5, Fläche `screen`, oben 1 hairline Weiß α 0,06.
- `keyEdge`: dunkel `Theme.bg.mixed(0.3, into: .black)`, hell `Theme.line.mixed(0.75, into: .black)`. Nur Startwerte, der Render-Test in Task 6 entscheidet.

- [ ] **Step 1: Tests schreiben**

Hilfsfunktionen im Test: `render(theme: ColorTheme, width: Int, height: Int, scale: CGFloat, _ draw: () -> Void) -> NSBitmapImageRep`, `luma(_ rep:, x:, y:) -> CGFloat`, `luma(_ c: NSColor) -> CGFloat`, `changedRows(_ rep:, against: NSColor) -> Int` (gespiegelter Kontext, wie `testStatusDotShapesOnlyWithDifferentiateWithoutColor` in `AccessibilityTests`), setzt `Theme.current` und `Theme.scale` für die Dauer und stellt beides zurück. Gezeichnet wird über `Theme.scaled`, damit der Maßstab wie in der App wirkt.

```swift
func testSeamDrawsDarkThenLight() {
    // Panel-Grund, Fuge bei y = 10: Zeile 10 dunkler als panel, Zeile 11 heller als panel.
    for t in ColorTheme.all {
        let rep = render(theme: t, width: 20, height: 20, scale: 1) { Theme.panel.setFill(); CGRect(x: 0, y: 0, width: 20, height: 20).fill(); Instrument.hSeam(x: 0, y: 10, width: 20) }
        XCTAssertLessThan(luma(rep, x: 5, y: 10), luma(t.panel), t.id)
        XCTAssertGreaterThan(luma(rep, x: 5, y: 11), luma(t.panel), t.id)
    }
}

func testSeamStaysOnePointAtScale() {
    // Bei 130 % genau zwei Pixelzeilen anders als panel (Bitmap im Maßstab 1 pt = 1 px), nicht drei.
    let rep = render(theme: .named("instrument-dark"), width: 20, height: 20, scale: 1.3) { … hSeam(x: 0, y: 5, width: 15) … }
    XCTAssertEqual(changedRows(rep, against: Theme.panel), 2)
}

func testSeamVisibleInEveryTheme() {
    // Unterschied der hellen Linie zu panel: mindestens 1,05:1 Kontrast (sichtbar, aber fein).
    for t in ColorTheme.all { … XCTAssertGreaterThanOrEqual(lightRowColor.contrastRatio(with: t.panel), 1.05, t.id) }
}

func testPressedKeySinks() {
    let up = Instrument.key(CGRect(x: 0, y: 0, width: 40, height: 20), pressed: false, fill: nil)
    let down = Instrument.key(CGRect(x: 0, y: 0, width: 40, height: 20), pressed: true, fill: nil)
    XCTAssertEqual(down.minY, up.minY + 1)
}

func testScreenIsBlackInEveryTheme() {
    for t in ColorTheme.all { Theme.current = t; XCTAssertGreaterThanOrEqual(Instrument.screenInk.contrastRatio(with: Instrument.screen), 4.5); XCTAssertGreaterThanOrEqual(Instrument.screenMuted.contrastRatio(with: Instrument.screen), 4.5) }
}
```

Sichtbarkeitsschwelle 1,05 ist ein Startwert: fällt ein Schema durch, dort die Fugenfarben nachschärfen (eigene Werte pro hell/dunkel, nicht pro Schema), nicht die Schwelle senken.

- [ ] **Step 2:** `$TEST InstrumentTests` · Expected: FAIL, `Instrument` unbekannt.
- [ ] **Step 3:** `Theme+Instrument.swift` mit den Signaturen oben implementieren.
- [ ] **Step 4:** `$TEST InstrumentTests` · Expected: PASS.
- [ ] **Step 5: Commit**

`git add app/Kadrell/Theme+Instrument.swift app/KadrellTests/InstrumentTests.swift && git commit -m "Instrument-Bauteile: Fuge, Taste, Display"`

---

### Task 3: Text und Lampe des Status-Displays

**Files:**
- Create: `app/Kadrell/Bar/StatusDisplay.swift`
- Modify: `app/Kadrell/Localization/Localizable.xcstrings`
- Test: `app/KadrellTests/StatusDisplayTests.swift`

**Interfaces:**
- Consumes: `StatusCounts` (`StatusBarView.swift`).
- Produces:
  - `enum StatusDisplay`
  - `enum Lamp { case waiting, error, ok, off }`
  - `static func lamp(_ c: StatusCounts) -> Lamp`: `waiting > 0` → `.waiting`, sonst `error > 0` → `.error`, sonst `running + idle > 0` → `.ok`, sonst `.off`.
  - `static func text(_ c: StatusCounts, compact: Bool) -> String`: nur Zustände > 0 in der Reihenfolge arbeiten, warten, Fehler, getrennt durch ` · `, Versalien. Leer, wenn alles 0.
  - `static func spoken(_ c: StatusCounts) -> String`: Langform aller fünf Zustände plus Gesamtzahl, gleicher Wortlaut wie der heutige Tooltip von `drawCounts` (`"2 arbeiten, 1 warten, 3 fertig, 0 mit Fehler, 1 getrennt · 7 Sessions"`, Fehler nur bei > 0 wie heute).

Strings (Plural-Variationen im String Catalog, Schlüssel mit `%lld`):

| Schlüssel | de one / other | en one / other |
|---|---|---|
| `%lld ARBEITEN` | `%lld ARBEITET` / `%lld ARBEITEN` | `%lld WORKING` / `%lld WORKING` |
| `%lld WARTEN` | `%lld WARTET` / `%lld WARTEN` | `%lld WAITING` / `%lld WAITING` |
| `%lld FEHLER` | `%lld FEHLER` / `%lld FEHLER` | `%lld ERROR` / `%lld ERRORS` |
| Kurzform `%lldA`, `%lldW`, `%lldF` | `%lldA` `%lldW` `%lldF` | `%lldW` `%lldQ` `%lldE` |

Englische Kurzform: W = working, Q = queued for you (W ist schon belegt), E = error.

- [ ] **Step 1: Tests schreiben**

```swift
func testOnlyNonZeroStatesInOrder() {
    Settings.language = .de
    XCTAssertEqual(StatusDisplay.text(StatusCounts(running: 2, waiting: 1, idle: 3, error: 0, detached: 1), compact: false), "2 ARBEITEN · 1 WARTET")
    XCTAssertEqual(StatusDisplay.text(StatusCounts(running: 1, waiting: 0, idle: 0, error: 2, detached: 0), compact: false), "1 ARBEITET · 2 FEHLER")
    XCTAssertEqual(StatusDisplay.text(StatusCounts(), compact: false), "")
    XCTAssertEqual(StatusDisplay.text(StatusCounts(running: 2, waiting: 1, idle: 0, error: 0, detached: 0), compact: true), "2A · 1W")
}

func testLampPriority() {
    XCTAssertEqual(StatusDisplay.lamp(StatusCounts(running: 1, waiting: 1, idle: 0, error: 1, detached: 0)), .waiting)
    XCTAssertEqual(StatusDisplay.lamp(StatusCounts(running: 1, waiting: 0, idle: 0, error: 1, detached: 0)), .error)
    XCTAssertEqual(StatusDisplay.lamp(StatusCounts(running: 0, waiting: 0, idle: 2, error: 0, detached: 3)), .ok)
    XCTAssertEqual(StatusDisplay.lamp(StatusCounts(running: 0, waiting: 0, idle: 0, error: 0, detached: 3)), .off)
}

func testDisplaySwitchesLanguage() {
    let c = StatusCounts(running: 2, waiting: 1, idle: 0, error: 1, detached: 0)
    Settings.language = .en
    XCTAssertEqual(StatusDisplay.text(c, compact: false), "2 WORKING · 1 WAITING · 1 ERROR")
    Settings.language = .de
    XCTAssertEqual(StatusDisplay.text(c, compact: false), "2 ARBEITEN · 1 WARTET · 1 FEHLER")
}

func testSpokenListsEverything() {
    Settings.language = .de
    XCTAssertEqual(StatusDisplay.spoken(StatusCounts(running: 2, waiting: 1, idle: 3, error: 0, detached: 1)), "2 arbeiten, 1 warten, 3 fertig, 1 getrennt · 7 Sessions")
}
```

`tearDown` wie in `LocalizationTests` (Sprache zurücksetzen).

- [ ] **Step 2:** `$TEST StatusDisplayTests` · Expected: FAIL.
- [ ] **Step 3:** `StatusDisplay.swift` und Strings in beiden Sprachen anlegen.
- [ ] **Step 4:** `$TEST StatusDisplayTests` und `$TEST LocalizationTests` · Expected: PASS.
- [ ] **Step 5: Commit**

`git add app/Kadrell/Bar/StatusDisplay.swift app/Kadrell/Localization/Localizable.xcstrings app/KadrellTests/StatusDisplayTests.swift && git commit -m "Status-Display: Text, Lampe und Langform"`

---

### Task 4: Leiste auf Fugen, Tasten und Display umbauen

**Files:**
- Modify: `app/Kadrell/Bar/StatusBarView.swift`
- Modify: `app/Kadrell/App/AppDelegate.swift:677-683` (Breadcrumb-Zuweisungen entfernen)
- Test: `app/KadrellTests/ExtensionPanelTests.swift`, `app/KadrellTests/AccessibilityTests.swift`

**Interfaces:**
- Consumes: `Instrument.hSeam/vSeam/key/screen`, `Instrument.hairline`, `StatusDisplay.text/lamp/spoken`, `Theme.accent`.
- Produces: `StatusBarView` ohne `crumb`, `crumbGroupAttrs`, `sessionCount`, `openCount`, `minCrumbWidth`, `crumbWidth`, `crumbText()`, `drawCrumb`. `errorText` bleibt und erscheint im Display. Neu: `private(set) var displayCompact: Bool` (für Tests: welche Form zuletzt gezeichnet wurde).

Aufbau nach dem Umbau:
- Fläche `Theme.panel`, unten `hSeam` bei `b.height - 2 * hairline` über die ganze Breite. Inhalt vertikal mittig in der Höhe darüber.
- Links: Layout-Symbol und `segment`-Schalter (AUTO, SYNC, GRP/FLACH, SORT, TEILT) und ‹ SP › werden Tasten (`Instrument.key`, Höhe 20, 4 pt Abstand untereinander, keine Trenner mehr). Eingeschaltet = `pressed: true` mit der heutigen Füllfarbe (`auto` → `Theme.waiting`, `sync` → `Theme.error`, FLACH/Sortierung → `Theme.sub`). Text darauf wie heute über `Theme.pillText(on:)`. ZOOM wird eine Taste mit `Theme.waiting`.
- Klick-Rückmeldung: nach `mouseDown` auf einer Trefferfläche zeichnet die zugehörige Taste 80 ms eingesunken (`pressedRect`, `pressedAt`, `animate(0.08)`). Bei `Feedback.reduceMotion` ohne diese Rückmeldung. Die Aktion feuert weiter beim `mouseDown`.
- Rechts von rechts nach links: Display (Lampe 7 pt, Text `StatusDisplay.text` in `Instrument.screenInk`, Uhr in `Instrument.screenMuted`, 10 pt Innenabstand, `Instrument.screen`), bei `errorText != nil` statt des Status-Texts `errorText` in `Theme.error` gemischt Richtung Weiß, bis ≥ 4,5:1 auf `Instrument.screen` erreicht ist. Lampenfarben: `.waiting` → `Theme.waiting`, `.error` → `Theme.error`, `.ok` → `Theme.idle`, `.off` → `Instrument.screenMuted`. Daneben die Tasten „N warten“ (`Theme.waiting`), „N trennen“ (`Theme.idle`), Update und Konto (`keyCap`). Dann Tipp, Versionswarnung, Nutzung, Extension-Einträge als flache Module wie heute, getrennt durch `vSeam` statt `divider`.
- Kurzform: passt das Display mit Langtext nicht zwischen Ende der linken Seite + 16 pt und dem nächsten Modul, `compact: true`. Extension-Einträge werden danach von links weggelassen wie heute (`limit` = Ende der linken Seite + 16).
- VoiceOver: zusätzlich zu den Knöpfen aus `hitRects` ein Element `role: .staticText`, `label: "Status"` (lokalisiert), `value: StatusDisplay.spoken(counts)` bzw. `errorText`, Rahmen = Display. Tooltip des Displays = derselbe Text.
- `drawCounts` und das Uhr-Modul entfallen. Keine Funktion über CCN 15: Tasten-, Display- und Modulzeichnen getrennt.

- [ ] **Step 1: Tests anpassen und ergänzen**

In `ExtensionPanelTests`: `testExtensionItemsLeaveRoomForBreadcrumb` ersetzen durch

```swift
func testNarrowBarUsesCompactDisplayWithoutOverlap() {
    for width in stride(from: 900 as CGFloat, through: 1100, by: 9) {
        let bar = StatusBarView(frame: .zero)
        let w = barWindow(bar, width: width); defer { w.close() }
        bar.counts = StatusCounts(running: 12, waiting: 3, idle: 4, error: 2, detached: 1)
        bar.waitingCount = 3; bar.detachableCount = 4
        bar.extensionItems = (1...6).map { ("e\($0)", "Ein langer Eintrag Nummer \($0)", .ok, "a:\($0)") }
        bar.display()
        let frames = barElements(bar).map { $0.accessibilityFrame() }
        for (i, a) in frames.enumerated() { for b in frames[(i + 1)...] { XCTAssertFalse(a.intersects(b), "Breite \(width)") } }
    }
    let bar = StatusBarView(frame: .zero); let w = barWindow(bar, width: 600); defer { w.close() }
    bar.counts = StatusCounts(running: 12, waiting: 3, idle: 4, error: 2, detached: 1); bar.display()
    XCTAssertTrue(bar.displayCompact)
}
```

`testRenderStatusBar`: `bar.crumb = …` streichen. In `AccessibilityTests.testStatusBarButtons` bleibt die Knopfliste gleich, zusätzlich:

```swift
let status = try XCTUnwrap(elements(bar).first { $0.accessibilityLabel() == "Status" })
XCTAssertEqual(status.accessibilityRole(), .staticText)
```

Plus Test für die Fehlermeldung: `bar.errorText = "x"` → Status-Element hat Wert `"x"`.

- [ ] **Step 2:** `$TEST ExtensionPanelTests` und `$TEST AccessibilityTests` · Expected: FAIL (kein Status-Element, `crumb` existiert noch).
- [ ] **Step 3:** `StatusBarView` umbauen, `AppDelegate.swift:677-683` die fünf Breadcrumb-Zeilen (`crumb`, `crumbGroupAttrs`, `sessionCount`, `openCount`) entfernen, `errorText` bleibt.
- [ ] **Step 4:** `$TEST ExtensionPanelTests`, `$TEST AccessibilityTests`, `$TEST SessionParsingTests`, `$TEST AccountServiceTests` · Expected: PASS. `cd app && uv tool run lizard Kadrell -w` · Expected: leer.
- [ ] **Step 5: Commit**

`git add app/Kadrell/Bar/StatusBarView.swift app/Kadrell/App/AppDelegate.swift app/KadrellTests/ExtensionPanelTests.swift app/KadrellTests/AccessibilityTests.swift app/Kadrell/Localization/Localizable.xcstrings && git commit -m "Leiste mit Fugen, Tasten und Status-Display, ohne Breadcrumb"`

---

### Task 5: Akzent statt Arbeitsfarbe in der Oberfläche

**Files (Modify):** `app/Kadrell/UI/Theme+SwiftUI.swift:25`, `UI/AboutView.swift:61,66,73`, `UI/SettingsView.swift:102,169,183,374,398`, `UI/FolderInput.swift:78`, `UI/StatsView.swift:64`, `UI/PaletteWindow.swift:355`, `UI/ConfirmView.swift:42`, `UI/NewSessionSheet.swift:181,187,235,317`, `Workspace/WorkspaceView.swift:613`, `Extensions/UI/ExtensionPanelView+Drawing.swift:9,37,47,62`, `Extensions/UI/ExtensionsView.swift:46,83,104,158,172`, `Extensions/UI/ExtensionDetail.swift:13,53` (alle unter `app/Kadrell/`).

**Interfaces:**
- Consumes: `Theme.accent`, `Theme.accentColor` (Task 1).

Regel: `running`/`runningColor` bleibt nur, wo es den Status „arbeitet“ meint (Statuspunkte, Zähler, Leiste). Überall in der Liste oben ist es Akzent (Link, Fokusring, Primärknopf, Abschnittstitel, Auswahlbalken, Lua-`accent`) und wird `accent`/`accentColor`. `Theme.runningColor` danach nur noch behalten, wenn es noch einen Aufrufer gibt.

- [ ] **Step 1: Test schreiben** (in `ContrastTests` oder `ExtensionPanelTests`)

```swift
func testLuaAccentIsThemeAccent() {
    let saved = Theme.current; defer { Theme.current = saved }
    Theme.current = .named("instrument-dark")
    XCTAssertEqual(ThemeColor.accent.nsColor, Theme.accent)
    XCTAssertNotEqual(ThemeColor.accent.nsColor, Theme.running)
}
```

- [ ] **Step 2:** `$TEST ExtensionPanelTests` · Expected: FAIL.
- [ ] **Step 3:** Ersetzungen vornehmen. Danach `grep -rn "runningColor\|Theme.running\b" app/Kadrell` zeigt nur noch Status-Stellen.
- [ ] **Step 4:** Gesamter Testlauf (`-only-testing` weglassen) · Expected: PASS.
- [ ] **Step 5: Commit**

`git add <die geänderten Dateien> && git commit -m "Akzentfarbe statt Arbeitsfarbe für Links, Fokus und Knöpfe"`

---

### Task 6: Default, Migration, Sichtprüfung, Release

**Files:**
- Modify: `app/Kadrell/App/Settings.swift:43`
- Modify: `app/Kadrell/App/AppDelegate.swift` (Aufruf am Anfang von `applicationDidFinishLaunching`, vor jedem Zugriff auf `Theme`)
- Test: `app/KadrellTests/SettingsModelTests.swift`
- Temporär: Render-Test für Sichtprüfung (nach der Prüfung wieder löschen)
- Modify: `CHANGELOG.md`, Version per `python3 tools/bump-version.py minor`

**Interfaces:**
- Produces: `static func pinThemeForExistingProfile(_ d: UserDefaults)` in `Settings`: hat `d` `helpShown == true` und keinen Wert für `colorTheme`, wird `"mocha"` gespeichert. Default von `colorTheme` wird `"instrument-dark"`.

`Theme.current` wird beim ersten Zugriff aus `Settings.colorTheme` gelesen: prüfen, dass vor `applicationDidFinishLaunching` niemand `Theme` anfasst (z. B. `main.swift`, `Profile`). Falls doch, den Aufruf dorthin vorziehen, wo `Profile.defaults` zuerst steht.

- [ ] **Step 1: Tests schreiben**

```swift
func testExistingProfileKeepsMocha() {
    let d = UserDefaults(suiteName: "kadrell-test-\(UUID())")!
    d.set(true, forKey: "helpShown")
    Settings.pinThemeForExistingProfile(d)
    XCTAssertEqual(d.string(forKey: "colorTheme"), "mocha")
}

func testFreshProfileGetsInstrument() {
    let d = UserDefaults(suiteName: "kadrell-test-\(UUID())")!
    Settings.pinThemeForExistingProfile(d)
    XCTAssertNil(d.string(forKey: "colorTheme"))
}

func testChosenThemeIsUntouched() {
    let d = UserDefaults(suiteName: "kadrell-test-\(UUID())")!
    d.set(true, forKey: "helpShown"); d.set("nord", forKey: "colorTheme")
    Settings.pinThemeForExistingProfile(d)
    XCTAssertEqual(d.string(forKey: "colorTheme"), "nord")
}
```

Dazu: `Settings.colorTheme` liefert ohne gespeicherten Wert `"instrument-dark"`.

- [ ] **Step 2:** `$TEST SettingsModelTests` · Expected: FAIL.
- [ ] **Step 3:** Implementieren, Aufruf in `AppDelegate` einbauen.
- [ ] **Step 4:** `$TEST SettingsModelTests` · Expected: PASS. Danach gesamter Testlauf, Clean Build ohne eigene Warnungen, `lizard` leer.
- [ ] **Step 5: Sichtprüfung.** Temporären Render-Test schreiben, der die Leiste (1200 und 900 pt breit, mit Zählern, SYNC an, Warte- und Trennen-Taste, zwei Extension-Einträgen) in allen zwölf Schemata und bei UI-Größe 100 % und 130 % als PNG nach `~/.claude/screenshots/kadrell/instrument-stufe-1/<schema>-<breite>-<größe>.png` schreibt. Mit `TEST_RUNNER_KADRELL_PROFILE=tmp` laufen lassen, Bilder ansehen. Fugen in einem Schema kaum sichtbar oder schmutzig: `seamDark`/`seamLight`/`keyEdge` für hell bzw. dunkel nachjustieren, Task-2-Tests erneut laufen lassen. Danach den Render-Test löschen (die Bilder bleiben).
- [ ] **Step 6: Laufende App.** Debug-Build mit `--profile tmp` starten, Instrument Dark und Light in den Einstellungen umschalten (ohne Neustart), Sprache umschalten, Tasten anklicken. Nur die eigene Instanz per `kill -TERM <pid>` beenden.
- [ ] **Step 7: Changelog und Version**

Unter `## Unreleased` oben in `CHANGELOG.md`:

```
- Feature: Neues Farbschema „Instrument“ (dunkel und hell), Standard für neue Installationen.
- Änderung: Die Leiste oben ist neu gestaltet: Schalter als Tasten, Trennungen als feine Fugen, rechts ein Status-Display mit Klartext („2 ARBEITEN · 1 WARTET“) und Uhr.
- Entfernt: Die Anzeige von Gruppe und Session in der Mitte der Leiste.
```

Dann `python3 tools/bump-version.py minor`.

- [ ] **Step 8: Commit, Merge, Push**

Im Worktree: `git add app/Kadrell/App/Settings.swift app/Kadrell/App/AppDelegate.swift app/KadrellTests/SettingsModelTests.swift app/Kadrell/Theme+Instrument.swift CHANGELOG.md app/project.yml app/Kadrell/Info.plist && git commit -m "Version <neu>: Instrument-Schema und neue Leiste"`. Dann im Hauptverzeichnis `git merge --no-ff instrument-stufe-1` nach `master` und `git push origin master`. Worktree danach entfernen (Skill finishing-a-development-branch).

Nicht deployen und kein Release bauen: Michael sichtet Stufe 1 zuerst im eigenen Kadrell.
