# Instrument Stufe 4: Dialoge und Extensions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Alle Dialoge, Overlays und Panels (⌘N, ⌘P, ⌘,, F1, F3, F4, Rückfragen, Gruppe bearbeiten, Umbenennen) und die rechte Sidebar bekommen die Instrument-Sprache: erhabene Fläche mit Fuge als Rand, Radius 14 und kurzem festem Schatten, Fugen statt Linien, gesperrte Versalien-Titel, Tasten statt flacher Knöpfe, eingelassene Felder, Auswahl als Umriss.

**Architecture:** Alles Zeichnen bleibt im AppKit-Kit `Theme+Instrument.swift` (neue Funktionen für Fläche, Rand, Gruppe, Feld, Zeilen-Auswahl). Eine einzige NSView `InstrumentSurfaceView` zeichnet jedes Bauteil, AppKit (Palette, rechte Sidebar) setzt sie direkt ein, SwiftUI über den Wrapper `InstrumentSurface` und wenige Modifier in `Instrument+SwiftUI.swift`. Die gemeinsame Hülle aller SwiftUI-Dialoge (`OverlayScroll` in `OverlayPanel.swift`) legt die erhabene Fläche einmal an, die Dialoge malen keinen eigenen Grund mehr.

**Tech Stack:** Swift 6, AppKit, SwiftUI (`NSViewRepresentable`, `ButtonStyle`), XCTest, String Catalog.

**Spec:** `docs/superpowers/specs/2026-10-22-instrument-fugen-design.md` (Ziel 1 für Dialoge/Palette/Extensions, Ziel 9, Stufe 4, Entscheidung „Akzentfarbe“). Stufen 1 und 2 sind auf master (1.71.0), Stufe 3 (Kacheln, Plan `2026-10-22-instrument-stufe-3.md`) läuft vorher auf demselben Branch.

**Branch:** Worktree `instrument-stufe-3-4`, nach den Commits von Stufe 3. Commits pro Task, Changelog und Version erst in Task 11 (ein `minor`-Sprung für Stufe 3 und 4 zusammen).

**Nicht anfassen (gehört Stufe 3):** `Workspace/CellView.swift`, `Workspace/WorkspaceView.swift`, `Workspace/Tiling.swift`, `Workspace/ModuleCornerView.swift`. `Sidebar/SidebarRenderer.swift` nur im bedingten Schritt 6 von Task 10.

## Global Constraints

- Clean Build ohne eigene Warnungen. Einzige erlaubte Meldung: `appintentsmetadataprocessor: Metadata extraction skipped`.
- `cd app && uv tool run lizard Kadrell -w` liefert nichts (CCN ≤ 15). Verzweigte Funktionen in kleine benannte Funktionen zerlegen.
- Alle Tests grün. Tests im Temp-Profil: `TEST="cd app && /opt/homebrew/bin/xcodegen generate && TEST_RUNNER_KADRELL_PROFILE=tmp xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug -derivedDataPath build -skipPackagePluginValidation -skipMacroValidation test -only-testing:KadrellTests/<Klasse>"`. Gesamtsuite gleiches Kommando ohne `-only-testing`, vor jedem Commit grün.
- Jeder neue Nutzertext in AppKit als `String(localized: "…", bundle: Bundle.app)`, in SwiftUI `Text("…")`, Deutsch und Englisch im selben Arbeitsgang in `app/Kadrell/Localization/Localizable.xcstrings`. Der Plan braucht keine neuen Texte; entsteht doch einer, gilt diese Regel.
- Fugen genau 1 pt dunkel (`Instrument.seamDark`) + 1 pt hell (`Instrument.seamLight`) bei jeder UI-Größe 90 bis 130 %: innerhalb von `Theme.scaled` immer `Instrument.hairline`, nie feste Bruchteile. Radien, Tastendicke und Schattentiefe sind logische Punkte und skalieren mit.
- Kontraste: Text ≥ 4,5:1 auf jeder neuen Fläche (`raised`, `well`, `bg`), Tastenbeschriftung ≥ 4,5:1 auf ihrer Kappe. `ContrastTests` erweitern.
- Alle zwölf Schemata, dunkel und hell. Keine Änderung an den gespeicherten Farbwerten der Schemata, nur abgeleitete Rollen.
- Keine neuen Schriften. Layouts, Inhalte, Kürzel, Fokusreihenfolge bleiben; nur das Aussehen ändert sich.
- Keine zwei Bauweisen nebeneinander: der alte flache Look verschwindet aus allen Dialogen.
- Tests, die `Theme.current`, `Theme.scale` oder `Settings.language` ändern, stellen sie per `defer` zurück.
- Eigenes Kadrell nie anfassen; laufende App nur mit `open -n app/build/Build/Products/Debug/Kadrell.app --args --profile tmp`, nur die eigene Instanz per pid beenden.
- Keine Em-Dashes. Deutsche Kommentare mit echten Umlauten. Git im Worktree als `/usr/bin/git`, Commits nur per Pfad (`/usr/bin/git add <dateien>`), keine Co-Author-Zeilen.

## Entscheidungen des Planers (Michael prüft sie beim Lesen)

- **Erhabene Fläche `raised`:** die hellste der drei Flächen `panel`, `bg`, `surface` des Schemas. Instrument Dark `#252523`, Instrument Light `#fbfaf7` (beides `raised` aus Monsters), GitHub Light `#ffffff`, Mocha `#1e1e2e`.
- **Eingelassenes Feld `well`:** dunkle Schemata `panel` zu 72 % Richtung Schwarz (Instrument Dark ergibt genau Monsters `#0f0f0e`), helle Schemata `bg` (Instrument Light `#f2f1ec`). Monsters' helles `#d8d5ce` drückt Platzhaltertext auf 4,0:1, deshalb nicht übernommen. In hellen Schemata erkennt man das Feld an der Fuge, nicht am Ton.
- **Text-Rollen `accentText` und `errorText`:** wie `waitingText` abgeleitet, Akzent bzw. Fehlerfarbe Richtung Weiß (dunkel) oder Schwarz (hell) geschoben, bis 4,5:1 gegen `panel`, `surface`, `raised`, `well`, `bg` erreicht ist. Instrument und Mocha bleiben unverändert, Latte, Nord, Solarized und andere werden lesbarer. Füllungen (Primärtaste, Fokusring) behalten `accent`/`error`.
- **Schatten:** 3 pt nach unten, ohne Weichzeichnung, Schwarz α 0,5 (dunkel) bzw. α 0,16 (hell). Der weiche Systemschatten der Overlay-Fenster entfällt (`hasShadow = false`).
- **Radien (logisch):** Dialog 14, Gruppe (Einstellungstabellen, Formulare) 9, Zeilen-Auswahl 6 wie im Baum, Taste und Feld 5 wie in der Leiste.
- **Werte sitzen in Feldern, Aktionen auf Tasten:** Textfelder, Dropdown-Werte der Einstellungen, Kürzel-Felder und der Log sind `well`. Knöpfe sind Tasten: Primär in `accent`, destruktiv in `error`, alles andere neutral (Kappe `Instrument.keyCap`). Ein Kürzel in Aufnahme ist ein Feld mit Akzentring statt einer Akzent-Füllung (Akzent-Füllung bleibt der Primärtaste vorbehalten).
- **Fokus:** ein Feld mit Fokus trägt 2 pt Akzentring innen am Rand (kein Abschneiden, kein Verrutschen). Tasten und Dropdowns behalten `kbdFocusRing` (außen), dessen Radius folgt den Tasten.
- **Auswahl in Dialog-Listen (⌘N, ⌘P, F4):** Fläche in Modulfarbe plus 1,5-pt-Umriss in `fg`, wie die gezeigte Session im Baum, ohne Akzentring (die Tastatur steht im Suchfeld). Der farbige Balken links entfällt. Rechte Sidebar: Hover = Fläche, Cursor mit Tastatur (⌘3) = Fläche, Umriss und Akzentring wie der Baum.
- **Rechte Sidebar:** Tabs sind Tasten, der gewählte Tab bleibt eingesunken. Fuge unter der Tab-Reihe, Abschnittstitel in gesperrten Versalien.
- **Native Bedienelemente bleiben nativ:** `Slider`, `Toggle` (Schalter, Checkbox), `ProgressView`, Kontextmenüs (`NSMenu`), `NSAlert`, `NSOpenPanel`, Tooltips. Sie folgen hell/dunkel und tragen schon den Akzent als Tönung.
- **Leerzustand-Knöpfe der Arbeitsfläche** (`WorkspaceView.drawEmptyButtons`) gehören zu Stufe 3, nicht hierher.

## Review Focus

1. **Farbschema wechseln, während die Einstellungen offen sind** (Instrument Dark → GitHub Light): Fläche, Rand, Tasten und Felder zeichnen sofort in den neuen Farben, nichts bleibt dunkel stehen. Test in Task 4 (`testThemeChangeRedrawsOverlaySurface`).
2. **UI-Größe 130 % und ein Dialog höher als das Fenster (Einstellungen auf kleinem Bildschirm):** Rand bleibt 1 pt + 1 pt scharf, der Dialog samt Schatten ragt nicht über 24 pt Abstand zum Fensterrand hinaus, die Fußleiste bleibt sichtbar. Tests in Task 2 (`testRaisedRimStaysHairlineAtScale`) und Task 4 (`testScrollLimitLeavesRoomForShadow`).
3. **Destruktive Rückfrage (⌘W) in jedem Schema:** rote Taste, Beschriftung „Entfernen  ⌘⏎“ lesbar (≥ 4,5:1), blankes ⏎ bestätigt weiterhin nicht. Test in Task 2 (`testKeyRoleLabelsReadable`), Verhalten unverändert durch `BackdropTests`/`SheetPresenter`.
4. **Helle Schemata (GitHub Light, Latte, Solarized Light), wo `raised`, `well` und `bg` gleich hell sind:** Felder und Gruppen bleiben über die Fuge erkennbar, Texte lesbar. Tests in Task 1 (`testDialogTextMeetsAA`) und Task 2 (`testRimVisibleOnRaisedInEveryTheme`).
5. **Nur Tastatur (Tab durch eine Rückfrage, ⌘3 in die rechte Sidebar):** Fokusring an Tasten am Dialogrand wird nicht abgeschnitten (die Hülle schneidet nichts ab), der Cursor der rechten Sidebar trägt den Akzentring. Tests in Task 3 (`testRaisedPanelDoesNotClipContent`) und Task 10 (`testKeyboardCursorHasAccentRing`).

---

### Task 1: Farbrollen für Fläche, Feld und lesbaren Akzent

**Files:**
- Modify: `app/Kadrell/Theme.swift` (`ColorTheme`, `Theme`, `NSColor`)
- Modify: `app/Kadrell/UI/Theme+SwiftUI.swift`
- Test: `app/KadrellTests/ContrastTests.swift`

**Interfaces:**
- Produces:
  - `ColorTheme.raised, well, accentText, errorText: NSColor` (in `init` berechnet, kein neuer Init-Parameter).
  - `Theme.raised`, `Theme.well`, `Theme.accentText`, `Theme.errorText` (lesen `current`).
  - `NSColor.relativeLuminance: CGFloat` (aus `contrastRatio` herausgezogen, das es danach benutzt).
  - SwiftUI: `Theme.raisedColor`, `Theme.wellColor`, `Theme.accentTextColor`, `Theme.errorTextColor` (alle `static var`). `Theme.errorColor` wird von `static let` zu `static var` (blieb sonst beim Schemawechsel stehen).
- Werte: `raised = [panel, bg, surface].max(by: relativeLuminance)`. `well = dark ? panel.mixed(0.72, into: NSColor(hex: 0)) : bg`. `accentText = accent.ensuringContrast(4.5, against: [panel, surface, raised, well, bg], toward: extreme)`, `errorText` genauso mit `error`; `extreme` ist das vorhandene Weiß/Schwarz aus `init`.

- [ ] **Step 1: Tests schreiben**

```swift
func testRaisedAndWellRoles() {
    XCTAssertEqual(ColorTheme.named("instrument-dark").raised.hexString, "#252523")
    XCTAssertEqual(ColorTheme.named("instrument-light").raised.hexString, "#fbfaf7")
    XCTAssertEqual(ColorTheme.named("github-light").raised.hexString, "#ffffff")
    XCTAssertEqual(ColorTheme.named("mocha").raised.hexString, "#1e1e2e")
    XCTAssertEqual(ColorTheme.named("instrument-dark").well.hexString, "#0f0f0e")
    XCTAssertEqual(ColorTheme.named("instrument-light").well.hexString, "#f2f1ec")
    XCTAssertEqual(ColorTheme.named("github-light").well.hexString, "#ffffff")
}

/// Alles, was in Dialogen als Text steht, auf jeder Fläche der Dialoge mindestens 4,5:1.
func testDialogTextMeetsAA() {
    for t in ColorTheme.all {
        for bg in [t.raised, t.well, t.bg] {
            for (name, c) in [("fg", t.fg), ("sub", t.sub), ("muted", t.muted), ("accentText", t.accentText), ("errorText", t.errorText)] {
                XCTAssertGreaterThanOrEqual(c.contrastRatio(with: bg), 4.5, "\(t.id): \(name) gegen \(bg.hexString)")
            }
        }
    }
}

func testAccentTextKeepsCompliantValues() {
    XCTAssertEqual(ColorTheme.named("instrument-dark").accentText.hexString, "#ff8a4c")
    XCTAssertEqual(ColorTheme.named("instrument-light").accentText.hexString, "#a83500")
    XCTAssertEqual(ColorTheme.named("mocha").accentText.hexString, ColorTheme.named("mocha").accent.hexString)
    XCTAssertNotEqual(ColorTheme.named("solarized-light").accentText.hexString, ColorTheme.named("solarized-light").accent.hexString)
}
```

- [ ] **Step 2:** `$TEST ContrastTests` · Expected: FAIL (`raised` unbekannt).
- [ ] **Step 3:** Implementieren wie unter Interfaces. `accessibleText` bleibt für `sub`/`muted`/`waitingText` unverändert, die neuen Rollen gehen direkt Richtung `extreme` (Richtung `fg` kippt der Farbton, aus Rot würde Petrol).
- [ ] **Step 4:** `$TEST ContrastTests` · Expected: PASS.
- [ ] **Step 5: Commit** `Farbrollen für erhabene Fläche, eingelassene Felder und lesbaren Akzent`

---

### Task 2: Kit: Fläche, Rand, Gruppe, Feld, Zeilen-Auswahl, Tastenrollen, Bauteil-View

**Files:**
- Modify: `app/Kadrell/Theme+Instrument.swift`
- Create: `app/Kadrell/UI/InstrumentSurfaceView.swift`
- Test: `app/KadrellTests/InstrumentTests.swift`

**Interfaces:**
- Consumes: Task 1 (`Theme.raised`, `Theme.well`), vorhandene `seamDark`, `seamLight`, `keyCap`, `keyEdge`, `hairline`, `snapped`, `key`, `hSeam`.
- Produces in `enum Instrument` (Zeichnen `@MainActor`, Koordinaten logisch innerhalb von `Theme.scaled`, gespiegelte Views):
  - Maße, alle `static let … : CGFloat`: `dialogRadius = 14`, `groupRadius = 9`, `rowRadius = 6`, `keyRadius = 5` und `keyDepth = 2` (bisher `private`, jetzt intern), `shadowDepth = 3`, `labelKern = 0.8`.
  - `static var dropShadow: NSColor`: Schwarz α 0,5 (dunkel) / 0,16 (hell).
  - `static func rim(_ r: CGRect, radius: CGFloat)`: äußerer Ring `hairline` breit in `seamDark`, direkt innen ein Ring `hairline` in `seamLight` (Ring als Pfad mit `.evenOdd` wie in `ModuleCornerView.ring`).
  - `static func raisedPanel(_ body: CGRect, radius: CGFloat = dialogRadius, depth: CGFloat = shadowDepth)`: Schatten = `body` um `depth` nach unten versetzt, gleicher Radius, `dropShadow`; darüber `body` in `Theme.raised`; darüber `rim(body, radius:)`.
  - `static func group(_ r: CGRect)`: `Theme.bg` mit `groupRadius`, dann `rim`.
  - `static func well(_ r: CGRect, focused: Bool)`: `Theme.well` mit `keyRadius`; `focused` → 2 pt Ring innen in `Theme.accent`, sonst `rim(r, radius: keyRadius)`.
  - Zeilen-Auswahl mit den Werten des Baums: `static func rowShape(_ row: CGRect) -> CGRect` (`insetBy(dx: 6, dy: 1)`), `static func fillRow(_ shape: CGRect)` (`Theme.bg`, `rowRadius`), `static func outline(_ shape: CGRect)` (1,5 pt `Theme.fg`, Pfad um 0,75 eingerückt, Radius 5,25), `static func keyRing(_ outer: CGRect)` (2 pt Ring `Theme.accent`, außen Radius 8, innen 6), `static func rowHighlight(_ row: CGRect, outline: Bool, ring: Bool)`: ohne Ring `fillRow(rowShape(row))`; mit Ring `outer = rowShape(row).insetBy(dx: -2.5, dy: -0.5)`, `keyRing(outer)`, Fläche `outer.insetBy(dx: 2.5, dy: 2.5)`; `outline` zeichnet danach `outline(Fläche)`.
- Produces in `Theme+Instrument.swift`: `enum KeyRole: CaseIterable, Equatable { case primary, neutral, destructive }` mit `var fill: NSColor?` (`Theme.accent` / `nil` / `Theme.error`) und `var label: NSColor` (`Theme.pillText(on: fill)` bzw. `Theme.fg` für neutral).
- Produces in `InstrumentSurfaceView.swift`: `final class InstrumentSurfaceView: NSView` mit `enum Part: Equatable { case raised, group, well(focused: Bool), row(outline: Bool, ring: Bool), key(pressed: Bool, role: KeyRole), hSeam }`, `init(part: Part, frame: NSRect = .zero)`, `var part: Part` (`didSet` → `needsDisplay`), `isFlipped == true`, `isOpaque == false`, `hitTest` → `nil`. `draw` ruft `Theme.scaled(bounds) { r in … }`: `.raised` → `raisedPanel(CGRect(x: 0, y: 0, width: r.width, height: r.height - shadowDepth))`, `.group` → `group(r)`, `.well` → `well(r, focused:)`, `.row` → `rowHighlight(r, …)`, `.key` → `key(r, pressed:, fill: role.fill)`, `.hSeam` → `hSeam(x: 0, y: 0, width: r.width)`.

- [ ] **Step 1: Tests schreiben** (Hilfen `render`, `luma`, `pixel`, `distance`, `panelCanvas` aus `PixelTesting.swift`)

```swift
func testRaisedPanelShadowBodyRim() {
    let t = ColorTheme.named("instrument-dark")
    let rep = render(theme: t, width: 60, height: 40, scale: 1) { Instrument.raisedPanel(CGRect(x: 0, y: 0, width: 60, height: 37)) }
    XCTAssertLessThan(distance(pixel(rep, x: 30, y: 20), t.raised), 0.02, "Fläche")
    XCTAssertLessThan(luma(rep, x: 30, y: 0), luma(t.raised), "dunkle Fugenlinie außen")
    XCTAssertGreaterThan(luma(rep, x: 30, y: 1), luma(rep, x: 30, y: 0), "helle Linie innen")
    XCTAssertGreaterThan(rep.colorAt(x: 30, y: 38)!.alphaComponent, 0.3, "Schatten unter der Fläche")
    XCTAssertLessThan(rep.colorAt(x: 0, y: 0)!.alphaComponent, 0.05, "Ecke rund")
}

func testRaisedRimStaysHairlineAtScale() {
    // 1,3: Bitmap 78 × 52, Körper logisch 60 × 37. Zeile 0 dunkel, Zeile 1 hell, Zeile 2 schon Fläche.
    let t = ColorTheme.named("instrument-dark")
    let rep = render(theme: t, width: 78, height: 52, scale: 1.3) { Instrument.raisedPanel(CGRect(x: 0, y: 0, width: 60, height: 37)) }
    XCTAssertLessThan(luma(rep, x: 39, y: 0), luma(rep, x: 39, y: 1))
    XCTAssertLessThan(distance(pixel(rep, x: 39, y: 2), t.raised), 0.02)
}

func testRimVisibleOnRaisedInEveryTheme() {
    for t in ColorTheme.all {
        let rep = render(theme: t, width: 40, height: 40, scale: 1) { Instrument.raisedPanel(CGRect(x: 0, y: 0, width: 40, height: 37)) }
        XCTAssertGreaterThan(abs(luma(rep, x: 20, y: 0) - luma(t.raised)), 0.02, t.id)
    }
}

func testFocusedWellShowsAccentRing() {
    let t = ColorTheme.named("instrument-light")
    let rep = render(theme: t, width: 60, height: 20, scale: 1) { Instrument.well(CGRect(x: 0, y: 0, width: 60, height: 20), focused: true) }
    XCTAssertLessThan(distance(pixel(rep, x: 30, y: 1), t.accent), 0.05)
    XCTAssertLessThan(distance(pixel(rep, x: 30, y: 10), t.well), 0.02)
}

func testRowHighlightMatchesTree() {
    let t = ColorTheme.named("instrument-dark")
    let rep = render(theme: t, width: 200, height: 24, scale: 1) {
        panelCanvas(200, 24); Instrument.rowHighlight(CGRect(x: 0, y: 0, width: 200, height: 24), outline: true, ring: true)
    }
    XCTAssertLessThan(distance(pixel(rep, x: 4, y: 12), t.accent), 0.1, "Ring außen")
    XCTAssertLessThan(distance(pixel(rep, x: 100, y: 12), t.bg), 0.02, "Fläche innen")
}

func testKeyRoleLabelsReadable() {
    for t in ColorTheme.all { withTheme(t) {
        XCTAssertEqual(KeyRole.primary.fill, Theme.accent); XCTAssertNil(KeyRole.neutral.fill); XCTAssertEqual(KeyRole.destructive.fill, Theme.error)
        for role in KeyRole.allCases {
            XCTAssertGreaterThanOrEqual(role.label.contrastRatio(with: role.fill ?? Instrument.keyCap), 4.5, "\(t.id) \(role)")
        }
    } }
}

func testSurfaceViewIgnoresClicks() {
    XCTAssertNil(InstrumentSurfaceView(part: .raised, frame: NSRect(x: 0, y: 0, width: 50, height: 50)).hitTest(NSPoint(x: 10, y: 10)))
}
```

- [ ] **Step 2:** `$TEST InstrumentTests` · Expected: FAIL (Funktionen unbekannt).
- [ ] **Step 3:** Implementieren. Bestehende Tests der Taste bleiben grün (nur Sichtbarkeit von `keyRadius`/`keyDepth` ändert sich).
- [ ] **Step 4:** `$TEST InstrumentTests`, `$TEST ContrastTests` · Expected: PASS. lizard leer.
- [ ] **Step 5: Commit** `Instrument-Kit: erhabene Fläche, Fugenrand, Felder, Zeilen-Auswahl, Tastenrollen`

---

### Task 3: SwiftUI-Gegenstücke

**Files:**
- Create: `app/Kadrell/UI/Instrument+SwiftUI.swift`
- Modify: `app/Kadrell/UI/Theme+SwiftUI.swift` (`KeyboardFocusRing`, `dialogFrame`, `dialogTitle`)
- Modify: `app/KadrellTests/PixelTesting.swift` (Helfer `snapshot`)
- Test: neue `app/KadrellTests/InstrumentSwiftUITests.swift`

**Interfaces:**
- Consumes: Task 2 (`InstrumentSurfaceView`, `KeyRole`, Maße), Task 1 (SwiftUI-Farben).
- Produces:
  - `struct InstrumentSurface: NSViewRepresentable` mit `let part: InstrumentSurfaceView.Part`, `var themeID = Theme.current.id`, `var scale = Theme.scale` (beide nur, damit SwiftUI bei Schema- oder Größenwechsel `updateNSView` ruft). `updateNSView` setzt `part` und `needsDisplay = true`. An jeder Verwendung `.accessibilityHidden(true)`.
  - `struct SeamLine: View`: `InstrumentSurface(part: .hSeam).frame(height: 2)`.
  - `extension View`:
    - `func raisedPanel() -> some View`: `padding(.bottom, Instrument.shadowDepth * Theme.scale)` und `background(InstrumentSurface(part: .raised))`. Kein `clipShape`: Fokusringe am Rand bleiben sichtbar.
    - `func instrumentGroup() -> some View`: `background(InstrumentSurface(part: .group))`.
    - `func instrumentWell(focused: Bool = false) -> some View`: `padding(.horizontal, 8).padding(.vertical, 3).background(InstrumentSurface(part: .well(focused: focused)))`.
    - `func instrumentRow(selected: Bool) -> some View`: bei `selected` `background(InstrumentSurface(part: .row(outline: true, ring: false)))`.
  - `struct InstrumentKeyStyle: ButtonStyle` mit `init(_ role: KeyRole = .neutral, compact: Bool = false)`. Label: `Theme.ui(12, bold: role != .neutral)` (kompakt `Theme.ui(11)`), Farbe `role.label`, deaktiviert `Theme.muted` (über eine kleine innere View mit `@Environment(\.isEnabled)`), Innenabstand 12/5 (kompakt 6/2), gedrückt `offset(y: Theme.scale)`, darunter `padding(.bottom, Instrument.keyDepth * Theme.scale)`, Hintergrund `InstrumentSurface(part: .key(pressed: configuration.isPressed, role: role))`.
  - `extension Text`: `func capsLabel(_ color: Color = Theme.mutedColor, bold: Bool = false) -> some View` (`Theme.ui(11, bold:)`, `kerning(Instrument.labelKern * Theme.scale)`), `func sectionTitle() -> some View` (= `capsLabel(Theme.accentTextColor, bold: true)`). `dialogTitle()` baut auf `capsLabel()` auf, Rahmen und Abstände wie bisher.
  - `KeyboardFocusRing`: Radius `(Instrument.keyRadius + 2) * Theme.scale`. `dialogFrame()` ohne `.background(Theme.panelColor)`.
  - Test-Helfer in `PixelTesting.swift`: `func snapshot<V: View>(_ view: V, theme: ColorTheme, scale: CGFloat = 1) -> (rep: NSBitmapImageRep, size: CGSize)`: setzt Schema und Maßstab (per `defer` zurück), `NSHostingView`, `fittingSize`, `cacheDisplay`. `func pixel(_ s: (rep: NSBitmapImageRep, size: CGSize), at p: CGPoint) -> NSColor` rechnet Punkte über `rep.pixelsWide / size.width` in Pixel um.

- [ ] **Step 1: Tests schreiben**

```swift
func testSeamLineDarkThenLight() {
    let t = ColorTheme.named("instrument-dark")
    let s = snapshot(ZStack { Color(nsColor: t.raised); SeamLine() }.frame(width: 40, height: 2), theme: t)
    XCTAssertLessThan(luma(pixel(s, at: CGPoint(x: 20, y: 0.25))), luma(t.raised))
    XCTAssertGreaterThan(luma(pixel(s, at: CGPoint(x: 20, y: 1.25))), luma(pixel(s, at: CGPoint(x: 20, y: 0.25))))
}

func testRaisedPanelAddsShadowDepth() {
    let s = snapshot(Color.clear.frame(width: 100, height: 50).raisedPanel(), theme: .named("instrument-dark"))
    XCTAssertEqual(s.size.height, 53, accuracy: 0.5)
}

func testRaisedPanelDoesNotClipContent() {
    // Ein Ring, der 2 pt über den Rand ragt (wie kbdFocusRing), bleibt sichtbar.
    let t = ColorTheme.named("instrument-light")
    let s = snapshot(Color.clear.frame(width: 100, height: 50)
        .overlay(Rectangle().stroke(Color(nsColor: t.accent), lineWidth: 2).padding(-2)).raisedPanel().padding(4), theme: t)
    XCTAssertLessThan(distance(pixel(s, at: CGPoint(x: 50, y: 3)), t.accent), 0.1)
}

func testPrimaryKeyShowsAccentCap() {
    let t = ColorTheme.named("instrument-light")
    let s = snapshot(Button("Starten") {}.buttonStyle(InstrumentKeyStyle(.primary)), theme: t)
    XCTAssertLessThan(distance(pixel(s, at: CGPoint(x: 3, y: s.size.height / 2)), t.accent), 0.05, "Kappe in accent")
    XCTAssertLessThan(distance(pixel(s, at: CGPoint(x: s.size.width / 2, y: s.size.height - 0.5)), Instrument.keyEdge), 0.05, "Dicke unten")
}

func testFocusedWellShowsAccentRing() {
    let t = ColorTheme.named("instrument-dark")
    let s = snapshot(Text("x").frame(width: 80).instrumentWell(focused: true), theme: t)
    XCTAssertLessThan(distance(pixel(s, at: CGPoint(x: 40, y: 0.5)), t.accent), 0.05)
}

func testDialogFrameHasNoOwnBackground() {
    let s = snapshot(Text("x").dialogFrame(), theme: .named("instrument-dark"))
    XCTAssertLessThan(s.rep.colorAt(x: s.rep.pixelsWide - 2, y: 2)!.alphaComponent, 0.05)
}
```

`Instrument.keyEdge` im Test unter gesetztem Schema auslesen (wie in `testKeyUsesFillAndEdge`).

- [ ] **Step 2:** `$TEST InstrumentSwiftUITests` · Expected: FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `$TEST InstrumentSwiftUITests`, `$TEST StatsViewRenderTests`, `$TEST ExtensionsViewTests` · Expected: PASS.
- [ ] **Step 5: Commit** `SwiftUI-Bauteile: Fuge, Fläche, Feld, Gruppe, Auswahl und Tasten`

---

### Task 4: Gemeinsame Dialog-Hülle (OverlayPanel, Fußleiste)

**Files:**
- Modify: `app/Kadrell/UI/OverlayPanel.swift` (`OverlayPanel`, `OverlayLimit`, `OverlayScroll`, `ChildPanel`)
- Modify: `app/Kadrell/UI/FolderInput.swift` (`DialogFootBar`)
- Test: neue `app/KadrellTests/DialogChromeTests.swift`

**Interfaces:**
- Consumes: Task 3 (`raisedPanel`, `SeamLine`, `InstrumentKeyStyle`).
- Produces:
  - `ChildPanel.init`: `hasShadow = false` (gilt für Overlay und Palette).
  - `OverlayPanel`: `isOpaque = false`, `backgroundColor = .clear`, kein Layer-Rand mehr. `let limit: OverlayLimit` (intern statt privat). `applyTheme()` setzt `appearance` und zählt `limit.themeGeneration += 1`.
  - `OverlayLimit.themeGeneration: Int = 0`.
  - `OverlayScroll.body`: der bisherige `VStack` bekommt `.raisedPanel()`; `body` liest `limit.themeGeneration` (z. B. `let _ = limit.themeGeneration`), damit die Fläche bei Schemawechsel neu entsteht. Scrollhöhe `min(height, limit.maxHeight - footHeight - Instrument.shadowDepth * Theme.scale)`.
  - `DialogFootBar`: `SeamLine()` statt `Divider`, Taste `InstrumentKeyStyle(.primary)` mit Text `"\(foot.button)  ⏎"`, `kbdFocusRing` bleibt, kein eigener Hintergrund.

- [ ] **Step 1: Tests schreiben**

```swift
func testOverlayPanelIsClearWithoutSystemShadow() {
    let p = OverlayPanel(rootView: Text("x"))
    XCTAssertFalse(p.isOpaque); XCTAssertEqual(p.backgroundColor, .clear); XCTAssertFalse(p.hasShadow)
    XCTAssertFalse(PaletteWindow().hasShadow)
}

func testThemeChangeRedrawsOverlaySurface() {
    let saved = Theme.current; defer { Theme.current = saved }
    let p = OverlayPanel(rootView: Text("x"))
    let before = p.limit.themeGeneration
    Theme.current = .named("github-light")
    p.applyTheme()
    XCTAssertEqual(p.limit.themeGeneration, before + 1)
}

func testScrollLimitLeavesRoomForShadow() {
    let limit = OverlayLimit(); limit.maxHeight = 300
    let host = NSHostingView(rootView: OverlayScroll(limit: limit, content: Color.clear.frame(width: 200, height: 1000)))
    host.layoutSubtreeIfNeeded()
    RunLoop.main.run(until: Date() + 0.05)   // onGeometryChange setzt die Höhe erst im zweiten Durchlauf
    host.layoutSubtreeIfNeeded()
    XCTAssertLessThanOrEqual(host.fittingSize.height, 300.5)
}

func testFootBarKeyIsPrimary() {
    // Dialog mit Fußleiste in Instrument Light: im unteren Streifen gibt es Pixel in accent (Primärtaste).
    let t = ColorTheme.named("instrument-light")
    let s = snapshot(OverlayScroll(limit: OverlayLimit(), content: VStack { Text("x"); DialogFoot(hint: "h", button: "OK") {} }.frame(width: 400)), theme: t)
    let hits = stride(from: 200.0, to: 395, by: 2).contains { x in distance(pixel(s, at: CGPoint(x: x, y: s.size.height - 22)), t.accent) < 0.05 }
    XCTAssertTrue(hits)
}
```

- [ ] **Step 2:** `$TEST DialogChromeTests` · Expected: FAIL.
- [ ] **Step 3:** Implementieren. `SheetPresenter` bleibt unverändert (ruft schon `applyTheme`).
- [ ] **Step 4:** `$TEST DialogChromeTests`, `$TEST BackdropTests`, `$TEST InstrumentSwiftUITests` · Expected: PASS.
- [ ] **Step 5: Commit** `Dialoge: erhabene Fläche mit Fugenrand, Radius 14 und festem Schatten`

---

### Task 5: Palette (⌘P)

**Files:**
- Modify: `app/Kadrell/UI/PaletteWindow.swift` (`init`, `PaletteRow.drawRow`)
- Test: `app/KadrellTests/DialogChromeTests.swift`

**Interfaces:**
- Consumes: Task 2 (`InstrumentSurfaceView`, `Instrument.rowHighlight`, `Instrument.shadowDepth`), Task 4 (`ChildPanel.hasShadow == false`).
- Produces:
  - Fensterhöhe `H = (420 * s).rounded() + d` mit `d = (Instrument.shadowDepth * s).rounded()`. Alle bisherigen y-Werte rücken um `d` nach oben (der Schatten liegt unten, `root` ist nicht gespiegelt).
  - `root`: schlichte `NSView` ohne Layer-Farbe und -Rand. Erste Unteransicht `InstrumentSurfaceView(part: .raised)` über die ganze Fläche (`autoresizingMask = [.width, .height]`).
  - Suchfeld auf einem Feld: `InstrumentSurfaceView(part: .well(focused: false))` mit `field.frame.insetBy(dx: -6 * s, dy: -4 * s)`, vor dem Feld eingefügt. Kein Fokusring, das Feld hat beim Öffnen immer die Tastatur.
  - Beide Trenner `sep`, `sep2`: `InstrumentSurfaceView(part: .hSeam)`, Höhe 2, gleiche Lage.
  - `PaletteRow.drawRow`: ausgewählt → `Instrument.rowHighlight(b, outline: true, ring: false)`; Balken links und `surface`-Fläche entfallen.

- [ ] **Step 1: Tests schreiben**

```swift
func testPaletteChromeUsesInstrumentParts() throws {
    let p = PaletteWindow()
    let parts = try XCTUnwrap(p.contentView).subviews.compactMap { ($0 as? InstrumentSurfaceView)?.part }
    XCTAssertEqual(parts.first, .raised)
    XCTAssertEqual(parts.filter { $0 == .hSeam }.count, 2)
    XCTAssertTrue(parts.contains(.well(focused: false)))
}

func testPaletteSelectionIsOutlineNotBar() {
    let t = ColorTheme.named("instrument-dark")
    // PaletteRow 400 × 44, ausgewählt, über raised gezeichnet (render-Helfer, Zeichnen über row.draw).
    // x = 1: kein Akzentbalken mehr, Pixel ≈ raised. x = 7: Umriss ≈ fg (Abstand < 0,15). x = 380, y = 40: Fläche ≈ bg.
}
```

Den zweiten Test mit echten Assertions nach dem Muster in `InstrumentTests` ausschreiben: `PaletteRow` mit `item` (Label „Alpha“, ohne Status) und `selected = true`, in `render(theme:width:height:scale:)` erst `t.raised` füllen, dann `row.draw(row.bounds)`.

- [ ] **Step 2:** `$TEST DialogChromeTests` · Expected: FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `$TEST DialogChromeTests`, `$TEST BackdropTests`, `$TEST AccessibilityTests` · Expected: PASS.
- [ ] **Step 5: Commit** `Palette: erhabene Fläche, eingelassenes Suchfeld, Fugen, Auswahl als Umriss`

---

### Task 6: Eingabe-Dialoge und Rückfragen (⌘N, Gruppe bearbeiten, Umbenennen, Bestätigen)

**Files:**
- Modify: `app/Kadrell/UI/NewSessionSheet.swift` (`NewSessionView`, `PathField`, `PathFieldRep`)
- Modify: `app/Kadrell/UI/EditGroupSheet.swift`
- Modify: `app/Kadrell/UI/ConfirmView.swift`
- Test: neue `app/KadrellTests/DialogLookTests.swift`, `app/KadrellTests/TextFieldCenteringTests.swift`

**Interfaces:**
- Consumes: Task 3 (`SeamLine`, `instrumentWell`, `instrumentRow`, `InstrumentKeyStyle`, `dialogTitle`, Farben).
- Produces:
  - `PathFieldRep`: neue letzte Eigenschaft `var onFocus: (Bool) -> Void = { _ in }` (hinter `onMove`, damit der memberwise-Init passt), gerufen aus `controlTextDidBeginEditing`/`controlTextDidEndEditing`. Layer-Rahmen, `wantsLayer` und `cornerRadius` entfallen.
  - `PathField`: `@State private var focused = false`, `body` endet auf `.instrumentWell(focused: focused)`. Damit ist jedes Pfadfeld (⌘N, Einstellungen) ein Feld.
  - `NewSessionView`: `SeamLine()` statt beider `Divider`, „ändern“ in `Theme.accentTextColor`, Hinweis „existiert nicht“ in `Theme.errorTextColor`, Zeilen `.instrumentRow(selected: on)` statt Fläche und Balken, Ablage-Rahmen `RoundedRectangle(cornerRadius: Instrument.dialogRadius * Theme.scale).strokeBorder(Theme.accentColor, lineWidth: 2)`.
  - `EditGroupView`, `RenameSessionView`: Namensfeld `.instrumentWell(focused: focused)` (vorhandener `@FocusState`). Farbfelder `RoundedRectangle(cornerRadius: 3 * Theme.scale)`, gewählt mit `strokeBorder(Theme.fgColor, lineWidth: 1.5)`.
  - `ConfirmView`: „Abbrechen“ `InstrumentKeyStyle(.neutral)`, Bestätigen `InstrumentKeyStyle(destructive ? .destructive : .primary)`, beide behalten `kbdFocusRing()`, Texte unverändert (auch „  ⌘⏎“). Kein eigener Hintergrund.
  - Test-Helfer in `DialogLookTests`: `func assertOnRaised<V: View>(_ view: V, at point: (CGSize) -> CGPoint, file:line:)` rendert `view.raisedPanel()` in Instrument Light (dort sind `raised` `#fbfaf7`, `bg` `#f2f1ec` und `panel` `#e3e1db` verschieden) und prüft `distance(pixel, Theme.raised) < 0,02`.

- [ ] **Step 1: Tests schreiben**

```swift
// Rechts in der Titelzeile steht nichts: dort muss die erhabene Fläche durchscheinen, kein eigener Dialoggrund.
func testNewSessionOnRaisedSurface() {
    let model = NewSessionModel(groups: [], counts: [:], index: FolderIndex(directory: tmp), askFinder: false)
    assertOnRaised(NewSessionView(model: model, onStart: { _, _ in }), at: { CGPoint(x: $0.width - 20, y: 14) })
}
func testConfirmOnRaisedSurface() {
    assertOnRaised(ConfirmView(title: "T", info: "i", button: "OK", destructive: true, onConfirm: {}, onCancel: {}), at: { CGPoint(x: $0.width - 20, y: 18) })
}
func testEditGroupOnRaisedSurface() { /* EditGroupView(model: EditGroupModel(group: g)), Punkt rechts in der Titelzeile */ }
func testRenameOnRaisedSurface() { /* RenameSessionView(model: RenameSessionModel(session: s)), Punkt rechts in der Titelzeile */ }

// TextFieldCenteringTests
func testPathFieldReportsFocus() {
    var seen: [Bool] = []
    let rep = PathFieldRep(text: .constant(""), autofocus: false, onTab: {}, onSubmit: {}, onMove: { _ in }, onFocus: { seen.append($0) })
    let c = rep.makeCoordinator(), f = NSTextField()
    c.controlTextDidBeginEditing(Notification(name: NSControl.textDidBeginEditingNotification, object: f))
    c.controlTextDidEndEditing(Notification(name: NSControl.textDidEndEditingNotification, object: f))
    XCTAssertEqual(seen, [true, false])
}
```

`tmp` ist ein frischer Ordner unter `FileManager.default.temporaryDirectory` (wie in `NewSessionModelTests`), `g`/`s` wie in `AccessibilityTests`.

- [ ] **Step 2:** `$TEST DialogLookTests`, `$TEST TextFieldCenteringTests` · Expected: FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `$TEST DialogLookTests`, `$TEST TextFieldCenteringTests`, `$TEST NewSessionModelTests`, `$TEST AccessibilityTests` · Expected: PASS.
- [ ] **Step 5: Commit** `Neue Session, Gruppe, Umbenennen, Rückfragen: Felder eingelassen, Tasten, Auswahl als Umriss`

---

### Task 7: Einstellungen (⌘,)

**Files:**
- Modify: `app/Kadrell/UI/SettingsView.swift`
- Test: `app/KadrellTests/DialogLookTests.swift`

**Interfaces:**
- Consumes: Task 3, Task 6 (`PathField` ist schon ein Feld).
- Produces (nur Aussehen, Reihenfolge und Inhalte bleiben):
  - Kopfzeile: `Text("EINSTELLUNGEN").capsLabel()`, Version rechts unverändert, darunter `SeamLine()` statt `Divider`.
  - `heading`: `Text(s.uppercased()).sectionTitle()`, Notiz wie bisher in `mutedColor`.
  - `table`: `SeamLine()` zwischen den Zeilen, `.instrumentGroup()` statt des Linienrahmens, `padding(.horizontal, 16)` bleibt.
  - `menu`-Label: `.instrumentWell()` statt `background(Theme.bgColor)` (Innenabstand kommt aus dem Modifier, `frame(width: controlWidth)` bleibt).
  - Editor-Feld: neuer `@FocusState private var editorFocused: Bool`, `.instrumentWell(focused: editorFocused)`.
  - `pathField`: nur noch `PathField(…)` plus `dropDestination`, ohne eigenen Abstand und Grund.
  - `iconButton`: `InstrumentKeyStyle(.neutral, compact: true)`, Symbol in `frame(width: 22 * Theme.scale, height: 16 * Theme.scale)` (mit Dicke 18 wie bisher), `kbdFocusRing` bleibt.
  - Konten: „aktiv“ in `accentTextColor`; „Wechseln“, „Entfernen“, „Hinzufügen“ `InstrumentKeyStyle(.neutral, compact: true)`.
  - Kürzel-Untergruppen (Navigation, Kacheln verwalten): `Text(title).capsLabel(bold: true)`.
  - `row(_:)`: „Standard“ `InstrumentKeyStyle(.neutral, compact: true)`. Kürzel-Feld: Label `.instrumentWell(focused: isRecording)`, Text in Aufnahme `accentTextColor`, sonst `fgColor`; Knopf bleibt `.plain` + `kbdFocusRing`.
  - `.background(Theme.panelColor)` am Ende entfällt.

- [ ] **Step 1: Test schreiben**

```swift
func testSettingsOnRaisedSurface() {
    // Mitte der Kopfzeile ist leer (Titel links, Version rechts).
    assertOnRaised(SettingsView(model: SettingsModel()), at: { CGPoint(x: $0.width / 2, y: 14) })
}
```

- [ ] **Step 2:** `$TEST DialogLookTests` · Expected: FAIL.
- [ ] **Step 3:** Implementieren. `SettingsView.body` ist lang: neue Verzweigungen in die vorhandenen Helfer legen, nicht in `body`.
- [ ] **Step 4:** `$TEST DialogLookTests`, `$TEST SettingsModelTests`, `$TEST HotkeyTests`, `$TEST LocalizationTests` · Expected: PASS. lizard leer.
- [ ] **Step 5: Commit** `Einstellungen: Gruppen mit Fugen, Werte in Feldern, Tasten, gesperrte Titel`

---

### Task 8: Hilfe (F1) und Statistik (F3)

**Files:**
- Modify: `app/Kadrell/UI/AboutView.swift`, `app/Kadrell/UI/StatsView.swift`
- Test: `app/KadrellTests/DialogLookTests.swift`

**Interfaces:**
- Consumes: Task 3.
- Produces:
  - `AboutView`: beide `Divider` → `SeamLine()` (Abstände bleiben), Bereichstitel `Text(title.uppercased()).sectionTitle()` mit den bisherigen Abständen, beide Links in `accentTextColor`, kein eigener Hintergrund.
  - `StatsView`: beide `Divider` → `SeamLine()`, Rekord-Datum in `accentTextColor`, kein eigener Hintergrund.

- [ ] **Step 1: Tests schreiben**

```swift
func testAboutOnRaisedSurface() { assertOnRaised(AboutView(), at: { CGPoint(x: $0.width - 20, y: 20) }) }
func testStatsOnRaisedSurface() { assertOnRaised(StatsView(), at: { CGPoint(x: $0.width - 20, y: 20) }) }
```

- [ ] **Step 2:** `$TEST DialogLookTests` · Expected: FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `$TEST DialogLookTests`, `$TEST StatsViewRenderTests` (beide Sprachen gleich hoch) · Expected: PASS.
- [ ] **Step 5: Commit** `Hilfe und Statistik: Fugen, gesperrte Bereichstitel, lesbare Links`

---

### Task 9: Extensions-Dialog (F4)

**Files:**
- Modify: `app/Kadrell/Extensions/UI/ExtensionsView.swift`, `app/Kadrell/Extensions/UI/ExtensionDetail.swift`
- Test: `app/KadrellTests/DialogLookTests.swift`, `app/KadrellTests/ExtensionsViewTests.swift` (bestehend)

**Interfaces:**
- Consumes: Task 3.
- Produces:
  - `ExtensionsView`: Kopf `Text(verbatim: "EXTENSIONS").capsLabel()`, „Extensions-Ordner öffnen“ `InstrumentKeyStyle(.neutral, compact: true)`, „Doku“ in `accentTextColor`. Alle `Divider` → `SeamLine()`. Fußhinweis unverändert.
  - Warnung vor dem ersten Einschalten: `.instrumentGroup()` plus `overlay(RoundedRectangle(cornerRadius: Instrument.groupRadius * Theme.scale).strokeBorder(Theme.errorTextColor, lineWidth: 1.5))`; „Einschalten“ `InstrumentKeyStyle(.destructive)`, „Abbrechen“ `InstrumentKeyStyle(.neutral)`.
  - Entdecken: `Text("ENTDECKEN").sectionTitle()`, Suchfeld mit neuem `@FocusState` `.instrumentWell(focused:)`, „Suchen“ neutral kompakt, Fehler in `errorTextColor`. `StoreRow`: Link in `accentTextColor`, „Installieren“/„Aktualisieren“ neutral kompakt.
  - `ExtensionRow`: `.instrumentRow(selected: selected)` statt `surfaceColor` und Balken; Zustandstext bei Fehler `errorTextColor`. Zustandspunkt und Schalter bleiben.
  - `ExtensionDetail`: Name `Text(ext.name.uppercased()).sectionTitle()`, `button(_:action:)` mit `InstrumentKeyStyle(.neutral, compact: true)`, Formular `SeamLine()` + `.instrumentGroup()`, `SettingField` `.instrumentWell(focused: focused)` statt Abstand und `bgColor`, Log in `.instrumentWell()` statt `bgColor` (eigenes `padding(8)` entfällt dafür).
  - `.background(Theme.panelColor)` entfällt.

- [ ] **Step 1: Test schreiben**

```swift
func testExtensionsOnRaisedSurface() {
    // Fußhinweis steht rechtsbündig: links unten ist Fläche.
    assertOnRaised(ExtensionsView(model: model), at: { CGPoint(x: 20, y: $0.height - 12) })
}
```

`model` wie in `ExtensionsViewTests.startModel()` (Fixture aus `ExtFixture.swift`); bei Bedarf den Test in `ExtensionsViewTests` statt `DialogLookTests` anlegen und `assertOnRaised` dorthin als `XCTestCase`-Extension in `PixelTesting.swift` verschieben.

- [ ] **Step 2:** Test · Expected: FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `$TEST ExtensionsViewTests` (inkl. `testDialogRenderSmoke`, beide Sprachen), `$TEST DialogLookTests` · Expected: PASS.
- [ ] **Step 5: Commit** `Extensions-Dialog: Tasten, eingelassene Felder, Fugen, Auswahl als Umriss`

---

### Task 10: Rechte Sidebar, Baum-Toolbar und Wächter-Test

**Files:**
- Modify: `app/Kadrell/Extensions/UI/ExtensionPanelView+Drawing.swift`, `app/Kadrell/Extensions/UI/ExtensionPanelView.swift` (`pressed` → `private(set)`)
- Modify: `app/Kadrell/Sidebar/SidebarView.swift` (`drawToolbar`)
- Modify: `app/Kadrell/UI/Theme+SwiftUI.swift` (`lineColor` entfernen)
- Bedingt: `app/Kadrell/Sidebar/SidebarRenderer.swift` (Step 6)
- Test: `app/KadrellTests/ExtensionPanelTests.swift`, `app/KadrellTests/DialogLookTests.swift`

**Interfaces:**
- Consumes: Task 2 (`Instrument.key`, `hSeam`, `rowHighlight`, `raisedPanel`, `fillRow`, `labelKern`), Task 1 (`accentText`, `errorText`).
- Produces:
  - `ThemeColor.textColor: NSColor`: `.accent` → `Theme.accentText`, `.err` → `Theme.errorText`, sonst `nsColor`. Text-Zeilen (`.text`) nutzen `textColor`, Punkte weiter `nsColor`.
  - `drawTabs`: `Instrument.hSeam(x: 0, y: tabHeight - 2, width: width)`. Jeder Tab ist `Instrument.key(CGRect(x: r.minX + 2, y: 5, width: r.width - 4, height: 22), pressed: on, fill: nil)`, Text mittig auf der zurückgegebenen Kappe: gewählt `fg` fett, sonst `muted`. Akzent-Unterstrich entfällt. `tabRects()` und Trefferflächen bleiben.
  - `drawHighlight(_ r:, hot:, ring:)`: `if hot || ring { Instrument.rowHighlight(r, outline: ring, ring: ring) }`.
  - Abschnittstitel: `title.uppercased()`, `Theme.attrs(11, Theme.accentText, bold: true)` plus `.kern: Instrument.labelKern`.
  - `drawButton(_:in:hot:pressed:)`: `Instrument.key(b, pressed: pressed, fill: nil)`, Text `fg` mittig auf der Kappe; `pressed` = `pressed == i` aus der View.
  - `SidebarView.drawToolbar`: `Instrument.raisedPanel(t, radius: Instrument.keyRadius, depth: 2)` statt `NSShadow`, Füllung und Strich; Hover-Knopf `Instrument.fillRow(b.hit.rect)` statt `Theme.line`.
  - `Theme.lineColor` gelöscht (keine Nutzer mehr).

- [ ] **Step 1: Tests schreiben**

```swift
// ExtensionPanelTests, Aufbau wie testRenderSmoke (Fenster, Backdrop, Panel 320 × 520), Schema instrument-dark gesetzt.
func testTabRowEndsInSeam() {
    // Zeile tabHeight - 2 dunkler als panel, Zeile tabHeight - 1 heller als Zeile tabHeight - 2 (bei x = 300, rechts neben den Tabs).
}
func testKeyboardCursorHasAccentRing() {
    // ⌘3-Fokus (makeFirstResponder), zweimal ↓: an x = 4, y = Mitte der Cursorzeile ≈ accent (Abstand < 0,1).
}
func testPanelTextUsesReadableRoles() {
    withTheme(.named("solarized-light")) {
        XCTAssertEqual(ThemeColor.accent.textColor, Theme.accentText); XCTAssertEqual(ThemeColor.err.textColor, Theme.errorText)
        XCTAssertEqual(ThemeColor.muted.textColor, Theme.muted)
    }
}

// DialogLookTests: Wächter, damit kein Dialog in den flachen Look zurückfällt.
func testDialogsUseInstrumentParts() throws {
    // Quellen wie in LocalizationTests.testEveryLocalizedStringUsesTheSelectedBundle lesen.
    // Dateien: alle *.swift in UI/ und Extensions/UI/, dazu Sidebar/SidebarView.swift und App/SheetPresenter.swift.
    // Verboten: "Theme.line", "lineColor", "Divider()", "Theme.panelColor", "NSShadow()". Erwartet: keine Fundstelle.
}
```

Die drei Panel-Tests mit echten Pixel-Assertions nach dem Muster in `InstrumentTests` ausschreiben; `testRenderSmoke` zeigt den Aufbau.

- [ ] **Step 2:** `$TEST ExtensionPanelTests`, `$TEST DialogLookTests` · Expected: FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `$TEST ExtensionPanelTests`, `$TEST DialogLookTests`, `$TEST AccessibilityTests`, `$TEST SidebarRendererTests` · Expected: PASS. lizard leer.
- [ ] **Step 5: Commit** `Rechte Sidebar: Tabs als Tasten, Fuge, Auswahl wie im Baum; Baum-Toolbar erhaben`
- [ ] **Step 6 (bedingt): Baum nutzt die Kit-Auswahl.** Nur wenn `SidebarRenderer.fillRowShape`, `drawFocusOutline`, `drawKeyFocusRing` und `rowShape` nach Stufe 3 noch die Werte von heute haben (Einzug 6/1, Radius 6, Umriss 1,5 pt Radius 5,25, Ring 2 pt Radius 8/6): ihre Körper durch `Instrument.fillRow`, `Instrument.outline`, `Instrument.keyRing`, `Instrument.rowShape` ersetzen, Signaturen bleiben. `$TEST SidebarRendererTests`, `$TEST AccessibilityTests` · Expected: PASS. Commit `Baum zeichnet die Auswahl über das Kit`. Haben sie sich geändert: nichts tun und im Abschluss melden.

---

### Task 11: Sichtprüfung, Changelog, Version

**Files:**
- Temporär: `app/KadrellTests/InstrumentStage4RenderTests.swift` (danach löschen, Bilder bleiben)
- Modify: `CHANGELOG.md`, Version per `python3 tools/bump-version.py minor` (`app/project.yml`, `app/Kadrell/Info.plist`)

- [ ] **Step 1: Render.** Temporärer Test rendert über `snapshot` jeweils `OverlayScroll(limit: OverlayLimit(), content: …)` für: ⌘N (drei Gruppen-Kandidaten, zweiter gewählt, Pfadfeld mit Text), Rückfrage destruktiv und nicht destruktiv mit „Nicht mehr fragen“, Gruppe bearbeiten, Einstellungen (ganze Höhe), F1, F3, F4 (Fixture wie `testDialogRenderSmoke` mit Warnung, Formular und Log), dazu die Palette (`PaletteWindow` über einem randlosen Testfenster öffnen wie in `BackdropTests`, `contentView` per `cacheDisplay`) und die rechte Sidebar (Aufbau wie `ExtensionPanelTests.testRenderSmoke`, mit Tastatur-Cursor). Jeweils in Instrument Dark, Instrument Light und GitHub Light bei 100 %, die Einstellungen zusätzlich bei 130 %. Ablage `~/.claude/screenshots/kadrell/instrument-stufe-4/<dialog>-<schema>[-130].png` (Ordner anlegen, nie Bilder löschen). Lauf: `TEST_RUNNER_KADRELL_PROFILE=tmp`, also `$TEST InstrumentStage4RenderTests`. Bilder mit dem Read-Werkzeug ansehen. Wirkt etwas schmutzig, unsichtbar oder abgeschnitten: nachjustieren (erlaubt: Schattenalpha, Fugenalpha je Schema), betroffene Tests erneut laufen lassen. Danach den Test löschen.
- [ ] **Step 2: Laufende App** mit `open -n app/build/Build/Products/Debug/Kadrell.app --args --profile tmp`: ⌘N, ⌘P, ⌘,, F1, F3, F4, ⌘W auf einer Session (Rückfrage), Tab durch eine Rückfrage, ⌘3 in die rechte Sidebar (falls eine Extension ein Panel hat), in den Einstellungen das Schema bei offenem Dialog wechseln und die UI-Größe auf 130 % stellen. Nur die eigene Instanz beenden: pid aus `/bin/ps -axo pid=,args= | grep "<worktree>/app/build.*profile tmp"`, `kill -TERM <pid>`.
- [ ] **Step 3:** Gesamtsuite grün, Clean Build ohne eigene Warnungen, `cd app && uv tool run lizard Kadrell -w` leer.
- [ ] **Step 4: Changelog.** Unter `## Unreleased` oben in `CHANGELOG.md` (Abschnitt legt Stufe 3 an; fehlt er, anlegen) über den Einträgen von Stufe 3:

```
- Änderung: Dialoge, Einstellungen, Hilfe (F1), Statistik (F3), Extensions (F4) und die Suche (⌘P) sind neu gestaltet: erhabene Fläche mit runden Ecken und feinen Fugen, Knöpfe als Tasten, Eingabefelder eingelassen, die Auswahl in Listen als Rahmen.
- Änderung: Die rechte Seitenleiste zeigt ihre Tabs als Tasten und ihre Abschnitte mit gesperrten Überschriften.
- Fix: Links, Überschriften und Fehlermeldungen in Dialogen sind auch in hellen Farbschemata, Nord und Solarized gut lesbar.
```

Dann `python3 tools/bump-version.py minor` (ein Sprung für Stufe 3 und 4).
- [ ] **Step 5: Commit** (Branch, kein Merge): `/usr/bin/git add CHANGELOG.md app/project.yml app/Kadrell/Info.plist && /usr/bin/git commit -m "Version <neu>: Instrument-Kacheln, -Dialoge und -Extensions"`
