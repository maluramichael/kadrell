# Alfred: Kadrell New Session

Alfred-Workflow, der die Kadrell-Projekte listet und im gewählten Projekt eine neue
Session startet, genau wie Command+N in Kadrell. Die laufende Instanz wird über den
Socket angesprochen (oder gestartet, falls sie nicht läuft).

## Benutzen

1. `Kadrell-New-Session.alfredworkflow` doppelklicken, Alfred importiert ihn.
2. In Alfred `kn` tippen. Die Liste zeigt die Projekte (Gruppen) des Standardprofils.
3. Projekt wählen, Enter: neue Session im Projekt, Kadrell kommt nach vorn.

Keyword `kn` ist in Alfred frei änderbar (Workflow öffnen, Script-Filter-Objekt).

## Voraussetzung

Kadrell.app installiert. Der Workflow findet das Binary selbst. Wer in Kadrell einmal
die CLI installiert (Menü), bekommt `~/.local/bin/kadrell` und spart die Pfadsuche.

## Bauen

Quelle liegt in `kadrell-new-session/` (info.plist wird generiert). Nach Änderungen
an den Scripts oder der Konfiguration:

```
python3 build.py
```

Das schreibt `kadrell-new-session/info.plist` neu und packt `Kadrell-New-Session.alfredworkflow`.
