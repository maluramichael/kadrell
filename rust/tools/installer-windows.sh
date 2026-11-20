#!/bin/bash
# Baut den Windows-Installer (Inno Setup) aus dem Release-Binary. Nur unter Windows (Git Bash).
#
#   rust/tools/installer-windows.sh [--binary <pfad>]
#
# Ohne Optionen: `rust/target/release/kadrell.exe` (vorher `cargo build --release -p kadrell`).
# Ausgabe: `rust/target/installer/Kadrell-Setup-<version>.exe`. Braucht Inno Setup 6 und mt.exe (Windows SDK)
# (`winget install -e --id JRSoftware.InnoSetup --scope user`). Bewusst unsigniert, kein Release.
set -euo pipefail
unset CDPATH

here="$(cd "$(dirname "$0")" && pwd)"
rust_dir="$(cd "$here/.." && pwd)"
binary="$rust_dir/target/release/kadrell.exe"

while [ $# -gt 0 ]; do
    case "$1" in
        --binary) binary="${2:?--binary braucht einen Pfad}"; shift ;;
        -h|--help) sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unbekannte Option: $1" >&2; exit 2 ;;
    esac
    shift
done

[ -f "$binary" ] || { echo "Fehlt: $binary (vorher cargo build --release -p kadrell)" >&2; exit 1; }
iscc="$(command -v iscc || true)"
for dir in "${LOCALAPPDATA:-}/Programs/Inno Setup 6" "${PROGRAMFILES:-}/Inno Setup 6" "${PROGRAMFILES:-} (x86)/Inno Setup 6"; do
    [ -n "$iscc" ] || { [ -f "$dir/ISCC.exe" ] && iscc="$dir/ISCC.exe"; } || true
done
[ -n "$iscc" ] || { echo "Inno Setup fehlt: winget install -e --id JRSoftware.InnoSetup --scope user" >&2; exit 1; }

# Version aus [workspace.package] in rust/Cargo.toml (wie bundle-mac.sh).
version="$(awk '/^\[workspace.package\]/{p=1;next} /^\[/{p=0} p && /^version[[:space:]]*=/{gsub(/"/,"",$3); print $3; exit}' "$rust_dir/Cargo.toml")"
[ -n "$version" ] || { echo "Keine Version in $rust_dir/Cargo.toml gefunden." >&2; exit 1; }

# mt.exe aus dem neuesten Windows SDK: führt installer/console.manifest mit dem Manifest von gpui-pre zusammen,
# damit eine Verknüpfung kein Konsolenfenster öffnet. Nur an einer Kopie, das Cargo-Binary bleibt unverändert.
mt="$(ls -d "${PROGRAMFILES:-} (x86)/Windows Kits/10/bin/"10.*/x64/mt.exe 2>/dev/null | sort -V | tail -1)"
[ -n "$mt" ] || { echo "mt.exe fehlt (Windows SDK, kommt mit den Visual Studio Build Tools)." >&2; exit 1; }
stage="$rust_dir/target/installer/stage"
mkdir -p "$stage"
cp "$binary" "$stage/kadrell.exe"
exe_w="$(cygpath -w "$stage/kadrell.exe")"

# Git Bash würde "/Q", "/D..." und "-...resource:" sonst als Pfade umschreiben.
export MSYS2_ARG_CONV_EXCL="*"
"$mt" -nologo "-inputresource:$exe_w;#1" -manifest "$(cygpath -w "$rust_dir/installer/console.manifest")" "-outputresource:$exe_w;#1"
"$iscc" /Q "/DAppVersion=$version" "/DBinary=$exe_w" "$(cygpath -w "$rust_dir/installer/kadrell.iss")"
echo "Installer: $rust_dir/target/installer/Kadrell-Setup-$version.exe"
