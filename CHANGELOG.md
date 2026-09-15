# Changelog

Neueste Version oben.

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
