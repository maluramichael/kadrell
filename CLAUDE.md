# Kadrell

Mac-App, Aufbau und Build: `app/README.md`.

## Positionierung (für Agenten, vor Feature-Entscheidungen lesen)

Kadrell hält alle parallel laufenden Claude-Code-Sessions eines Nutzers in einem Fenster sichtbar und bedienbar,
für Leute mit sehr vielen gleichzeitigen Sessions über mehrere Projekte. Nicht-Ziele: keine zoombare Karte (das
war der verworfene erste Entwurf), kein eigener Daemon oder Server, keine App-Store-Sandbox
(würde Claude Code den Zugriff auf Projektordner und Schlüsselbund nehmen). Maßstab für neue Features: komplett
tastaturbedienbar, kein zweites Tool neben Kadrell (weitere Fenster derselben Instanz sind ok), Bedienung folgt tmux-Gewohnheiten statt eigener
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

## Codequalität: keine Warnungen, Komplexität niedrig

- Jede Änderung baut ohne eigene Compiler-Warnungen (Clean Build). Einzige Ausnahme ist der Toolchain-Hinweis
  `appintentsmetadataprocessor: Metadata extraction skipped`, der kommt nicht aus unserem Code.
- `lizard` bleibt ohne Warnungen (CCN ≤ 15): `cd app && uv tool run lizard Kadrell -w` muss leer sein.
- Wird eine Funktion zu verzweigt, in kleine benannte Funktionen zerlegen, statt Zweige anzuhängen. Vorhandene
  Services und Helfer wiederverwenden, keine Logik duplizieren.
- Vor dem Commit beides prüfen, genau wie die Tests.

## Zweisprachig: jeder Nutzertext über das Bundle der eingestellten Sprache

Kadrell ist deutsch und englisch, umschaltbar in den Einstellungen ohne Neustart (ein Neustart würde alle
Claude-Prozesse mitnehmen). Deshalb:

- Jeder neue Nutzertext in AppKit: `String(localized: "…", bundle: Bundle.app)`. Ohne `bundle:` bleibt er beim
  Umschalten in der Startsprache stehen; `LocalizationTests` schlägt dann fehl.
- SwiftUI-Dialoge erben die Sprache über `\.locale` (siehe `OverlayPanel`), dort genügt `Text("…")`.
- Deutsch und Englisch gehören im selben Arbeitsgang in `app/Kadrell/Localization/Localizable.xcstrings`.
- Texte, die beim Erzeugen gebaut werden (Palette), müssen beim Sprachwechsel neu entstehen, siehe `applyAppearance`.

## Testen an der laufenden App: immer ein frisches Profil

Das eigene Kadrell (Standardprofil) nie beenden, neu starten oder dessen Daten anfassen. Zum Testen den Debug-Build
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
- Für Sichtprüfungen einen temporären Render-Test schreiben und im Temp-Profil laufen lassen, damit er die eigenen
  Einstellungen nicht anfasst: `TEST_RUNNER_KADRELL_PROFILE=tmp xcodebuild … test -only-testing:KadrellTests/<Test>`.

### Parallel an mehreren Features arbeiten

Weil jedes Temp-Profil eigene Sessions, Einstellungen, Socket und Lock hat, können beliebig viele Sessions gleichzeitig
an verschiedenen Features arbeiten und testen, ohne sich oder das eigene Kadrell zu stören:

1. Jede Session arbeitet in ihrem eigenen Worktree und baut dort (`app/build` liegt pro Worktree getrennt).
2. Jede startet ihren eigenen Build mit `--profile tmp`, bekommt damit ein eigenes Profil und findet ihre Instanz über
   die pid ihres Build-Pfads (`/bin/ps -axo pid=,args= | grep "<worktree>/app/build.*profile tmp"`).
3. Nur die eigene Instanz beenden, nie per Name (`pkill Kadrell` trifft alle Profile).

## Release

Vertrieb per Developer ID und Notarisierung, Download über https://kadrell.malura.de, bewusst nicht über den Mac
App Store (die Sandbox würde `claude` als Kindprozess den Zugriff auf `~/.claude`, Projektordner und Schlüsselbund
nehmen). Die genauen Release-Schritte stehen im privaten Runbook und laufen nur lokal auf dem Mac.
