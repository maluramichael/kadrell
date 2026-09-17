# Changelog

Neueste Version oben.

## 1.44.0 (2026-09-17)

- Feature: Die Gruppierung nach Projekt lässt sich abschalten („GRP“ in der Leiste, ⌥G). Aus stehen alle Sessions in einer Liste, mit dem Projektnamen vor dem Titel.
- Feature: Die flache Liste hat ihre eigene Reihenfolge zum Ziehen. Umschalten zwischen gruppiert und flach sortiert nichts um, beide Reihenfolgen bleiben gespeichert.
- Änderung: Beim Sortieren nach Status steht bei gleichem Stand jetzt die neuere Session oben. Frisch gestartete tauchen oben auf und rutschen nach unten, sobald sie fertig sind.

## 1.43.0 (2026-09-17)

- Feature: Die Sprache lässt sich in den Einstellungen zwischen Deutsch, Englisch und der Sprache des Systems umschalten, sofort und ohne Neustart.
- Feature: Beim ersten Start stehen zwei Flaggen über „Erste Session starten“, damit die Sprache gleich passt.

## 1.42.0 (2026-09-17)

- Feature: Rechtsklick auf markierten Text in einer Session zeigt Kopieren und Einsetzen statt des Session-Menüs.

## 1.41.0 (2026-09-17)

- Feature: In der Modellauswahl steht jetzt auch Opus 4.8 mit 1M-Kontext.

## 1.40.3 (2026-09-17)

- Änderung: Ein Klick auf eine schon ausgewählte Session in der Seitenleiste nimmt sie wieder aus der Auswahl.

## 1.40.2 (2026-09-17)

- Fix: Wurde Kadrell während des Betriebs aktualisiert, hängt die App beim Wählen eines Hintergrundbilds oder Ordners nicht mehr. Stattdessen kommt der Hinweis, Kadrell neu zu starten.

## 1.40.1 (2026-09-17)

- Fix: Mit ⌘A markierte Sessions sind in der Seitenleiste sofort hervorgehoben, nicht erst nach einigen Sekunden.

## 1.40.0 (2026-09-17)

- Feature: Neue Einstellung „Verlauf zum Zurückscrollen“ (1.000 bis 50.000 Zeilen, Standard 10.000). Bisher hielt jedes Terminal nur 500 Zeilen, lange Claude-Antworten waren nach oben hin abgeschnitten. Gilt sofort auch für laufende Sessions.

## 1.39.0 (2026-09-17)

- Feature: Einstellung „Sessions dürfen andere Sessions steuern“. Ist sie aus, darf eine Session per `kadrell` nur Sessions ihrer eigenen Gruppe steuern und lesen.
- Feature: ⌘N bietet an, einen nicht existierenden Ordner anzulegen und dort zu starten (⌘⏎), statt stumm nichts zu tun, und zeigt an, solange Git-Repos gesucht werden.
- Feature: Die Statusleiste zeigt Tooltips zu jedem Modul (Layout, Nutzung, laufende Sessions und mehr).
- Feature: Systembenachrichtigung, wenn eine Session wartet oder fertig wird und du sie gerade nicht siehst. Ein Klick holt das Fenster und fokussiert die Session. Einstellbar: aus, nur wartet (Standard) oder wartet und fertig.
- Feature: Das Menüleisten-Icon listet wartende und ungesehen fertige Sessions als Menü, ein Klick springt zur Session. Titel und Dock-Badge zählen ungesehene fertige mit.
- Feature: Kadrell prüft beim Start und danach alle 24 Stunden, ob eine neuere Version bereitsteht, und zeigt das dezent in der Statusleiste mit Changelog-Zeilen und Download-Link. Abschaltbar unter „Nach Updates suchen“.
- Feature: Nach einem Update zeigt Kadrell einmalig, was neu ist, jederzeit erneut über den Menüpunkt „Was ist neu“.
- Feature: Fehlt claude, zeigt Kadrell den Installationsbefehl zum Kopieren und „Erneut prüfen“, ohne Neustart. Eine zu alte claude-Version blockiert nicht mehr, nur ein Hinweis in der Leiste.
- Feature: Der ⌘N-Dialog zeigt, mit welchem Modell und Rechte-Modus eine neue Session startet.
- Feature: Der Baum ist per Tastatur bedienbar: ←/→ klappt Gruppen, ⏎ öffnet, ⌫ schließt, ⌃⏎ zeigt das Kontextmenü, ⌥⌘↑/↓ tauscht mit dem Nachbarn. Für VoiceOver ist er eine echte Gliederung, und VoiceOver kündigt an, wenn eine Session wartet oder fertig ist.
- Feature: Status-Punkte zeigen bei „Farben nicht unterscheiden“ zusätzlich eine Form, und Animationen respektieren „Bewegung reduzieren“.
- Feature: Sichtbarer Tastaturfokus in Einstellungen, Rückfragen und „Gruppe bearbeiten“, die Farbwahl dort ist per Tastatur bedienbar, „Nicht mehr fragen“ ist eine echte Checkbox.
- Änderung: Kadrell erkennt selbst, aus welcher Session ein `kadrell`-Befehl kommt. Eine Umgebungsvariable reicht nicht mehr, um sich als andere Session auszugeben.
- Änderung: Terminal-Inhalte und Startbefehle stehen nicht mehr lesbar im Systemprotokoll. Hooks laufen nur noch, wenn sie dir gehören und nicht für andere beschreibbar sind.
- Änderung: Geschlossene Sessions merken sich ihre Konversations-Id (die letzten 50), zuletzt benutzte Ordner für „Neue Session“ verfallen nach 90 Tagen.
- Änderung: Weniger Last: Baum, Arbeitsfläche und Statusleiste zeichnen nicht, solange das Fenster verdeckt ist, Transcripts werden nur noch ab der letzten Stelle gelesen, und Shell- und Remote-Sessions lösen keine Transcript-Suche mehr aus.
- Änderung: Mehrere Sessions starten in Wellen statt strikt nacheinander.
- Änderung: Ohne Sessions führt der Leerzustand mit einem großen Knopf direkt zur ersten Session, statt beim ersten Start ungefragt die Hilfe zu öffnen. Ein einmaliger Tipp in der Leiste erklärt ⌘⏎ und die Bedeutung von Gelb.
- Änderung: ⌘N durchsucht beim ersten Mal nicht mehr das ganze Home-Verzeichnis, sondern übliche Projektordner.
- Änderung: Sekundärtext, Warte-Anzeige und Beschriftungen auf farbigen Feldern (Marke „neu“, Leiste) sind in allen Farbschemata gut lesbar, besonders in hellen. Das verändert die gedämpften Grautöne jedes Schemas leicht.
- Fix: Eine neue Session, die während einer Aktualisierung angelegt wird, verschwindet nicht mehr, und ihr Claude-Prozess läuft weiter. Auch Umbenennen oder Schließen wird nicht mehr von einer gleichzeitigen Aktualisierung rückgängig gemacht.
- Fix: Gestoppte und geschlossene Terminals geben ihren Speicher frei, und beim Stoppen enden auch die von Claude gestarteten Unterprozesse.
- Fix: Kadrell hängt beim Start nicht mehr, wenn das Shell-Profil einen Hintergrunddienst startet.
- Fix: Mit verlegtem Claude-Datenordner (`CLAUDE_CONFIG_DIR`) werden Konversationen nach dem Neustart wieder fortgesetzt.
- Fix: Eine liegengebliebene Datei eines alten Claude-Prozesses ordnet einer Kachel keine fremde Konversation mehr zu.
- Fix: Eine erste Nachricht, die mit einem Bindestrich beginnt, kann keine Claude-Optionen mehr einschleusen. Remote-Sessions mit Sonderzeichen im tmux-Namen oder Hostnamen führen keine fremden Befehle mehr aus.
- Fix: `kadrell send` mischt Text und Enter nicht mehr, wenn direkt danach ein weiterer `send` folgt.
- Fix: `kadrell` bricht nicht mehr mit „alte Version“ ab, wenn die App gerade erst startet, sondern wartet.
- Fix: Kaputte oder halb geschriebene Sessions- und Gruppen-Dateien werden nicht mehr still geleert, das Original bleibt als Sicherungskopie liegen und Kadrell meldet es.
- Fix: Ein fehlerhafter Ladevorgang räumt keine Gruppen mehr leer, und eine doppelte Session-Id bringt Kadrell nicht mehr zum Absturz.
- Fix: Die ⌘P-Suche reagiert bei vielen Terminals mit langem Verlauf nicht mehr träge.
- Fix: Startet Claude sofort mit Fehler, zeigt die Kachel Exit-Code und letzte Zeilen statt denselben Fehlversuch stumm zu wiederholen. Ein gelöschter Ordner zeigt „Ordner fehlt“.
- Fix: Der Übernahme-Dialog für Hintergrund-Sessions braucht ⌘⏎, sobald eine Session gerade arbeitet. Fehlermeldungen zeigen nur noch „OK“, wo Abbrechen nichts anderes täte.

## 1.38.0 (2026-09-17)

- Feature: Kadrell gibt es jetzt auch auf Englisch. Die Sprache folgt dem System: steht Deutsch in der Sprachliste, bleibt es deutsch, sonst Englisch. Einstellbar pro App unter Systemeinstellungen › Allgemein › Sprache & Region › Apps.

## 1.37.0 (2026-09-17)

- Feature: Neues Fenster (⌘⇧T) zeigt dieselben Sessions mit eigener Auswahl, eigenem Layout und Fokus, zum Beispiel für einen zweiten Monitor. Offene Fenster kommen beim Neustart wieder.
- Feature: Zeigt ein anderes Fenster dieselbe Session, steht dort „In anderem Fenster“. Ein Klick holt das Terminal herüber.
- Feature: Ein Zusatzfenster schließen (⌘⇧W oder roter Knopf) beendet keine Session.

## 1.36.0 (2026-09-17)

- Feature: Neues Layout „Frei“ wie in i3: an jeder Kachel legst du mit ⌃⌥⇧→ oder ⌃⌥⇧↓ fest, ob die nächste rechts oder unten entsteht, auch per Klick auf „TEILT“ in der Leiste.
- Feature: Neues Layout „Scrollen“ wie in niri: Spalten mit fester Breite (⌃⌥←/→ schaltet ⅓, ½, ⅔), die Fläche scrollt seitlich per Wischen oder ⇧ + Mausrad, die fokussierte Kachel rückt von selbst ins Bild.
- Feature: Zeittracking-Hooks erkennen den Worktree, in dem eine Session tatsächlich arbeitet, auch wenn sie im Hauptverzeichnis gestartet wurde. Wechselt die fokussierte Session den Worktree, feuert der Hook erneut, und der Kachelkopf zeigt dessen Branch.
- Feature: VoiceOver und Sprachsteuerung erkennen Baum, Statusleiste, Kacheln, Stack-Zeilen und Suche, samt Status und Aktionen wie Umbenennen und Schließen.
- Feature: Kadrell meldet, wenn die installierte Claude-CLI älter als die getestete Version ist.
- Feature: Ein leerer Erststart zeigt, wie viele Claude-Sessions gerade interaktiv in anderen Terminals laufen, und verweist auf den tmux-Import.
- Änderung: Deutlich weniger Hintergrundlast: Terminal-Text wird nur noch für die Suche gelesen statt zweimal pro Sekunde, und bei verstecktem oder inaktivem Fenster fragt Kadrell Sessions seltener ab.

## 1.35.0 (2026-09-17)

- Änderung: Wird eine Session fertig oder wartet auf dich, während du woanders bist (andere Session oder Kadrell im Hintergrund), bekommt sie im Baum einen hellen, fetten Titel und die Marke „neu“, auch wenn sie die fokussierte ist. Die Marke bleibt, bis du die Session anklickst, in ihr Terminal klickst oder sie fokussierst, und übersteht einen Neustart. Arbeitet die Session wieder, verschwindet sie.
- Entfernt: Fettschrift bei jeder neuen Ausgabe im Transcript, sie markierte auch laufende Sessions und fehlte an der fokussierten.

## 1.34.0 (2026-09-17)

- Feature: Neue Layouts „Haupt + Spalte“ (eine große Kachel links, der Rest rechts untereinander) und „Spirale“ (jede Kachel halbiert den Rest, wie bspwm). Das Layout-Symbol in der Leiste öffnet die Auswahl, ⌘L schaltet reihum.
- Feature: Trennlinien zwischen Kacheln lassen sich mit der Maus ziehen, Doppelklick verteilt wieder gleich. ⌃⌥-Pfeiltasten verschieben die Linie an der fokussierten Kachel um 5 %. Die Aufteilung gehört zum Layout: Terminals werden der Reihe nach eingefüllt, die Größen bleiben, auch wenn andere Sessions nachrücken.
- Feature: Im Grid legt „‹ SP ›“ in der Leiste die Spaltenzahl fest, unter 1 wählt Kadrell sie wieder automatisch.
- Änderung: Fokus bewegen und Kacheln tauschen mit ⌥- und ⌥⇧-Pfeilen folgen jetzt der tatsächlichen Lage der Kacheln.
- Fix: Reste von temporären Profilen werden beim nächsten Start weggeräumt.

## 1.33.0 (2026-09-17)

- Feature: Profile: `--profile <name>` startet eine weitere Kadrell-Instanz mit eigenen Sessions, Gruppen und Einstellungen. Ohne Namen gilt wie bisher das Standardprofil, bestehende Daten bleiben unverändert. Ein neues Profil übernimmt die Einstellungen des Standardprofils.
- Feature: „Neue Instanz mit temporärem Profil“ im App-Menü öffnet ein Wegwerfprofil, das beim Beenden spurlos verschwindet.
- Änderung: Nur noch eine Instanz pro Profil statt einer Instanz insgesamt. Das Fenster zeigt den Profilnamen im Titel.

## 1.32.2 (2026-09-16)

- Fix: Der Schieberegler für die UI-Größe übernimmt den Wert erst beim Loslassen, der Einstellungsdialog springt beim Ziehen nicht mehr mit.

## 1.32.1 (2026-09-16)

- Änderung: Zahlenwerte in den Einstellungen (Schriftgröße, Zeilenabstand, UI-Größe, Abstände, Deckkraft) haben Schieberegler statt langer Dropdowns, der Wert steht daneben.
- Änderung: Host-Gruppen im Baum tragen kein Server-Symbol mehr, nur ihre Sessions.

## 1.32.0 (2026-09-16)

- Feature: Neuer Bereich „Anpassen“ in den Einstellungen: Hintergrundbild für die Arbeitsfläche (wählen, hineinziehen oder Pfad eintippen) und Deckkraft der Kacheln von 0 bis 100 %. Unter 100 % scheint das Bild durch Kacheln und Terminals, der Text bleibt voll sichtbar.

## 1.31.0 (2026-09-16)

- Feature: Neue Einstellung „Abstand zwischen Kacheln“, von 0 bis 64 px.
- Änderung: Viel größere Bereiche in den Einstellungen: Terminal-Schrift 6 bis 72 pt, Zeilenabstand 50 bis 300 %, UI-Größe 50 bis 200 %, Innenabstand der Kacheln 0 bis 64 px.

## 1.30.0 (2026-09-16)

- Feature: Einstellungen gelten sofort, ohne Speichern. Esc oder „Fertig“ schließt den Dialog.
- Feature: Eigener Bereich „Auto-Modus“ in den Einstellungen: der Auto-Modus gilt jetzt standardmäßig für alle Sessions im Baum, nicht nur für die Auswahl. Umstellbar auf „nur die Auswahl im Baum“.

## 1.29.0 (2026-09-16)

- Feature: Sounds: leiser Ton, wenn eine Session auf dich wartet oder fertig wird, die du gerade nicht siehst. Dazu kurze Töne beim Öffnen und Entfernen von Sessions und beim Umschalten von AUTO und SYNC. Einstellbar unter „Sounds“ (aus, nur wartet und fertig, alle).
- Feature: Der Statuspunkt im Baum pulsiert zweimal, wenn eine Session zu warten beginnt, und blitzt kurz auf, wenn sie fertig ist.
- Feature: Neue Sessions gleiten im Baum ein, Kacheln blenden beim Öffnen ein und beim Entfernen aus.
- Feature: Der Kachelrahmen blendet beim Fokuswechsel weich über, AUTO, SYNC und SORT füllen sich beim Einschalten aus der Mitte, die Nutzungswerte zählen zum neuen Stand hoch.
- Feature: Beim Umsortieren per Ziehen rastet ein Force-Touch-Trackpad spürbar ein.

## 1.28.0 (2026-09-16)

- Feature: Remote-Sessions: ⌘⇧N oder „@“ in der Palette verbindet per ssh mit einem Host aus der ssh-Konfiguration (inklusive eingebundener Dateien) und hängt sich an dessen tmux. „@host:“ zeigt die tmux-Sessions des Hosts zur Auswahl oder legt eine neue an. Jeder Host bekommt eine eigene Gruppe mit Server-Symbol, mehrere Sessions pro Host sind möglich, nach einem Neustart wird neu verbunden.
- Änderung: Das „+“ einer Host-Gruppe öffnet die Session-Auswahl des Hosts. Schließen einer Remote-Kachel trennt nur die Verbindung, die tmux-Session auf dem Host läuft weiter.

## 1.27.1 (2026-09-16)

- Fix: Einstellungen ließen sich immer mit Esc schließen, auch wenn der Dialog nach einem Klick daneben oder einem Dropdown den Fokus verloren hatte. Vorher konnte die abgedunkelte Fläche liegen bleiben und alle Klicks blockieren.

## 1.27.0 (2026-09-16)

- Feature: ⌘N ist jetzt ein einziges Suchfeld: ein paar Buchstaben finden Gruppen, zuletzt benutzte Ordner und alle Git-Projekte unter dem Projektordner. Oft und kürzlich benutzte Ordner stehen oben.
- Feature: Pfade lassen sich abkürzen: `~/d/p/kad` findet `~/development/projects/kadrell`. Tab übernimmt den markierten Pfad, ein `/` danach zeigt seine Unterordner.
- Feature: Ordner aus dem Finder in den ⌘N-Dialog oder direkt in den Baum ziehen startet dort eine Session.
- Feature: Ganz oben in ⌘N stehen der Ordner des vordersten Finder-Fensters und ein Ordnerpfad aus der Zwischenablage. Für den Finder fragt macOS einmal nach der Erlaubnis.
- Feature: ⌘O im ⌘N-Dialog öffnet die Ordnerauswahl im Finder.
- Änderung: Der Startordner in den Einstellungen heißt jetzt Projektordner, lässt sich direkt tippen (mit Tab-Abkürzung) oder hineinziehen und ist die Wurzel für die Projektsuche.

## 1.26.2 (2026-09-16)

- Änderung: Dialoge (Einstellungen, neue Session, Gruppe bearbeiten, umbenennen) bestätigt jetzt ⏎ statt ⌘⏎. Rückfragen, die löschen oder beenden, brauchen weiter ⌘⏎.

## 1.26.1 (2026-09-16)

- Änderung: Im kompakten Baum stehen die Status-Punkte im Gruppenkopf nur noch bei eingeklappter Gruppe. Aufgeklappt zeigen die Zeilen den Status ja schon.

## 1.26.0 (2026-09-16)

- Änderung: Die Einstellungen sind jetzt eine Tabelle: jede Auswahl ist ein gleich breites Dropdown statt einer Reihe von Knöpfen, auch an/aus.
- Änderung: Die Tastenkürzel scrollen mit den übrigen Einstellungen statt in einem eigenen kleinen Bereich.
- Fix: In Dialogen bleibt die Fußleiste mit dem Knopf (z. B. Speichern) immer sichtbar, auch wenn der Inhalt scrollt.

## 1.25.1 (2026-09-16)

- Änderung: Ungelesene Sessions sind nur noch am fetten Titel erkennbar, der kleine Punkt direkt vor dem Titel ist weg.
- Fix: Nach dem Update auf 1.25.0 waren fast alle Sessions als ungelesen markiert. Jetzt zählt nur, was seit dem ersten Start neu dazukommt.

## 1.25.0 (2026-09-16)

- Feature: Terminal ohne Claude direkt aus einer Gruppe: ⌘ gedrückt halten und auf den +-Knopf einer Gruppe klicken öffnet statt einer Claude-Session ein Terminal, das Icon wechselt dabei zum Terminal-Symbol.
- Feature: Wartende Sessions sind jetzt überall sichtbar: Zahl im Dock-Icon, „N warten“ in der Leiste (klickbar wählt alle aus), Anzeige auch an eingeklappten Gruppen, in der Suche zuoberst, und das Dock meldet sich bei einer neu wartenden Session. Neues Kürzel springt zur nächsten wartenden Session.
- Feature: Menüleisten-Icon mit Kurzstatus. ⌘W und der rote Knopf schließen nur noch das Fenster, Kadrell läuft im Hintergrund weiter, ein Klick aufs Dock- oder Menüleisten-Icon holt es zurück.
- Feature: Ungelesen-Markierung: Sessions mit neuer Antwort seit dem letzten Blick zeigen ihren Titel fett mit Punkt, wie in Mail.
- Feature: Rechtsklick-Kontextmenü für Sessions an Baum-Zeile, Kachel und Stack-Zeile, das Menü Session bekommt Tastenkürzel.
- Änderung: Destruktive Rückfragen (Stoppen, Entfernen, Beenden) bestätigt man jetzt mit ⌘⏎ statt mit blankem ⏎, das verhindert versehentliches Löschen.
- Änderung: Schließen ohne Rückfrage läuft jetzt über ⌥ statt ⌘, damit ein ⌘-Klick zur Auswahl nicht mehr aus Versehen eine Session löscht.
- Fix: Die leere Arbeitsfläche zeigt jetzt den passenden Zustand: Laden, „noch keine Session“ mit Hinweis auf ⌘N, oder eine Fehlermeldung, wenn claude nicht gefunden wird.
- Fix: Gruppen mit eigenem Namen oder eigener Farbe verschwinden nicht mehr, wenn ihre letzte Session endet oder die Sessionliste kurz leer ist.
- Fix: Ein unerwartetes Ende des Terminal-Prozesses löscht keine Session mehr ungewollt.

## 1.24.0 (2026-09-16)

- Feature: Farbschemata für die ganze Oberfläche und die Terminals, dunkel und hell: Catppuccin Mocha und Latte, Monokai, Dracula, Nord, Gruvbox, One Dark, Solarized Dark und Light, GitHub Light. Auswahl in den Einstellungen unter Darstellung.

## 1.23.4 (2026-09-16)

- Fix: Im kompakten Baum sitzt die Trennlinie direkt unter der letzten Session einer Gruppe, ohne Lücke.

## 1.23.3 (2026-09-16)

- Fix: Terminals ohne Claude starten nach dem Beenden der App wieder in dem Ordner, in den zuletzt mit `cd` gewechselt wurde.

## 1.23.2 (2026-09-16)

- Fix: Beim Beenden mit ⌘Q startet die zuletzt beendete Session nicht mehr kurz neu.

## 1.23.1 (2026-09-16)

- Fix: Die Vorschau (⌥J/⌥K) zeigt bei einer noch nicht gestarteten Session nicht mehr endlos „Startet …“, sondern dass ⏎ oder ein Klick sie startet.

## 1.23.0 (2026-09-16)

- Feature: Klick auf die Farbe im Dialog „Gruppe bearbeiten“ klappt sieben Farbpaletten auf (Catppuccin, Tailwind, Nord, Dracula, Gruvbox, Rosé Pine, Okabe-Ito), ein Klick auf ein Kästchen übernimmt die Farbe.
- Änderung: Neue Gruppen bekommen reihum 14 statt 10 Farben.

## 1.22.2 (2026-09-16)

- Änderung: Terminals ohne Claude tragen im Baum ein kleines Monitor-Symbol vor dem Statuspunkt statt „Terminal ·“ im Namen.

## 1.22.1 (2026-09-16)

- Fix: Bei eingeschaltetem Sync sind alle Kacheln, die die Eingabe bekommen, sichtbar ausgewählt.

## 1.22.0 (2026-09-16)

- Feature: ⌘T öffnet ein Terminal ohne Claude im Ordner der fokussierten Session. `exit` schließt die Kachel wieder.

## 1.21.1 (2026-09-16)

- Fix: Sync-Eingabe schickt Tippen jetzt wirklich an alle offenen Kacheln statt mehrfach in die aktive.

## 1.21.0 (2026-09-16)

- Feature: ⌥O schaltet die Sortierung des Baums weiter (aus, A–Z, Status).
- Fix: Die gewählte Sortierung ordnet den Baum jetzt wirklich um, auch beim Blättern mit ⌥J und ⌥K.
- Änderung: Dialoge erscheinen mittig im Fenster und wachsen nach oben und unten.

## 1.20.0 (2026-09-16)

- Feature: Sync-Eingabe: Mit ⌥I oder Klick auf „SYNC“ oben links geht alles, was du tippst oder einfügst, an alle offenen Sessions gleichzeitig.

## 1.19.0 (2026-09-16)

- Feature: ⌘⇧B klappt alle Gruppen im Baum auf einmal zu, nochmal ⌘⇧B klappt alle wieder auf. Auch im Menü Ansicht.

## 1.18.0 (2026-09-16)

- Änderung: ⌘A wählt alle Sessions der Gruppen aus, in denen schon etwas ausgewählt ist. Nochmal ⌘A stellt die Auswahl davor wieder her.
- Feature: ⌘⇧A wählt alle Sessions über alle Gruppen aus, nochmal ⌘⇧A geht zurück zur Auswahl davor.

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
