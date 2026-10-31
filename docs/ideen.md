# Ideen

Gesammelte Ideen. Vor der Umsetzung jeweils erst durchsprechen.

## Umgesetzt in 1.73.0

Alle Ideen vom 30.10.2026 sind in 1.73.0 eingebaut, Einzelheiten im CHANGELOG. Bewusst anders als notiert:

- Schnellstart liegt auf ⌘⇧I statt ⌘⇧N, weil ⌘⇧N schon „Remote verbinden“ ist.
- Die Extension „system-info“ liegt als eigenes Repo unter `~/development/projects/kadrell-ext-system-info`,
  noch ohne GitHub-Repo und nicht installiert.
- In den Instrument-Farbschemata ist der atmende Punkt hell bzw. dunkel statt blau, weil die Arbeitsfarbe dort die
  Schriftfarbe ist. In den übrigen Schemata folgt er deren Arbeitsfarbe.
- llama.cpp lädt Modelle nicht selbst herunter (Kadrell startet `llama-server` offline, sonst hängt der Start an
  der Online-Prüfung). Modelle kommen per `llama-server -hf <Repo>` im Terminal oder als GGUF in den Modellordner.
- Die Ollama-Suche liest ollama.com/search aus, eine offizielle Such-Schnittstelle gibt es nicht.

## Noch offen

- Lokalisierte Labels für Extension-Einstellungen (das Manifest kennt nur eine Sprache).
