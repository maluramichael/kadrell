# Kadrell

Mac-App, Aufbau und Build: `app/README.md`.

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

## Release (kein App Store)

Vertrieb per Developer ID und Notarisierung, Download über https://kadrell.malura.de. Der Mac App Store
verlangt die App Sandbox, und die würde `claude` als Kindprozess den Zugriff auf `~/.claude`, Projektordner
und Schlüsselbund nehmen. `tools/release.sh` archiviert (Release, Hardened Runtime, Mikrofon-Entitlement),
exportiert mit Developer ID, baut das DMG, notarisiert, stapelt und lädt nach `/var/www/kadrell/download/`.
Einmalige Vorbereitung (Zertifikat, `notarytool store-credentials kadrell`) steht im Kopf des Skripts.
Landingpage: `../kadrell.malura.de`.
