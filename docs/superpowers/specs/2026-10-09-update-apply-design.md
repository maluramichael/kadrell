# Update anwenden und neu starten: Design (Teil B)

Stand 2026-10-09. Baut auf dem Neu-starten-Feature (Teil A, v1.67.0) auf und erweitert dessen Relaunch-Helfer.
Setzt `UpdateChecker` (erkennt neuere Versionen über `download/latest.json`) voraus.

## Ziel

Ist eine neuere Version verfügbar, lädt Kadrell auf einen Befehl das DMG, prüft es, ersetzt das eigene App-Bundle
und startet sich neu (über Teil A). Ohne manuelles Herunterladen, Mounten und Hineinziehen.

## Nicht-Ziele

- Keine stille Hintergrund-Installation ohne Zutun des Nutzers. Update wird angeboten, nicht erzwungen.
- Kein privilegierter Helfer/kein `sudo`. Wenn das Bundle nicht ohne Adminrechte austauschbar ist, führt v1 nicht
  aus, sondern verweist auf die manuelle Installation aus dem DMG.
- Keine Delta-Updates, kein eigener Update-Server (es bleibt bei `latest.json` + DMG aus `tools/release.sh`).

## Entscheidungen

| Frage | Entscheidung | Grund |
|---|---|---|
| Auslöser | Eintrag „Update laden und neu starten" in Menü und Palette, nur sichtbar wenn `UpdateChecker.available != nil` | Kein toter Knopf ohne Update |
| Prüfung vor Austausch | sha256 == Manifest UND Developer-ID-Signatur/Notarisierung UND erwartete Team-ID | Sicherheitsgrenze, nicht kürzbar |
| Austausch | Erst nach dem Beenden durch den Relaunch-Helfer (Bundle ist dann nicht in Benutzung) | Ein laufendes Bundle lässt sich nicht sicher ersetzen |
| Ort | `Bundle.main.bundlePath` (dort, wo die App liegt) | Austausch am selben Ort, Pfad bleibt stabil |
| Nicht schreibbar | Abbrechen mit Hinweis auf manuelle Installation | Kein Adminrechte-Flow in v1 |

## Ablauf

1. Nutzer wählt „Update laden und neu starten". `m = UpdateChecker.available` (Version, url, sha256, notes).
2. **Download:** `m.url` (nur https) in eine private Temp-Datei laden, Fortschritt über die Statusleiste.
3. **Prüfen (vor jedem Schreiben am Bundle):**
   - sha256 der Datei == `m.sha256`, sonst Abbruch.
   - DMG mounten (`hdiutil attach -nobrowse -readonly`), `Kadrell.app` darin finden.
   - `codesign --verify --deep --strict` auf der App im DMG, `spctl -a -vv -t install` (Gatekeeper), und die
     **Team-ID gegen die fest eingebaute erwartete Team-ID** prüfen. Jede Abweichung: Abbruch, DMG unmounten, nichts ersetzen.
4. **Schreibbarkeit prüfen:** ist `Bundle.main.bundlePath` (bzw. sein Elternordner) für den Nutzer schreibbar? Wenn
   nein: Abbruch mit Hinweis „bitte aus dem DMG manuell installieren", DMG bleibt gemountet oder im Finder geöffnet.
5. **Stagen:** die geprüfte App per `ditto` in einen Temp-Ordner neben dem Ziel kopieren, DMG unmounten.
6. **Austausch + Neustart:** den Relaunch-Helfer aus Teil A erweitern: nach dem Warten auf das Prozessende erst die
   gestagte App über das alte Bundle schieben (`ditto`/`mv`, alte Version beiseite, neue an den Platz), dann `open`.
   Schlägt der Austausch fehl, die alte Version zurückschieben und normal starten.
7. Teil-A-Neustart übernimmt Rückfrage (laufende Sessions) und Session-Fortsetzung.

## Betroffene Stellen (Vorschlag)

- Neuer Service `UpdateInstaller` (Download, sha256, mount, codesign/spctl/Team-ID, stage). Reine Prüf-Logik
  (sha256-Vergleich, Team-ID-Extraktion) in testbare Funktionen trennen.
- `Profile.relaunchCommand` um einen optionalen „erst dieses Bundle an den Zielort schieben"-Schritt erweitern
  (oder ein zweiter, analoger Builder), weiter rein und testbar.
- `AppDelegate`: Menü-/Palette-Eintrag, sichtbar nur bei verfügbarem Update; stößt Download+Prüfung an, setzt bei
  Erfolg den gestagten Pfad und ruft den Teil-A-Neustart mit „Update anwenden" auf.
- Statusleiste/Panel für Fortschritt und Fehlgründe.

## Sicherheit (nicht kürzen)

- Kein Austausch ohne gültige sha256 UND gültige Developer-ID-Signatur/Notarisierung UND passende Team-ID.
- Download nur über https. Temp-Dateien in einem privaten, nur dem Nutzer lesbaren Ordner.
- Bei jeder Unsicherheit abbrechen und die alte, laufende Version unangetastet lassen.

## Tests

- Unit: sha256-Vergleich (passt/passt nicht), Team-ID-Vergleich, Relaunch-/Austausch-Kommando-Builder (gestagter
  Pfad → korrektes Verschieben-dann-öffnen, Pfade mit Leerzeichen).
- Soweit möglich: Download gegen einen lokalen Mini-Server, Mount/Hash eines Dummy-DMG.
- Ehrliche Grenze: Signatur-/Notarisierungsprüfung und der echte Austausch brauchen ein echtes signiertes,
  notarisiertes neueres Release; dieser Pfad ist hier nicht end-to-end testbar (wie zuvor OAuth/Live-APIs).

## Offene Punkte für die Umsetzung

- Erwartete Team-ID aus dem Release-Runbook holen und als Konstante fest einbauen (gegen `codesign -dvv` der
  aktuellen App verifizieren, nicht raten).
- Genauen `.app`-Pfad im DMG gegen `tools/release.sh` prüfen.
- Verhalten, wenn die App aus dem DMG selbst läuft oder an einem schreibgeschützten Ort (dann Schritt 4 greift).
- Changelog-Eintrag und Versionssprung (`minor`) beim Commit der Umsetzung.
</content>
