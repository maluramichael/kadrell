#!/bin/bash
# Run Script: startet eine neue Session in der gewählten Projektgruppe ($1 = Gruppen-Id).
# Entspricht Command+N in Kadrell. Spricht die laufende Instanz an oder startet sie.
set -euo pipefail

id="$1"

bin="$HOME/.local/bin/kadrell"
if [ ! -x "$bin" ]; then
  app="$(osascript -e 'POSIX path of (path to application id "de.malura.kadrell")' 2>/dev/null || true)"
  bin="$app/Contents/MacOS/Kadrell"
fi

"$bin" new -t "$id"
open -b de.malura.kadrell
