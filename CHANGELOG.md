# Changelog

Neueste Version oben.

## 1.17.1 (2026-09-16)

- Fix: In schmalen Kacheln überlappen sich Titel und Branch nicht mehr. Wird es eng, verschwinden erst Projekt und Branch, der Titel bleibt stehen.

## 1.17.0 (2026-09-16)

- Feature: ⌘F sucht im Terminal, ⌘G und ⌘⇧G springen zum nächsten und vorigen Treffer.
- Feature: ⌘⇧F sucht in allen laufenden Terminals gleichzeitig. Ein Treffer öffnet die Session und springt genau an die Stelle.

## 1.16.0 (2026-09-16)

- Feature: Neuer Sortier-Schalter oben in der Leiste: aus (eigene Reihenfolge), A–Z oder nach Status (wartet, arbeitet, fertig). Sortiert Gruppen und die Sessions darin.

## 1.15.0 (2026-09-16)

- Feature: `kadrell move` verschiebt eine Session in eine andere Gruppe, ohne sie zu beenden.

## 1.14.0 (2026-09-16)

- Feature: `kadrell new --resume` übernimmt eine bestehende Claude-Konversation, zum Beispiel aus tmux, sobald sie dort beendet ist.

## 1.13.2 (2026-09-16)

- Fix: Dialoge, die höher als das Fenster sind (etwa die Einstellungen), lassen sich jetzt scrollen.
- Fix: Beim Design „Getönte Gruppen“ gibt es über der ersten Gruppe keinen leeren Streifen mehr.

## 1.13.1 (2026-09-16)

- Fix: Beim Anklicken einer Session im Baum blitzt die vorher gewählte Zeile nicht mehr kurz mit einem Rahmen auf.

## 1.13.0 (2026-09-16)

- Feature: Auto-Modus (Schalter „AUTO“ oben links): von den ausgewählten Sessions erscheinen nur die, die gerade auf deine Antwort warten. Nach der Antwort verschwindet die Kachel nach drei Sekunden wieder. In den Einstellungen lassen sich auch arbeitende Sessions dazunehmen.

## 1.12.0 (2026-09-16)

- Feature: Kadrell lässt sich wie tmux von außen steuern: `kadrell ls`, `kadrell new-group`, `kadrell new`, `kadrell send`, `kadrell capture` und weitere Befehle, auch aus einer Claude-Session heraus. `kadrell help` zeigt alle.
- Feature: Neuer Menüpunkt „Kommandozeilen-Tool installieren“ legt das Kommando `kadrell` an.

## 1.11.0 (2026-09-16)

- Feature: Der Baum hat jetzt drei Designs zur Auswahl (Einstellungen, Baum und Kacheln): Klassisch, Getönte Gruppen und Kompakt.
- Feature: Die Laufzeit der Sessions im Baum lässt sich ausblenden.
- Änderung: Die Knöpfe im Baum erscheinen beim Überfahren als kleine schwebende Leiste. Laufzeit und Namen verrutschen nicht mehr, und der Knopf unter der Maus wird hervorgehoben.
- Änderung: Gruppenköpfe zeigen die Anzahl ihrer Sessions.

## 1.10.1 (2026-09-16)

- Fix: Der Trenner zwischen Baum und Arbeitsfläche lässt sich wieder zuverlässig greifen, auch nach einer Änderung der UI-Größe. Vorher reagierte er oft nur genau auf der Linie.

## 1.10.0 (2026-09-16)

- Feature: Hooks wie bei tmux. Eigene Skripte laufen, wenn eine Session angelegt, fokussiert oder entfernt wird, und bekommen Ordner, Session-Id und Titel mit. Damit lässt sich z. B. ein Zeiterfassungs-Ticket beim Wechseln der Session umschalten.

## 1.9.0 (2026-09-16)

- Feature: Rückfragen wie „Session beenden und entfernen?“ haben ein Häkchen „Nicht mehr fragen“. In den Einstellungen unter „Rückfragen“ lässt sich jede einzeln wieder ein- und ausschalten.

## 1.8.1 (2026-09-16)

- Fix: Ein Klick in das Terminal einer Kachel gibt ihr wieder die Tastatur, auch wenn mehrere Sessions offen sind. Vorher ging das nur über die Titelzeile.

## 1.8.0 (2026-09-16)

- Fix: Sessions, denen Claude Code keinen Titel gibt, heißen jetzt nach ihrer ersten Nachricht statt nach Ordner und Id.
- Feature: ⌥J und ⌥K blättern durch alle Sessions im Baum und zeigen die jeweilige Session groß als Vorschau. ⏎ öffnet sie, Esc bringt die vorherigen Kacheln unverändert zurück.
- Feature: ⌘ + Mausrad ändert die Schriftgröße aller Terminals auf einmal.
- Feature: Sessions lassen sich umbenennen, per F2 (auch im Baum) oder über den Stift neben dem X an Kachel und Baum-Zeile. Ein selbst vergebener Name bleibt, auch wenn Claude Code später einen eigenen Titel setzt. Leer speichern gibt den Namen wieder an Claude zurück.
- Änderung: Terminals werden auf der GPU gezeichnet (Metal). Das senkt die CPU-Last bei laufender Ausgabe deutlich; abschaltbar in den Einstellungen unter Darstellung.
- Änderung: Der Status der Sessions kommt direkt aus den Session-Dateien von Claude Code, statt alle zwei Sekunden einen Claude-Prozess zu starten.
- Fix: Der Baum zeichnet beim Pulsieren nur noch die Statuspunkte laufender Sessions neu, nicht mehr alle Zeilen.
- Fix: Textänderungen in sichtbaren Terminals lösen kein Neuzeichnen aller Kacheln und des Baums mehr aus.

## 1.7.0 (2026-09-16)

- Feature: Neuer Hotkey ⌥E öffnet den Ordner der fokussierten Session im externen Editor. Das Editor-Kommando (z. B. `code`) steht in den Einstellungen unter Sessions.

## 1.6.0 (2026-09-16)

- Feature: Die Kopfzeile jeder Session zeigt den aktuellen Git-Branch des Projektordners.

## 1.5.1 (2026-09-16)

- Fix: ⌘N öffnet immer den Dialog zur Gruppenwahl, auch wenn der Baum den Fokus hat. Vorher startete es dort wie ⌘⏎ sofort eine Session in der Gruppe der fokussierten Session.

## 1.5.0 (2026-09-16)

- Änderung: Die Einstellungen sind wie die F1-Hilfe in Bereiche gegliedert: Sessions, Claude, Darstellung, Baum und Kacheln, Tastenkürzel. Die Tastenkürzel-Liste ist zusätzlich in Navigation und Kacheln verwalten unterteilt.

## 1.4.0 (2026-09-16)

- Feature: Gruppen lassen sich per Herz als Favorit markieren. Eine favorisierte Gruppe bleibt im Baum stehen, auch wenn ihre letzte Session geschlossen wurde, und startet per Plus wieder eine neue.
- Feature: Neuer Bereich „Claude“ in den Einstellungen: Bypass-Modus erlauben, Startmodus, Modell und Effort für neu gestartete Sessions festlegen.

## 1.3.3 (2026-09-16)

- Änderung: ⌘W schließt nicht mehr Kadrell, sondern beendet und entfernt die fokussierte Session (mit Rückfrage).

## 1.3.2 (2026-09-16)

- Fix: Die Mikrofon-Freigabe gilt jetzt für die ganze App, macOS fragt nicht mehr in jeder Session neu.

## 1.3.1 (2026-09-16)

- Änderung: Die Hilfe (F1) ist nach Bereichen gegliedert: Sessions öffnen, Navigation, Kacheln verwalten, Fenster und App.

## 1.3.0 (2026-09-16)

- Feature: In den Einstellungen unter „Sessions" lässt sich wählen, ob die Kachel nach dem Beenden von Claude (zweimal ⌃C, /exit) stehen bleibt oder sich schließt. Fortsetzen geht danach weiter per Klick im Baum.

## 1.2.0 (2026-09-16)

- Änderung: Kacheln tragen immer ihre Gruppenfarbe in Kopfzeile, Rahmen und leicht im Hintergrund, die ausgewählte Kachel etwas kräftiger.

## 1.1.1 (2026-09-16)

- Fix: ⌘M legt das Fenster wieder im Dock ab, dazu gibt es jetzt das Menü „Fenster".

## 1.1.0 (2026-09-16)

- Änderung: Der Pfad einer Gruppe steht in der Seitenleiste klein und eingerückt unter dem Gruppennamen, bei wenig Platz werden die vorderen Ordner abgekürzt (`~/d/p/projekt`).
- Änderung: Stack-Zeilen zeigen den Pfad der Session jetzt standardmäßig, ebenfalls abgekürzt, wenn der Platz nicht reicht.
- Entfernt: Der Ordner einer Gruppe lässt sich nicht mehr ändern, für einen anderen Ordner eine neue Gruppe anlegen.
- Fix: F1 schließt die offene Hilfe wieder, nicht nur Esc.

## 1.0.0 (2026-09-16)

- Änderung: Kadrell startet Claude selbst statt als Hintergrund-Session. Claude arbeitet dadurch nicht mehr automatisch in einem eigenen Worktree.
- Änderung: Beim Beenden von Kadrell enden die Sessions. Beim nächsten Start geht es in jeder angezeigten Session mit dem bisherigen Verlauf weiter.
- Feature: Beenden (⌘Q, Fenster schließen, Dock, Abmelden) fragt nach, solange Claude läuft, und zeigt, welche Sessions gerade arbeiten. Erst nach der Bestätigung wird Claude sauber beendet.
- Änderung: Eine neue Session steht sofort im Baum und rechts, ohne Wartezeit.
- Änderung: Beendet sich Claude in einer Kachel, bleibt die Kachel stehen. Ein Klick darauf setzt die Session fort.
- Änderung: Schließen entfernt die Session nur aus Kadrell, die Konversation bleibt erhalten.
- Feature: Laufende Hintergrund-Sessions werden beim Start zur Übernahme angeboten.
- Entfernt: Sessions aus anderen Terminals erscheinen nicht mehr automatisch, ebenso das Angebot, doppelte Sessions zu löschen.

## 0.2.1 (2026-09-16)

- Fix: Der Rahmen einer Kachel unter der Maus wird nur noch minimal heller statt fast weiß.

## 0.2.0 (2026-09-15)

- Feature: Die Einstellungen (⌘,) zeigen jetzt die installierte Version.

## 0.1.0 (2026-09-15)

- Feature: Alle Claude-Hintergrund-Sessions als Baum links, Terminals rechts als Grid oder Stack.
- Feature: Sessions und Gruppen lassen sich per Ziehen umsortieren.
- Feature: Tastenkürzel nach tmux-Vorbild, in den Einstellungen frei belegbar.
- Feature: ⌘N merkt sich den zuletzt gewählten Ordner, ⌘⏎ startet im Ordner der fokussierten Session.
- Feature: Beim Start werden doppelte Sessions zum Löschen angeboten.
- Feature: Claude-Nutzung (5h, 7 Tage) in der Leiste.
- Fix: Geschlossene Sessions verschwinden sofort aus der Liste.
- Fix: Schnell hintereinander gestartete Sessions bleiben alle sichtbar.
- Fix: Claude darf in den Terminals aufs Mikrofon zugreifen.
