# Instrument Stufe 2: Baum und Split-Trenner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Der Baum und die Trenner bekommen die Instrument-Sprache: Fugen statt 1-px-Linien zwischen Baum, Arbeitsfläche, rechter Sidebar und linkem Dock, eine runde Modul-Ecke oben links an der Arbeitsfläche, ruhige Gruppen-Labels mit Farbakzent, Auswahl als Umriss, Strich-Anzeige statt pulsierendem Punkt. „Getönte Gruppen“ entfällt.

**Architecture:** Neue Bauteile (Strich-Anzeige, Modul-Ecke) kommen ins vorhandene Kit `Theme+Instrument.swift` bzw. als kleine View neben die Arbeitsfläche. Der Baum ändert sich nur in den gemeinsamen Bausteinen von `SidebarRenderer` (Extension) und in den zwei verbleibenden Renderern. `ThinSplitView` zeichnet seinen Trenner selbst als Fuge.

**Tech Stack:** Swift 6, AppKit (handgezeichnete NSViews), XCTest, String Catalog.

**Spec:** `docs/superpowers/specs/2026-10-22-instrument-fugen-design.md` (Stufe 2, Ziele 2, 5, 6, 7, Entscheidungen „Auswahl im Baum“, „Bash-Kommando läuft“, „Getönte Gruppen fällt weg“). Stufe 1 ist seit 1.70.0 auf master.

**Branch:** Worktree auf `instrument-stufe-2` (Skill using-git-worktrees). Commits pro Task, Changelog und Version erst in Task 6.

## Global Constraints

- Clean Build ohne eigene Warnungen. Einzige erlaubte Meldung: `appintentsmetadataprocessor: Metadata extraction skipped`.
- `cd app && uv tool run lizard Kadrell -w` liefert nichts (CCN ≤ 15).
- Jeder neue Nutzertext als `String(localized: "…", bundle: Bundle.app)`, Deutsch und Englisch in `Localizable.xcstrings`.
- Fugen: 1 pt dunkel (`Instrument.seamDark`) + 1 pt hell (`Instrument.seamLight`), dunkle Linie oben bzw. links. In skalierten Views über `Instrument.hSeam/vSeam` (rasten selbst ein), in unskalierten Views (Split) direkt 1 pt.
- Keine Änderung an den Farbwerten der zehn alten Schemata (neue Rolle `shell` nur mit Default).
- Bewegung in Stufen, kein Überblenden für die Strich-Anzeige; bei `Feedback.reduceMotion` steht sie still, alle Striche an.
- Status nie nur über Farbe: Strich-Anzeige (Form) für arbeitet, Punkt für alles andere, „Farben nicht unterscheiden“ behält seine Glyphen.
- Tests im Temp-Profil: `TEST="cd app && /opt/homebrew/bin/xcodegen generate && TEST_RUNNER_KADRELL_PROFILE=tmp xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug -derivedDataPath build -skipPackagePluginValidation -skipMacroValidation test -only-testing:KadrellTests/<Klasse>"`. Gesamtsuite vor jedem Commit grün (Stand: 399).
- Tests, die `Theme.current`, `Theme.scale` oder `Feedback.reduceMotion` ändern, stellen sie per `defer` zurück.
- Laufende App nur mit `--profile tmp`, nur die eigene Instanz per pid beenden.
- Keine Em-Dashes. Deutsche Kommentare mit echten Umlauten. Git im Worktree als `/usr/bin/git`, Commits per Pfad.

## Review Focus

1. **Tastaturfokus im Baum (⌘1):** der Akzentring um die fokussierte Zeile ist in allen zwölf Schemata sichtbar und liegt nicht unter dem Umriss. Test in Task 4 (`testKeyFocusRingOutsideOutline`).
2. **„Bewegung reduzieren“ an:** die Strich-Anzeige zeigt alle Striche ruhig, nichts flackert. Test in Task 2 (`testMeterStillWithReduceMotion`).
3. **Gespeichertes Baum-Design „tinted“ aus einer alten Version:** startet ohne Absturz mit „Klassisch“. Test in Task 3 (`testStoredTintedFallsBackToClassic`).
4. **Hintergrundbild mit Deckkraft unter 100 % und ausgeblendeter Baum (⌘B):** die Modul-Ecke sitzt bündig an Leiste und linkem Fensterrand, keine Lücke, kein Doppelrand. Test in Task 1 (`testCornerFollowsWorkspaceOrigin`) plus Render in Task 6.
5. **Sehr lange Gruppennamen in Versalien:** werden gekürzt, überdecken weder Warte-Marke noch Herz. Test in Task 4 (`testLongGroupLabelTruncatesBeforeBadges`).

---

### Task 1: Split-Trenner als Fuge und Modul-Ecke

**Files:**
- Modify: `app/Kadrell/App/AppDelegate.swift` (`ThinSplitView`, ca. Zeile 1276)
- Create: `app/Kadrell/Workspace/ModuleCornerView.swift`
- Modify: `app/Kadrell/Workspace/WorkspaceView.swift` (Ecke anlegen, in `relayout()` bzw. `setFrameSize` oben links halten und zuoberst einordnen)
- Test: `app/KadrellTests/InstrumentTests.swift`

**Interfaces:**
- Consumes: `Instrument.seamDark`, `Instrument.seamLight`, `Theme.panel`, `Settings.tileOpacity`, `Theme.scale`.
- Produces: `ThinSplitView.dividerThickness == 2`, `ThinSplitView.drawDivider(in:)` zeichnet die Fuge (senkrecht: links dunkel, rechts hell; waagerecht im linken Dock: oben dunkel, unten hell; `isFlipped` des Splits beachten). `final class ModuleCornerView: NSView` mit `static let radius: CGFloat = 14` (logisch, mal `Theme.scale`), `hitTest` liefert immer `nil`. Füllt außerhalb des Viertelkreises `Theme.panel.withAlphaComponent(Settings.tileOpacity)` (wie der Baum), zieht entlang des Bogens dunkle und direkt innen helle Linie, je 1 pt.

- [ ] **Step 1: Tests schreiben**

```swift
func testSplitDividerIsSeam() {
    // senkrechter Split, Trenner bei x = 50: Spalte 50 dunkler als panel, 51 heller oder gleich, Breite 2
    let split = ThinSplitView(frame: CGRect(x: 0, y: 0, width: 100, height: 40)); split.isVertical = true
    XCTAssertEqual(split.dividerThickness, 2)
    let rep = renderDivider(split, rect: CGRect(x: 50, y: 0, width: 2, height: 40), theme: .named("instrument-dark"))
    XCTAssertLessThan(luma(rep, x: 50, y: 20), luma(Theme.panel))
    XCTAssertGreaterThanOrEqual(luma(rep, x: 51, y: 20), luma(rep, x: 50, y: 20) * 1.15)
}

func testCornerIgnoresClicksAndCoversOnlyTheCorner() {
    let c = ModuleCornerView(frame: CGRect(x: 0, y: 0, width: 14, height: 14))
    XCTAssertNil(c.hitTest(CGPoint(x: 1, y: 1)))
    // Pixel ganz in der Ecke = Gehäusefarbe, Pixel nahe der Gegenecke = durchsichtig
    let rep = render(c)
    XCTAssertEqual(alpha(rep, x: 0, y: 0), 1, accuracy: 0.05)
    XCTAssertEqual(alpha(rep, x: 13, y: 13), 0, accuracy: 0.05)
}

func testCornerFollowsWorkspaceOrigin() {
    let ws = WorkspaceView(frame: CGRect(x: 0, y: 0, width: 400, height: 300))
    ws.relayout()
    let corner = try XCTUnwrap(ws.subviews.last as? ModuleCornerView, "Ecke liegt zuoberst")
    XCTAssertEqual(corner.frame.origin, .zero)   // WorkspaceView ist flipped: oben links
    XCTAssertEqual(corner.frame.width, ModuleCornerView.radius * Theme.scale, accuracy: 0.01)
}
```

Hilfsfunktionen (`renderDivider`, `render`, `alpha`) im Stil der vorhandenen `render`/`luma` in `InstrumentTests`.

- [ ] **Step 2:** `$TEST InstrumentTests` · Expected: FAIL (Dicke 1, `ModuleCornerView` unbekannt).
- [ ] **Step 3:** Implementieren. `dividerColor` entfällt bzw. bleibt nur für den Fallback. Die Griffzone (`grabRects`, 8 pt) bleibt, rechnet aber mit der neuen Dicke. Ecke nach jedem Hinzufügen von Kachel-Views wieder nach oben holen (`addSubview(corner, positioned: .above, relativeTo: nil)`).
- [ ] **Step 4:** `$TEST InstrumentTests`, `$TEST MultiWindowTests`, `$TEST LeftDockTests`, `$TEST SidebarToggleTests` · Expected: PASS.
- [ ] **Step 5: Commit** `Split-Trenner als Fuge, Arbeitsfläche mit runder Modul-Ecke`

---

### Task 2: Strich-Anzeige und Shell-Farbe

**Files:**
- Modify: `app/Kadrell/Theme+Instrument.swift`
- Modify: `app/Kadrell/Services/Feedback.swift`
- Modify: `app/Kadrell/Theme.swift` (Rolle `shell`)
- Test: `app/KadrellTests/InstrumentTests.swift`, `app/KadrellTests/ContrastTests.swift`

**Interfaces:**
- Produces:
  - `Instrument.meter(_ r: CGRect, lit: Int?, color: NSColor)`: vier senkrechte Striche, Lücke 1,5 pt, Breite `(r.width - 4.5) / 4`, volle Höhe, x-Positionen über `Instrument.snapped`. `lit == nil` → alle vier in `color`; sonst die ersten `lit` (1…4) in `color`, die übrigen in `color` mit Alpha 0,25.
  - `Feedback.meterLit(now: CFTimeInterval = CACurrentMediaTime()) -> Int?`: `nil` bei `reduceMotion`, sonst `Int(now / 0.16) % 4 + 1` (Schritt 160 ms, passt zum 80-ms-Takt des Baums).
  - `ColorTheme.shell: NSColor` (neuer Init-Parameter `shell: UInt32? = nil`, Default `ansi[4]`), `Theme.shell` liest `current.shell`. Instrument Dark `0xa3a19a`, Instrument Light `0x5c5a54`.

- [ ] **Step 1: Tests schreiben**

```swift
func testMeterLightsInSteps() {
    XCTAssertEqual(Feedback.meterLit(now: 0.00), 1)
    XCTAssertEqual(Feedback.meterLit(now: 0.17), 2)
    XCTAssertEqual(Feedback.meterLit(now: 0.49), 4)
    XCTAssertEqual(Feedback.meterLit(now: 0.65), 1)
}

func testMeterStillWithReduceMotion() {
    let saved = Feedback.reduceMotion; defer { Feedback.reduceMotion = saved }
    Feedback.reduceMotion = true
    XCTAssertNil(Feedback.meterLit(now: 0.17))
}

func testMeterDrawsLitAndDimBars() {
    // 12×10, lit = 2: Mitte von Strich 1 und 2 deckend, von Strich 3 und 4 ca. 25 %
    let rep = render(width: 12, height: 10) { Instrument.meter(CGRect(x: 0, y: 0, width: 12, height: 10), lit: 2, color: .white) }
    XCTAssertEqual(alpha(rep, x: 1, y: 5), 1, accuracy: 0.05)
    XCTAssertEqual(alpha(rep, x: 10, y: 5), 0.25, accuracy: 0.08)
}

// ContrastTests
func testShellDefaultsToBlueExceptInstrument() {
    for t in ColorTheme.all where !t.id.hasPrefix("instrument") { XCTAssertEqual(t.shell, t.ansi[4], t.id) }
    XCTAssertEqual(ColorTheme.named("instrument-dark").shell.hexString, "#a3a19a")
    XCTAssertEqual(ColorTheme.named("instrument-light").shell.hexString, "#5c5a54")
}
```

- [ ] **Step 2:** `$TEST InstrumentTests`, `$TEST ContrastTests` · Expected: FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** beide Klassen · Expected: PASS.
- [ ] **Step 5: Commit** `Strich-Anzeige und eigene Shell-Farbe im Instrument-Schema`

---

### Task 3: „Getönte Gruppen“ entfernen

**Files:**
- Delete: `app/Kadrell/Sidebar/TintedSidebarRenderer.swift`
- Modify: `app/Kadrell/Sidebar/SidebarRenderer.swift` (`SidebarStyle`), `app/Kadrell/App/Settings.swift:54` (Default `.classic`), `app/Kadrell/Localization/Localizable.xcstrings` (Eintrag „Getönte Gruppen“ entfernen)
- Test: `app/KadrellTests/SettingsModelTests.swift`

**Interfaces:**
- Produces: `SidebarStyle` mit den Fällen `classic`, `compact`. `Settings.sidebarStyle` Default `.classic`. Ein gespeichertes `"tinted"` ergibt über `enumValue` den Default.

- [ ] **Step 1: Tests schreiben**

```swift
func testStoredTintedFallsBackToClassic() {
    let key = "sidebar.style", saved = Profile.defaults.object(forKey: key)
    defer { Profile.defaults.set(saved, forKey: key) }   // nutzt das Temp-Profil (TEST_RUNNER_KADRELL_PROFILE=tmp)
    Profile.defaults.set("tinted", forKey: key)
    XCTAssertEqual(Settings.sidebarStyle, .classic)
    XCTAssertEqual(SidebarStyle.allCases, [.classic, .compact])
}
```

- [ ] **Step 2:** `$TEST SettingsModelTests` · Expected: FAIL (Fall `tinted` existiert noch).
- [ ] **Step 3:** Implementieren, `xcodegen generate` nimmt die gelöschte Datei mit.
- [ ] **Step 4:** `$TEST SettingsModelTests`, `$TEST LocalizationTests` · Expected: PASS.
- [ ] **Step 5: Commit** `Baum-Design „Getönte Gruppen“ entfernt, Klassisch ist Standard`

---

### Task 4: Gruppen-Labels, Auswahl-Umriss, Hover, Fuge im Kompakt-Design

**Files:**
- Modify: `app/Kadrell/Sidebar/SidebarRenderer.swift` (Extension mit den gemeinsamen Bausteinen)
- Modify: `app/Kadrell/Sidebar/ClassicSidebarRenderer.swift`, `app/Kadrell/Sidebar/CompactSidebarRenderer.swift`
- Test: neue `app/KadrellTests/SidebarRendererTests.swift`

**Interfaces:**
- Consumes: `Instrument.hSeam`, `Theme.bg` (Modulfarbe), `Theme.accent`, `Theme.fg`.
- Produces in der `SidebarRenderer`-Extension:
  - `static func groupLabel(_ g: SidebarGroupItem) -> NSAttributedString`: `"\(name.uppercased()) · \(g.dots.count)"`, `Theme.attrs(10.5, Theme.sub, bold: true)` plus `.kern: 0.8`.
  - `func rowShape(_ row: CGRect) -> CGRect`: `row.insetBy(dx: 6, dy: 1)`, Radius 6.
  - Gruppenkopf: Chevron bleibt bei x = 6, Farbakzent 6 × 6 pt, Radius 1,5, in `g.color` bei x = 26 mittig, Label ab x = 37. Rechts Herz und Warte-Marke wie bisher, `drawCount` entfällt (Zahl steht im Label). Klassisch behält die Pfadzeile darunter, ohne „▪ “-Präfix.
  - Session-Hintergrund statt `drawSessionBackground`: Hover → `rowShape` in `Theme.bg`. Ausgewählt (Mehrfachauswahl) → `rowShape` in `Theme.bg` plus Balken links 3 pt in Gruppenfarbe (wie bisher bei x = 0). Fokussiert → `rowShape` in `Theme.bg` plus 1,5-pt-Umriss innen in `Theme.fg`. Fokussiert mit Tastatur (`keyFocus`) → zusätzlich 2-pt-Ring in `Theme.accent` um `rowShape.insetBy(dx: -2.5, dy: -0.5)` (liegt außerhalb des Umrisses, innerhalb der Zeile, Radius 8).
  - Gruppen-Hover: `rowShape` in `Theme.bg`. Ausgewählte Gruppe: Balken links wie bisher.
  - Kompakt: Trennlinie zwischen Gruppen (`Theme.line`) wird `Instrument.hSeam` an der Oberkante.

- [ ] **Step 1: Tests schreiben** (Renderer direkt in eine Bitmap zeichnen, `Theme.current`/`Theme.scale` gepinnt)

```swift
func testGroupLabelIsUppercaseWithCount() {
    let label = ClassicSidebarRenderer.groupLabel(item(name: "kadrell", sessions: 2))
    XCTAssertEqual(label.string, "KADRELL · 2")
    XCTAssertEqual(label.attribute(.kern, at: 0, effectiveRange: nil) as? CGFloat, 0.8)
}

func testFocusedRowHasOutline() {
    // Zeile 200×24, fokussiert: Pixel auf dem Umriss (x = 6,5, y = 12) ≈ fg, Pixel innen (x = 100, y = 12) ≈ bg
}

func testKeyFocusRingOutsideOutline() {
    // für alle 12 Schemata: Pixel bei x = 4 (Ring) Kontrast zu panel ≥ 3:1, Pixel bei x = 6,5 (Umriss) bleibt fg
}

func testLongGroupLabelTruncatesBeforeBadges() {
    // Gruppe mit 80 Zeichen Namen, waitingCount 2, Favorit: rechte Kante des gezeichneten Labels < linke Kante der Warte-Marke
}

func testCompactGroupSeparatorIsSeam() {
    // zweite Gruppe im Kompakt-Design: Zeile minY dunkler als panel, minY + 1 heller als minY
}
```

Die Platzhalter-Tests mit Kommentar vervollständigt der Umsetzer mit echten Pixel-Assertions nach dem Muster in `InstrumentTests`. `testLongGroupLabelTruncatesBeforeBadges` braucht die gezeichneten Rechtecke: dafür gibt `drawGroupName` die belegte Fläche zurück (`@discardableResult -> CGRect`).

- [ ] **Step 2:** `$TEST SidebarRendererTests` · Expected: FAIL.
- [ ] **Step 3:** Implementieren. Bestehende Bausteine wiederverwenden, keine zweite Kopie von `drawSessionContent`.
- [ ] **Step 4:** `$TEST SidebarRendererTests`, `$TEST AccessibilityTests`, `$TEST SidebarFlatTests` · Expected: PASS. lizard leer.
- [ ] **Step 5: Commit** `Baum: Gruppen-Labels mit Farbakzent, Auswahl als Umriss, Fuge zwischen Gruppen`

---

### Task 5: Strich-Anzeige im Baum

**Files:**
- Modify: `app/Kadrell/Sidebar/SidebarRenderer.swift` (`SidebarSessionItem`, `drawSessionContent`, neues `statusRect`)
- Modify: `app/Kadrell/Sidebar/SidebarView.swift` (`drawSession`, `dotColor`, Puls-Schleife)
- Test: `app/KadrellTests/SidebarRendererTests.swift`

**Interfaces:**
- Consumes: `Instrument.meter`, `Feedback.meterLit`, `Theme.statusColor`.
- Produces:
  - `SidebarSessionItem.meter: (color: NSColor, lit: Int?)?`: gesetzt, wenn angehängt und (`status == .running` oder `hasRunningShell`). Farbe = `Theme.statusColor(…, shellRunning:)` ohne Puls-Alpha.
  - `func meterRect(_ row: CGRect) -> CGRect { CGRect(x: 26, y: headRect(row).midY - 5, width: 12, height: 10) }`, `func statusRect(_ row: CGRect) -> CGRect { dotRect(row).union(meterRect(row)) }`.
  - `drawSessionContent`: mit `meter` zeichnet es `Instrument.meter(meterRect, …)` statt `Icons.statusDot`, sonst den Punkt wie bisher (inkl. Aufblitzen bei Wechsel auf wartet/fertig).
  - Puls-Schleife in `SidebarView` invalidiert `renderer.statusRect` statt `dotRect`.

- [ ] **Step 1: Tests schreiben**

```swift
func testWorkingSessionShowsMeterNotDot() {
    // running + attached: in meterRect sind bei lit = nil vier getrennte deckende Striche (Lücken bei x ≈ 29, 32,5, 36 durchsichtig bzw. Zeilenhintergrund)
}

func testWaitingSessionKeepsDot() {
    // waiting: Pixel in der Mitte von dotRect deckend, Pixel bei x = 37 (rechts außerhalb des Punkts, innerhalb meterRect) = Hintergrund
}

func testShellRunningUsesShellColor() {
    // idle + hasRunningShell, Instrument Dark: Strichfarbe ≈ #a3a19a
}
```

- [ ] **Step 2:** `$TEST SidebarRendererTests` · Expected: FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `$TEST SidebarRendererTests`, `$TEST AccessibilityTests` (VoiceOver-Text der Zeile unverändert) · Expected: PASS.
- [ ] **Step 5: Commit** `Baum: Strich-Anzeige statt pulsierendem Punkt für arbeitende Sessions`

---

### Task 6: Sichtprüfung, Changelog, Version

**Files:**
- Temporär: Render-Test (danach löschen, Bilder bleiben)
- Modify: `CHANGELOG.md`, Version per `python3 tools/bump-version.py minor`

- [ ] **Step 1: Render.** Temporärer Test rendert das Fensterinnere (Baum + Split + Arbeitsfläche mit zwei leeren Kacheln, ohne laufende Terminals) bzw. Baum und Ecke einzeln in Instrument Dark, Instrument Light, Mocha, GitHub Light, bei 100 % und 130 %, mit Gruppen (kurz und sehr lang), fokussierter Session, Mehrfachauswahl, Hover, wartender und arbeitender Session, Kompakt und Klassisch, einmal mit Tastaturfokus. Bilder nach `~/.claude/screenshots/kadrell/instrument-stufe-2/`. Ansehen. Fällt etwas schmutzig oder unsichtbar aus: nachjustieren, betroffene Tests erneut laufen lassen.
- [ ] **Step 2: Laufende App** mit `--profile tmp` starten, ⌘B, ⌘1, Fenster schmal ziehen; nur die eigene Instanz per pid beenden.
- [ ] **Step 3:** Gesamtsuite, Clean Build, lizard.
- [ ] **Step 4: Changelog** unter `## Unreleased` oben in `CHANGELOG.md`:

```
- Änderung: Der Baum ist neu gestaltet: Gruppen stehen als ruhige Überschriften mit kleinem Farbpunkt da, die gezeigte Session hat einen hellen Rahmen, arbeitende Sessions zeigen eine Strich-Anzeige statt eines pulsierenden Punkts.
- Änderung: Baum, Arbeitsfläche und Seitenleisten sind durch feine Fugen getrennt, die Arbeitsfläche hat oben links eine runde Ecke.
- Entfernt: Das Baum-Design „Getönte Gruppen“. Wer es benutzt hat, sieht jetzt „Klassisch“.
```

Dann `python3 tools/bump-version.py minor`.
- [ ] **Step 5: Commit** (Branch, kein Merge) `Version <neu>: Instrument-Baum und Fugen zwischen den Bereichen`
