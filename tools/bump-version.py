#!/usr/bin/env python3
"""Hebt die Kadrell-Version an: bump-version.py patch|minor|major

Setzt CFBundleShortVersionString (x.y.z) und CFBundleVersion (+1) in app/project.yml und
app/Kadrell/Info.plist und macht aus "## Unreleased" in CHANGELOG.md "## x.y.z (Datum)".
"""
import datetime
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
PROJECT = ROOT / "app/project.yml"
PLIST = ROOT / "app/Kadrell/Info.plist"
CHANGELOG = ROOT / "CHANGELOG.md"


def bump(version, part):
    major, minor, patch = map(int, version.split("."))
    if part == "major":
        return f"{major + 1}.0.0"
    if part == "minor":
        return f"{major}.{minor + 1}.0"
    return f"{major}.{minor}.{patch + 1}"


def sub_once(pattern, repl, text, path):
    new, n = re.subn(pattern, repl, text, count=1)
    if n != 1:
        sys.exit(f"{path.name}: Muster nicht gefunden: {pattern}")
    return new


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in ("patch", "minor", "major"):
        sys.exit("Aufruf: tools/bump-version.py patch|minor|major")

    changelog = CHANGELOG.read_text()
    if not re.search(r"^## Unreleased\s*\n+- ", changelog, re.M):
        sys.exit("CHANGELOG.md: erst Einträge unter '## Unreleased' schreiben.")

    project = PROJECT.read_text()
    plist = PLIST.read_text()
    old = re.search(r'CFBundleShortVersionString: "(\d+\.\d+\.\d+)"', project).group(1)
    build = int(re.search(r'CFBundleVersion: "(\d+)"', project).group(1)) + 1
    new = bump(old, sys.argv[1])

    project = sub_once(r'(CFBundleShortVersionString: )"[\d.]+"', rf'\g<1>"{new}"', project, PROJECT)
    project = sub_once(r'(CFBundleVersion: )"\d+"', rf'\g<1>"{build}"', project, PROJECT)
    plist = sub_once(r"(<key>CFBundleShortVersionString</key>\s*<string>)[\d.]+", rf"\g<1>{new}", plist, PLIST)
    plist = sub_once(r"(<key>CFBundleVersion</key>\s*<string>)\d+", rf"\g<1>{build}", plist, PLIST)
    changelog = sub_once(r"(?m)^## Unreleased", f"## {new} ({datetime.date.today():%Y-%m-%d})", changelog, CHANGELOG)

    PROJECT.write_text(project)
    PLIST.write_text(plist)
    CHANGELOG.write_text(changelog)
    print(f"{old} -> {new} (Build {build})")


if __name__ == "__main__":
    assert bump("1.2.3", "patch") == "1.2.4"
    assert bump("1.2.3", "minor") == "1.3.0"
    assert bump("1.2.3", "major") == "2.0.0"
    main()
