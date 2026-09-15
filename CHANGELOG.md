# Changelog

Neueste Version oben.

## 1.0.0 (2026-09-16)

- Änderung: Kadrell startet Claude selbst statt als Hintergrund-Session. Claude arbeitet dadurch nicht mehr automatisch in einem eigenen Worktree.
- Änderung: Beim Beenden von Kadrell enden die Sessions. Beim nächsten Start geht es in jeder angezeigten Session mit dem bisherigen Verlauf weiter.
- Änderung: Eine neue Session steht sofort im Baum und rechts, ohne Wartezeit.
- Änderung: Beendet sich Claude in einer Kachel, bleibt die Kachel stehen. Ein Klick darauf setzt die Session fort.
- Änderung: Schließen entfernt die Session nur aus Kadrell, die Konversation bleibt erhalten.
- Feature: Laufende Hintergrund-Sessions werden beim Start zur Übernahme angeboten.
- Entfernt: Sessions aus anderen Terminals erscheinen nicht mehr automatisch, ebenso das Angebot, doppelte Sessions zu löschen.

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
