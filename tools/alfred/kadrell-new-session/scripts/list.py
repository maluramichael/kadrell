#!/usr/bin/python3
# Script Filter: listet die Kadrell-Projekte (Gruppen) des Standardprofils.
# Quelle ist `kadrell ls --json`. Gefiltert wird von Alfred selbst.
import json
import os
import subprocess
import sys

SOCKET = os.path.expanduser(
    "~/Library/Application Support/de.malura.kadrell/kadrell.sock"
)


def kadrell_bin():
    local = os.path.expanduser("~/.local/bin/kadrell")
    if os.access(local, os.X_OK):
        return local
    try:
        p = subprocess.run(
            ["osascript", "-e",
             'POSIX path of (path to application id "de.malura.kadrell")'],
            capture_output=True, text=True, timeout=5,
        )
        app = p.stdout.strip()
        if app:
            return os.path.join(app, "Contents/MacOS/Kadrell")
    except Exception:
        pass
    return None


def emit(items):
    print(json.dumps({"items": items}))
    sys.exit(0)


# Ohne Socket läuft Kadrell nicht. Nicht `new`/`ls` aufrufen, sonst würde
# der CLI-Client die App starten und bis zu 30 s warten.
if not os.path.exists(SOCKET):
    emit([{"title": "Kadrell läuft nicht",
           "subtitle": "Starte Kadrell und lege ein Projekt an.",
           "valid": False}])

binary = kadrell_bin()
if not binary:
    emit([{"title": "Kadrell nicht gefunden",
           "subtitle": "Ist Kadrell.app installiert?",
           "valid": False}])

try:
    out = subprocess.run([binary, "ls", "--json"],
                         capture_output=True, text=True, timeout=5)
    data = json.loads(out.stdout)
except Exception:
    data = {"groups": []}

items = []
for g in data.get("groups", []):
    star = " ★" if g.get("favorite") else ""
    name = g.get("name", "?")
    cwd = g.get("cwd", "")
    items.append({
        "uid": g.get("id", cwd),
        "title": name + star,
        "subtitle": cwd,
        "arg": g.get("id", ""),
        "match": "{} {}".format(name, cwd),
    })

if not items:
    items = [{"title": "Keine Kadrell-Projekte",
              "subtitle": "Lege in Kadrell ein Projekt (Gruppe) an.",
              "valid": False}]

emit(items)
