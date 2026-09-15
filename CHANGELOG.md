# Changelog

Neueste Version oben.

## 0.3.0 (2026-09-16)

- Änderung: Der Pfad einer Gruppe steht in der Seitenleiste klein und eingerückt unter dem Gruppennamen, bei wenig Platz werden die vorderen Ordner abgekürzt (`~/d/p/projekt`).
- Änderung: Stack-Zeilen zeigen den Pfad der Session jetzt standardmäßig, ebenfalls abgekürzt, wenn der Platz nicht reicht.
- Entfernt: Der Ordner einer Gruppe lässt sich nicht mehr ändern, für einen anderen Ordner eine neue Gruppe anlegen.

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
