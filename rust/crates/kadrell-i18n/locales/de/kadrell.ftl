# Erzeugt von rust/tools/xcstrings2ftl.py aus den xcstrings der Mac-App. Nicht von Hand ändern.

warten-neu = { " " } { $arg1 } warten · { $arg2 } neu

arbeitet-6cc684 = { " " } ARBEITET

wartet-auf-antwort = { " " } WARTET AUF ANTWORT

arbeitet-gerade = { " " }(arbeitet gerade)

sessions-1f3dd5 = { " " }· { $arg1 } Sessions

fenster-3ee0e6 = { " " }· Fenster { $arg1 }

nicht-vorhanden = { $arg1 } (nicht vorhanden)

existiert-nicht-mehr = { $arg1 } existiert nicht mehr

existiert-nicht-anlegen-geht-nur-im-home-ordner = { $arg1 } existiert nicht · anlegen geht nur im Home-Ordner

fertig-0919da = { $arg1 } fertig

findet-kein-passendes-projekt-zuletzt-benutzte = { $arg1 } findet kein passendes Projekt · zuletzt benutzte zuerst

gehoert-nicht-dir-oder-ist-fuer-andere = { $arg1 } gehört nicht dir oder ist für andere beschreibbar

laden-und-einschalten = { $arg1 } laden und einschalten

laeuft-ohne-sandbox-und-darf-auf-dateien = { $arg1 } läuft ohne Sandbox und darf auf Dateien, Netzwerk und Programme zugreifen. Wirklich einschalten?

nicht-erreichbar-eec24d = { $arg1 } nicht erreichbar

verfuegbar = { $arg1 } verfügbar

vermutet-bitte-pruefen = { $arg1 } vermutet, bitte prüfen

wartet-5b9fac = { $arg1 } wartet

zeigt-auf-diese-app-kadrell-help-listet-die = { $arg1 } zeigt auf diese App. `kadrell help` listet die Befehle. In Sessions dieser App steht der Pfad zusätzlich in $KADRELL.

verbraucht-nach-plan-bd7ef5 = { $arg1 }: { $arg2 } % verbraucht, { $arg3 } % nach Plan

verbraucht = { $arg1 }: { $arg2 } % verbraucht

verbraucht-nach-plan-15e25a = { $arg1 }: { $arg2 } % verbraucht, { $arg3 } % nach Plan

nutzung-nicht-abrufbar = { $arg1 }: Nutzung nicht abrufbar

claude-version-fehlgeschlagen = { $arg1 }: claude --version fehlgeschlagen: { $arg2 }

claude-version-liefert-kein-erkennbares = { $arg1 }: claude --version liefert kein erkennbares Versionsformat: { $arg2 }

claude-nicht-gefunden = { $arg1 }: claude nicht gefunden

llama-server-nicht-gefunden = { $arg1 }: llama-server nicht gefunden

ins-feld-modell-laden = { $arg1 }:{ $arg2 } ins Feld „Modell laden“

m-fe7e36 = { $arg1 } %

arbeiten-67c3cb =
    { $arg1 ->
         [one] { $arg1 } ARBEITET
        *[other] { $arg1 } ARBEITEN
    }

claude-prozess-e-werden-sauber-beendet-laufende = { $arg1 } Claude-Prozess(e) werden sauber beendet. Laufende Arbeit bricht dabei ab. Die Konversationen bleiben erhalten und werden beim nächsten Start fortgesetzt.

claude-sessions-laufen-interaktiv-in-anderen-7262f9 = { $arg1 } Claude-Sessions laufen interaktiv in anderen Terminals

claude-sessions-laufen-interaktiv-in-anderen-356d06 = { $arg1 } Claude-Sessions laufen interaktiv in anderen Terminals · Import: tools/tmux-dump.py

fehler-bec06c =
    { $arg1 ->
         [one] { $arg1 } FEHLER
        *[other] { $arg1 } FEHLER
    }

hintergrund-session-s-uebernehmen = { $arg1 } Hintergrund-Session(s) übernehmen?

prompts-2f48fc = { $arg1 } Prompts

sp = { $arg1 } SP

session-s-konnten-nicht-uebernommen-werden = { $arg1 } Session(s) konnten nicht übernommen werden

sessions-b904b0 = { $arg1 } Sessions

treffer = { $arg1 } Treffer

warten-0375a0 =
    { $arg1 ->
         [one] { $arg1 } WARTET
        *[other] { $arg1 } WARTEN
    }

zeilen-uebersprungen = { $arg1 } Zeilen übersprungen

gewaehlt = { $arg1 } gewählt

trennen = { $arg1 } trennen

warten-d4b148 = { $arg1 } warten

a = { $arg1 }A

f = { $arg1 }F

w = { $arg1 }W

an-gruppe = + an Gruppe

eingeklappt = , eingeklappt

sucht = / sucht · { $arg1 }

m-1-claude-session-laeuft-interaktiv-in-anderen-cb1c53 = 1 Claude-Session läuft interaktiv in anderen Terminals

m-1-claude-session-laeuft-interaktiv-in-anderen-d2db54 = 1 Claude-Session läuft interaktiv in anderen Terminals · Import: tools/tmux-dump.py

m-1-jahr = 1 Jahr

m-1-bis-4-oder-zeitraum-projekt-leertaste-filtert = 1 bis 4 oder ←/→ Zeitraum · ↑/↓ Projekt · Leertaste filtert · ⌫ alle · s sortiert

m-24-std = 24 Std.

m-3-abstuerze-in-2-min-bleibt-aus = 3 Abstürze in 2 min, bleibt aus

m-30-tage = 30 Tage

m-5-stunden = 5 Stunden

m-5-stunden-limit-in-der-leiste-bei-ultra-nie = 5-Stunden-Limit in der Leiste (bei Ultra nie)

m-7-tage = 7 Tage

m-7-tage-limit-in-der-leiste = 7-Tage-Limit in der Leiste

m-7-tage-limit-laeuft-zu-schnell-voll = 7-Tage-Limit läuft zu schnell voll

auto-sp = AUTO SP

auto-in-der-leiste = AUTO in der Leiste

abbrechen = Abbrechen

abstand-zwischen-kacheln = Abstand zwischen Kacheln

account-automatisch-gewechselt = Account automatisch gewechselt

adresse-des-ollama-dienstes = Adresse des Ollama-Dienstes

adresse-eines-laufenden-llama-server-optional = Adresse eines laufenden llama-server (optional)

adresse-ungueltig = Adresse ungültig

aktion-ohne-id-oder-label-uebersprungen = Aktion ohne id oder label übersprungen

aktives-konto = Aktives Konto

aktualisieren = Aktualisieren

aktuellen-account-hinzufuegen = Aktuellen Account hinzufügen

aktueller-account = Aktueller Account

alle-claude-sessions-durchsuchen-auch-beendete = Alle Claude-Sessions durchsuchen, auch beendete

alle-gruppen-auf-oder-zuklappen = Alle Gruppen auf- oder zuklappen

alle-gruppen-auf-zuklappen = Alle Gruppen auf-/zuklappen

alle-sessions-auswaehlen = Alle Sessions auswählen

alle-sessions-von = Alle Sessions von { $arg1 }

alle-sessions-zeigen = Alle Sessions zeigen

andere = Andere

anderes-projekt-suchen = Anderes Projekt suchen

angegebene-rechte-nicht-erzwungen = Angegebene Rechte: { $arg1 } (nicht erzwungen)

angemeldeten-account-hinzufuegen = Angemeldeten Account hinzufügen

anlegen = Anlegen

anpassen = Anpassen

ansicht = Ansicht

antwort-nicht-lesbar = Antwort nicht lesbar

antwortet-nicht-mehr-kein-pong-binnen-5-s = Antwortet nicht mehr (kein Pong binnen 5 s)

anzahl = Anzahl

arbeitsflaeche = Arbeitsfläche

arbeitsflaeche-tastatur-an-claude = Arbeitsfläche: Tastatur an Claude

archiv-liess-sich-nicht-entpacken = Archiv ließ sich nicht entpacken

auf-github-suchen-topic-kadrell-extension = Auf GitHub suchen (Topic kadrell-extension)

auf-ollama-com-suchen-webseite-keine-offizielle = Auf ollama.com suchen (Webseite, keine offizielle Schnittstelle)

aus-f0ce94 = Aus

aus-ansicht-nehmen = Aus Ansicht nehmen

aus-den-transcripts-auch-von-sessions = Aus den Transcripts, auch von Sessions außerhalb von Kadrell.

ausklappen = Ausklappen

ausschneiden = Ausschneiden

auswahl-verschieben-nach = Auswahl verschieben nach ({ $arg1 })

auto-modus = Auto-Modus

auto-modus-ein-aus = Auto-Modus ein/aus

auto-modus-kacheln-erscheinen-sobald-claude = Auto-Modus: Kacheln erscheinen, sobald Claude etwas von dir will

automatisch-wechseln-bei-limit = Automatisch wechseln bei Limit

beendet-klick-setzt-fort = BEENDET · KLICK SETZT FORT

baum-als-vorschau-durchblaettern-auswahl-bleibt = Baum als Vorschau durchblättern, Auswahl bleibt · ⏎ übernimmt, Esc zurück

baum-ein-aus = Baum ein/aus

baum-nach-projekt-gruppieren = Baum nach Projekt gruppieren

baum-sortieren-aus-a-z-status = Baum sortieren: aus → A–Z → Status

baum-und-kacheln = Baum und Kacheln

baum-tastatur-hierher-waehlt-session = Baum: Tastatur hierher, ↑↓ wählt Session

bearbeiten-84e45e = Bearbeiten

bearbeiten-92b34c = Bearbeiten …

beenden = Beenden

beendet-durch-signal = Beendet durch Signal { $arg1 }

beendet-mit-status = Beendet mit Status { $arg1 }

bei-diesem-tempo-ist-das-7-tage-limit-am-voll = Bei diesem Tempo ist das 7-Tage-Limit am { $arg1 } voll. Reset ist aber erst am { $arg2 }, bis dahin wärst du gesperrt.

bereich-seit-dem-letzten-klick-dazu = Bereich seit dem letzten Klick dazu

bild-hinter-baum-und-kacheln = Bild hinter Baum und Kacheln

bild-waehlen = Bild wählen

braucht-neueres-kadrell-api-unterstuetzt = Braucht neueres Kadrell (API { $arg1 }, unterstützt: { $arg2 })

bypass = Bypass

bypass-modus-erlauben-allow-dangerously-skip = Bypass-Modus erlauben (--allow-dangerously-skip-permissions)

claude = Claude

claude-cli-meldet-einen-fehler = Claude CLI meldet einen Fehler

claude-wird-beendet-und-die-kachel-entfernt-die = Claude wird beendet und die Kachel entfernt. Die Konversation bleibt erhalten: claude --resume { $arg1 }

claude-wird-beendet-die-kachel-bleibt-ein-klick = Claude wird beendet, die Kachel bleibt. Ein Klick setzt die Konversation fort.

claude-wird-in-allen-sessions-beendet-und-die = Claude wird in allen Sessions beendet und die Gruppe entfernt. Die Konversationen bleiben erhalten.

claude-code-sessions-als-terminals-baum-links = Claude-Code-Sessions als Terminals: Baum links, rechts als Grid oder Stack. Beim Beenden enden sie, beim nächsten Start geht es mit --resume weiter.

claude-nutzung-der-letzten-5-stunden = Claude-Nutzung der letzten 5 Stunden

claude-nutzung-der-letzten-7-tage = Claude-Nutzung der letzten 7 Tage

darstellung = Darstellung

das-transcript-von-ist-nicht-mehr-da = Das Transcript von „{ $arg1 }“ ist nicht mehr da.

datei = Datei

deckkraft-von-baum-und-kacheln = Deckkraft von Baum und Kacheln

den-ordner-gibt-es-nicht-mehr = Den Ordner { $arg1 } gibt es nicht mehr.

den-ordner-hat-auch-die-gruppe = Den Ordner hat auch die Gruppe „{ $arg1 }“.

der-dateiauswahl-dialog-oeffnet-sich-erst-nach = Der Dateiauswahl-Dialog öffnet sich erst nach einem Neustart von Kadrell. Pfade lassen sich weiter eintippen oder hineinziehen.

design-des-baums = Design des Baums

die-shell-und-alles-was-darin-laeuft-wird = Die Shell und alles, was darin läuft, wird beendet.

die-verbindung-wird-getrennt-die-tmux-session = Die Verbindung wird getrennt, die tmux-Session auf { $arg1 } läuft weiter.

doku = Doku

doku-oeffnen = Doku öffnen

download-ohne-abschluss-beendet = Download ohne Abschluss beendet

download-von-github-fehlgeschlagen = Download von GitHub fehlgeschlagen

einstellungen-09eaab = EINSTELLUNGEN

entdecken = ENTDECKEN

erster-prompt = ERSTER PROMPT

effort = Effort

eigene = Eigene

eigene-erweiterungen-in-lua = Eigene Erweiterungen in Lua:

eigene-farbe = Eigene Farbe

einklappen = Einklappen

einschalten = Einschalten

einsetzen = Einsetzen

einstellungen-8f24d4 = Einstellungen …

einstellungen-projektordner-darstellung = Einstellungen: Projektordner, Darstellung, Tastenkürzel

empfohlenes-modell = Empfohlenes Modell

entfernen-382837 = Entfernen

entfernen-ffcb43 = Entfernen …

enthaelt-lua-5-5-1-mit-lua-org-und-json-lua-mit = Enthält Lua 5.5.1 (MIT, lua.org) und json.lua (MIT, rxi) sowie Farbwerte des Schemas Instrument aus Monsters (Apache-2.0, Saturday Company)

erneut-pruefen = Erneut prüfen

erste-session-starten = Erste Session starten

esc-abbrechen = Esc abbrechen

esc-beendet-die-suche = Esc beendet die Suche

esc-oder-schliessen = Esc oder { $arg1 } schließen

esc-schliesst = Esc schließt

extensions-fe3436 = Extensions

extensions-dbcb25 = Extensions …

extensions-ordner-oeffnen = Extensions-Ordner öffnen

extensions-7b0999 = Extensions: { $arg1 }

extensions-an-und-abschalten-neu-laden = Extensions: an- und abschalten, neu laden, Einstellungen, Log

externer-editor-kommando-wie-im-terminal = Externer Editor (Kommando wie im Terminal)

flach = FLACH

fable = Fable

fable-kontingent = Fable-Kontingent

fable-kontingent-in-der-leiste = Fable-Kontingent in der Leiste

farbe-aus-einer-palette-waehlen = Farbe aus einer Palette wählen

farbe-waehlen = Farbe wählen

farbschema = Farbschema

favorit = Favorit

fehler-f6cc01 = Fehler

fehler-7d0a41 = Fehler: { $arg1 }

fenster-22c978 = Fenster

fenster-nur-verstecken-sessions-laufen-weiter = Fenster nur verstecken, Sessions laufen weiter

fenster-schliessen = Fenster schließen

fenster-schliessen-sessions-laufen-weiter = Fenster schließen, Sessions laufen weiter

fenster-und-app = Fenster und App

fenster-zurueckholen = Fenster zurückholen

fertig-25d52d = Fertig

fertige-sessions-automatisch-trennen-0-nie = Fertige Sessions automatisch trennen (0 = nie)

fertige-sessions-trennen = Fertige Sessions trennen

filtern = Filtern …

fokus-bewegen-im-stack-auf-und-zuklappen = Fokus bewegen · im Stack auf- und zuklappen

fokus-links = Fokus links

fokus-oben = Fokus oben

fokus-rechts = Fokus rechts

fokus-unten = Fokus unten

fokus-kachel-mit-nachbar-tauschen = Fokus-Kachel mit Nachbar tauschen

fokus-kachel-schliessen = Fokus-Kachel schließen

fokus-kachel-schliessen-esc-selbst-geht-an = Fokus-Kachel schließen (Esc selbst geht an Claude)

fokus-session-beenden-und-entfernen-mit = Fokus-Session beenden und entfernen, mit Rückfrage

fokus-session-umbenennen-auch-im-baum-claude = Fokus-Session umbenennen, auch im Baum · Claude überschreibt den Namen danach nicht mehr

fokussierte-session-verschieben-nach = Fokussierte Session verschieben nach: { $arg1 }

fortsetzen = Fortsetzen

frei-3b09ee = Frei

frei-teilung-an-der-fokus-kachel-klick-schaltet = Frei: Teilung an der Fokus-Kachel, Klick schaltet → ↓ AUTO (längere Seite)

frei-naechste-kachel-rechts = Frei: nächste Kachel rechts

frei-naechste-kachel-unten = Frei: nächste Kachel unten

gesamt = GESAMT

gruppe-bearbeiten = GRUPPE BEARBEITEN

ganze-gruppen-auswaehlen = Ganze Gruppen auswählen

gelb-heisst-claude-wartet-auf-dich-n-springt = Gelb heißt: Claude wartet auf dich. ⌥N springt zur nächsten wartenden Session.

gerade-wartet-keine-session = Gerade wartet keine Session

gestoppt = Gestoppt

gezaehlt-wird-solange-kadrell-laeuft = Gezählt wird, solange Kadrell läuft.

gilt-fuer = Gilt für

grid = Grid

gruppe-b4c022 = Gruppe

gruppe-sessions = Gruppe { $arg1 }, { $arg2 } Sessions

gruppe-1-session = Gruppe { $arg1 }, 1 Session

gruppe-bearbeiten-der-fokussierten-session = Gruppe bearbeiten (der fokussierten Session)

gruppe-schliessen-0b5167 = Gruppe schließen

gruppe-schliessen-f13a36 = Gruppe schließen …

gruppe-c99929 = Gruppe · { $arg1 }

gruppe-1c9b17 = Gruppe „{ $arg1 }“

gruppe-mit-session-s-schliessen = Gruppe „{ $arg1 }“ mit { $arg2 } Session(s) schließen?

gruppen-aus-alle-sessions-in-einer-flachen = Gruppen aus: alle Sessions in einer flachen Liste, mit eigener Handreihenfolge

gruppierung-nach-projekt = Gruppierung nach Projekt

groessen-des-layouts-aendern-doppelklick = Größen des Layouts ändern, Doppelklick verteilt gleich · Terminals füllen die Felder der Reihe nach

haupt-spalte = Haupt + Spalte

herunterladen = Herunterladen

hilfe-mit-allen-kuerzeln = Hilfe mit allen Kürzeln

hintergrund-sessions-uebernehmen = Hintergrund-Sessions übernehmen

hintergrundbild = Hintergrundbild

hinzufuegen = Hinzufügen

in-anderem-fenster-klick-holt-hierher = IN ANDEREM FENSTER · KLICK HOLT HIERHER

idee-tippen-kadrell-erkennt-das-projekt-und = Idee tippen, Kadrell erkennt das Projekt und startet dort eine Session mit der Idee als erstem Prompt

idee-tippen-z-b-in-kadrell-habe-ich-die-idee = Idee tippen, z. B. „In Kadrell habe ich die Idee, dass …“

im-baum-menue-der-session-bei-zugeklappter = Im Baum: Menü der Session, bei zugeklappter Gruppe das der Gruppe · ⌃⏎ geht an die Session

im-dock-ablegen = Im Dock ablegen

im-editor-oeffnen = Im Editor öffnen

im-finder-waehlen = Im Finder wählen

im-finder-waehlen-o = Im Finder wählen (⌘O)

im-finder-zeigen = Im Finder zeigen

im-fullscreen-starten-kein-doppelter-text-beim = Im Fullscreen starten (kein doppelter Text beim Scrollen)

im-terminal-suchen = Im Terminal suchen …

im-terminal-suchen-g-weiter = Im Terminal suchen, ⌘G weiter

in-allen-terminals-suchen-204468 = In allen Terminals suchen

in-allen-terminals-suchen-6a2679 = In allen Terminals suchen …

innenabstand-der-kacheln = Innenabstand der Kacheln

installation-fehlgeschlagen = Installation fehlgeschlagen

installationsbefehl-kopieren = Installationsbefehl kopieren

installieren = Installieren

installieren-fehlgeschlagen = Installieren fehlgeschlagen: { $arg1 }

jetzt-aktiv-grund-nutzungslimit-erreicht = Jetzt aktiv: „{ $arg1 }“. Grund: Nutzungslimit erreicht.

kachel = Kachel { $arg1 }

kachel-1-9-fokussieren = Kachel 1–9 fokussieren

kachel-bleibt-klick-setzt-fort = Kachel bleibt, Klick setzt fort

kachel-nach-links-tauschen = Kachel nach links tauschen

kachel-nach-oben-tauschen = Kachel nach oben tauschen

kachel-nach-rechts-tauschen = Kachel nach rechts tauschen

kachel-nach-unten-tauschen = Kachel nach unten tauschen

kachel-schliessen = Kachel schließen

kachel-waehlen = Kachel wählen

kacheln-verwalten = Kacheln verwalten

kachelwechsel = Kachelwechsel

kadrell-verfuegbar = Kadrell { $arg1 } verfügbar

kadrell-ausblenden = Kadrell ausblenden

kadrell-beenden-8d4202 = Kadrell beenden

kadrell-beenden-waehrend-claude-laeuft = Kadrell beenden, während Claude läuft

kadrell-beenden-c78937 = Kadrell beenden?

kadrell-beenden-session-s-arbeiten-gerade = Kadrell beenden? { $arg1 } Session(s) arbeiten gerade!

kadrell-neu-starten-5a0b36 = Kadrell neu starten

kadrell-neu-starten-f7c56e = Kadrell neu starten?

kadrell-neu-starten-session-s-arbeiten-gerade = Kadrell neu starten? { $arg1 } Session(s) arbeiten gerade!

kadrell-startet-claude-jetzt-selbst-statt-mit = Kadrell startet Claude jetzt selbst statt mit claude --bg. Diese Sessions werden mit claude stop angehalten (laufende Arbeit bricht ab) und hier fortgesetzt:

kadrell-wurde-seit-dem-start-ersetzt = Kadrell wurde seit dem Start ersetzt

kadrell-oeffnen = Kadrell öffnen

kein-bild = Kein Bild

kein-favorit = Kein Favorit

kein-ordner-gefunden-o-waehlt-im-finder = Kein Ordner gefunden. ⌘O wählt im Finder.

kein-projekt-gefunden = Kein Projekt gefunden.

kein-angemeldeter-claude-account-gefunden = Kein angemeldeter Claude-Account gefunden

kein-tmux-auf-diesem-host-normale-shell = Kein tmux auf diesem Host, normale Shell

keine-eintraege-gefunden = Keine Einträge gefunden.

keine-modelle-installiert = Keine Modelle installiert

keine-prompts-in-diesem-zeitraum = Keine Prompts in diesem Zeitraum.

keine-session-fokussiert = Keine Session fokussiert

keine-treffer = Keine Treffer

keine-wartenden-sessions = Keine wartenden Sessions

klassisch = Klassisch

klick-d1b731 = Klick

knoten-ohne-typ-uebersprungen = Knoten ohne Typ übersprungen

knoten-ohne-feld-uebersprungen = Knoten „{ $arg1 }“ ohne Feld „{ $arg2 }“ übersprungen

kommando = Kommando

kommando-laeuft-noch = Kommando läuft noch

kommandozeilen-tool-installieren = Kommandozeilen-Tool installieren …

kommandozeilen-tool-installiert = Kommandozeilen-Tool installiert

kompakt = Kompakt

konten = Konten

konten-verwalten = Konten verwalten …

konto-df8525 = Konto

konto-928bdd = Konto { $arg1 }

konto-hinzufuegen-aktuell-angemeldetes = Konto hinzufügen (aktuell angemeldetes)

konto-hinzufuegen = Konto hinzufügen …

konto-waehlen = Konto wählen

kopieren = Kopieren

kuerzel-anklicken-tasten-druecken-entfernt = Kürzel anklicken, Tasten drücken · ⌫ entfernt

limits = LIMITS

lade-sessions = Lade Sessions …

laden = Laden

laufende-claude-prozesse-sessions = Laufende Claude-Prozesse / Sessions

laufzeit-im-baum = Laufzeit im Baum

layout-a51190 = Layout

layout-frei-wie-i3-die-kachel-nach-der = Layout Frei (wie i3): die Kachel nach der fokussierten entsteht rechts bzw. unten

layout-wechseln-grid-haupt-spalte-spirale-frei = Layout wechseln: Grid → Haupt + Spalte → Spirale → Frei → Scrollen → Stack

layout-49fd56 = Layout: { $arg1 }

leer-lassen-wieder-der-titel-von-claude-code = Leer lassen: wieder der Titel von Claude Code ({ $arg1 })

letzte-antwort-von-claude-im-baum = Letzte Antwort von Claude im Baum

linie = Linie

log-l = Log  L

log-ausblenden-l = Log ausblenden  L

lokale-modelle-ollama = Lokale Modelle: Ollama

lokale-modelle-llama-cpp = Lokale Modelle: llama.cpp

laedt-neu = Lädt neu

laeuft = Läuft

loeschen = Löschen

mehr-spalten = Mehr Spalten

meiste-sessions-gleichzeitig = Meiste Sessions gleichzeitig

melde-dich-zuerst-in-einer-session-mit-claude = Melde dich zuerst in einer Session mit „claude“ an, dann versuch es erneut.

menue-der-session-oder-gruppe-verschieben-nach = Menü der Session oder Gruppe: Verschieben nach, Bearbeiten, Ordner, Schließen

menueleiste = Menüleiste

menueleisten-icon-dock = Menüleisten-Icon / Dock

mittelklick-im-baum = Mittelklick im Baum

modell-ff461e = Modell

modell-fuer-pi-statt-claude-im-schnellstart-mit = Modell für Pi statt Claude, im Schnellstart mit Tab umschalten

modell-laden-name-aus-der-bibliothek = Modell laden (Name aus der Bibliothek)

modell-b79aa1 = Modell: { $arg1 }

modelle-fuer-funktionen = Modelle für Funktionen

modellordner-mit-gguf-dateien-optional = Modellordner mit GGUF-Dateien (optional)

neue-gruppe-0b6e85 = NEUE GRUPPE

neue-session-884c7f = NEUE SESSION

nach-updates-suchen = Nach Updates suchen

nach-wochentempo-verteilen = Nach Wochentempo verteilen

nachricht-zu-gross-ueber-1-mb = Nachricht zu groß (über 1 MB)

nachrichten-geschickt = Nachrichten geschickt

name = Name

name-ist-schon-von-einer-anderen-extension = Name { $arg1 } ist schon von einer anderen Extension belegt, erst entfernen

name-im-manifest-passt-nicht-zum-ordnernamen = Name im Manifest („{ $arg1 }“) passt nicht zum Ordnernamen

navigation = Navigation

neu-in = Neu in { $arg1 }

neu-laden = Neu laden

neu-laden-r = Neu laden  R

neu-starten = Neu starten

neu-197517 = Neu: { $arg1 }

neue-gruppe-leer = Neue Gruppe (leer)

neue-gruppe-60a101 = Neue Gruppe …

neue-instanz-mit-temporaerem-profil = Neue Instanz mit temporärem Profil

neue-modelle-einmal-im-terminal-mit-llama = Neue Modelle: einmal im Terminal mit llama-server -hf <Repo> laden (landet im Cache) oder GGUF-Dateien in den Modellordner legen

neue-session-e0d5a1 = Neue Session

neue-session-hier = Neue Session hier

neue-session-im-ordner-der-fokussierten = Neue Session im Ordner der fokussierten

neue-session-im-selben-ordner = Neue Session im selben Ordner

neue-session-in-dieser-gruppe = Neue Session in dieser Gruppe

neue-session-starten-n = Neue Session starten  ⌘N

neue-session-projekt-suchen-d-p-abkuerzen-o = Neue Session: Projekt suchen, ~/d/p abkürzen, ⌘O Finder, Ordner hineinziehen

neue-sessions-starten-im-neuen-ordner-laufende = Neue Sessions starten im neuen Ordner, laufende bleiben in ihrem.

neue-leere-gruppe-mit-name-ordner-und-farbe = Neue leere Gruppe mit Name, Ordner und Farbe

neuen-ordner-anlegen = Neuen Ordner „{ $arg1 }“ anlegen

neues-fenster = Neues Fenster

neues-fenster-eigene-auswahl-und-eigenes-layout = Neues Fenster: eigene Auswahl und eigenes Layout, dieselben Sessions

neues-terminal-in-dieser-gruppe = Neues Terminal in dieser Gruppe

neues-terminal-ohne-claude = Neues Terminal ohne Claude

nicht-mehr-anzeigen = Nicht mehr anzeigen

nicht-mehr-fragen = Nicht mehr fragen

nichts-gefunden-c4323e = Nichts gefunden.

nimmt-keine-nachrichten-mehr-an-ueber-1-mb = Nimmt keine Nachrichten mehr an (über 1 MB unverarbeitet)

noch-keine-extensions-ordner-hineinlegen-oder = Noch keine Extensions. Ordner hineinlegen oder per git clone holen, sie erscheinen sofort.

noch-keine-logzeilen = Noch keine Logzeilen.

noch-keine-messwerte-in-diesem-zeitraum = Noch keine Messwerte in diesem Zeitraum.

noch-keine-session = Noch keine Session

noch-nichts-gezaehlt = Noch nichts gezählt.

nur-diese-zeigen = Nur diese zeigen

naechste-vorige-kachel = Nächste / vorige Kachel

naechste-kachel = Nächste Kachel

naechste-kachel-teilt = Nächste Kachel teilt

naechste-wartende-session = Nächste wartende Session

ok = OK

ordner-fehlt = ORDNER FEHLT · { $arg1 }

ollama-nutzen = Ollama nutzen

ollama-starten-app-oder-ollama-serve-kadrell = Ollama starten (App oder ollama serve), Kadrell bringt keinen eigenen Dienst mit

ordner-d-p-kad-kuerzt-ab = Ordner (~/d/p/kad kürzt ab)

ordner-der-fokus-session-im-editor-aus-den = Ordner der Fokus-Session im Editor aus den Einstellungen öffnen

ordner-im-finder-waehlen = Ordner im Finder wählen

ordner-im-externen-editor-oeffnen = Ordner im externen Editor öffnen

ordner-nicht-angelegt = Ordner nicht angelegt: { $arg1 }

ordner-nicht-gefunden-tab-vervollstaendigt-o = Ordner nicht gefunden · Tab vervollständigt, ⌘O wählt im Finder

ordner-oeffnen = Ordner öffnen

originaltext-tab-ohne-projektverweis = Originaltext · Tab: ohne Projektverweis

prompts-b0f562 = PROMPTS

panel-zu-gross-knoten-hoechstens = Panel zu groß ({ $arg1 } Knoten, höchstens { $arg2 })

pfad-in-stack-zeilen = Pfad in Stack-Zeilen

pfad-kopieren = Pfad kopieren

pfad-zu-llama-server-leer-automatisch = Pfad zu llama-server (leer = automatisch)

pi-ist-nicht-installiert-pi-im-path-oder-unter = Pi ist nicht installiert (pi im PATH oder unter ~/.local/bin)

pi-sessions = Pi-Sessions

projekt-suchen-oder-pfad-tippen-d-p-kad = Projekt suchen oder Pfad tippen (~/d/p/kad)

projektordner-fuer-n-nach-git-repos-durchsucht = Projektordner für ⌘N (nach Git-Repos durchsucht)

projektverweis-aus-dem-ersten-prompt-entfernen = Projektverweis aus dem ersten Prompt entfernen, wenn sicher

projektverweis-entfernt-tab-originaltext = Projektverweis entfernt · Tab: Originaltext

prompts-5f2e66 = Prompts

prozent = Prozent

rechte-sidebar-der-extensions-eintrag-tab-erste = Rechte Sidebar der Extensions: ↑↓ Eintrag, ←→ Tab, ⏎ erste Aktion, ⌥⏎ alle Aktionen, Esc zurück

rechte-sidebar-ein-aus = Rechte Sidebar ein/aus

rechte-sidebar-tastatur-hierher-waehlt-eintrag = Rechte Sidebar: Tastatur hierher, ↑↓ wählt Eintrag, ←→ Tab

rechte = Rechte: { $arg1 }

rechtsklick-im-baum = Rechtsklick im Baum

reihe = Reihe

reload = Reload

remote-verbinden-ssh-tmux = Remote verbinden (ssh → tmux)

remote-verbinden = Remote verbinden …

remote-session-waehlen = Remote-Session wählen …

repo-enthaelt-keine-extension-kadrell-json-fehlt = Repo enthält keine Extension (kadrell.json fehlt)

repo-ist-zu-gross-ueber-mb = Repo ist zu groß (über { $arg1 } MB)

reset = Reset { $arg1 } ({ $arg2 })

rundes-x = Rundes X

rueckfragen = Rückfragen

rueckwaerts-suchen = Rückwärts suchen

schnellstart = SCHNELLSTART

session-umbenennen-399a7e = SESSION UMBENENNEN

start-fehlgeschlagen-exit = START FEHLGESCHLAGEN · EXIT { $arg1 }

starten-mit = STARTEN MIT

startet-7ec244 = STARTET …

schliessen = Schließen

schliessen-ohne-rueckfrage = Schließen ohne Rückfrage

schnellstart-idee-rein-session-raus = Schnellstart: Idee rein, Session raus

scrollen = Scrollen

scrollen-spalten-verschieben-maus-rad-der-fokus = Scrollen: Spalten verschieben (Maus: ⇧ + Rad), der Fokus rückt von selbst ins Bild

seit = Seit

seitlich-wischen = Seitlich wischen

session-6959b4 = Session

session-d7b46c = Session { $arg1 }

session-beenden-und-entfernen = Session beenden und entfernen

session-dazu-oder-weg = Session dazu oder weg

session-fortsetzen-fokussierte = Session fortsetzen (fokussierte)

session-im-baum-waehlen = Session im Baum wählen

session-laesst-sich-nicht-fortsetzen = Session lässt sich nicht fortsetzen

session-oder-gruppe-umsortieren-baum-und-dc767a = Session oder Gruppe umsortieren, Baum und Kacheln gleich

session-oder-gruppe-umsortieren-baum-und-bab36d = Session oder Gruppe umsortieren, Baum und Kacheln gleich · Session im Baum auf eine andere Gruppe: dorthin verschieben

session-schliessen = Session schließen

session-schliessen-fokussierte = Session schließen (fokussierte)

session-schliessen-auf-gruppe-die-ganze-gruppe = Session schließen · auf Gruppe: die ganze Gruppe · mit ⌥ ohne Rückfrage

session-stoppen-9a52bc = Session stoppen

session-stoppen-fokussierte = Session stoppen (fokussierte)

session-umbenennen-b4e8d8 = Session umbenennen

session-umbenennen-fokussierte = Session umbenennen (fokussierte)

session-stoppen-54b8de = Session „{ $arg1 }“ stoppen?

session-gruppe-pfad-suchen-kommandos-in-allen = Session, Gruppe, Pfad suchen …  ( > Kommandos, / in allen Terminals, # alle Claude-Sessions, @ Remote )

sessions-6fa3cb = Sessions

sessions-duerfen-andere-sessions-steuern = Sessions dürfen andere Sessions steuern (kadrell send, capture, kill)

sessions-geoeffnet = Sessions geöffnet

sessions-koennen-nicht-geladen-werden = Sessions können nicht geladen werden

sessions-oeffnen = Sessions öffnen

sessiontitel = Sessiontitel

sortiert = Sortiert:

sortierung = Sortierung

sounds = Sounds

spalten-im-grid-unter-1-wieder-automatisch = Spalten im Grid, unter 1 wieder automatisch

speichern = Speichern

spirale = Spirale

sprache = Sprache

sprache-des-systems = Sprache des Systems

springt-zur-naechsten-session-die-auf-dich = Springt zur nächsten Session, die auf dich wartet

stack = Stack

stand-von-vor-min = Stand von vor { $arg1 } min

standard = Standard

standard-von-pi = Standard von Pi

standardmodell = Standardmodell

starten = Starten

startet-f4522a = Startet

startet-mit = Startet mit: { $arg1 }

startet-nicht-kein-ready-binnen-3-s = Startet nicht (kein Ready binnen 3 s)

startmodus = Startmodus

statistik = Statistik

statistik-limits-prompts-und-zaehler = Statistik: Limits, Prompts und Zähler

status = Status

statusleiste-0140bb = Statusleiste

statusleiste-der-extensions = Statusleiste der Extensions

statusleiste-699106 = Statusleiste: { $arg1 } · { $arg2 }

statusleiste-eintrag-ohne-id-uebersprungen = Statusleiste: Eintrag ohne „id“ übersprungen

statusleiste-eintrag-ohne-inhalt-uebersprungen = Statusleiste: Eintrag „{ $arg1 }“ ohne Inhalt übersprungen

statusleiste-hoechstens-eintraege-der-rest-fehlt = Statusleiste: höchstens { $arg1 } Einträge, der Rest fehlt

statusleiste-id-doppelt-uebersprungen = Statusleiste: id „{ $arg1 }“ doppelt, übersprungen

stift = Stift

stoppen = Stoppen

suche = Suche

suche-git-repos-unter = Suche Git-Repos unter ~ …

suche-fehlgeschlagen = Suche fehlgeschlagen: { $arg1 }

suche-mit-kommandos = Suche, mit > Kommandos

suchen = Suchen

sucht-in-titeln-und-eigenen-prompts-aller = Sucht in Titeln und eigenen Prompts aller bisherigen Claude-Sessions · ⏎ setzt sie im alten Ordner fort oder springt zur laufenden

super = Super

symbol-und-text = Symbol und Text

sync-eingabe-an-alle-kacheln = Sync: Eingabe an alle Kacheln

sync-tippen-und-v-gehen-an-alle-offenen-kacheln = Sync: Tippen und ⌘V gehen an alle offenen Kacheln gleichzeitig · Badge „SYNC“

systembenachrichtigungen = Systembenachrichtigungen

teilt-auto = TEILT AUTO

teilt-in-der-leiste = TEILT in der Leiste

teilt-554b3b = TEILT →

teilt-494a75 = TEILT ↓

tab-vervollstaendigt-o-finder-ordner = Tab vervollständigt · ⌘O Finder · Ordner hineinziehen

tasten-druecken = Tasten drücken …

tastenkuerzel = Tastenkürzel

terminal-auf-der-gpu-zeichnen-metal = Terminal auf der GPU zeichnen (Metal)

terminal-ohne-claude-im-ordner-der-fokussierten = Terminal ohne Claude im Ordner der fokussierten

terminal-schrift = Terminal-Schrift

terminal-schrift-standardgroesse = Terminal-Schrift Standardgröße

terminal-schrift-aller-sessions-groesser-kleiner = Terminal-Schrift aller Sessions größer/kleiner

terminal-schrift-groesser = Terminal-Schrift größer

terminal-schrift-kleiner = Terminal-Schrift kleiner

terminal-schriftgroesse-0-mausrad = Terminal-Schriftgröße  ⌘+ ⌘- ⌘0  ⌘ Mausrad

terminals-geoeffnet = Terminals geöffnet

tipp = Tipp

tippen-filtert = Tippen filtert

transcripts-werden-gelesen = Transcripts werden gelesen …

trennlinie-an-der-fokus-kachel-um-5-verschieben = Trennlinie an der Fokus-Kachel um 5 % verschieben · Scrollen: Spalte ⅓ ½ ⅔ breit

trennlinie-nach-links = Trennlinie nach links

trennlinie-nach-oben = Trennlinie nach oben

trennlinie-nach-rechts = Trennlinie nach rechts

trennlinie-nach-unten = Trennlinie nach unten

trennlinie-ziehen = Trennlinie ziehen

trennlinien-zuruecksetzen-gleich-verteilt = Trennlinien zurücksetzen (gleich verteilt)

ui-groesse = UI-Größe

umbenennen = Umbenennen

umschalten-ab-auslastung = Umschalten ab Auslastung

unbekannte-farbe-ignoriert-erlaubt-accent-muted = Unbekannte Farbe ignoriert (erlaubt: accent, muted, ok, warn, err)

unbekannter-knotentyp-uebersprungen = Unbekannter Knotentyp „{ $arg1 }“ übersprungen

update-verfuegbar = Update verfügbar

updates = Updates

vorschau-nicht-gestartet-oder-klick-startet = VORSCHAU · NICHT GESTARTET · ⏎ ODER KLICK STARTET

verbraucht-in-prozent = Verbraucht in Prozent

verlauf-wird-eingelesen = Verlauf wird eingelesen …

verlauf-zum-zurueckscrollen = Verlauf zum Zurückscrollen

verschieben-nach = Verschieben nach

vorige-kachel = Vorige Kachel

vorschau-naechste-session-im-baum = Vorschau: nächste Session im Baum

vorschau-vorige-session-im-baum = Vorschau: vorige Session im Baum

warnen-bei-zu-schnellem-7-tage-verbrauch = Warnen bei zu schnellem 7-Tage-Verbrauch

wartende-sessions = Wartende Sessions

was-ist-neu = Was ist neu

wechsel-meldung-anzeigen = Wechsel-Meldung anzeigen

wechseln = Wechseln

weiter = Weiter

weitersuchen = Weitersuchen

weniger-spalten = Weniger Spalten

wenn-claude-endet-zweimal-c-exit = Wenn Claude endet (zweimal ⌃C, /exit)

wieder-einschalten-einstellungen-rueckfragen = Wieder einschalten: Einstellungen (⌘,) › Rückfragen

wirklich-loeschen = Wirklich löschen

waehlen-eda09c = Wählen

zeigt = Zeigt

zeilen = Zeilen

zeilenabstand = Zeilenabstand

zeit = Zeit

ziehen = Ziehen

zoom-aufheben = Zoom aufheben

zoom-ein-aus-fokussierte = Zoom ein/aus (fokussierte)

zoom-fokus-kachel-allein = Zoom: Fokus-Kachel allein

zoomen = Zoomen

zu-konto-wechseln = Zu Konto wechseln: { $arg1 }

zu-viele-nachrichten-mehr-als-50-pro-sekunde = Zu viele Nachrichten (mehr als 50 pro Sekunde)

zuletzt = Zuletzt

zuletzt-fokussierte-kachel = Zuletzt fokussierte Kachel

zurueck-auf = Zurück auf { $arg1 }

zwischenablage = Zwischenablage

aktiv = aktiv

alle = alle

alle-kuerzel = alle Kürzel

alle-sessions = alle Sessions

alle-sessions-nochmal-zurueck = alle Sessions, nochmal: zurück

an = an

an-kachel-und-baum-zeile-umbenennen = an Kachel und Baum-Zeile: umbenennen

arbeiten-f1ce4c = arbeiten

arbeitende = arbeitende

arbeitet-4d00bc = arbeitet

aus-2337c3 = aus

aus-ohne-nachfrage-ausfuehren = aus = ohne Nachfrage ausführen

ausgewaehlt = ausgewählt

automatisch = automatisch

beendet = beendet

beim-ersten-mal-einmalig-alle-transcripts = beim ersten Mal einmalig alle Transcripts, danach nur neue

benutzter-ordner = benutzter Ordner

bis = bis { $arg1 }

claude-aelter-als-die-von-kadrell-getestete-6f465f = claude { $arg1 }: älter als die von Kadrell getestete Version { $arg2 }, bitte aktualisieren

claude-aelter-als-die-von-kadrell-getestete-1afb80 = claude { $arg1 }: älter als die von Kadrell getestete Version { $arg2 }, bitte mit „{ $arg3 }“ aktualisieren

claude-alt-claude-update = claude alt · claude update

diese-hilfe = diese Hilfe

erkannt-am-namen-im-text = erkannt am Namen im Text

erkannt-von = erkannt von { $arg1 }

erkannt = erkannt: { $arg1 } · { $arg2 }

fertig-fdc275 = fertig

frage = frage { $arg1 } …

fragt-nach-rechten = fragt nach Rechten

frei-a03d4e = frei

ganze-gruppen-der-auswahl-nochmal-zurueck = ganze Gruppen der Auswahl, nochmal: zurück

gerade-eben = gerade eben

gesetzt = gesetzt

getrennt = getrennt

gezeigt-wird-nur-was-installiert-ist = gezeigt wird nur, was installiert ist

gibt-neuen-claude-sessions-nach-dem-ersten = gibt neuen Claude-Sessions nach dem ersten Prompt einen kurzen Titel, startet dafür bei Bedarf Ollama

gilt-fuer-neu-gestartete-claude-prozesse = gilt für neu gestartete Claude-Prozesse

init-lua-fehlt = init.lua fehlt

ist-fertig = ist fertig

je-funktion-an-aus-und-modell-aus-den = je Funktion an/aus und Modell aus den vorhandenen

kadrell-json-fehlt-oder-ist-kaputt = kadrell.json fehlt oder ist kaputt

kein-bild-datei-hineinziehen = kein Bild · Datei hineinziehen

kein-modell-titel-von-claude-code = kein Modell (Titel von Claude Code)

kein-modell-nur-claude = kein Modell (nur Claude)

kein-modell-nur-namenssuche = kein Modell (nur Namenssuche)

kein-modell-gewaehlt = kein Modell gewählt

kein-name-im-text-hilft-beim-zuordnen = kein Name im Text · { $arg1 } hilft beim Zuordnen

kein-name-im-text-ohne-lokales-modell-zuletzt = kein Name im Text · ohne lokales Modell zuletzt benutzte zuerst

kein-projekt-im-text-erkannt-zuletzt-benutzte = kein Projekt im Text erkannt · zuletzt benutzte zuerst

kein-tmux-server-auf = kein tmux-Server auf { $arg1 }

keine = keine

keine-antwort-in-der-zeit = keine Antwort in der Zeit

leer-kadrell-startet-ihn = leer = Kadrell startet ihn

llama-server-startet-nicht = llama-server startet nicht

llama-cpp = llama.cpp

llama-cpp-installieren-brew-install-llama-cpp = llama.cpp installieren (brew install llama.cpp) oder den Pfad setzen

llama-cpp-nutzen = llama.cpp nutzen

laedt = lädt …

mehrere-claude-accounts-umschalten-ohne-neues = mehrere Claude-Accounts, Umschalten ohne neues Login

mehrere-projekte-passen-zum-namen = mehrere Projekte passen zum Namen

min = min

mit-fehler = mit Fehler

neu-b1688c = neu

neue-session-in-der-gruppe-mit-terminal-ohne = neue Session in der Gruppe · mit ⌘: Terminal ohne Claude

neuer-wert = neuer Wert

nicht-erreichbar-92f2c4 = nicht erreichbar: { $arg1 }

nicht-gefunden = nicht gefunden

nicht-gesetzt = nicht gesetzt

nicht-gestartet = nicht gestartet

nichts-gefunden-eb11e0 = nichts gefunden

nur-symbol = nur Symbol

nur-der-cache-von-llama-cpp = nur der Cache von llama.cpp

nur-die-auswahl-im-baum = nur die Auswahl im Baum

nur-diese-session-zeigen-auf-gruppe-alle-ihre = nur diese Session zeigen · auf Gruppe: alle ihre Sessions

nur-wartet = nur wartet

nur-wartet-und-fertig = nur wartet und fertig

ohne-adresse-startet-kadrell-llama-server = ohne Adresse startet Kadrell llama-server selbst, er endet mit Kadrell

ordnet-die-idee-einem-projekt-zu-wenn-der-name = ordnet die Idee einem Projekt zu, wenn der Name im Text nicht eindeutig ist

prueft = prüft …

rechts = rechts

ssh-tmux-attach-waehlt-die-session = ssh · ⏎ tmux attach · „{ $arg1 }:“ wählt die Session

startet-e85bbb = startet …

sucht-projekt-oder-ordner = sucht Projekt oder Ordner

tmux-ls-auf = tmux ls auf { $arg1 }

tmux-new-as-auf = tmux new -As auf { $arg1 }

tmux-session-auf = tmux-Session auf { $arg1 }

ungueltige-nachricht-verworfen = ungültige Nachricht verworfen

unten = unten

von-kadrell-gestartet = von Kadrell gestartet · { $arg1 }

von-michael-malura = von Michael Malura ·

vor-min = vor { $arg1 } min

warten-227835 = warten

wartende = wartende

wartende-und-arbeitende = wartende und arbeitende

wartet-542760 = wartet

wartet-auf-dich = wartet auf dich

wartet-und-fertig = wartet und fertig

waehlen-e11cb9 = wählen

z-b-code = z. B. code

z-b-qwen3 = z. B. qwen3

zweite-session-im-selben-ordner = zweite Session im selben Ordner

aeltere-claude-version = Ältere claude-Version

aenderungen-gelten-sofort-sucht-esc-schliesst = Änderungen gelten sofort · / sucht · Esc schließt

aenderungen-gelten-sofort-esc-schliesst = Änderungen gelten sofort · Esc schließt

ueber-kadrell = Über Kadrell

uebernehmen = Übernehmen

aehnlicher-name-im-text-bitte-pruefen = ähnlicher Name im Text, bitte prüfen

aendern = ändern

uebersprungen = übersprungen

beenden-und-entfernen = „{ $arg1 }“ beenden und entfernen?

laeuft-noch-ausserhalb-dieses-kadrell-pid-erst = „{ $arg1 }“ läuft noch außerhalb dieses Kadrell (pid { $arg2 }). Erst dort beenden.

neu-versucht-es-trotzdem = „Neu: …“ versucht es trotzdem

und-weitere-warnungen = … und { $arg1 } weitere Warnungen

sp-in-der-leiste = ‹ SP › in der Leiste

waehlen-leertaste-an-aus-r-neu-laden-l-log = ↑↓ wählen · Leertaste an/aus · R neu laden · L Log · { $arg1 }

waehlen-oeffnen-esc-schliessen = ↑↓ wählen · ⏎ öffnen · Esc schließen

klick-838533 = ⇧ Klick

klick-3f3474 = ⌘ Klick

mausrad = ⌘ Mausrad

klick-fuer-mehrere-klick-bereich-gruppe-alle-b0c4c9 = ⌘-Klick für mehrere · ⇧-Klick Bereich · Gruppe = alle

klick-fuer-mehrere-klick-bereich-gruppe-alle-887670 = ⌘-Klick für mehrere · ⇧-Klick Bereich · Gruppe = alle · { $arg1 } Hilfe

n-startet-eine = ⌘N startet eine

n-sucht-projekt-oder-ordner-zweite-session-im = ⌘N sucht Projekt oder Ordner · ⌘⏎ zweite Session im selben Ordner · F1 alle Kürzel

startet-eine-zweite-session-im-selben-ordner = ⌘⏎ startet eine zweite Session im selben Ordner. Kadrell meldet sich, sobald eine auf dich wartet.

klick-auf-x = ⌥ + Klick auf X

projekt-erkennen-esc-schliessen = ⏎ Projekt erkennen · Esc schließen

noch-einmal-anlegen-und-starten = ⏎ noch einmal: „{ $arg1 }“ anlegen und starten

ohne-modell-weiter-esc-zurueck = ⏎ ohne Modell weiter · Esc zurück

starten-tab-uebernehmen-o-finder-ordner = ⏎ starten · Tab übernehmen · ⌘O Finder · Ordner hineinziehen

starten-oder-tippen-waehlt-ein-anderes-projekt = ⏎ starten · ↑↓ oder tippen wählt ein anderes Projekt · Esc zurück

starten-waehlt-esc-abbrechen = ⏎ starten · ↑↓ wählt · Esc abbrechen

zum-ersten-treffer-esc-beendet-die-suche = ⏎ zum ersten Treffer · Esc beendet die Suche

infoplist-nsappleeventsusagedescription = Kadrell schlägt bei ⌘N den Ordner des vordersten Finder-Fensters vor.

infoplist-nsmicrophoneusagedescription = Claude-Sessions im Terminal nutzen das Mikrofon für die Spracheingabe.

infoplist-nsusernotificationsusagedescription = Kadrell benachrichtigt dich, wenn eine Session auf dich wartet oder fertig ist.
