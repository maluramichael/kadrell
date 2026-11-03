# Harness-Auswahl beim Start neuer Sessions

## Warum

Zwischen Claude und Pi zu wechseln geht heute nur über Tab im Schnellstart (⌘⇧I), und das funktioniert nicht
zuverlässig. ⌘N, ⌘⏎ und die Kontextmenüs starten immer Claude. Michael will bei jeder neuen Session mit einem
Tastendruck mehr entscheiden, welcher Harness startet, und im Normalfall nur ⏎ drücken.

## Ziele

- Jeder interaktive Weg zu einer neuen Session zeigt vor dem Start eine kleine Auswahl der Harnesse.
- Der zuletzt gewählte Harness steht oben und ist vorausgewählt: ⌘⏎ ⏎ startet wie bisher, nur mit einem Druck mehr.
- Komplett per Tastatur: ↑↓ wählen, Tippen filtert unscharf (wie fzf), ⏎ starten, Esc abbrechen. Keine Ziffern.
- Neue Harnesse kommen später als ein Eintrag in einer Liste dazu, ohne neue Oberfläche.

## Nicht-Ziele

- Keine Varianten in der Auswahl (Accounts, mehrere lokale Modelle). Nur der Harness: Claude, Pi.
- `kadrell new` auf der Kommandozeile bleibt ohne Popup (Claude, mit `--pi` Pi).
- ⌘T (Terminal), ⌘⇧N (Remote) und das Fortsetzen aus der Verlaufssuche (⌘⇧H, setzt eine Claude-Konversation fort)
  bleiben ohne Auswahl.
- Kein neuer Hotkey.

## Ansatz

Gewählt: **A, ein eigenes Auswahl-Overlay vor dem Start.** Alle interaktiven Wege rufen dieselbe Funktion „Harness
wählen, dann starten“ vor `startSession` auf.

Verworfen: **B, Wahl als zweite Stufe in jedem Dialog.** Bei ⌘N nahtloser, aber ⌘⏎ und Kontextmenüs bräuchten trotzdem
ein eigenes Popup, die Logik stünde zwei- bis dreimal im Code. Ebenso verworfen: eine Zusatztaste (⌘⌥⏎) ohne
Popup, weil Michael die Auswahl sehen will.

## Verhalten

Wege mit Auswahl (heutige Aufrufer von `startSession`):

- ⌘N: nach der Projektwahl.
- ⌘⏎: sofort, Ordner der fokussierten Session (ohne fokussierte Session wie ⌘N).
- „Neue Session hier“ im Session-Kontextmenü, „Neue Session in dieser Gruppe“ bzw. Plus im Baum.
- Ordner in den Baum ziehen.
- Schnellstart ⌘⇧I: nach der Projektbestätigung. Tab-Wechsel und die Zeile „startet mit …“ entfallen.

Das Overlay:

- Klein, mittig, folgt dem Fenster (auf `SheetPresenter.present`/`OverlayPanel`, Breite über `overlayWidth(_:)`).
- Zeigt oben den Zielordner bzw. das Projekt, darunter die Harnesse.
- Zuletzt gewählter Harness oben und markiert, die übrigen in fester Reihenfolge darunter.
- Pi zeigt sein Modell: „Pi · qwen3-coder“.
- Suchfeld oben, Tippen filtert die Liste unscharf (vorhandenes `PaletteWindow.fuzzy` wiederverwenden), die
  Markierung springt auf den ersten Treffer. ↑↓ wählen, ⏎ startet die markierte Zeile. Keine Ziffern.
- Esc schließt ohne Start. Beim ⌘N-Weg kehrt Esc nicht in den Projektdialog zurück, sondern schließt.

Erinnerung: eine Einstellung für alle Wege, global, nicht pro Projekt. Startwert aus der bisherigen
Schnellstart-Einstellung (Pi im Schnellstart an → Pi).

## Meine Entscheidungen (bitte prüfen)

- **Nur ein Harness nutzbar** (Pi nicht installiert oder kein Pi-Modell gesetzt): kein Popup, Claude startet direkt
  wie heute. Wer Pi nicht nutzt, bekommt keinen Extra-Tastendruck.
- **Esc bei ⌘N schließt ganz**, statt zurück in den Projektdialog. Einfacher, und der Dialog ist mit ⌘N schnell
  wieder da.
- Ist Pi oben, wärmt Kadrell beim Öffnen das lokale Modell vor, wie der Schnellstart heute.

## Rahmen

- Zweisprachig: Texte in `Localizable.xcstrings` (de und en), SwiftUI-Overlay erbt `\.locale`.
- Tests: Auswahlmodell (Reihenfolge, Vorauswahl, Filter, Erinnerung, ein Harness → kein Popup),
  `OverlayFollowsWindowTests` deckt das neue Overlay mit ab, `LocalizationTests` grün.
- Clean Build ohne Warnungen, `lizard` ohne Warnungen.
- Changelog: Feature-Eintrag für die Auswahl, Entfernt-Eintrag für den Tab im Schnellstart.

## Offen für den Bau

- Name und Ablage der neuen Typen (`Harness`, Auswahlmodell, View).
- Ob die Einstellung unter `LocalModelSettings` oder `Settings` liegt.
- Aufwand: etwa 2 bis 3 Stunden inklusive Tests.
