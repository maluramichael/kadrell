# Erzeugt von rust/tools/xcstrings2ftl.py aus den xcstrings der Mac-App. Nicht von Hand ändern.

warten-neu = { " " } { $arg1 } waiting · { $arg2 } new

arbeitet-6cc684 = { " " } WORKING

wartet-auf-antwort = { " " } WAITING FOR REPLY

arbeitet-gerade = { " " }(working right now)

sessions-1f3dd5 = { " " }· { $arg1 } sessions

fenster-3ee0e6 = { " " }· Window { $arg1 }

nicht-vorhanden = { $arg1 } (not available)

existiert-nicht-mehr = { $arg1 } no longer exists

existiert-nicht-anlegen-geht-nur-im-home-ordner = { $arg1 } doesn't exist · folders can only be created in your home folder

fertig-0919da = { $arg1 } done

findet-kein-passendes-projekt-zuletzt-benutzte = { $arg1 } finds no matching project · recently used first

gehoert-nicht-dir-oder-ist-fuer-andere = { $arg1 } is not owned by you or is writable by others

laden-und-einschalten = Download { $arg1 } and turn on

laeuft-ohne-sandbox-und-darf-auf-dateien = { $arg1 } runs without a sandbox and can access files, network and programs. Enable anyway?

nicht-erreichbar-eec24d = { $arg1 } not reachable

verfuegbar = { $arg1 } available

vermutet-bitte-pruefen = { $arg1 } guesses, please check

wartet-5b9fac = { $arg1 } waiting

zeigt-auf-diese-app-kadrell-help-listet-die = { $arg1 } points to this app. `kadrell help` lists the commands. Sessions in this app also have the path in $KADRELL.

verbraucht-nach-plan-bd7ef5 = { $arg1 }: { $arg2 }% used, { $arg3 }% on plan

verbraucht = { $arg1 }: { $arg2 }% used

verbraucht-nach-plan-15e25a = { $arg1 }: { $arg2 }% used, { $arg3 }% on plan

nutzung-nicht-abrufbar = { $arg1 }: usage unavailable

claude-version-fehlgeschlagen = { $arg1 }: claude --version failed: { $arg2 }

claude-version-liefert-kein-erkennbares = { $arg1 }: claude --version returned no recognizable version: { $arg2 }

claude-nicht-gefunden = { $arg1 }: claude not found

llama-server-nicht-gefunden = { $arg1 }: llama-server not found

ins-feld-modell-laden = { $arg1 }:{ $arg2 } into the “Download model” field

m-fe7e36 = { $arg1 } %

arbeiten-67c3cb =
    { $arg1 ->
         [one] { $arg1 } RUNNING
        *[other] { $arg1 } RUNNING
    }

claude-prozess-e-werden-sauber-beendet-laufende =
    { $arg1 ->
         [one] { $arg1 } Claude process will be shut down cleanly. Work in progress is interrupted. The conversations are kept and continue on the next launch.
        *[other] { $arg1 } Claude processes will be shut down cleanly. Work in progress is interrupted. The conversations are kept and continue on the next launch.
    }

claude-sessions-laufen-interaktiv-in-anderen-7262f9 = { $arg1 } Claude sessions are running interactively in other terminals

claude-sessions-laufen-interaktiv-in-anderen-356d06 = { $arg1 } Claude sessions are running interactively in other terminals · Import: tools/tmux-dump.py

fehler-bec06c =
    { $arg1 ->
         [one] { $arg1 } ERROR
        *[other] { $arg1 } ERRORS
    }

hintergrund-session-s-uebernehmen =
    { $arg1 ->
         [one] Take over { $arg1 } background session?
        *[other] Take over { $arg1 } background sessions?
    }

prompts-2f48fc = { $arg1 } prompts

sp = { $arg1 } COL

session-s-konnten-nicht-uebernommen-werden = { $arg1 } session(s) could not be adopted

sessions-b904b0 =
    { $arg1 ->
         [one] { $arg1 } session
        *[other] { $arg1 } sessions
    }

treffer = { $arg1 } results

warten-0375a0 =
    { $arg1 ->
         [one] { $arg1 } WAITING
        *[other] { $arg1 } WAITING
    }

zeilen-uebersprungen = { $arg1 } lines skipped

gewaehlt = { $arg1 } selected

trennen = detach { $arg1 }

warten-d4b148 = { $arg1 } waiting

a = { $arg1 }R

f = { $arg1 }E

w = { $arg1 }W

an-gruppe = + on group

eingeklappt = , collapsed

sucht = / searches · { $arg1 }

m-1-claude-session-laeuft-interaktiv-in-anderen-cb1c53 = 1 Claude session is running interactively in other terminals

m-1-claude-session-laeuft-interaktiv-in-anderen-d2db54 = 1 Claude session is running interactively in other terminals · Import: tools/tmux-dump.py

m-1-jahr = 1 year

m-1-bis-4-oder-zeitraum-projekt-leertaste-filtert = 1 to 4 or ←/→ period · ↑/↓ project · Space filters · ⌫ all · s sorts

m-24-std = 24 h

m-3-abstuerze-in-2-min-bleibt-aus = 3 crashes in 2 min, stays off

m-30-tage = 30 days

m-5-stunden = 5 hours

m-5-stunden-limit-in-der-leiste-bei-ultra-nie = 5-hour limit in the bar (never on Ultra)

m-7-tage = 7 days

m-7-tage-limit-in-der-leiste = 7-day limit in the bar

m-7-tage-limit-laeuft-zu-schnell-voll = 7-day limit filling up too fast

auto-sp = AUTO COL

auto-in-der-leiste = AUTO in the bar

abbrechen = Cancel

abstand-zwischen-kacheln = Gap between tiles

account-automatisch-gewechselt = Account switched automatically

adresse-des-ollama-dienstes = Address of the Ollama service

adresse-eines-laufenden-llama-server-optional = Address of a running llama-server (optional)

adresse-ungueltig = invalid address

aktion-ohne-id-oder-label-uebersprungen = Skipped an action without id or label

aktives-konto = Active account

aktualisieren = Update

aktuellen-account-hinzufuegen = Add current account

aktueller-account = Current account

alle-claude-sessions-durchsuchen-auch-beendete = Search all Claude sessions, including ended ones

alle-gruppen-auf-oder-zuklappen = Expand or collapse all groups

alle-gruppen-auf-zuklappen = Expand/Collapse All Groups

alle-sessions-auswaehlen = Select All Sessions

alle-sessions-von = All sessions of { $arg1 }

alle-sessions-zeigen = Show all sessions

andere = Other

anderes-projekt-suchen = Search another project

angegebene-rechte-nicht-erzwungen = Declared permissions: { $arg1 } (not enforced)

angemeldeten-account-hinzufuegen = Add signed-in account

anlegen = Create

anpassen = Customize

ansicht = View

antwort-nicht-lesbar = unreadable answer

antwortet-nicht-mehr-kein-pong-binnen-5-s = Not responding (no pong within 5 s)

anzahl = Count

arbeitsflaeche = Workspace

arbeitsflaeche-tastatur-an-claude = Workspace: keyboard to Claude

archiv-liess-sich-nicht-entpacken = Could not extract the archive

auf-github-suchen-topic-kadrell-extension = Search GitHub (topic kadrell-extension)

auf-ollama-com-suchen-webseite-keine-offizielle = Search ollama.com (website, no official API)

aus-f0ce94 = Off

aus-ansicht-nehmen = Remove from View

aus-den-transcripts-auch-von-sessions = From the transcripts, including sessions outside Kadrell.

ausklappen = Expand

ausschneiden = Cut

auswahl-verschieben-nach = Move selection to ({ $arg1 })

auto-modus = Auto mode

auto-modus-ein-aus = Toggle Auto Mode

auto-modus-kacheln-erscheinen-sobald-claude = Auto mode: tiles appear as soon as Claude needs something from you

automatisch-wechseln-bei-limit = Auto-switch at limit

beendet-klick-setzt-fort = ENDED · CLICK TO RESUME

baum-als-vorschau-durchblaettern-auswahl-bleibt = Browse the sidebar as a preview, selection stays · ⏎ takes it, Esc goes back

baum-ein-aus = Toggle Sidebar

baum-nach-projekt-gruppieren = Group tree by project

baum-sortieren-aus-a-z-status = Sort sidebar: off → A–Z → status

baum-und-kacheln = Sidebar and tiles

baum-tastatur-hierher-waehlt-session = Sidebar: keyboard here, ↑↓ picks a session

bearbeiten-84e45e = Edit

bearbeiten-92b34c = Edit …

beenden = Quit

beendet-durch-signal = Killed by signal { $arg1 }

beendet-mit-status = Exited with status { $arg1 }

bei-diesem-tempo-ist-das-7-tage-limit-am-voll = At this pace the 7-day limit is full on { $arg1 }. But the reset isn’t until { $arg2 }, so you’d be locked out until then.

bereich-seit-dem-letzten-klick-dazu = add the range since the last click

bild-hinter-baum-und-kacheln = image behind tree and tiles

bild-waehlen = Choose image

braucht-neueres-kadrell-api-unterstuetzt = Needs a newer Kadrell (API { $arg1 }, supported: { $arg2 })

bypass = Bypass

bypass-modus-erlauben-allow-dangerously-skip = Allow bypass mode (--allow-dangerously-skip-permissions)

claude = Claude

claude-cli-meldet-einen-fehler = Claude CLI reported an error

claude-wird-beendet-und-die-kachel-entfernt-die = Claude will be stopped and the tile removed. The conversation is kept: claude --resume { $arg1 }

claude-wird-beendet-die-kachel-bleibt-ein-klick = Claude will be stopped, the tile stays. A click resumes the conversation.

claude-wird-in-allen-sessions-beendet-und-die = Claude will be stopped in all sessions and the group removed. The conversations are kept.

claude-code-sessions-als-terminals-baum-links = Claude Code sessions as terminals: sidebar on the left, grid or stack on the right. They end when you quit and continue with --resume on the next launch.

claude-nutzung-der-letzten-5-stunden = Claude usage over the last 5 hours

claude-nutzung-der-letzten-7-tage = Claude usage over the last 7 days

darstellung = Appearance

das-transcript-von-ist-nicht-mehr-da = The transcript of “{ $arg1 }” is gone.

datei = File

deckkraft-von-baum-und-kacheln = Tree and tile opacity

den-ordner-gibt-es-nicht-mehr = The folder { $arg1 } no longer exists.

den-ordner-hat-auch-die-gruppe = The group “{ $arg1 }” uses this folder too.

der-dateiauswahl-dialog-oeffnet-sich-erst-nach = The file dialog only opens again after restarting Kadrell. You can still type or drop paths.

design-des-baums = Sidebar style

die-shell-und-alles-was-darin-laeuft-wird = The shell and everything running in it will be stopped.

die-verbindung-wird-getrennt-die-tmux-session = The connection will be closed, the tmux session on { $arg1 } keeps running.

doku = Docs

doku-oeffnen = Open docs

download-ohne-abschluss-beendet = Download ended without finishing

download-von-github-fehlgeschlagen = Download from GitHub failed

einstellungen-09eaab = SETTINGS

entdecken = DISCOVER

erster-prompt = FIRST PROMPT

effort = Effort

eigene = Custom

eigene-erweiterungen-in-lua = Your own extensions in Lua:

eigene-farbe = Custom color

einklappen = Collapse

einschalten = Enable

einsetzen = Paste

einstellungen-8f24d4 = Settings…

einstellungen-projektordner-darstellung = Settings: project folder, appearance, shortcuts

empfohlenes-modell = Recommended model

entfernen-382837 = Remove

entfernen-ffcb43 = Remove…

enthaelt-lua-5-5-1-mit-lua-org-und-json-lua-mit = Includes Lua 5.5.1 (MIT, lua.org) and json.lua (MIT, rxi) as well as colour values of the Instrument scheme from Monsters (Apache-2.0, Saturday Company)

erneut-pruefen = Check again

erste-session-starten = Start first session

esc-abbrechen = Esc to cancel

esc-beendet-die-suche = Esc ends the search

esc-oder-schliessen = Esc or { $arg1 } closes

esc-schliesst = Esc closes

extensions-fe3436 = Extensions

extensions-dbcb25 = Extensions …

extensions-ordner-oeffnen = Open extensions folder

extensions-7b0999 = Extensions: { $arg1 }

extensions-an-und-abschalten-neu-laden = Extensions: turn on and off, reload, settings, log

externer-editor-kommando-wie-im-terminal = External editor (command as in the terminal)

flach = FLAT

fable = Fable

fable-kontingent = Fable quota

fable-kontingent-in-der-leiste = Fable allowance in the bar

farbe-aus-einer-palette-waehlen = Pick a color from a palette

farbe-waehlen = Choose color

farbschema = Color scheme

favorit = Favorite

fehler-f6cc01 = error

fehler-7d0a41 = Error: { $arg1 }

fenster-22c978 = Window

fenster-nur-verstecken-sessions-laufen-weiter = only hides the window, sessions keep running

fenster-schliessen = Close Window

fenster-schliessen-sessions-laufen-weiter = Close window, sessions keep running

fenster-und-app = Window and app

fenster-zurueckholen = bring the window back

fertig-25d52d = Done

fertige-sessions-automatisch-trennen-0-nie = Detach finished sessions automatically (0 = never)

fertige-sessions-trennen = Detach finished sessions

filtern = Filter …

fokus-bewegen-im-stack-auf-und-zuklappen = Move focus · expands and collapses in Stack

fokus-links = Focus left

fokus-oben = Focus up

fokus-rechts = Focus right

fokus-unten = Focus down

fokus-kachel-mit-nachbar-tauschen = Swap focused tile with its neighbor

fokus-kachel-schliessen = Close focused tile

fokus-kachel-schliessen-esc-selbst-geht-an = Close focused tile (Esc itself goes to Claude)

fokus-session-beenden-und-entfernen-mit = Stop and remove the focused session, with confirmation

fokus-session-umbenennen-auch-im-baum-claude = Rename the focused session, also in the sidebar · Claude no longer overwrites the name

fokussierte-session-verschieben-nach = Move focused session to: { $arg1 }

fortsetzen = Resume

frei-3b09ee = Free

frei-teilung-an-der-fokus-kachel-klick-schaltet = Free: split at the focused tile, click cycles → ↓ AUTO (longer side)

frei-naechste-kachel-rechts = Free: next tile to the right

frei-naechste-kachel-unten = Free: next tile below

gesamt = TOTAL

gruppe-bearbeiten = EDIT GROUP

ganze-gruppen-auswaehlen = Select Whole Groups

gelb-heisst-claude-wartet-auf-dich-n-springt = Yellow means: Claude is waiting for you. ⌥N jumps to the next waiting session.

gerade-wartet-keine-session = No session is waiting right now

gestoppt = Stopped

gezaehlt-wird-solange-kadrell-laeuft = Counting happens while Kadrell is running.

gilt-fuer = Applies to

grid = Grid

gruppe-b4c022 = Group

gruppe-sessions = Group { $arg1 }, { $arg2 } sessions

gruppe-1-session = Group { $arg1 }, 1 session

gruppe-bearbeiten-der-fokussierten-session = Edit group (of the focused session)

gruppe-schliessen-0b5167 = Close group

gruppe-schliessen-f13a36 = Close group …

gruppe-c99929 = Group · { $arg1 }

gruppe-1c9b17 = Group “{ $arg1 }”

gruppe-mit-session-s-schliessen = Close group “{ $arg1 }” with { $arg2 ->
         [one] { $arg2 }{ " " }session
        *[other] { $arg2 }{ " " }sessions
    }?

gruppen-aus-alle-sessions-in-einer-flachen = Groups off: every session in one flat list, with its own hand-sorted order

gruppierung-nach-projekt = Group by project

groessen-des-layouts-aendern-doppelklick = Resize the layout, double-click evens out · terminals fill the fields in order

haupt-spalte = Main + Column

herunterladen = Download

hilfe-mit-allen-kuerzeln = Help with all shortcuts

hintergrund-sessions-uebernehmen = Take over background sessions

hintergrundbild = Background image

hinzufuegen = Add

in-anderem-fenster-klick-holt-hierher = IN ANOTHER WINDOW · CLICK TO BRING HERE

idee-tippen-kadrell-erkennt-das-projekt-und = Type an idea, Kadrell recognises the project and starts a session there with the idea as first prompt

idee-tippen-z-b-in-kadrell-habe-ich-die-idee = Type an idea, e.g. “In Kadrell I have the idea that …”

im-baum-menue-der-session-bei-zugeklappter = In the tree: menu of the session, of the group when it is collapsed · ⌃⏎ goes to the session

im-dock-ablegen = Minimize

im-editor-oeffnen = Open in editor

im-finder-waehlen = Choose in Finder

im-finder-waehlen-o = Choose in Finder (⌘O)

im-finder-zeigen = Show in Finder

im-fullscreen-starten-kein-doppelter-text-beim = Start in fullscreen (no duplicated text when scrolling)

im-terminal-suchen = Find in Terminal…

im-terminal-suchen-g-weiter = Find in terminal, ⌘G next

in-allen-terminals-suchen-204468 = Find in all terminals

in-allen-terminals-suchen-6a2679 = Find in All Terminals…

innenabstand-der-kacheln = Tile padding

installation-fehlgeschlagen = Installation failed

installationsbefehl-kopieren = Copy install command

installieren = Install

installieren-fehlgeschlagen = Install failed: { $arg1 }

jetzt-aktiv-grund-nutzungslimit-erreicht = Now active: “{ $arg1 }”. Reason: usage limit reached.

kachel = Tile { $arg1 }

kachel-1-9-fokussieren = Focus tile 1–9

kachel-bleibt-klick-setzt-fort = Tile stays, click resumes

kachel-nach-links-tauschen = Swap tile left

kachel-nach-oben-tauschen = Swap tile up

kachel-nach-rechts-tauschen = Swap tile right

kachel-nach-unten-tauschen = Swap tile down

kachel-schliessen = Close tile

kachel-waehlen = Select Tile

kacheln-verwalten = Manage tiles

kachelwechsel = Tile switches

kadrell-verfuegbar = Kadrell { $arg1 } available

kadrell-ausblenden = Hide Kadrell

kadrell-beenden-8d4202 = Quit Kadrell

kadrell-beenden-waehrend-claude-laeuft = Quit Kadrell while Claude is running

kadrell-beenden-c78937 = Quit Kadrell?

kadrell-beenden-session-s-arbeiten-gerade =
    { $arg1 ->
         [one] Quit Kadrell? { $arg1 } session is working right now!
        *[other] Quit Kadrell? { $arg1 } sessions are working right now!
    }

kadrell-neu-starten-5a0b36 = Restart Kadrell

kadrell-neu-starten-f7c56e = Restart Kadrell?

kadrell-neu-starten-session-s-arbeiten-gerade =
    { $arg1 ->
         [one] Restart Kadrell? { $arg1 } session is working right now!
        *[other] Restart Kadrell? { $arg1 } sessions are working right now!
    }

kadrell-startet-claude-jetzt-selbst-statt-mit = Kadrell now starts Claude itself instead of using claude --bg. These sessions will be stopped with claude stop (work in progress is interrupted) and resumed here:

kadrell-wurde-seit-dem-start-ersetzt = Kadrell was replaced since launch

kadrell-oeffnen = Open Kadrell

kein-bild = No image

kein-favorit = Not a favorite

kein-ordner-gefunden-o-waehlt-im-finder = No folder found. ⌘O picks one in Finder.

kein-projekt-gefunden = No project found.

kein-angemeldeter-claude-account-gefunden = No signed-in Claude account found

kein-tmux-auf-diesem-host-normale-shell = No tmux on this host, plain shell

keine-eintraege-gefunden = No entries found.

keine-modelle-installiert = No models installed

keine-prompts-in-diesem-zeitraum = No prompts in this period.

keine-session-fokussiert = No session focused

keine-treffer = No results

keine-wartenden-sessions = No waiting sessions

klassisch = Classic

klick-d1b731 = Click

knoten-ohne-typ-uebersprungen = Skipped a node without a type

knoten-ohne-feld-uebersprungen = Skipped “{ $arg1 }” node without field “{ $arg2 }”

kommando = Command

kommando-laeuft-noch = command still running

kommandozeilen-tool-installieren = Install Command Line Tool…

kommandozeilen-tool-installiert = Command line tool installed

kompakt = Compact

konten = Accounts

konten-verwalten = Manage accounts …

konto-df8525 = Account

konto-928bdd = Account { $arg1 }

konto-hinzufuegen-aktuell-angemeldetes = Add account (currently signed in)

konto-hinzufuegen = Add account …

konto-waehlen = Choose account

kopieren = Copy

kuerzel-anklicken-tasten-druecken-entfernt = click a shortcut, press keys · ⌫ removes

limits = LIMITS

lade-sessions = Loading sessions…

laden = Download

laufende-claude-prozesse-sessions = Running Claude processes / sessions

laufzeit-im-baum = Runtime in sidebar

layout-a51190 = Layout

layout-frei-wie-i3-die-kachel-nach-der = Free layout (like i3): the tile after the focused one appears to the right or below

layout-wechseln-grid-haupt-spalte-spirale-frei = Switch layout: Grid → Main + Column → Spiral → Free → Scrolling → Stack

layout-49fd56 = Layout: { $arg1 }

leer-lassen-wieder-der-titel-von-claude-code = Leave empty to use the Claude Code title again ({ $arg1 })

letzte-antwort-von-claude-im-baum = Claude's last reply in sidebar

linie = Line

log-l = Log  L

log-ausblenden-l = Hide log  L

lokale-modelle-ollama = Local models: Ollama

lokale-modelle-llama-cpp = Local models: llama.cpp

laedt-neu = Reloading

laeuft = Running

loeschen = Delete

mehr-spalten = More columns

meiste-sessions-gleichzeitig = Most sessions at once

melde-dich-zuerst-in-einer-session-mit-claude = Sign in with “claude” in a session first, then try again.

menue-der-session-oder-gruppe-verschieben-nach = Menu of the session or group: move to, edit, folder, close

menueleiste = Menu bar

menueleisten-icon-dock = Menu bar icon / Dock

mittelklick-im-baum = Middle-click in the tree

modell-ff461e = Model

modell-fuer-pi-statt-claude-im-schnellstart-mit = model for Pi instead of Claude, switch with Tab in Quick Start

modell-laden-name-aus-der-bibliothek = Download model (name from the library)

modell-b79aa1 = Model: { $arg1 }

modelle-fuer-funktionen = Models for features

modellordner-mit-gguf-dateien-optional = Model folder with GGUF files (optional)

neue-gruppe-0b6e85 = NEW GROUP

neue-session-884c7f = NEW SESSION

nach-updates-suchen = Check for updates

nach-wochentempo-verteilen = Balance by weekly pace

nachricht-zu-gross-ueber-1-mb = Message too large (over 1 MB)

nachrichten-geschickt = Messages sent

name = Name

name-ist-schon-von-einer-anderen-extension = Name { $arg1 } is already taken by another extension, remove it first

name-im-manifest-passt-nicht-zum-ordnernamen = Name in the manifest (“{ $arg1 }”) does not match the folder name

navigation = Navigation

neu-in = New in { $arg1 }

neu-laden = Reload

neu-laden-r = Reload  R

neu-starten = Restart

neu-197517 = New: { $arg1 }

neue-gruppe-leer = New group (empty)

neue-gruppe-60a101 = New group …

neue-instanz-mit-temporaerem-profil = New Instance with Temporary Profile

neue-modelle-einmal-im-terminal-mit-llama = New models: load once in the terminal with llama-server -hf <repo> (goes to the cache) or put GGUF files into the model folder

neue-session-e0d5a1 = New Session

neue-session-hier = New Session Here

neue-session-im-ordner-der-fokussierten = New session in the focused session's folder

neue-session-im-selben-ordner = New Session in Same Folder

neue-session-in-dieser-gruppe = New session in this group

neue-session-starten-n = Start new session  ⌘N

neue-session-projekt-suchen-d-p-abkuerzen-o = New session: search a project, abbreviate ~/d/p, ⌘O Finder, drop a folder

neue-sessions-starten-im-neuen-ordner-laufende = New sessions start in the new folder, running ones stay in theirs.

neue-leere-gruppe-mit-name-ordner-und-farbe = New empty group with name, folder and color

neuen-ordner-anlegen = Create new folder “{ $arg1 }”

neues-fenster = New Window

neues-fenster-eigene-auswahl-und-eigenes-layout = New window: own selection and layout, same sessions

neues-terminal-in-dieser-gruppe = New terminal in this group

neues-terminal-ohne-claude = New Terminal without Claude

nicht-mehr-anzeigen = Don’t show again

nicht-mehr-fragen = Don't ask again

nichts-gefunden-c4323e = Nothing found.

nimmt-keine-nachrichten-mehr-an-ueber-1-mb = No longer accepts messages (over 1 MB unprocessed)

noch-keine-extensions-ordner-hineinlegen-oder = No extensions yet. Drop a folder in or git clone one, it shows up right away.

noch-keine-logzeilen = No log lines yet.

noch-keine-messwerte-in-diesem-zeitraum = No measurements in this period yet.

noch-keine-session = No sessions yet

noch-nichts-gezaehlt = Nothing counted yet.

nur-diese-zeigen = Show Only This

naechste-vorige-kachel = Next / previous tile

naechste-kachel = Next tile

naechste-kachel-teilt = Next tile splits

naechste-wartende-session = Next waiting session

ok = OK

ordner-fehlt = FOLDER MISSING · { $arg1 }

ollama-nutzen = Use Ollama

ollama-starten-app-oder-ollama-serve-kadrell = Start Ollama (app or ollama serve), Kadrell does not ship its own service

ordner-d-p-kad-kuerzt-ab = Folder (~/d/p/kad abbreviates)

ordner-der-fokus-session-im-editor-aus-den = Open the focused session's folder in the editor from Settings

ordner-im-finder-waehlen = Choose folder in Finder

ordner-im-externen-editor-oeffnen = Open folder in external editor

ordner-nicht-angelegt = Folder not created: { $arg1 }

ordner-nicht-gefunden-tab-vervollstaendigt-o = Folder not found · Tab completes, ⌘O picks in Finder

ordner-oeffnen = Open folder

originaltext-tab-ohne-projektverweis = Original text · Tab: without project reference

prompts-b0f562 = PROMPTS

panel-zu-gross-knoten-hoechstens = Panel too large ({ $arg1 } nodes, at most { $arg2 })

pfad-in-stack-zeilen = Path in Stack rows

pfad-kopieren = Copy Path

pfad-zu-llama-server-leer-automatisch = Path to llama-server (empty = automatic)

pi-ist-nicht-installiert-pi-im-path-oder-unter = Pi is not installed (pi in PATH or in ~/.local/bin)

pi-sessions = Pi sessions

projekt-suchen-oder-pfad-tippen-d-p-kad = Search a project or type a path (~/d/p/kad)

projektordner-fuer-n-nach-git-repos-durchsucht = Project folder for ⌘N (searched for Git repos)

projektverweis-aus-dem-ersten-prompt-entfernen = Remove the project reference from the first prompt when sure

projektverweis-entfernt-tab-originaltext = Project reference removed · Tab: original text

prompts-5f2e66 = Prompts

prozent = Percent

rechte-sidebar-der-extensions-eintrag-tab-erste = Extensions in the right sidebar: ↑↓ entry, ←→ tab, ⏎ first action, ⌥⏎ all actions, Esc back

rechte-sidebar-ein-aus = Toggle Right Sidebar

rechte-sidebar-tastatur-hierher-waehlt-eintrag = Right sidebar: keyboard here, ↑↓ picks an entry, ←→ a tab

rechte = Permissions: { $arg1 }

rechtsklick-im-baum = Right-click in the tree

reihe = Row

reload = Reload

remote-verbinden-ssh-tmux = Connect to remote (ssh → tmux)

remote-verbinden = Connect to Remote…

remote-session-waehlen = Choose remote session …

repo-enthaelt-keine-extension-kadrell-json-fehlt = Repo contains no extension (kadrell.json missing)

repo-ist-zu-gross-ueber-mb = Repo is too large (over { $arg1 } MB)

reset = Resets { $arg1 } ({ $arg2 })

rundes-x = Round X

rueckfragen = Confirmations

rueckwaerts-suchen = Find Previous

schnellstart = QUICK START

session-umbenennen-399a7e = RENAME SESSION

start-fehlgeschlagen-exit = START FAILED · EXIT { $arg1 }

starten-mit = START WITH

startet-7ec244 = STARTING…

schliessen = Close

schliessen-ohne-rueckfrage = Close without confirmation

schnellstart-idee-rein-session-raus = Quick start: idea in, session out

scrollen = Scrolling

scrollen-spalten-verschieben-maus-rad-der-fokus = Scrolling: move columns (mouse: ⇧ + wheel), the focus scrolls into view by itself

seit = Since

seitlich-wischen = Swipe sideways

session-6959b4 = Session

session-d7b46c = Session { $arg1 }

session-beenden-und-entfernen = Stop and remove session

session-dazu-oder-weg = add or remove the session

session-fortsetzen-fokussierte = Resume session (focused)

session-im-baum-waehlen = Select a session in the sidebar

session-laesst-sich-nicht-fortsetzen = Session cannot be resumed

session-oder-gruppe-umsortieren-baum-und-dc767a = Reorder a session or group, sidebar and tiles alike

session-oder-gruppe-umsortieren-baum-und-bab36d = Reorder a session or group, tree and tiles alike · drop a session on another group in the tree: move it there

session-schliessen = Close Session

session-schliessen-fokussierte = Close session (focused)

session-schliessen-auf-gruppe-die-ganze-gruppe = Close session · on a group: the whole group · with ⌥ no confirmation

session-stoppen-9a52bc = Stop session

session-stoppen-fokussierte = Stop session (focused)

session-umbenennen-b4e8d8 = Rename session

session-umbenennen-fokussierte = Rename session (focused)

session-stoppen-54b8de = Stop session “{ $arg1 }”?

session-gruppe-pfad-suchen-kommandos-in-allen = Search session, group, path…  ( > commands, / in all terminals, # all Claude sessions, @ remote )

sessions-6fa3cb = Sessions

sessions-duerfen-andere-sessions-steuern = Sessions may control other sessions (kadrell send, capture, kill)

sessions-geoeffnet = Sessions opened

sessions-koennen-nicht-geladen-werden = Sessions cannot be loaded

sessions-oeffnen = Open sessions

sessiontitel = Session title

sortiert = Sorted:

sortierung = Sorting

sounds = Sounds

spalten-im-grid-unter-1-wieder-automatisch = columns in Grid, below 1 automatic again

speichern = Save

spirale = Spiral

sprache = Language

sprache-des-systems = System language

springt-zur-naechsten-session-die-auf-dich = Jumps to the next session waiting for you

stack = Stack

stand-von-vor-min = as of { $arg1 } min ago

standard = Default

standard-von-pi = Pi's default

standardmodell = Default model

starten = Start

startet-f4522a = Starting

startet-mit = Starts with: { $arg1 }

startet-nicht-kein-ready-binnen-3-s = Does not start (no ready within 3 s)

startmodus = Start mode

statistik = Stats

statistik-limits-prompts-und-zaehler = Stats: limits, prompts and counters

status = Status

statusleiste-0140bb = Status bar

statusleiste-der-extensions = Extension status bar

statusleiste-699106 = Status bar: { $arg1 } · { $arg2 }

statusleiste-eintrag-ohne-id-uebersprungen = Status bar: skipped entry without “id”

statusleiste-eintrag-ohne-inhalt-uebersprungen = Status bar: skipped entry “{ $arg1 }” without content

statusleiste-hoechstens-eintraege-der-rest-fehlt = Status bar: at most { $arg1 } entries, the rest is left out

statusleiste-id-doppelt-uebersprungen = Status bar: duplicate id “{ $arg1 }”, skipped

stift = Pencil

stoppen = Stop

suche = Search

suche-git-repos-unter = Searching Git repos under ~ …

suche-fehlgeschlagen = Search failed: { $arg1 }

suche-mit-kommandos = Search, with > commands

suchen = Search

sucht-in-titeln-und-eigenen-prompts-aller = Searches titles and your own prompts of all past Claude sessions · ⏎ resumes one in its folder or jumps to it if it is running

super = Great

symbol-und-text = Icon and text

sync-eingabe-an-alle-kacheln = Sync: input to all tiles

sync-tippen-und-v-gehen-an-alle-offenen-kacheln = Sync: typing and ⌘V go to all open tiles at once · badge “SYNC”

systembenachrichtigungen = System notifications

teilt-auto = SPLIT AUTO

teilt-in-der-leiste = SPLIT in the bar

teilt-554b3b = SPLIT →

teilt-494a75 = SPLIT ↓

tab-vervollstaendigt-o-finder-ordner = Tab completes · ⌘O Finder · drop a folder here

tasten-druecken = Press keys…

tastenkuerzel = Shortcuts

terminal-auf-der-gpu-zeichnen-metal = Draw terminal on the GPU (Metal)

terminal-ohne-claude-im-ordner-der-fokussierten = Terminal without Claude in the focused session's folder

terminal-schrift = Terminal font

terminal-schrift-standardgroesse = Default Terminal Font Size

terminal-schrift-aller-sessions-groesser-kleiner = Terminal font of all sessions bigger/smaller

terminal-schrift-groesser = Bigger Terminal Font

terminal-schrift-kleiner = Smaller Terminal Font

terminal-schriftgroesse-0-mausrad = Terminal font size  ⌘+ ⌘- ⌘0  ⌘ scroll wheel

terminals-geoeffnet = Terminals opened

tipp = Tip

tippen-filtert = Type to filter

transcripts-werden-gelesen = Reading transcripts …

trennlinie-an-der-fokus-kachel-um-5-verschieben = Move the divider at the focused tile by 5 % · Scrolling: column ⅓ ½ ⅔ wide

trennlinie-nach-links = Divider left

trennlinie-nach-oben = Divider up

trennlinie-nach-rechts = Divider right

trennlinie-nach-unten = Divider down

trennlinie-ziehen = Drag divider

trennlinien-zuruecksetzen-gleich-verteilt = Reset dividers (evenly spaced)

ui-groesse = UI size

umbenennen = Rename

umschalten-ab-auslastung = Switch at usage

unbekannte-farbe-ignoriert-erlaubt-accent-muted = Ignored unknown color (allowed: accent, muted, ok, warn, err)

unbekannter-knotentyp-uebersprungen = Skipped unknown node type “{ $arg1 }”

update-verfuegbar = Update available

updates = Updates

vorschau-nicht-gestartet-oder-klick-startet = PREVIEW · NOT STARTED · ⏎ OR CLICK STARTS

verbraucht-in-prozent = Used, in percent

verlauf-wird-eingelesen = Reading history…

verlauf-zum-zurueckscrollen = Scrollback

verschieben-nach = Move to

vorige-kachel = Previous tile

vorschau-naechste-session-im-baum = Preview: next session in sidebar

vorschau-vorige-session-im-baum = Preview: previous session in sidebar

warnen-bei-zu-schnellem-7-tage-verbrauch = Warn on too-fast 7-day usage

wartende-sessions = Waiting sessions

was-ist-neu = What's new

wechsel-meldung-anzeigen = Show switch notice

wechseln = Switch

weiter = Continue

weitersuchen = Find Next

weniger-spalten = Fewer columns

wenn-claude-endet-zweimal-c-exit = When Claude exits (⌃C twice, /exit)

wieder-einschalten-einstellungen-rueckfragen = Turn back on: Settings (⌘,) › Confirmations

wirklich-loeschen = Really delete

waehlen-eda09c = Choose

zeigt = Shows

zeilen = lines

zeilenabstand = Line spacing

zeit = Time

ziehen = Drag

zoom-aufheben = Exit zoom

zoom-ein-aus-fokussierte = Toggle zoom (focused)

zoom-fokus-kachel-allein = Zoom: focused tile alone

zoomen = Zoom

zu-konto-wechseln = Switch to account: { $arg1 }

zu-viele-nachrichten-mehr-als-50-pro-sekunde = Too many messages (more than 50 per second)

zuletzt = Recent

zuletzt-fokussierte-kachel = Last focused tile

zurueck-auf = Back to { $arg1 }

zwischenablage = Clipboard

aktiv = active

alle = all

alle-kuerzel = all shortcuts

alle-sessions = all sessions

alle-sessions-nochmal-zurueck = all sessions, again: back

an = on

an-kachel-und-baum-zeile-umbenennen = on tile and sidebar row: rename

arbeiten-f1ce4c = working

arbeitende = working

arbeitet-4d00bc = working

aus-2337c3 = off

aus-ohne-nachfrage-ausfuehren = off = run without asking

ausgewaehlt = selected

automatisch = automatic

beendet = ended

beim-ersten-mal-einmalig-alle-transcripts = all transcripts once the first time, only new ones after that

benutzter-ordner = used folder

bis = until { $arg1 }

claude-aelter-als-die-von-kadrell-getestete-6f465f = claude { $arg1 }: older than version { $arg2 } tested with Kadrell, please update

claude-aelter-als-die-von-kadrell-getestete-1afb80 = claude { $arg1 }: older than the version Kadrell was tested with ({ $arg2 }), update with “{ $arg3 }”

claude-alt-claude-update = claude outdated · claude update

diese-hilfe = this help

erkannt-am-namen-im-text = recognised by the name in the text

erkannt-von = recognised by { $arg1 }

erkannt = recognised: { $arg1 } · { $arg2 }

fertig-fdc275 = done

frage = asking { $arg1 } …

fragt-nach-rechten = asks for permissions

frei-a03d4e = free

ganze-gruppen-der-auswahl-nochmal-zurueck = whole groups of the selection, again: back

gerade-eben = just now

gesetzt = set

getrennt = detached

gezeigt-wird-nur-was-installiert-ist = only installed models are listed

gibt-neuen-claude-sessions-nach-dem-ersten = gives new Claude sessions a short title after the first prompt, starts Ollama if needed

gilt-fuer-neu-gestartete-claude-prozesse = applies to newly started Claude processes

init-lua-fehlt = init.lua is missing

ist-fertig = is done

je-funktion-an-aus-und-modell-aus-den = per feature on/off and a model from the installed ones

kadrell-json-fehlt-oder-ist-kaputt = kadrell.json is missing or broken

kein-bild-datei-hineinziehen = no image · drop a file

kein-modell-titel-von-claude-code = no model (title from Claude Code)

kein-modell-nur-claude = no model (Claude only)

kein-modell-nur-namenssuche = no model (name search only)

kein-modell-gewaehlt = no model chosen

kein-name-im-text-hilft-beim-zuordnen = no name in the text · { $arg1 } helps to match

kein-name-im-text-ohne-lokales-modell-zuletzt = no name in the text · without a local model, recently used first

kein-projekt-im-text-erkannt-zuletzt-benutzte = no project recognised in the text · recently used first

kein-tmux-server-auf = no tmux server on { $arg1 }

keine = none

keine-antwort-in-der-zeit = no answer in time

leer-kadrell-startet-ihn = empty = Kadrell starts it

llama-server-startet-nicht = llama-server does not start

llama-cpp = llama.cpp

llama-cpp-installieren-brew-install-llama-cpp = Install llama.cpp (brew install llama.cpp) or set the path

llama-cpp-nutzen = Use llama.cpp

laedt = loading…

mehrere-claude-accounts-umschalten-ohne-neues = several Claude accounts, switch without signing in again

mehrere-projekte-passen-zum-namen = several projects match the name

min = min

mit-fehler = with errors

neu-b1688c = new

neue-session-in-der-gruppe-mit-terminal-ohne = new session in the group · with ⌘: terminal without Claude

neuer-wert = new value

nicht-erreichbar-92f2c4 = not reachable: { $arg1 }

nicht-gefunden = not found

nicht-gesetzt = not set

nicht-gestartet = not started

nichts-gefunden-eb11e0 = nothing found

nur-symbol = Icon only

nur-der-cache-von-llama-cpp = only the llama.cpp cache

nur-die-auswahl-im-baum = only the selection in the sidebar

nur-diese-session-zeigen-auf-gruppe-alle-ihre = show only this session · on a group: all its sessions

nur-wartet = waiting only

nur-wartet-und-fertig = only waiting and done

ohne-adresse-startet-kadrell-llama-server = without an address Kadrell starts llama-server itself, it quits with Kadrell

ordnet-die-idee-einem-projekt-zu-wenn-der-name = matches the idea to a project when the name in the text is unclear

prueft = checking …

rechts = right

ssh-tmux-attach-waehlt-die-session = ssh · ⏎ tmux attach · “{ $arg1 }:” picks the session

startet-e85bbb = starting …

sucht-projekt-oder-ordner = searches for a project or folder

tmux-ls-auf = tmux ls on { $arg1 }

tmux-new-as-auf = tmux new -As on { $arg1 }

tmux-session-auf = tmux session on { $arg1 }

ungueltige-nachricht-verworfen = invalid message discarded

unten = below

von-kadrell-gestartet = started by Kadrell · { $arg1 }

von-michael-malura = by Michael Malura ·

vor-min = { $arg1 } min ago

warten-227835 = waiting

wartende = waiting

wartende-und-arbeitende = waiting and working

wartet-542760 = waiting

wartet-auf-dich = is waiting for you

wartet-und-fertig = waiting and done

waehlen-e11cb9 = pick

z-b-code = e.g. code

z-b-qwen3 = e.g. qwen3

zweite-session-im-selben-ordner = second session in the same folder

aeltere-claude-version = Older claude version

aenderungen-gelten-sofort-sucht-esc-schliesst = Changes apply immediately · / searches · Esc closes

aenderungen-gelten-sofort-esc-schliesst = Changes apply immediately · Esc closes

ueber-kadrell = About Kadrell

uebernehmen = Take Over

aehnlicher-name-im-text-bitte-pruefen = similar name in the text, please check

aendern = change

uebersprungen = skipped

beenden-und-entfernen = Stop and remove “{ $arg1 }”?

laeuft-noch-ausserhalb-dieses-kadrell-pid-erst = “{ $arg1 }” is still running outside this Kadrell (pid { $arg2 }). Quit it there first.

neu-versucht-es-trotzdem = “New: …” tries anyway

und-weitere-warnungen = … and { $arg1 } more warnings

sp-in-der-leiste = ‹ COL › in the bar

waehlen-leertaste-an-aus-r-neu-laden-l-log = ↑↓ select · Space on/off · R reload · L log · { $arg1 }

waehlen-oeffnen-esc-schliessen = ↑↓ select · ⏎ open · Esc close

klick-838533 = ⇧ Click

klick-3f3474 = ⌘ Click

mausrad = ⌘ scroll wheel

klick-fuer-mehrere-klick-bereich-gruppe-alle-b0c4c9 = ⌘-click for several · ⇧-click range · group = all

klick-fuer-mehrere-klick-bereich-gruppe-alle-887670 = ⌘-click for several · ⇧-click range · group = all · { $arg1 } help

n-startet-eine = ⌘N starts one

n-sucht-projekt-oder-ordner-zweite-session-im = ⌘N searches for a project or folder · ⌘⏎ second session in the same folder · F1 all shortcuts

startet-eine-zweite-session-im-selben-ordner = ⌘⏎ starts a second session in the same folder. Kadrell lets you know as soon as one is waiting for you.

klick-auf-x = ⌥ + click on X

projekt-erkennen-esc-schliessen = ⏎ recognise project · Esc close

noch-einmal-anlegen-und-starten = ⏎ again: create “{ $arg1 }” and start

ohne-modell-weiter-esc-zurueck = ⏎ continue without model · Esc back

starten-tab-uebernehmen-o-finder-ordner = ⏎ start · Tab complete · ⌘O Finder · drop a folder

starten-oder-tippen-waehlt-ein-anderes-projekt = ⏎ start · ↑↓ or typing picks another project · Esc back

starten-waehlt-esc-abbrechen = ⏎ start · ↑↓ chooses · Esc cancel

zum-ersten-treffer-esc-beendet-die-suche = ⏎ first result · Esc ends the search

infoplist-nsappleeventsusagedescription = Kadrell suggests the folder of the frontmost Finder window when you press ⌘N.

infoplist-nsmicrophoneusagedescription = Claude sessions in the terminal use the microphone for voice input.

infoplist-nsusernotificationsusagedescription = Kadrell notifies you when a session is waiting for you or has finished.
