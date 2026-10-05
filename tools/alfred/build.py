#!/usr/bin/env python3
"""Baut info.plist und das importierbare Kadrell-New-Session.alfredworkflow.

Aufruf: python3 build.py
Quelle der Scripts: kadrell-new-session/scripts/. Nach Änderungen neu laufen lassen.
"""
import os
import plistlib
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
BUNDLE = os.path.join(HERE, "kadrell-new-session")
OUT = os.path.join(HERE, "Kadrell-New-Session.alfredworkflow")

SCRIPTFILTER = "6A1C0F10-0001-4000-8000-000000000001"
RUNSCRIPT = "6A1C0F10-0002-4000-8000-000000000002"

plist = {
    "bundleid": "de.malura.kadrell.alfred.newsession",
    "name": "Kadrell New Session",
    "description": "Listet die Kadrell-Projekte und startet im gewählten Projekt eine neue Session (wie Command+N).",
    "createdby": "Michael Malura",
    "webaddress": "https://kadrell.malura.de",
    "disabled": False,
    "version": "1.0.0",
    "readme": (
        "Keyword `kn` zeigt die Kadrell-Projekte (Gruppen des Standardprofils).\n"
        "Enter startet im gewählten Projekt eine neue Session und holt Kadrell nach vorn.\n\n"
        "Setzt Kadrell.app voraus. Schneller wird es, wenn in Kadrell einmal die "
        "CLI installiert wurde (Menü), dann liegt ~/.local/bin/kadrell bereit."
    ),
    "objects": [
        {
            "type": "alfred.workflow.input.scriptfilter",
            "uid": SCRIPTFILTER,
            "version": 3,
            "config": {
                "alfredfiltersresults": True,
                "alfredfiltersresultsmatchmode": 0,
                "argumenttrimmode": 0,
                "argumenttype": 1,
                "escaping": 0,
                "keyword": "kn",
                "queuedelaycustom": 3,
                "queuedelayimmediatelyinitially": True,
                "queuedelaymode": 0,
                "queuemode": 2,
                "runningsubtext": "Projekte laden …",
                "script": "",
                "scriptargtype": 1,
                "scriptfile": "scripts/list.py",
                "subtext": "Kadrell-Projekt wählen, Enter startet eine neue Session",
                "title": "Kadrell New Session",
                "type": 8,
                "withspace": True,
            },
        },
        {
            "type": "alfred.workflow.action.script",
            "uid": RUNSCRIPT,
            "version": 2,
            "config": {
                "concurrently": False,
                "escaping": 0,
                "script": "",
                "scriptargtype": 1,
                "scriptfile": "scripts/new.sh",
                "type": 8,
            },
        },
    ],
    "connections": {
        SCRIPTFILTER: [
            {
                "destinationuid": RUNSCRIPT,
                "modifiers": 0,
                "modifiersubtext": "",
                "vitoclose": False,
            }
        ],
        RUNSCRIPT: [],
    },
    "uidata": {
        SCRIPTFILTER: {"xpos": 180.0, "ypos": 120.0},
        RUNSCRIPT: {"xpos": 480.0, "ypos": 120.0},
    },
}

with open(os.path.join(BUNDLE, "info.plist"), "wb") as f:
    plistlib.dump(plist, f)

if os.path.exists(OUT):
    os.remove(OUT)
with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as z:
    for root, _, files in os.walk(BUNDLE):
        for name in files:
            full = os.path.join(root, name)
            z.write(full, os.path.relpath(full, BUNDLE))

print("info.plist + %s gebaut." % os.path.basename(OUT))
