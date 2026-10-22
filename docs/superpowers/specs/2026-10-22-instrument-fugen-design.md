# Instrument: Fugen, Tasten und Display für Kadrell

Stand 22.10.2026. Vorbild: Monsters (github.com/Saturday-Company/monsters, Apache-2.0), dort `docs/design.md`,
Konzept „Instrument“. Gewählt im Browser-Vergleich: Variante B2 (Fugen plus Instrument-Teile).

## Absicht

Michael gefallen an Monsters vor allem die feinen Linien. Im Vergleich hat er sich für die **Fugen** entschieden und
dazu für die übrigen Instrument-Teile: Kadrell soll wirken wie ein Gerät (Teenage Engineering, Nothing Phone), nicht
wie flache Flächen mit Trennstrichen. Das ersetzt den heutigen flachen Look vollständig, in allen Farbschemata.

## Ziele

1. **Fugen statt harter Trenner.** Überall, wo heute `Theme.line` eine 1-px-Linie zieht (35 Stellen in 17 Dateien:
   Leiste, Baum, Split-Trenner, Kacheln, Stack, Dialoge, Palette, Extensions), steht eine Fuge: 1 px dunkle Linie,
   direkt darunter bzw. daneben 1 px helle.
2. **Eingesetztes Modul.** Die Kachelfläche liegt als Modul im Gehäuse (Leiste und Baum): Fuge zu Leiste und Baum,
   obere linke Ecke Radius 14.
3. **Tasten mit Dicke** für die Schalter der Leiste (Layout, Spalten, AUTO, SYNC, GRP, SORT): Kappe, 1 px Kante,
   2 bis 3 px feste Dicke unten. Gedrückt sinkt die Taste ein. Ein eingeschalteter Schalter bleibt eingesunken.
4. **Schwarzes Status-Display** in der Leiste rechts: ersetzt die farbigen Zähler (`0 0 0 0 · 0`) und die Uhr durch
   Text in Worten, z. B. `● 2 ARBEITEN · 1 WARTET  14:21`. Das Display ist in allen Schemata schwarz (ein Bauteil,
   keine Fläche).
5. **Gesperrte Gruppen-Labels** im Baum (`KADRELL · 2`, Versalien, Laufweite ca. +0,08 em), die Gruppenfarbe nur noch
   als kleiner Akzent (Punkt oder schmaler Strich) am Label und an Kachelkopf bzw. Stack-Zeile.
6. **Auswahl im Baum** als Umriss: 1,5 px innen in `fg`, Radius ca. 9, auf der Modulfarbe.
7. **Strich-Anzeige für „arbeitet“** (vier kurze senkrechte Striche, die nacheinander aufleuchten) statt des
   pulsierenden Punkts, überall wo heute der Statuspunkt einer arbeitenden Session pulsiert.
8. **Neues Farbschema „Instrument“**, dunkel und hell, Werte aus Monsters (unten).
9. **Dialoge und Popover** (OverlayPanel, Palette, Einstellungen, Extensions): erhabene Fläche, Fuge als Rand,
   Radius 14, kurzer fester Schatten.

## Nicht-Ziele

- Keine neuen Schriften (Doto, Martian Mono, Instrument Sans). Kadrell bleibt bei JetBrains Mono. Eigener Schritt
  später.
- Keine Punkt-Avatare pro Gruppe oder Session. Die Monster-Figuren sind Markenzeichen von Saturday Company
  (`TRADEMARKS.md`) und dürfen ohnehin nicht übernommen werden.
- Keine Umstellung aller Animationen auf Einzelbilder. Nur die neue Strich-Anzeige läuft in Stufen.
- Kein Umbau von Layouts, Bedienung, Kürzeln oder Inhalten der Leiste. Nur das Aussehen ändert sich.
- Kein Inhalt in den Terminals: was Claude Code zeichnet, bleibt unberührt.

## Anti-Ziele (wäre ein Fehlschlag, auch wenn es technisch läuft)

- Zwei Bauweisen nebeneinander (alter flacher Look als Option). Ausdrücklich abgelehnt.
- Fugen, die in einem der vorhandenen Schemata nicht sichtbar sind oder wie Schmutz wirken (v. a. Solarized, GitHub
  Light, Latte).
- Weniger Platz für Terminals. Fugen und Modul dürfen pro Trenner höchstens 1 px mehr kosten als heute.
- Status, der nur über Farbe lesbar ist: Display und Labels sagen den Zustand auch in Worten oder Zahlen.

## Randbedingungen

- Clean Build ohne eigene Warnungen, `lizard` ohne Warnungen (CCN ≤ 15), alle Tests grün (`CLAUDE.md`).
- Jeder neue Nutzertext (Display-Wörter wie „ARBEITEN“, „WARTET“, Schema-Namen) über `Bundle.app`, Deutsch und
  Englisch in `Localizable.xcstrings`. Das Display muss beim Sprachwechsel ohne Neustart umschalten.
- UI-Größe 90 bis 130 %: Fugen bleiben bei jeder Größe genau 1 px + 1 px (Haarlinien, nicht mitskaliert), Radien und
  Tastendicke skalieren mit.
- Kontraste: `ContrastTests` decken die neuen Schemata ab (Text ≥ 4,5:1 auf Gehäuse, Modul, Kappe; Display-Text
  ≥ 4,5:1 auf Schwarz; Strich-Anzeige ≥ 3:1).
- Bestehende Nutzer behalten ihr Schema. Heute ist der Default `ColorTheme.all[0]` (Mocha), wenn nichts gespeichert
  ist. Wird Instrument der neue Default, müssen Profile ohne gespeicherte Wahl, die schon existieren, Mocha behalten
  (beim ersten Start nach dem Update festschreiben).
- Herkunft: Farbwerte und Konzept stammen aus Monsters (Apache-2.0). Quelle im Code-Kommentar beim
  Schema und in den Lizenzhinweisen der App nennen.
- Testen nur mit `--profile tmp`, Sichtprüfung per temporärem Render-Test (`CLAUDE.md`).

## Bauweg (gewählt: 1, Bauteil-Kit mit Auslieferung in Stufen)

Erwogen:

1. **Bauteil-Kit, Auslieferung in Stufen (gewählt).** Die Bauteile entstehen einmal zentral, dann stellen vier
   Stufen nacheinander um, jede für sich releasbar. Vorteil: nach der ersten Stufe echte Fugen im eigenen Kadrell,
   Nachsteuern möglich. Nachteil: ein paar Tage gemischter Look.
2. **Bauteil-Kit, alles in einem Release.** Gleiches Kit, ein Branch, ein Sprung. Kein Mischzustand, aber 3 bis 5
   Tage ohne Zwischenstand und späte Erkenntnis, falls die Fuge in echt nicht wirkt.
3. **Neue Renderer neben den alten** (wie die Baum-Designs). Verworfen: der alte Look soll verschwinden, das wäre
   doppelter Code.

Michael hat Weg 1 gewählt, damit er nach der ersten Stufe in echt sieht, ob die Fugen tragen.

### Bauteil-Kit

Zentral in der Nähe von `Theme.swift` (z. B. `Theme+Instrument.swift`), AppKit und SwiftUI:

- `seam` horizontal und vertikal an einer Kante eines Rechtecks.
- `key(rect, pressed:)` mit Kappe, Kante, Dicke; Text/Icon darauf.
- `screen(rect)` schwarzes Display mit 1 px Innenreflex oben.
- `module` Fläche mit Radius oben links und Fugen an den Kanten zu Gehäuse.
- `meter(rect, phase:)` Strich-Anzeige, vier Striche.
- SwiftUI-Gegenstücke als ViewModifier (`.seam(edge:)`, `.instrumentKey()`, `.instrumentScreen()`).

Abgeleitete Farben pro Schema (keine Änderung an den Werten der zehn vorhandenen Schemata):

| Rolle | dunkle Schemata | helle Schemata |
|---|---|---|
| Fuge dunkel | Schwarz, Alpha 0,6 | Schwarz, Alpha 0,13 |
| Fuge hell | Weiß, Alpha 0,05 | Weiß, Alpha 0,65 |
| Tastenkante/Dicke | `bg` stark Richtung Schwarz | `line` Richtung Schwarz |
| Tastenkappe | `surface` | `surface` bzw. heller |
| Display | `#050505` | `#0d0d0c` |
| Display-Text / gedämpft | `#efe9dc` / `#8f8b81` | gleich |

Die Werte für die Fugen kommen aus Monsters. Ob sie in jedem der zehn Schemata tragen, zeigt erst der Render-Test,
Feinjustierung pro Schema ist erlaubt.

### Schema „Instrument“ (Werte aus Monsters `docs/design.md`)

| Rolle (Monsters) | Dunkel | Hell | Kadrell-Verwendung |
|---|---|---|---|
| chassis | `#151514` | `#e3e1db` | Gehäuse: Leiste, Baum, rechte Sidebar |
| panel | `#1c1c1a` | `#f2f1ec` | Modul: Kachelfläche, Auswahl im Baum |
| raised | `#252523` | `#fbfaf7` | Tastenkappen, Kachelköpfe, Dialoge |
| well | `#0f0f0e` | `#d8d5ce` | eingelassene Felder (Suche, Eingaben) |
| key-edge | `#0a0a09` | `#b5b2a9` | Tastendicke |
| ink | `#ecebe5` | `#1a1a18` | Text |
| ink-muted | `#a3a19a` | `#5c5a54` | Sekundärtext |
| ink-faint | `#8d8b85` | `#6b6963` | Labels, Zeiten |
| signal | `#ff5e14` | `#ff4f00` | **wartet** (die eine Aktionsfarbe) |
| signal-ink | `#ff8a4c` | `#a83500` | „wartet“ als Text |
| ok | `#4fd98a` | `#1f8f4e` | fertig |
| stop | `#ff3b2f` | `#d8231a` | Fehler, SYNC |

„Arbeitet“ hat in Instrument keine eigene Buntfarbe: die Strich-Anzeige in `ink` (Kennzeichen ist die Form, nicht
die Farbe). Die acht ANSI-Farben für Terminals wählt der Umsetzer passend zum Schema (Kontrast auf `panel` prüfen).

### Stufen

1. **Kit + Leiste + Schema Instrument.** Bauteile, abgeleitete Farben, Schema dunkel/hell in den Einstellungen,
   Leiste mit Fuge unten, Tasten, Display. Default-Migration für bestehende Profile.
2. **Baum und Split-Trenner.** Fuge zwischen Baum und Modul (statt `dividerColor`), Modul mit Radius, Gruppen-Labels,
   Auswahl-Umriss, Strich-Anzeige im Baum, linkes Dock.
3. **Kacheln.** Fugen zwischen Kacheln und unter Kachelköpfen, neutrale Kachelköpfe mit Gruppen-Akzent statt
   Gruppen-Tönung, Stack-Zeilen, Strich-Anzeige in Kachelkopf und Stack.
4. **Dialoge und Extensions.** OverlayPanel, Palette, Einstellungen, Bestätigungen, Extensions-Dialog und -Panels,
   rechte Sidebar.

Jede Stufe: Changelog-Eintrag und Versionssprung nach `CLAUDE.md` (Stufe 1 `minor`, die übrigen je nach Umfang).

## Dem Umsetzer überlassen

- Genaue Maße (Tastenhöhe, Dicke 2 oder 3 px, Abstände), solange die Leiste nicht höher wird als heute plus 4 px.
- Ob der gedrückte Zustand animiert (zwei Stufen à 40 ms) oder sofort umspringt.
- Wie die Strich-Anzeige getaktet ist (Richtwert 120 ms pro Schritt), solange „Bewegung reduzieren“ sie anhält.
- Aufteilung der Bauteile auf Dateien und Namen.
- Feinjustierung der abgeleiteten Fugenfarben je Schema, wenn der Render-Test es verlangt.

## Entscheidungen nach der Umsetzer-Prüfung

Mit Michael besprochen:

- **Instrument wird Default für Neuinstallationen**, als zwei Einträge „Instrument Dunkel“ und „Instrument Hell“ wie
  die übrigen Schemata (kein Folgen der Systemdarstellung). Neuinstallationen bekommen Dunkel. Profile, die schon
  existieren und kein Schema gespeichert haben, bekommen beim ersten Start nach dem Update Mocha festgeschrieben.

Calls des Planers (Michael prüft sie beim Lesen):

- **Status-Display:** zeigt nur Zustände ungleich null, in Worten, einfarbig in `screen-ink`, die Zahl vor dem Wort
  (`2 ARBEITEN · 1 WARTET · 1 FEHLER`), dahinter die Uhr. „Fertig“ und „getrennt“ erscheinen nicht im Display. Die
  Gesamtzahl steht schon im Breadcrumb. Der Punkt vorn ist eine Lampe: orange, solange etwas wartet, sonst grün, bei
  Fehler rot. Die klickbaren Pillen „N warten“ und „N trennen“ bleiben eigene Tasten links neben dem Display. Wird es
  eng, kürzt das Display auf Zahlen mit Anfangsbuchstaben (`2A · 1W`), VoiceOver liest immer die Langform.
- **Auswahl im Baum:** die gezeigte Session (`focused`) bekommt den 1,5-px-Umriss in `fg`. Die Mehrfachauswahl
  (`selected`) behält den Balken links. Der Tastaturfokus des Baums (`keyFocus`) ist ein 2-px-Ring in der Akzentfarbe,
  außerhalb des Umrisses.
- **Akzentfarbe:** neue Rolle `accent` im Farbmodell. In den zehn vorhandenen Schemata gleich `running` (keine
  Änderung). In Instrument `signal-ink` (`#ff8a4c` / `#a83500`) wie bei Monsters: Fokusring, Links, Primärknöpfe,
  Abschnittstitel, Lua-`accent`.
- **Modul, Hintergrundbild, Deckkraft:** das Modul verhält sich bei Deckkraft unter 100 % wie die Kachelfläche heute
  (halbtransparent über dem Bild). Außen an der runden Ecke liegt die Gehäusefarbe. In Instrument ist der Terminalgrund
  die Modulfarbe. Wirken die Fugen über einem Bild schmutzig, zeigt das der Render-Test, dann Fugen dort deckend
  vormischen.
- **Bash-Kommando läuft:** ebenfalls Strich-Anzeige, in der Shell-Farbe des Schemas (Blau). In Instrument `ink-muted`,
  damit es sich von „arbeitet“ (`ink`) unterscheidet.
- **Eingeschaltete Tasten:** sinken ein **und** behalten ihre heutige Füllfarbe (SYNC rot, AUTO Warte-Farbe), damit der
  Warnzweck von SYNC bleibt.

- **Baum-Design „Getönte Gruppen“ fällt weg** (Michael, 22.10.). Wer es eingestellt hat (auch per Default), landet
  auf „Klassisch“. Klassisch und Kompakt bleiben, beide mit gesperrten Labels, Gruppen-Akzent und Auswahl-Umriss.
  Gehört zu Stufe 2.

## Offene Fragen an Michael

Keine.
