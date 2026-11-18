#!/bin/bash
# Baut aus dem Rust-Binary ein Kadrell.app-Bündel (nur macOS), ad-hoc signiert mit Hardened Runtime.
#
#   rust/tools/bundle-mac.sh [--release] [--binary <pfad>] [--out <ordner>]
#
# Ohne Optionen: Debug-Binary `rust/target/debug/kadrell`, Ausgabe `rust/target/bundle/Kadrell.app`.
# Gibt es das Binary nicht, wird nichts gebaut (vorher `cargo build -p kadrell`).
#
# Bewusst keine Developer-ID-Signatur, keine Notarisierung, kein Release.
# Die Bundle-ID ist eine eigene (`de.malura.kadrell.rust`), damit die Rust-Variante nie mit der Swift-App
# (`de.malura.kadrell`), ihren Einstellungen oder ihren TCC-Einträgen (Mikrofon, Automation) kollidiert.
set -euo pipefail
unset CDPATH

BUNDLE_ID="de.malura.kadrell.rust"
EXECUTABLE="kadrell"

here="$(cd "$(dirname "$0")" && pwd)"
rust_dir="$(cd "$here/.." && pwd)"
repo_dir="$(cd "$rust_dir/.." && pwd)"
swift_plist="$repo_dir/app/Kadrell/Info.plist"
entitlements="$repo_dir/app/Kadrell/Kadrell.entitlements"
icon="$repo_dir/app/Kadrell/Resources/Kadrell.icns"

profile="debug"
binary=""
out_dir="$rust_dir/target/bundle"

while [ $# -gt 0 ]; do
    case "$1" in
        --release) profile="release" ;;
        --binary) binary="${2:?--binary braucht einen Pfad}"; shift ;;
        --out) out_dir="${2:?--out braucht einen Ordner}"; shift ;;
        -h|--help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unbekannte Option: $1" >&2; exit 2 ;;
    esac
    shift
done

[ "$(uname -s)" = "Darwin" ] || { echo "bundle-mac.sh läuft nur auf macOS." >&2; exit 1; }
[ -n "$binary" ] || binary="$rust_dir/target/$profile/$EXECUTABLE"
for f in "$binary" "$swift_plist" "$entitlements" "$icon"; do
    [ -f "$f" ] || { echo "Fehlt: $f" >&2; exit 1; }
done

# Version aus [workspace.package] in rust/Cargo.toml.
version="$(awk '/^\[workspace.package\]/{p=1;next} /^\[/{p=0} p && /^version[[:space:]]*=/{gsub(/"/,"",$3); print $3; exit}' "$rust_dir/Cargo.toml")"
[ -n "$version" ] || { echo "Keine Version in $rust_dir/Cargo.toml gefunden." >&2; exit 1; }

app="$out_dir/Kadrell.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/$EXECUTABLE"
cp "$icon" "$app/Contents/Resources/Kadrell.icns"

# Info.plist: Vorlage der Swift-App (Nutzungstexte, Mindest-macOS, Icon, Copyright), nur Kennung und Version
# werden ersetzt. So laufen Texte und LSMinimumSystemVersion nie auseinander.
plist="$app/Contents/Info.plist"
cp "$swift_plist" "$plist"
pb() { /usr/libexec/PlistBuddy -c "$1" "$plist"; }
pb "Set :CFBundleExecutable $EXECUTABLE"
pb "Set :CFBundleIdentifier $BUNDLE_ID"
pb "Set :CFBundleShortVersionString $version"
pb "Set :CFBundleVersion $version"
plutil -lint "$plist" >/dev/null

# Ad-hoc signieren: Hardened Runtime plus Entitlements der Swift-App (Mikrofon, Apple Events), sonst nichts.
codesign --force --sign - --options runtime --entitlements "$entitlements" --timestamp=none "$app"
codesign --verify --strict --verbose=1 "$app" 2>&1 | sed 's/^/codesign: /'

echo "Bündel: $app"
echo "  Version $version, Bundle-ID $BUNDLE_ID, Binary $binary"
