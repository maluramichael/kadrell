# Instrument Stufe 3: Kacheln Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kacheln und Stack-Zeilen bekommen die Instrument-Sprache: neutrale Köpfe mit kleinem Gruppen-Akzent statt Gruppen-Tönung, Fugen unter den Köpfen und als Rand jeder Kachel, Strich-Anzeige statt pulsierendem Punkt, saubere Modul-Ecke auch beim seitlichen Scrollen und im Stack. Dazu die offenen Punkte aus der Prüfung von Stufe 2.

**Architecture:** Das Kit `Theme+Instrument.swift` bekommt die fehlenden Bauteile (eingerastete Strich-Anzeige, Rand ohne Unterkante, Gruppen-Label und -Akzent, Rechteck mit runder Ecke oben links); Baum, Kachel und Stack-Zeile nutzen dieselben. Die Regel „wann Striche, welche Farbe“ liegt einmal in `Theme` (`statusColor`, neu `meterColor`), der Puls entfällt danach ganz. Die Kachel (`CellView`) zeichnet Kopf, Körper und hellen Innenrand selbst, den dunklen Außenrand weiter als Layer-Rand; Stack-Zeilen zeichnet die Arbeitsfläche. Seitlich gescrollte Kacheln unter der Ecke bekommen eine Layer-Maske entlang des Modul-Bogens.

**Tech Stack:** Swift 6, AppKit (handgezeichnete NSViews, CALayer), XCTest, String Catalog.

**Spec:** `docs/superpowers/specs/2026-10-22-instrument-fugen-design.md` (Stufe 3, Ziele 1, 5, 7, Entscheidungen „Bash-Kommando läuft“, „Modul, Hintergrundbild, Deckkraft“). Stufe 2 ist seit 1.71.0 auf master. Plan-Vorbild: `docs/superpowers/plans/2026-10-22-instrument-stufe-2.md`.

**Branch:** Worktree `.claude/worktrees/instrument-stufe-3-4`, Branch `worktree-instrument-stufe-3-4`. Stufe 4 folgt im selben Worktree mit eigenem Plan. Commits pro Task. Changelog in Task 8, **kein Versionssprung** in dieser Stufe (der Plan für Stufe 4 springt einmal für beide).

## Global Constraints

- Clean Build ohne eigene Warnungen. Einzige erlaubte Meldung: `appintentsmetadataprocessor: Metadata extraction skipped`.
- `cd app && uv tool run lizard Kadrell -w` liefert nichts (CCN ≤ 15). Wird eine Funktion zu verzweigt (`CellView.drawHeader`, `WorkspaceView.drawStackRow`), in kleine benannte Funktionen zerlegen.
- Jeder neue Nutzertext als `String(localized: "…", bundle: Bundle.app)`, Deutsch und Englisch im selben Arbeitsgang in `app/Kadrell/Localization/Localizable.xcstrings`. Geplant sind keine neuen Texte; entsteht doch einer, gilt die Regel.
- Fugen bleiben bei jeder UI-Größe (90 bis 130 %) genau 1 pt dunkel + 1 pt hell (Haarlinien, `Instrument.hairline`, eingerastet über `Instrument.snapped`); Radien skalieren mit (`ModuleCornerView.scaledRadius`).
- Fugen kosten pro Trenner höchstens 1 pt mehr als heute: Kopfhöhe (`Tiling.rowHeight` 26) und Terminal-Abstand (`CellView.terminalRect`, 2 pt) bleiben.
- Keine Änderung an den Farbwerten der zwölf Schemata. Gruppenfarbe nur noch als Akzent (6 × 6 pt) am Label.
- Status nie nur über Farbe: Striche (Form) für arbeitet und laufende Shell bei fertig, Punkt für alles andere, „Farben nicht unterscheiden“ behält seine Glyphen.
- Bewegung in Stufen; bei `Feedback.reduceMotion` stehen die Striche still, alle an.
- Tests im Temp-Profil: `TEST="cd app && /opt/homebrew/bin/xcodegen generate && TEST_RUNNER_KADRELL_PROFILE=tmp xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug -derivedDataPath build -skipPackagePluginValidation -skipMacroValidation test -only-testing:KadrellTests/<Klasse>"`. Gesamtsuite (ohne `-only-testing`) vor jedem Commit grün (Stand vor Stufe 3: 432 Testfunktionen).
- Tests, die `Theme.current`, `Theme.scale`, `Settings.tileGap`, `Settings.tileOpacity` oder `Feedback.reduceMotion` ändern, stellen sie per `defer` zurück. Arbeitsflächen in Tests mit eigenem `defaultsSuffix` und Aufräumen wie in `testWorkspaceRoundsOnlyTopLeftCell`.
- Laufende App nur mit `open -n app/build/Build/Products/Debug/Kadrell.app --args --profile tmp`, nur die eigene Instanz per pid beenden (`/bin/ps -axo pid=,args= | grep "instrument-stufe-3-4/app/build.*profile tmp"`), nie `pkill`.
- Keine Em-Dashes. Deutsche Kommentare mit echten Umlauten. Git im Worktree als `/usr/bin/git`, Commits per Pfad (`/usr/bin/git add <Pfade> && /usr/bin/git commit -m "…" -- <Pfade>`), keine Co-Author-Zeilen.

## Entscheidungen dieser Stufe (Michael prüft sie beim Lesen)

- **Wartet schlägt Shell-Farbe** (Vorgabe des Controllers): Wartet eine Session und läuft im Hintergrund noch ein Kommando, zeigt der Punkt die Warte-Farbe (Instrument: Orange), nicht die Shell-Farbe. Gilt im Baum, im Kachelkopf und in der Stack-Zeile. Planer-Erweiterung: Fehler schlägt die Shell-Farbe ebenso, und VoiceOver sagt in beiden Fällen den Status („wartet“, „Fehler“) statt „Kommando läuft noch“. Shell-Farbe und „Kommando läuft noch“ bleiben genau dem Fall fertig + laufende Shell vorbehalten, in dem auch die Striche erscheinen.
- **Kachelrand als Fuge:** außen 1 pt dunkel (`Instrument.seamDark`, Layer-Rand), innen direkt daneben 1 pt hell (`Instrument.seamLight`, gezeichnet). Zwei Kacheln bei Abstand 0 ergeben so hell-dunkel-dunkel-hell, eine symmetrische Fuge; der Terminal-Abstand von 2 pt bleibt.
- **Fokus-Kachel:** 2 pt Rand in `Theme.fg` statt in Gruppenfarbe (wie der Umriss der gezeigten Session im Baum). Wartet (angehängt) 1 pt `Theme.waiting`, Fehler 1 pt `Theme.error`, Ablegeziel 2 pt `Theme.fg` wie bisher. Hover ändert den Rand nicht mehr (die Knöpfe im Kopf zeigen ihn).
- **Kachelkopf** in `Theme.surface` (Monsters „raised“), unabhängig von Fokus und Gruppe. **Kachelkörper** in `Theme.bg` (Modulfarbe) mit Kachel-Deckkraft, ohne Gruppen-Tönung; damit ist in Instrument der Terminalgrund genau die Modulfarbe (Spec). In den alten Schemata verliert der Terminalgrund die 5-%-Tönung.
- **Gruppe im Kachelkopf und in der Stack-Zeile:** Farbakzent 6 × 6 pt (Radius 1,5) plus gesperrtes Versal-Label wie im Baum (`Instrument.groupLabel`), ohne Zahl.
- **Stack-Zeilen:** fokussierte Zeile `Theme.surface`, übrige `Theme.panel` (heutige Helligkeitsstufen ohne Tönung), kein Farbbalken links mehr. Die Spalte (Zeilen + Körper) ist eine eingesetzte Fläche: Zeilen tragen links und rechts den Rand der Kachel, die erste auch oben; unter jeder Zeile eine Fuge, außer direkt über dem Körper (dort ist der obere Kachelrand die Fuge).
- **Puls entfällt:** nach dieser Stufe zeichnet kein Ort mehr einen pulsierenden Punkt. `Theme.dotColor`, `Feedback.pulse`, `CellView.pulse` und `WorkspaceView.pulse` werden gelöscht.
- **Deckender Stummel an der Ecke (Prüfpunkt e):** wird behoben, ist billig. Der Split-Trenner spart die Fläche der Ecke aus, die Ecke malt dort den Grund mit Kachel-Deckkraft wie der Trenner. Nur der obere Streifen (über der deckenden Leiste) bleibt deckend.

## Review Focus

1. **Layout Scrollen, Kachel halb nach links gescrollt, Abstand 0 und 6:** die Kachel endet sauber am Bogen der Modul-Ecke, kein Kopf und kein Terminal außerhalb. Test in Task 5 (`testScrolledTileUnderCornerIsMasked`).
2. **Stack bei Abstand 0:** die erste Zeile folgt dem Bogen, auch ihr Rand. Test in Task 4 (`testFirstStackRowFollowsModuleCorner`).
3. **Session wartet, Hintergrund-Shell läuft:** oranger Punkt in Baum und Kachelkopf, VoiceOver „wartet“. Tests in Task 2 (`testWaitingWithShellKeepsWaitingColor`, `testWaitingWithShellSpeaksWaiting`, `testWaitingWithShellKeepsWaitingDot`) und Task 3 (`testWaitingWithShellTileDotIsWaitingColor`).
4. **Kachel-Deckkraft 80 % mit Hintergrundbild:** kein deckender Streifen links an der Ecke, Trenner und Ecke gleich durchscheinend. Tests in Task 6 (`testDividerLeavesCornerAreaToModuleCorner`, `testCornerLeftStripTranslucentLikeDivider`) plus Render in Task 7.
5. **UI-Größe 130 %:** Fuge unter dem Kachelkopf bleibt 1 + 1 pt, Strich-Lücken gleich breit. Tests in Task 1 (`testMeterGapsEvenAtEveryScale`) und Task 3 (`testHeaderSeamStaysHairlineAtScale`).

---

### Task 1: Kit: eingerastete Strich-Anzeige, Rand, Gruppen-Akzent

**Files:**
- Modify: `app/Kadrell/Theme+Instrument.swift`
- Modify: `app/Kadrell/Sidebar/SidebarRenderer.swift` (`groupName`, `groupCount`, `groupLabelAttributes`, `drawGroupAccent` nutzen das Kit)
- Test: `app/KadrellTests/InstrumentTests.swift`, `app/KadrellTests/SidebarRendererTests.swift`

**Interfaces:**
- Produces (alle in `enum Instrument`):
  - `@MainActor static func meter(_ r: CGRect, lit: Int?, color: NSColor)`: Signatur bleibt, Rechnung in View-Punkten (Prüfpunkt d):

    ```swift
    let s = Theme.scale, total = (r.width * s).rounded()
    let gap = max(1, (1.5 * s).rounded(.down)), bar = max(1, ((total - 3 * gap) / 4).rounded(.down))
    let x0 = snapped(r.minX) + ((total - 4 * bar - 3 * gap) / 2).rounded(.down) / s
    // Strich i: x = x0 + i * (bar + gap) / s, Breite bar / s; y und Höhe über snapped(r.minY) ... snapped(r.maxY)
    ```
    Bei 12 pt Breite: 100 % → Striche 2, Lücken 1 (bei x 0, 3, 6, 9); 130 % → Striche 3, Lücken 1.
  - `static func topLeftRounded(_ r: CGRect, radius: CGFloat) -> NSBezierPath`: Rechteck, nur die Ecke bei (minX, minY) rund (gespiegelte Views: oben links); `radius <= 0` = Rechteck.
  - `@MainActor static func rim(_ r: CGRect, top: Bool, radius: CGFloat = 0)`: Rand einer eingesetzten Fläche ohne Unterkante. Links, rechts und mit `top` oben je außen 1 Haarlinie `seamDark`, direkt innen 1 Haarlinie `seamLight`; als offener Pfad (unten links hoch, mit `top` Bogen mit `radius - Einzug` und oben entlang, rechts runter), dunkel bei Einzug `hairline / 2`, hell bei `1.5 * hairline`, Strichbreite `hairline`, Kanten über `snapped`.
  - `static func groupLabel(_ text: String) -> NSAttributedString`: `text.uppercased()`, `Theme.attrs(10.5, Theme.sub, bold: true)` plus `.kern: CGFloat(0.8)`.
  - `@MainActor static func groupAccent(center: CGPoint, color: NSColor)`: 6 × 6 pt, Radius 1,5, mittig auf `center`.
- `SidebarRenderer.groupName/groupCount` bauen über `Instrument.groupLabel`, `drawGroupAccent` ruft `Instrument.groupAccent(center: CGPoint(x: 29, y: head.midY), color: g.color)`. Gezeichnetes Ergebnis im Baum unverändert.

- [ ] **Step 1: Tests schreiben** (`InstrumentTests`)

```swift
func testMeterGapsEvenAtEveryScale() {
    for s: CGFloat in [0.9, 1, 1.1, 1.2, 1.3] {
        let w = Int((14 * s).rounded(.up)), h = Int((12 * s).rounded(.up))
        let rep = render(theme: Theme.current, width: w, height: h, scale: s) {
            Instrument.meter(CGRect(x: 1, y: 1, width: 12, height: 10), lit: nil, color: .white)
        }
        // Zeile in der Mitte abtasten: Folgen deckender Pixel (Striche) und Lücken dazwischen
        let (bars, gaps) = runs(rep, y: Int(6 * s))
        XCTAssertEqual(bars.count, 4, "\(s)")
        XCTAssertEqual(Set(bars).count, 1, "\(s): Striche gleich breit \(bars)")
        XCTAssertEqual(Set(gaps).count, 1, "\(s): Lücken gleich breit \(gaps)")
        XCTAssertGreaterThanOrEqual(gaps.first ?? 0, 1)
    }
}

func testRimDarkOutsideLightInsideWithoutBottom() {
    // 40×20 auf panel, rim(top: true), Instrument Dark: Spalte 0 dunkler als panel, Spalte 1 heller als Spalte 0,
    // Spalte 39 dunkel, 38 hell, Zeile 0 dunkel, Zeile 1 hell, Zeile 19 (Mitte) = panel (keine Unterkante)
}

func testGroupLabelAndAccent() {
    let l = Instrument.groupLabel("kadrell")
    XCTAssertEqual(l.string, "KADRELL")
    XCTAssertEqual(l.attribute(.kern, at: 0, effectiveRange: nil) as? CGFloat, 0.8)
    // groupAccent(center: (10, 10), color: .red) auf 20×20: (10,10) rot, (6,10) durchsichtig
}
```

`runs(_:y:)` als private Hilfsfunktion in `InstrumentTests` (Alpha > 0,5 = Strich). `testMeterWithoutLitDrawsAllBarsSolid` an die neuen Lagen anpassen: Striche bei x 1, 4, 7, 10, Lücke bei 2. In `SidebarRendererTests` die Striche im Baum (`meterRect` x = 26) nachziehen: Striche bei 27, 30, 33, 36, Lücken bei 28, 31, 34, x = 37 Hintergrund (`testWorkingSessionShowsMeterNotDot`, `testMeterLitDimsTheRestAndFollowsClockStep` gedimmt bei 33 und 36, `testShellRunningUsesShellColor` bei 27 und 36).

- [ ] **Step 2:** `$TEST InstrumentTests` · Expected: FAIL (Lücken 1, 2, 1 bei 100 %; `rim`, `groupLabel`, `groupAccent` unbekannt).
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `$TEST InstrumentTests`, `$TEST SidebarRendererTests` · Expected: PASS.
- [ ] **Step 5: Commit** `Kit: Strich-Anzeige rastet gleichmäßig ein, Rand und Gruppen-Akzent zentral`

---

### Task 2: Wartet und Fehler schlagen die Shell-Farbe

**Files:**
- Modify: `app/Kadrell/Theme.swift` (`statusColor`, neu `meterColor`)
- Modify: `app/Kadrell/UI/Accessibility.swift` (`SessionStatus.spoken(attached:ended:shellRunning:)`)
- Modify: `app/Kadrell/Sidebar/SidebarRenderer.swift` (`SidebarSessionItem.meter(for:attached:lit:)`)
- Modify: `app/Kadrell/Sidebar/SidebarView.swift` (`dotColor`, Bedingung der Puls-Schleife)
- Test: `app/KadrellTests/BackgroundShellsTests.swift` (`StatusDotColorTests`), `app/KadrellTests/SidebarRendererTests.swift`

**Interfaces:**
- Produces:
  - `Theme.statusColor(_:attached:shellRunning:)`: Shell-Farbe nur noch bei `shellRunning && status == .idle` (bisher `status != .running`, das ist die alte Regel, die Warten blau bzw. grau färbte).
  - `static func meterColor(_ s: Session, attached: Bool) -> NSColor?` in `Theme`: nil = Punkt. Gesetzt bei `attached && (s.status == .running || (s.hasRunningShell && s.status == .idle))`, Wert `statusColor(s.status, attached: true, shellRunning: s.hasRunningShell)`. Einzige Quelle für „Striche ja/nein“ in Baum, Kachel und Stack.
  - `SidebarSessionItem.meter(for:attached:lit:)` = `Theme.meterColor(s, attached:).map { ($0, lit) }`.
  - `SessionStatus.spoken(attached:ended:shellRunning:)`: „Kommando läuft noch“ nur bei `shellRunning && self == .idle`.
  - `SidebarView.dotColor`: ohne Puls, `Theme.statusColor(s.status, attached:, shellRunning: s.hasRunningShell)` (Aufblitzen bleibt). Die 80-ms-Schleife invalidiert nur Zeilen mit `Theme.meterColor(s, attached: isAttached(s.id)) != nil`.

- [ ] **Step 1: Tests schreiben**

```swift
// StatusDotColorTests (BackgroundShellsTests.swift)
func testIdleWithShellPaintsShellColor() {
    XCTAssertEqual(Theme.statusColor(.idle, attached: true, shellRunning: true), Theme.shell)
}

func testWaitingWithShellKeepsWaitingColor() {
    withTheme(.named("instrument-dark")) {
        XCTAssertEqual(Theme.statusColor(.waiting, attached: true, shellRunning: true).hexString, "#ff5e14")
    }
    XCTAssertEqual(Theme.statusColor(.waiting, attached: true, shellRunning: true), Theme.waiting)
}

func testErrorWithShellKeepsErrorColor() {
    XCTAssertEqual(Theme.statusColor(.error, attached: true, shellRunning: true), Theme.error)
}

func testWaitingWithShellSpeaksWaiting() {
    XCTAssertEqual(SessionStatus.waiting.spoken(attached: true, shellRunning: true), SessionStatus.waiting.spoken)
    XCTAssertEqual(SessionStatus.idle.spoken(attached: true, shellRunning: true), String(localized: "Kommando läuft noch", bundle: Bundle.app))
}

// SidebarRendererTests
func testWaitingWithShellKeepsWaitingDot() {
    let rep = renderSession(session(status: "waiting", shellRunning: true), theme: dark)
    withTheme(dark) { assertColor(rep, 31, 22, Theme.waiting, "Punkt in Warte-Farbe, nicht Shell-Farbe") }
}
```

`StatusDotColorTests` ist nicht `@MainActor`; für `withTheme` die Klasse mit `@MainActor` markieren wie `SidebarRendererTests`. Den Doc-Kommentar der Klasse („blau, solange in einer wartenden Session …“) auf die neue Regel ändern. `testMeterOnlyWhenAttachedAndWorking` bleibt unverändert grün.

- [ ] **Step 2:** `$TEST StatusDotColorTests`, `$TEST SidebarRendererTests` · Expected: FAIL (Warten + Shell liefert Shell-Farbe).
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `$TEST StatusDotColorTests`, `$TEST SidebarRendererTests`, `$TEST ContrastTests`, `$TEST AccessibilityTests` · Expected: PASS.
- [ ] **Step 5: Commit** `Wartet und Fehler zeigen ihre Farbe auch bei laufendem Kommando`

---

### Task 3: Kachel: neutraler Kopf, Gruppen-Akzent, Fugen, Strich-Anzeige

**Files:**
- Modify: `app/Kadrell/Workspace/CellView.swift`
- Modify: `app/Kadrell/Workspace/WorkspaceView.swift` (`tick`: Kacheln)
- Create: `app/KadrellTests/TileTests.swift`
- Modify: `app/KadrellTests/InstrumentTests.swift` (`testTopLeftTileFrameFollowsModuleCorner`)

**Interfaces:**
- Consumes: `Instrument.hSeam`, `Instrument.groupLabel`, `Instrument.groupAccent`, `Instrument.topLeftRounded`, `Instrument.meter`, `Theme.meterColor` (Task 1, 2), `Feedback.meterLit()`.
- Produces in `CellView`:
  - `bodyColor = Theme.bg.withAlphaComponent(CGFloat(Settings.tileOpacity))`.
  - Kopf: Fläche `Theme.surface`, unten `Instrument.hSeam(x: 0, y: b.height - 2 * Instrument.hairline, width: b.width)` (ersetzt die `Theme.line`-Linie). Status: mit `Theme.meterColor(state.session, attached: state.attached)` die Striche in `meterRectLogical = CGRect(x: 7, y: 8, width: 12, height: 10)` mit `lit: Feedback.meterLit()`, sonst `Icons.statusDot` in `dotRectLogical` (9, 9, 8, 8) in `Theme.statusColor(…)` ohne Puls. Gruppe nach dem Branch: Akzent `groupAccent(center: CGPoint(x: x + 3, y: 13), color: state.groupColor)`, Label `Instrument.groupLabel(state.groupName)` ab `x + 11` bis zum Rest, nur wenn `rest >= min(Label + 11, 51)`. Titel, Branch, Laufzeit, Z-Badge, Hover-Knöpfe unverändert.
  - `var statusRect: CGRect`: `dotRectLogical.union(meterRectLogical).scaled(Theme.scale)` (ersetzt `dotRect` beim Invalidieren).
  - Rand (`viewWillDraw`): Farbe nach `borderColor`: Ablegeziel `fg`, Ordner fehlt/Startfehler `error`, fokussiert `fg`, wartet (angehängt) `waiting`, Fehler `error`, sonst `Instrument.seamDark`. Breite 2 bei fokussiert oder Ablegeziel, sonst 1. Überblenden 120 ms bleibt.
  - Heller Innenrand: am Ende von `draw` 1 pt `Instrument.seamLight` als Strich von `Instrument.topLeftRounded(bounds.insetBy(dx: 1.5, dy: 1.5), radius: max(0, moduleCornerRadius - 1.5))`, Strichbreite 1 (unskaliert, View-Punkte).
  - `drawHeader` in benannte Teile zerlegen (z. B. `drawStatus`, `drawZoomBadge`, `drawTitleLine`, `drawHoverIcons`), damit lizard leer bleibt. Kommentare zu `State.focused` („Rahmen in Gruppenfarbe“) und `bodyColor` („getönt“) nachziehen.
- In `WorkspaceView.tick`: für sichtbare Kacheln mit Kopf und `Theme.meterColor(v.state.session, attached: v.state.attached) != nil` nur `v.setNeedsDisplay(v.statusRect)`; `v.pulse` wird nicht mehr gesetzt (Löschen in Task 4).

- [ ] **Step 1: Tests schreiben** (`TileTests`, `@MainActor`, Instrument Dark, `Feedback.reduceMotion = true` per `defer` zurück)

Hilfsfunktion `renderCell(_ cell: CellView, width:height:scale:)`: setzt `Theme.scale`, zeichnet `cell.draw(cell.bounds)` direkt in ein gespiegeltes Bitmap (ohne äußeres `Theme.scaled`, die Kachel skaliert selbst), Bild 1 px = 1 View-Punkt. `cell(status:shell:attached:groupColor:focused:)` baut eine `CellView` 240 × 120 mit gesetztem `state`. Zeilen von Kopf und Körper bei x = 200 abtasten (links davon Titel, in der Körpermitte Spinner und Label, die Laufzeit steht erst ab y = 6).

```swift
func testTileHeaderIsNeutral() {
    // Gruppenfarbe rot und blau, fokussiert und nicht: Pixel (200, 3) im Kopf jeweils ≈ Theme.surface
}

func testTileHeaderShowsGroupAccent() {
    // groupName "kadrell", groupColor 0xff0000: in Zeile 13 gibt es ein Pixel ≈ rot (Akzent),
    // in Zeile 3 keins (Kopf nicht getönt)
}

func testTileHeaderSeamUnderHeader() {
    // Maßstab 1, Kopf 26: Zeile 24 dunkler als surface, Zeile 25 heller als Zeile 24 (× 1,15), Zeile 26 ≈ bg (Körper)
}

func testHeaderSeamStaysHairlineAtScale() {
    // Maßstab 1,3, Kopf 34: Zeile 31 ≈ surface, 32 dunkel, 33 hell, 34 ≈ bg
}

func testTileBodyIsNeutral() {
    // bodyColor gleich Theme.bg (mit Settings.tileOpacity) für rote und blaue Gruppe
}

func testTileRimIsSeam() {
    // nicht fokussiert: nach viewWillDraw() borderWidth 1, borderColor == Instrument.seamDark.cgColor;
    // im Bild Spalte 1 bei y = 80 heller als Spalte 3 (Körper). Fokussiert: borderWidth 2, borderColor == Theme.fg.cgColor
}

func testWorkingTileShowsMeter() {
    // busy + angehängt: Striche bei x 7, 10, 13, 16 (Zeile 13) ≈ Theme.running, Lücken 9, 12, 15 ≈ surface
}

func testWaitingTileKeepsDot() {
    // waiting + angehängt: (13, 13) ≈ Theme.waiting, (8, 13) ≈ surface
}

func testWaitingWithShellTileDotIsWaitingColor() {
    // waiting + hasRunningShell: (13, 13) ≈ #ff5e14
}
```

In `InstrumentTests.testTopLeftTileFrameFollowsModuleCorner` die erwarteten Farben anpassen: Kopf bei (80, 8) ≈ `Theme.surface`, Rahmen auf dem Bogen und an den geraden Kanten ≈ `Theme.fg` (fokussiert). Die übrigen Erwartungen (Bogen, außerhalb nichts) bleiben.

- [ ] **Step 2:** `$TEST TileTests` · Expected: FAIL (Kopf getönt, Linie statt Fuge, Punkt statt Striche).
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `$TEST TileTests`, `$TEST InstrumentTests`, `$TEST AccessibilityTests`, `$TEST MultiWindowTests` · Expected: PASS. lizard leer.
- [ ] **Step 5: Commit** `Kacheln: neutraler Kopf mit Gruppen-Akzent, Fugen als Rand und unter dem Kopf, Strich-Anzeige`

---

### Task 4: Stack-Zeilen und Abschied vom Puls

**Files:**
- Modify: `app/Kadrell/Workspace/WorkspaceView.swift` (`draw`, `drawStackRow`, `stackDotRect`, `tick`, `pulse`, `stackRows`)
- Modify: `app/Kadrell/Workspace/CellView.swift` (`pulse` entfernen)
- Modify: `app/Kadrell/Theme.swift` (`dotColor` entfernen), `app/Kadrell/Services/Feedback.swift` (`pulse` entfernen)
- Test: `app/KadrellTests/TileTests.swift`, `app/KadrellTests/BackgroundShellsTests.swift`

**Interfaces:**
- Consumes: `Instrument.rim`, `Instrument.hSeam`, `Instrument.topLeftRounded`, `Instrument.groupLabel`, `Instrument.groupAccent`, `Instrument.meter`, `Theme.meterColor`, `WorkspaceView.moduleCornerRadius(for:gap:)`.
- Produces:
  - `private(set) var stackRows: [(CGRect, String)]` (für Tests lesbar).
  - Zeichenschleife: `for (i, (r, key)) in stackRows.enumerated()` mit `radius = Self.moduleCornerRadius(for: r, gap: CGFloat(Settings.tileGap)) / Theme.scale`, `first: i == 0`, `bodyBelow: tileFrames[key] != nil`.
  - `private func drawStackRow(_ r: CGRect, key: String, first: Bool, bodyBelow: Bool, radius: CGFloat)`, in logischen Punkten: Clip auf `Instrument.topLeftRounded(r, radius: radius)`; Fläche `Theme.surface` (fokussiert oder Sync) bzw. `Theme.panel`; `Instrument.rim(r, top: first, radius: radius)`; ohne `bodyBelow` unten `Instrument.hSeam(x: 0, y: r.maxY - 2 * Instrument.hairline, width: r.width)`. Status: Striche in `CGRect(x: r.minX + 10, y: r.midY - 5, width: 12, height: 10)` bzw. Punkt in `CGRect(x: r.minX + 12, y: r.midY - 4, width: 8, height: 8)` nach `Theme.meterColor`/`Theme.statusColor`. Rechts: Hover-Knöpfe, Laufzeit, dann Gruppen-Label rechtsbündig mit Akzent (Mitte 8 pt links vom Label), dann Pfad; Titel ab x 28 wie bisher. Kein Farbbalken links. Zerlegen (z. B. `drawStackRowGround`, `drawStackRowRight -> CGFloat`), damit lizard leer bleibt.
  - `stackDotRect` wird `stackStatusRect(_ r: CGRect) -> CGRect` (Union aus Punkt- und Strich-Rechteck, mal `Theme.scale`); `tick` invalidiert es nur für Zeilen mit `Theme.meterColor(…) != nil`.
  - Entfernt: `Theme.dotColor`, `Feedback.pulse`, `CellView.pulse`, `WorkspaceView.pulse`. In `StatusDotColorTests` fallen `testRunningShellPulses` und `testIdleWithoutShellDoesNotPulse` weg, die übrigen `dotColor`-Tests laufen über `Theme.statusColor`.

- [ ] **Step 1: Tests schreiben** (`TileTests`; Arbeitsfläche 400 × 300, Stack, drei Sessions „A“, „B“, „C“ mit cwd „/p“ in einer Gruppe „G“, `Theme.scale = 1`, Bild über `cacheDisplay` wie `render(_ view:)` in `InstrumentTests`; Flächen bei x = 150 abtasten, dort steht kein Text)

```swift
func testStackRowsAreNeutralWithSeams() {
    // Abstand 6, Fokus auf der ersten Session: Zeile 0 Mitte ≈ surface, Zeile 2 Mitte ≈ panel (beide ohne Tönung
    // bei roter Gruppenfarbe); unter Zeile 1 (maxY - 2) dunkler als panel, maxY - 1 heller;
    // unter Zeile 0 (Körper folgt) keine eigene Fuge: Zeile 0 maxY - 2 ≈ surface;
    // links an Zeile 2: Spalte minX dunkler, minX + 1 heller als Spalte minX
}

func testStackRowShowsMeterForWorkingSession() {
    // Angehängt geht nur über einen echten AttachManager: ws.attach = AttachManager(cli: ClaudeCLI(binary: "/usr/bin/false",
    // environment: ["SHELL": "/bin/sh"])), Sessions mit Id Session.shellPrefix + …, rawStatus "busy" bzw. "waiting", attachNow.
    // Nicht fokussierte arbeitende Zeile: Striche bei x = minX + 11, 14, 17, 20 auf der Zeilenmitte ≈ Theme.running,
    // Lücke bei minX + 12 ≈ panel. Wartende Zeile: Punktmitte (minX + 16) ≈ Theme.waiting.
}

func testFirstStackRowFollowsModuleCorner() {
    // Abstand 0: Pixel (1, 1) durchsichtig (außerhalb des Bogens), (14, 4) ≈ Zeilenfläche;
    // Abstand 6 (Radius 8 um (14, 14)): (7, 7) durchsichtig, (20, 8) ≈ Zeilenfläche
}
```

Nach dem Test mit `AttachManager` alle Shells per `attach.stop(id)` beenden (Muster `AttachManagerTests.testTerminalIsReleasedAfterStop`).

- [ ] **Step 2:** `$TEST TileTests` · Expected: FAIL (getönte Zeilen, `Theme.line`, Punkt, eckige erste Zeile).
- [ ] **Step 3:** Implementieren, danach den Puls löschen; der Build zeigt jede verbliebene Stelle.
- [ ] **Step 4:** `$TEST TileTests`, `$TEST StatusDotColorTests`, `$TEST AccessibilityTests` (Stack-Zeilen-Labels unverändert), `$TEST FeedbackTests` · Expected: PASS. lizard leer.
- [ ] **Step 5: Commit** `Stack-Zeilen neutral mit Fugen und Strich-Anzeige, Puls entfällt`

---

### Task 5: Seitlich gescrollte Kacheln unter der Modul-Ecke

**Files:**
- Modify: `app/Kadrell/Workspace/WorkspaceView.swift` (`moduleMask(for:)`, `updateCell`)
- Modify: `app/Kadrell/Workspace/CellView.swift` (`moduleMask`)
- Test: `app/KadrellTests/TileTests.swift`

**Interfaces:**
- Produces:
  - `static func moduleMask(for frame: CGRect) -> CGPath?` in `WorkspaceView`, Pfad in Kachel-Koordinaten, gefüllt gerade-ungerade. nil, wenn die Kachel nicht in den Bereich außerhalb des Bogens ragt:

    ```swift
    let r = ModuleCornerView.scaledRadius
    let near = CGPoint(x: max(frame.minX, 0), y: max(frame.minY, 0))   // fernster Punkt von (r, r) in Kachel ∩ Eckquadrat
    guard near.x < r, near.y < r, hypot(near.x - r, near.y - r) > r else { return nil }
    // Pfad: Kachel-Bounds + Quadrat (−minX, −minY, r, r) + Kreis (−minX, −minY, 2r, 2r)
    ```
  - `var moduleMask: CGPath?` in `CellView`: setzt `layer.mask` auf eine `CAShapeLayer` (`fillRule = .evenOdd`, `frame = layer.bounds`) bzw. nil.
  - `updateCell`: nach `moduleCornerRadius` `v.moduleMask = v.moduleCornerRadius > 0 ? nil : Self.moduleMask(for: f)`. Die Kachel genau in der Ecke behält ihren konzentrischen Radius, alle übrigen eckig, nur angeschnittene bekommen die Maske.

- [ ] **Step 1: Tests schreiben**

```swift
func testModuleMaskFollowsArc() {
    let saved = Theme.scale; defer { Theme.scale = saved }
    Theme.scale = 1                                                    // r = 14, Mitte (14, 14)
    let p = try XCTUnwrap(WorkspaceView.moduleMask(for: CGRect(x: -100, y: 6, width: 300, height: 200)))
    XCTAssertFalse(p.contains(CGPoint(x: 101, y: 1), using: .evenOdd), "Fläche (1, 7): außerhalb des Bogens")
    XCTAssertTrue(p.contains(CGPoint(x: 110, y: 2), using: .evenOdd), "Fläche (10, 8): innerhalb")
    XCTAssertTrue(p.contains(CGPoint(x: 150, y: 2), using: .evenOdd), "rechts der Ecke")
    XCTAssertNil(WorkspaceView.moduleMask(for: CGRect(x: 306, y: 6, width: 300, height: 200)))
    XCTAssertNil(WorkspaceView.moduleMask(for: CGRect(x: 6, y: 6, width: 300, height: 200)), "Kachel in der Ecke bei Abstand 6")
}

func testScrolledTileUnderCornerIsMasked() {
    // Arbeitsfläche 600 × 400, Layout Scrollen, drei Sessions, Abstand 0 und 6:
    // scrollBy(100) → die erste Kachel liegt links angeschnitten: layer?.mask ist CAShapeLayer;
    // die zweite: mask nil; scrollBy(-100) → erste Kachel wieder in der Ecke: mask nil, moduleCornerRadius > 0
}
```

- [ ] **Step 2:** `$TEST TileTests` · Expected: FAIL (`moduleMask` unbekannt).
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `$TEST TileTests`, `$TEST InstrumentTests` · Expected: PASS.
- [ ] **Step 5: Commit** `Scrollen: angeschnittene Kacheln folgen dem Bogen der Modul-Ecke`

---

### Task 6: Fenster: Ecke ohne deckenden Streifen, Beobachter abmelden

**Files:**
- Modify: `app/Kadrell/App/AppDelegate.swift` (`ThinSplitView.cornerRect`, `drawDivider`; `closeWindow` ruft `c.close()`)
- Modify: `app/Kadrell/App/MainWindow.swift` (`cornerObserver`, `close()`, `placeCorner`)
- Modify: `app/Kadrell/Workspace/ModuleCornerView.swift` (linker Streifen nicht mehr deckend, Doc-Kommentar)
- Test: `app/KadrellTests/InstrumentTests.swift`

**Interfaces:**
- Produces:
  - `ThinSplitView.cornerRect: CGRect = .zero` (Split-Koordinaten). `drawDivider(in:)` zeichnet Grund und Fuge nur im Teil von `rect`, der nicht in `cornerRect` liegt (senkrechter Trenner: oben um `cornerRect.intersection(rect).height` kürzen, Lage über `isFlipped`).
  - `placeCorner()` setzt zusätzlich `split.cornerRect = root.convert(corner.frame, to: split)` und bei Änderung `split.needsDisplay = true`.
  - `ModuleCornerView.draw`: außerhalb des Bogens `Theme.panel` mit Kachel-Deckkraft; deckend nur noch der obere Streifen (Höhe 2, unter der deckenden Leiste). Kein deckender linker Streifen.
  - `MainWindowController`: `private var cornerObserver: NSObjectProtocol?` (Token aus `addObserver(forName:object:queue:using:)` im `init`), `func close()` meldet ihn ab (`NotificationCenter.default.removeObserver`), setzt ihn auf nil und ruft `workspace.close()`. `AppDelegate.closeWindow` ruft `c.close()` statt `c.workspace.close()` (Prüfpunkt c).

- [ ] **Step 1: Tests schreiben**

```swift
func testDividerLeavesCornerAreaToModuleCorner() {
    let split = ThinSplitView(frame: CGRect(x: 0, y: 0, width: 100, height: 40)); split.isVertical = true
    split.cornerRect = CGRect(x: 50, y: -2, width: 16, height: 16)
    let rep = render(theme: .named("instrument-dark"), width: 100, height: 40, scale: 1) {
        split.drawDivider(in: CGRect(x: 50, y: 0, width: 2, height: 40))
    }
    XCTAssertEqual(alpha(rep, x: 50, y: 5), 0, accuracy: 0.01, "unter der Ecke malt der Trenner nichts")
    XCTAssertGreaterThan(alpha(rep, x: 50, y: 20), 0.5, "darunter Grund und Fuge")
}

func testCornerLeftStripTranslucentLikeDivider() {
    let old = Settings.tileOpacity; defer { Settings.tileOpacity = old }
    Settings.tileOpacity = 0.5
    let rep = render(ModuleCornerView(frame: CGRect(x: 0, y: 0, width: 16, height: 16)))
    XCTAssertEqual(alpha(rep, x: 0, y: 8), 0.5, accuracy: 0.05, "links wie der Trenner")
    XCTAssertEqual(alpha(rep, x: 8, y: 0), 1, accuracy: 0.05, "oben wie die deckende Leiste")
}

func testClosedWindowNoLongerMovesCorner() {
    let wc = MainWindowController(index: 26, cascade: nil)
    defer { wc.window.close() }
    wc.window.setContentSize(NSSize(width: 1000, height: 700)); wc.split.layoutSubtreeIfNeeded()
    wc.split.setPosition(300, ofDividerAt: 0)
    XCTAssertEqual(wc.corner.frame.minX, 300, "Beobachter aktiv")
    wc.close()
    wc.split.setPosition(400, ofDividerAt: 0)
    RunLoop.main.run(until: Date().addingTimeInterval(0.05))
    XCTAssertEqual(wc.corner.frame.minX, 300, "nach close() zieht niemand mehr die Ecke nach")
}
```

`testCornerContinuesStraightSeams`, `testCornerIgnoresClicksAndCoversOnlyTheCorner` und `testApplyAppearanceRedrawsSeamDividers` laufen unverändert mit.

- [ ] **Step 2:** `$TEST InstrumentTests` · Expected: FAIL (`cornerRect`, `close()` unbekannt; Alpha 1 im linken Streifen).
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `$TEST InstrumentTests`, `$TEST MultiWindowTests`, `$TEST LeftDockTests`, `$TEST SidebarToggleTests` · Expected: PASS.
- [ ] **Step 5: Commit** `Ecke: kein deckender Streifen bei Deckkraft unter 100 %, Beobachter beim Schließen abgemeldet`

---

### Task 7: Sichtprüfung und Gesamtprüfung

**Files:**
- Temporär: `app/KadrellTests/RenderStufe3Tests.swift` (nach der Prüfung löschen, nie committen; Bilder bleiben)

- [ ] **Step 1: Render-Test schreiben.** Ein `MainWindowController` (Index 30, Fenster 1400 × 900, durchsichtig vorn wie in `testApplyAppearanceRedrawsSeamDividers`) mit `AttachManager(cli: ClaudeCLI(binary: "/usr/bin/false", environment: ["SHELL": "/bin/sh"]))` an Baum und Arbeitsfläche. Sechs Shell-Sessions (Id mit `Session.shellPrefix`) in zwei Gruppen mit deutlich verschiedenen Farben, Status über `rawStatus`/`hasRunningShell`: arbeitet, wartet, fertig + Shell, wartet + Shell, Fehler, fertig; alle per `attachNow` angehängt, Laden wie in `AppDelegate.reloadViews`, alle ausgewählt, eine fokussiert, eine Kachel mit Hover. Varianten: Instrument Dark, Instrument Light, GitHub Light × UI-Größe 100 % und 130 % × Layout Grid und Stack, dazu je Schema einmal Abstand 0, einmal Deckkraft 80 % mit Hintergrundbild (ein vorhandenes Bild aus `/System/Library/Desktop Pictures`, falls keins da ist ohne) und einmal Scrollen mit `scrollBy(120)`. Bild über die Ebenen des Fensterinhalts wie `renderCellLayer` (zurückgespiegelt), dazu je ein vierfach vergrößerter Ausschnitt der Ecke. `layer.render(in:)` übergeht Masken und `maskedCorners`: die Ecke beim Scrollen und oben links rund prüfen die Tests aus Task 3 bis 5 und Step 3. Speichern als PNG nach `~/.claude/screenshots/kadrell/instrument-stufe-3/<layout>-<schema>-<größe>[-<variante>].png` (Ordner anlegen, nichts darin löschen). Am Ende alle Shells stoppen, Einstellungen per `defer` zurück.
- [ ] **Step 2: Laufen lassen und ansehen.** `$TEST RenderStufe3Tests` (läuft durch `TEST_RUNNER_KADRELL_PROFILE=tmp` im Temp-Profil). Jedes Bild ansehen: Fugen in allen drei Schemata sichtbar und nicht schmutzig, Kopf neutral, Akzent und Label lesbar, Striche gleichmäßig, Fokus-Rahmen klar, Ecke ohne Kante oder Streifen. Fällt etwas auf: nachjustieren (Fugenfarben je Schema sind erlaubt, Spec „Dem Umsetzer überlassen“), betroffene Tests erneut, Bilder neu.
- [ ] **Step 3: Laufende App.** Debug-Build mit `--profile tmp` starten, zwei Terminals (⌘T) öffnen, Layouts reihum (⌘L) bis Scrollen und Stack, seitlich scrollen, Kachel-Deckkraft in den Einstellungen auf 80 %, Fenster schließen und neu öffnen (⌘⇧T); per Socket `ls` prüfen, dass die Instanz lebt. Nur die eigene Instanz per pid beenden.
- [ ] **Step 4: Gesamtprüfung.** Render-Test löschen. Gesamtsuite grün, Clean Build ohne eigene Warnungen (`xcodebuild … clean build 2>&1 | grep -E "warning:|error:" | grep -v appintentsmetadataprocessor` leer), `cd app && uv tool run lizard Kadrell -w` leer, `grep -rn "Theme.line" app/Kadrell/Workspace` nur noch beim Knopf im Leerzustand.
- [ ] **Step 5: Commit** nur, wenn Step 2 Nachjustierungen gebracht hat: `Kacheln: Feinschliff nach der Sichtprüfung`

---

### Task 8: Changelog

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 1:** Oben unter `# Changelog` / „Neueste Version oben.“ einen Abschnitt `## Unreleased` anlegen (über `## 1.71.0`) mit:

```
- Änderung: Kacheln und Stack-Zeilen sind neu gestaltet: neutrale Köpfe mit kleinem Farbpunkt der Gruppe statt eingefärbter Flächen, feine Fugen zwischen den Kacheln und unter den Köpfen. Die fokussierte Kachel hat einen hellen Rahmen.
- Änderung: Arbeitende Sessions zeigen auch in Kachelköpfen und Stack-Zeilen die Strich-Anzeige statt eines pulsierenden Punkts.
- Änderung: Wartet eine Session, während im Hintergrund noch ein Kommando läuft, zeigt ihr Punkt die Warte-Farbe, und VoiceOver sagt „wartet“.
- Fix: Seitlich gescrollte Kacheln und die erste Stack-Zeile folgen der runden Ecke der Arbeitsfläche, statt abgeschnitten zu werden.
- Fix: Bei Kachel-Deckkraft unter 100 % zeigt die runde Ecke keinen deckenden Streifen mehr.
- Fix: Die Striche der Arbeits-Anzeige stehen bei jeder UI-Größe gleichmäßig weit auseinander.
- Fix: Das Layout-Symbol in der Leiste ist so groß wie die Schrift der übrigen Tasten.
```

Kein `bump-version.py`: die Version setzt der Plan für Stufe 4 einmal für beide Stufen.
- [ ] **Step 2: Commit** `Changelog: Instrument-Kacheln (Stufe 3)` (nur `CHANGELOG.md`).
