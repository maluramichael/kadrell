# Ideen

Gesammelte Ideen, noch nicht umgesetzt. Vor der Umsetzung jeweils erst durchsprechen.

## Statusleiste unten, befüllbar über Extensions

- Unten im Fenster eine Statusleiste, deren Inhalt ausschließlich von Extensions kommt.
- Standard: leer. Ist nichts drin, wird die Leiste gar nicht angezeigt und nimmt keinen Platz weg.
- Sobald mindestens eine Extension etwas einträgt, erscheint sie.
- Braucht eine Erweiterung der Lua-Extension-API (Statusleisten-Einträge setzen und aktualisieren).

## Extension: System-Info

- Erste Extension für die Statusleiste.
- Zeigt auf einen Blick: Plattenbelegung, CPU-Auslastung.
- Nicht live, sondern schonend: seltenes Aktualisieren, kein Dauerpolling, das selbst Last erzeugt.
- Konfigurierbar, mit verschiedenen Darstellungen zur Auswahl:
  - kompakt: nur eine Gesamtzahl (Durchschnitt über alle Kerne)
  - detailliert: kleine Balken, einer pro Kern
- Für die Platte vermutlich analog: Zahl oder kleiner Balken.

## Arbeits-Indikator im Baum überarbeiten

- Bug: Der blaue Strich läuft oft weiter, obwohl Claude nichts mehr tut, etwa nach einem Abbruch in der Session.
  Der Zustand muss zuverlässig auf "fertig" zurückfallen.
- Die Animation ist zu schnell, sie soll deutlich langsamer und ruhiger werden.
- Statt des Strichs vielleicht ein blauer Punkt, der leicht wabert.
- Hinweis: Mit Version 1.71.0 wurde der pulsierende Punkt gerade durch den Strich ersetzt. Beim Umsetzen klären,
  was am alten Punkt gestört hat, damit der neue Punkt sich davon unterscheidet.

## Gruppe bearbeiten: auch den Ordner ändern

- Im Dialog zum Bearbeiten einer Gruppe lassen sich bisher nur Name und Farbe ändern, der Ordner nicht.
- Der Ordner soll dort ebenfalls änderbar sein.
- Offen: Was passiert mit Sessions, die schon in der Gruppe laufen? Ihr Arbeitsordner ist beim Start festgelegt.
  Vermutlich gilt der neue Ordner nur für neue Sessions, die laufenden bleiben, wo sie sind.

## Sessions in andere Gruppen verschieben

- Alles unterhalb einer Gruppe (Claude-Sessions, Terminal-Sessions usw.) in eine andere Gruppe verschieben.
- Per Drag and Drop im Baum.
- Per Rechtsklick: "Verschieben nach …" mit Auswahl der Zielgruppe. Das ist zugleich der Weg per Tastatur.
- Offen: Die Session behält ihren Arbeitsordner, auch wenn die Zielgruppe einen anderen hat. Vermutlich richtig so,
  aber bewusst entscheiden.

## Rechtsklickmenü für Gruppen

- Gruppen haben bisher kein Kontextmenü. Das fällt auf, weil Sessions eins haben.
- Naheliegende Einträge: Bearbeiten, neue Session in der Gruppe, Löschen.

## ⌃↩ nicht mehr als Kontextmenü-Taste

- ⌃↩ öffnet derzeit das Kontextmenü. Claude fordert aber selbst oft dazu auf, ⌃↩ zu drücken, und das geht dann nicht.
- ⌃↩ soll wieder an die Session durchgehen.
- Offen: Welche Taste öffnet stattdessen das Kontextmenü? Es muss per Tastatur erreichbar bleiben, am besten über
  das Prefix wie bei tmux.

## Einstellungen neu gestalten

- Fast so breit wie das Kadrell-Fenster (rund 99 %), statt der heutigen schmaleren Box in der Mitte.
- Zwei Spalten statt einer langen Liste von oben nach unten.
- Fährt zügig von oben herein, mit einer schönen Animation.
- Screenshot des heutigen Stands: `~/.claude/screenshots/kadrell/idee-settings-aktuell.png`

## Grundregel: Overlays folgen der Fenstergröße

- Bug: Wird das Fenster verkleinert oder vergrößert, bleiben Overlays in alter Größe und Position stehen.
  - Die Einstellungen ragen dann rechts über das Fenster hinaus
    (`~/.claude/screenshots/kadrell/idee-settings-ragt-nach-resize-raus.png`).
  - Der Dialog "Neue Session" sitzt nicht mehr mittig
    (`~/.claude/screenshots/kadrell/idee-neue-session-nach-resize-verrutscht.png`).
- Regel für alle Overlays, heutige und künftige: Größe und Position passen sich beim Ändern der Fenstergröße
  sofort an.
- Am besten zentral im gemeinsamen Overlay-Unterbau lösen, nicht pro Dialog, und die Regel in die Projekt-Doku
  aufnehmen.

## Lokale Modelle (Ollama, llama.cpp) und Schnellstart per Idee

Noch unscharf, erster Entwurf.

Zwei Teile: erst die lokalen Provider, dann die Zuordnung von Modellen zu Funktionen.

### Teil 1: Lokale Provider einrichten und steuern

- Neben OpenRouter, Claude und Pi auch Ollama und llama.cpp als Provider.
- Konfiguration: Pfad zum jeweiligen Tool bzw. wie es erreichbar ist.
- Kadrell steuert die lokalen Provider selbst: Modelle anstoßen und herunterladen.
- Modelllisten immer dynamisch: Angezeigt wird nur, was im Provider wirklich installiert ist. Hat Ollama drei
  Modelle, stehen da drei, nicht zwanzig.
- Über die Ollama-Suche weitere Modelle finden und nachladen.

### Teil 2: Lokale Modelle für Funktionen

- Einzelne Funktionen in Kadrell können ein lokales Modell nutzen, jede für sich an- und ausschaltbar.
- Pro Funktion einstellbar, welches Modell sie verwendet, ausgewählt aus den tatsächlich vorhandenen Modellen.
- Die erste Funktion dafür ist der Schnellstart unten.

### Erste Funktion: Idee rein, Session raus

- Tastenkürzel (etwa ⌘⇧N) öffnet ein Eingabefeld.
- Man tippt frei, z. B. "In RoboThomas habe ich die Idee, dass …".
- Ein kleines lokales Modell kennt die Historie: wann, wo und unter welchem Pfad Projekte geöffnet und Sessions
  gestartet wurden. Daraus erkennt es das gemeinte Projekt und den Ordner.
- Es antwortet schnell, Kadrell startet direkt eine neue Session in diesem Ordner, mit der Idee als erstem Prompt.

### Offen

- Wie sicher muss die Zuordnung sein? Bei Unsicherheit kurz das erkannte Projekt zur Bestätigung zeigen oder
  direkt starten?
- Reicht für die Projektzuordnung vielleicht schon eine einfache Suche über die Historie, und das Modell braucht es
  erst für unscharfe Fälle?
- Abgleich mit den Nicht-Zielen (kein eigener Daemon/Server): Ollama läuft als eigener Dienst. Ist das in Ordnung,
  solange Kadrell ihn nur nutzt und nicht selbst mitbringt?
