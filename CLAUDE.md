# Kadrell

Mac-App, Aufbau und Build: `app/README.md`.

## Positionierung (für Agenten, vor Feature-Entscheidungen lesen)

Kadrell hält alle parallel laufenden Claude-Code-Sessions eines Nutzers in einem Fenster sichtbar und bedienbar,
für Leute mit sehr vielen gleichzeitigen Sessions über mehrere Projekte. Nicht-Ziele: keine zoombare Karte (das
war der verworfene erste Entwurf, siehe `prototype/`), kein eigener Daemon oder Server, keine App-Store-Sandbox
(würde Claude Code den Zugriff auf Projektordner und Schlüsselbund nehmen). Maßstab für neue Features: komplett
tastaturbedienbar, alles bleibt in diesem einen Fenster, Bedienung folgt tmux-Gewohnheiten statt eigener
Erfindungen. Passt ein Vorschlag da nicht rein, erst nachfragen statt bauen.

## Changelog und Version bei jedem Commit/Merge

1. Nutzersichtbare Änderungen oben in `CHANGELOG.md` unter `## Unreleased` eintragen (Abschnitt anlegen,
   falls er fehlt). Neueste Einträge immer oben.
   - Eine Zeile pro Änderung, für normale Nutzer geschrieben: `- Feature: Sidebar kann jetzt maximal 900 px breit sein.`
   - Präfixe: `Feature:`, `Fix:`, `Änderung:`, `Entfernt:`.
   - Kein Code, keine Dateinamen, keine Klassen. Reine Interna (Refactor, Tests, Doku) nicht eintragen.
2. Selbst entscheiden, wie groß der Sprung ist, dann `python3 tools/bump-version.py <teil>`:
   - `patch`: nur kleine Bugfixes.
   - `minor`: neue Features oder mehrere spürbare Änderungen, nichts bricht.
   - `major`: etwas richtig Großes oder Brechendes (Bedienung grundlegend anders, Daten/Einstellungen inkompatibel).
3. Das Skript setzt die Version in `app/project.yml` und `app/Kadrell/Info.plist` und macht aus
   `## Unreleased` die Versionsüberschrift. Die drei Dateien mit committen.

Die Version steht in der App unter Einstellungen (⌘,) und im About (F1).

## Testen an der laufenden App: immer ein frisches Profil

Michaels eigenes Kadrell (Standardprofil) nie beenden, neu starten oder dessen Daten anfassen. Zum Testen den Debug-Build
selbst starten, immer mit frischem Temp-Profil:

```bash
open -n app/build/Build/Products/Debug/Kadrell.app --args --profile tmp
```

- `tmp` legt Sessions, Gruppen, Socket und Lock in `$TMPDIR/kadrell-tmp-<pid>/`, übernimmt die Einstellungen des
  Standardprofils (ohne Auswahl) und löscht beim Beenden alles wieder. Keine Hintergrund-Sessions übernehmen.
- Fernsteuern über den Socket des Profils: `KADRELL_SOCKET=$TMPDIR/kadrell-tmp-<pid>/kadrell.sock app/build/Build/Products/Debug/Kadrell.app/Contents/MacOS/Kadrell ls`.
- Beenden nur die eigene Testinstanz: `kill -TERM <pid>` (pid aus `/bin/ps -axo pid=,args= | grep "profile tmp"`).
- Benannte Profile (`--profile <name>`) bleiben unter `~/Library/Application Support/de.malura.kadrell/profiles/<name>/`
  liegen, zum Testen deshalb nicht verwenden.
- Reste abgestürzter oder beendeter Temp-Profile (plist, Temp-Ordner) räumt jeder Start weg, nichts von Hand löschen.
- Screenshots der laufenden Instanz gehen nicht (`screencapture` hat in der Tool-Shell kein Bildschirmaufnahme-Recht).
  Für Sichtprüfungen einen temporären Render-Test schreiben und im Temp-Profil laufen lassen, damit er Michaels
  Einstellungen nicht anfasst: `TEST_RUNNER_KADRELL_PROFILE=tmp xcodebuild … test -only-testing:KadrellTests/<Test>`.

### Parallel an mehreren Features arbeiten

Weil jedes Temp-Profil eigene Sessions, Einstellungen, Socket und Lock hat, können beliebig viele Sessions gleichzeitig
an verschiedenen Features arbeiten und testen, ohne sich oder Michaels Kadrell zu stören:

1. Jede Session arbeitet in ihrem eigenen Worktree und baut dort (`app/build` liegt pro Worktree getrennt).
2. Jede startet ihren eigenen Build mit `--profile tmp`, bekommt damit ein eigenes Profil und findet ihre Instanz über
   die pid ihres Build-Pfads (`/bin/ps -axo pid=,args= | grep "<worktree>/app/build.*profile tmp"`).
3. Nur die eigene Instanz beenden, nie per Name (`pkill Kadrell` trifft alle Profile und Michaels App).

## Release (kein App Store)

Vertrieb per Developer ID und Notarisierung, Download über https://kadrell.malura.de. Der Mac App Store
verlangt die App Sandbox, und die würde `claude` als Kindprozess den Zugriff auf `~/.claude`, Projektordner
und Schlüsselbund nehmen. `tools/release.sh` archiviert (Release, Hardened Runtime, Mikrofon-Entitlement),
exportiert mit Developer ID, baut das DMG, notarisiert, stapelt und lädt nach `/var/www/kadrell/download/`.
Einmalige Vorbereitung (Zertifikat, `notarytool store-credentials kadrell`) steht im Kopf des Skripts.
Landingpage: `../kadrell.malura.de`.

**Wann releasen: nur nach Rückfrage, und selten.** Jedes Release geht zur Notarisierung an Apple, das soll
nicht nach jedem Fix passieren. Änderungen sammeln, mehrere Features und Fixes kommen gemeinsam in ein Release.
Nach einem Auftrag normal den Debug-Build bauen (`app/README.md`), committen und pushen: Michael testet lokal.
`tools/release.sh` erst laufen lassen, wenn Michael es ausdrücklich will. Wenn sich seit dem letzten DMG
(Version auf dem Server) spürbar viel angesammelt hat, darf Claude im Abschlussbericht in einer Zeile fragen,
ob released werden soll, aber nicht selbst starten. Reine Interna, Doku, Tests: nie Anlass für ein Release.
Notarisierung dauert 1 bis 5 Minuten, das Skript wartet. Danach die Version und den Link im Abschlussbericht nennen.
Nach dem Release in `../kadrell.malura.de/index.html` die Versionsnummer im Hero-Pill („Version x.y.z“) nachziehen
und dort pushen (deployt automatisch).
Danach die frisch gebaute Release-App lokal installieren, damit Michael sie per ⌘Space startet:
`xcrun stapler staple app/build-release/export/Kadrell.app && rm -rf /Applications/Kadrell.app && ditto app/build-release/export/Kadrell.app /Applications/Kadrell.app`,
prüfen mit `spctl -a -vv /Applications/Kadrell.app` (muss `accepted` melden). Die laufende App nicht neu starten.
Das kann nur auf dem Mac laufen (Xcode, Schlüsselbund), nicht per Hook auf examplehost.
Alle Versionen bleiben unter https://kadrell.malura.de/download/ liegen (Caddy-Listing), `Kadrell.dmg`
ist immer die neueste. App-Größe: rund 7 MB, DMG rund 3,5 MB.
