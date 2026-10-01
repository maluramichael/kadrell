#!/bin/bash
# Release ohne App Store: Archiv (Release, Hardened Runtime) -> Export mit Developer ID ->
# DMG -> Notarisierung bei Apple -> Staple -> Upload nach kadrell.malura.de/download/.
#
# Einmalig vorher (interaktiv, macht Michael):
#   1. Zertifikat "Developer ID Application" im Developer-Portal anlegen und in den Schlüsselbund
#      importieren. Prüfen: security find-identity -v -p codesigning | grep "Developer ID"
#   2. xcrun notarytool store-credentials kadrell --apple-id <Apple-ID> --team-id <your-team-id>
#      (fragt nach einem App-spezifischen Passwort von appleid.apple.com)
#
# Aufruf aus dem Repo-Root: tools/release.sh            # baut, notarisiert, lädt hoch
#                           tools/release.sh --no-upload  # nur lokal bauen
#                           tools/release.sh --force      # auch wenn die Version schon auf dem Server liegt
#                           tools/release.sh --skip-gate  # Notausgang: ohne Gate (Tests, lizard, sauberer Arbeitsbaum)
# Gate vor dem Archiv (Abbruch bei Fehler): 1. Arbeitsbaum sauber (app, CHANGELOG.md, tools), 2. Tests grün
# (Log in $TMPDIR/kadrell-release-tests.log), 3. lizard ohne Warnungen. Nach erfolgreichem Upload setzt das Skript
# den Tag v<version> (nur lokal, Push-Befehl steht in der Ausgabe). Archiv-Log: build-release/archive.log, export.log.
# Die dSYMs der vorigen Version landen vor dem Aufräumen in app/build-release-archive/<version>/.
# Idempotent: liegt Kadrell-<version>.dmg schon unter /download/, passiert nichts (Exit 0).
# Alle Versionen bleiben dort liegen, Kadrell.dmg zeigt immer auf die neueste.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$(dirname "$0")/../app"

UPLOAD=1; FORCE=0; SKIP_GATE=0
for a in "$@"; do case "$a" in --no-upload) UPLOAD=0;; --force) FORCE=1;; --skip-gate) SKIP_GATE=1;; esac; done
PROFILE=kadrell                       # notarytool keychain profile
# Deploy-Ziel aus der Umgebung, optional aus der gitignored tools/deploy.local.env geladen.
[ -f "$SCRIPT_DIR/deploy.local.env" ] && . "$SCRIPT_DIR/deploy.local.env"
TARGET="${KADRELL_DEPLOY_TARGET:-}"          # z. B. user@host:/var/www/kadrell/download (SSH-Host in ~/.ssh/config)
DEPLOY_HOST="${TARGET%%:*}"; DEPLOY_PATH="${TARGET#*:}"
if [ "$UPLOAD" = 1 ] && [ -z "$TARGET" ]; then
  echo "FEHLER: KADRELL_DEPLOY_TARGET nicht gesetzt (tools/deploy.local.env oder Umgebung). Mit --no-upload nur lokal bauen." >&2; exit 1
fi
OUT=build-release
VERSION=$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' Kadrell/Info.plist)
DMG="$OUT/Kadrell-$VERSION.dmg"

security find-identity -v -p codesigning | grep -q "Developer ID Application" \
  || { echo "FEHLER: kein 'Developer ID Application'-Zertifikat im Schlüsselbund (siehe Kopf des Skripts)." >&2; exit 1; }
xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null 2>&1 \
  || { echo "FEHLER: notarytool-Profil '$PROFILE' fehlt (siehe Kopf des Skripts)." >&2; exit 1; }

if [ "$UPLOAD" = 1 ] && [ "$FORCE" = 0 ] && ssh "$DEPLOY_HOST" "test -s $DEPLOY_PATH/Kadrell-$VERSION.dmg"; then
  echo "==> Kadrell-$VERSION.dmg liegt schon auf dem Server, nichts zu tun (--force zum Überschreiben)."
  exit 0
fi

if [ "$SKIP_GATE" = 1 ]; then
  echo "==> Gate übersprungen (--skip-gate)"
else
  echo "==> Gate: Arbeitsbaum"
  DIRTY=$(git status --porcelain -- . ../CHANGELOG.md ../tools)
  [ -z "$DIRTY" ] || { echo "FEHLER: uncommittete Änderungen würden ins DMG gehen:" >&2; echo "$DIRTY" >&2; exit 1; }
  echo "==> Gate: Tests"
  TEST_LOG="${TMPDIR:-/tmp}/kadrell-release-tests.log"
  /opt/homebrew/bin/xcodegen generate >/dev/null
  xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug -derivedDataPath build \
    -skipPackagePluginValidation -skipMacroValidation test >"$TEST_LOG" 2>&1 \
    || { grep -E "error:|\*\* TEST FAILED" "$TEST_LOG" | head -20 >&2; echo "FEHLER: Tests fehlgeschlagen, Log: $TEST_LOG" >&2; exit 1; }
  grep -E "Executed [0-9]+ tests|TEST SUCCEEDED" "$TEST_LOG" | tail -2 || true
  echo "==> Gate: lizard"
  UV=$(command -v uv || echo "$HOME/.local/bin/uv")
  LIZARD=$("$UV" tool run lizard Kadrell -w) || true
  [ -z "$LIZARD" ] || { echo "$LIZARD" >&2; echo "FEHLER: lizard meldet Warnungen (CCN > 15)." >&2; exit 1; }
fi

# dSYMs der noch ausgelieferten Version sichern, sonst lassen sich Crash-Reports nicht mehr symbolisieren.
OLD_APP="$OUT/Kadrell.xcarchive/Products/Applications/Kadrell.app"
if [ -d "$OUT/Kadrell.xcarchive/dSYMs" ] && [ -f "$OLD_APP/Contents/Info.plist" ]; then
  OLD_VERSION=$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$OLD_APP/Contents/Info.plist" 2>/dev/null || true)
  if [ -n "$OLD_VERSION" ]; then
    mkdir -p "build-release-archive/$OLD_VERSION"
    cp -R "$OUT/Kadrell.xcarchive/dSYMs/." "build-release-archive/$OLD_VERSION/"
    echo "==> dSYMs von $OLD_VERSION gesichert: app/build-release-archive/$OLD_VERSION/"
  fi
fi
rm -rf "$OUT"; mkdir -p "$OUT"
/opt/homebrew/bin/xcodegen generate >/dev/null

echo "==> Archiv $VERSION"
# Hardened Runtime und Entitlements nur hier, nicht in project.yml: Debug-Builds bleiben unverändert.
xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Release \
  -archivePath "$OUT/Kadrell.xcarchive" -derivedDataPath "$OUT/DerivedData" \
  -skipPackagePluginValidation -skipMacroValidation \
  ENABLE_HARDENED_RUNTIME=YES CODE_SIGN_ENTITLEMENTS=Kadrell/Kadrell.entitlements \
  archive 2>&1 | tee "$OUT/archive.log" | grep -E "error|warning: .*sign|ARCHIVE" || true
[ -d "$OUT/Kadrell.xcarchive/Products/Applications/Kadrell.app" ] || { echo "FEHLER: Archiv fehlt, siehe app/$OUT/archive.log." >&2; exit 1; }

echo "==> Export mit Developer ID"
xcodebuild -exportArchive -archivePath "$OUT/Kadrell.xcarchive" \
  -exportOptionsPlist ExportOptions.plist -exportPath "$OUT/export" 2>&1 | tee "$OUT/export.log" | grep -E "error|EXPORT" || true
APP="$OUT/export/Kadrell.app"
[ -d "$APP" ] || { echo "FEHLER: Export fehlt, siehe app/$OUT/export.log." >&2; exit 1; }
codesign --verify --deep --strict "$APP"
codesign -d --entitlements - "$APP" 2>/dev/null | grep -q audio-input || { echo "FEHLER: Mikrofon-Entitlement fehlt." >&2; exit 1; }

echo "==> DMG"
STAGE=$(mktemp -d); cp -R "$APP" "$STAGE/"; ln -s /Applications "$STAGE/Applications"
hdiutil create -quiet -volname "Kadrell $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
rm -rf "$STAGE"
codesign --sign "Developer ID Application" --timestamp "$DMG"

echo "==> Notarisierung (wartet auf Apple, meist 1 bis 5 Minuten)"
xcrun notarytool submit "$DMG" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$DMG"
spctl -a -t open --context context:primary-signature -v "$DMG" 2>&1 | tail -1

echo "==> Manifest (Update-Prüfung der App, siehe UpdateChecker.swift)"
SHA256_FILE="$DMG.sha256"
SHA=$(shasum -a 256 "$DMG" | awk '{print $1}')
echo "$SHA" > "$SHA256_FILE"
# Changelog-Zeilen der Version als JSON-Array, gleiche Logik wie WhatsNew.notes(version:changelog:).
NOTES_JSON=$(python3 - "$VERSION" <<'PY'
import json, sys
version = sys.argv[1]
lines, in_section = [], False
with open("../CHANGELOG.md", encoding="utf-8") as f:
    for raw in f:
        raw = raw.rstrip("\n")
        if raw.startswith("## "):
            if in_section: break
            in_section = raw == f"## {version}" or raw.startswith(f"## {version} ")
            continue
        if not in_section: continue
        t = raw.strip()
        if t.startswith("- "): lines.append(t[2:])
print(json.dumps(lines, ensure_ascii=False))
PY
)
if [ "$UPLOAD" = 1 ] && [ "$NOTES_JSON" = "[]" ]; then
  echo "FEHLER: CHANGELOG.md hat keine Einträge unter '## $VERSION', latest.json hätte leere notes. Kein Upload." >&2; exit 1
fi
cat > "$OUT/latest.json" <<JSON
{"version": "$VERSION", "url": "https://kadrell.malura.de/download/Kadrell-$VERSION.dmg", "sha256": "$SHA", "notes": $NOTES_JSON}
JSON

if [ "$UPLOAD" = 1 ]; then
  echo "==> Upload nach $TARGET"
  ssh "$DEPLOY_HOST" "mkdir -p $DEPLOY_PATH"
  # Kein --chmod: das macOS-System-rsync (openrsync) kennt --chmod=F644 nicht. Rechte lokal setzen.
  chmod 644 "$DMG" "$SHA256_FILE" "$OUT/latest.json"
  rsync -a --no-o --no-g "$DMG" "$SHA256_FILE" "$OUT/latest.json" "$TARGET/"
  ssh "$DEPLOY_HOST" "cd $DEPLOY_PATH && cp -f Kadrell-$VERSION.dmg Kadrell.dmg && cp -f Kadrell-$VERSION.dmg.sha256 Kadrell.dmg.sha256 && chmod 644 *.dmg *.sha256 latest.json && chown -R www-data:www-data $DEPLOY_PATH"
  echo "==> https://kadrell.malura.de/download/Kadrell-$VERSION.dmg (und /download/Kadrell.dmg, /download/latest.json)"
  if git rev-parse -q --verify "refs/tags/v$VERSION" >/dev/null; then
    echo "==> Tag v$VERSION existiert schon, nicht neu gesetzt."
  else
    git tag -a "v$VERSION" -m "Kadrell $VERSION"
    echo "==> Tag v$VERSION gesetzt. Pushen: git push github v$VERSION && git push vault v$VERSION"
  fi
fi
echo "==> fertig: $DMG"
