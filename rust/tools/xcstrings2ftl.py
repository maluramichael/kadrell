"""Wandelt die Xcode-String-Kataloge der Mac-App in Fluent-Dateien für kadrell-i18n um.

Aufruf (im Repo-Root): uv run --no-project python rust/tools/xcstrings2ftl.py [--check]

Quellen:  app/Kadrell/Localization/Localizable.xcstrings, InfoPlist.xcstrings (Quellsprache de)
Ausgabe:  rust/crates/kadrell-i18n/locales/{de,en}/kadrell.ftl
          rust/crates/kadrell-i18n/src/catalog.rs (deutscher Schlüssel -> Nachrichten-Id, sortiert)

Nachrichten-Id (Fluent-Bezeichner, nur [a-z0-9-]):
  1. Slug des deutschen Schlüssels: Umlaute ae/oe/ue/ss, Kleinbuchstaben, Platzhalter (%@, %lld, ...) entfallen,
     alles Übrige wird zu "-", gekürzt auf 48 Zeichen am Wortrand. Beginnt der Slug nicht mit einem Buchstaben,
     bekommt er das Präfix "m-".
  2. Ist der Slug leer oder teilen ihn mehrere Schlüssel, bekommen alle diese Schlüssel den Zusatz
     "-<6 Hex von sha256(Schlüssel)>" (bei weiterer Kollision mehr Hex-Zeichen). Eindeutigkeit wird geprueft.
  3. Eintraege aus InfoPlist.xcstrings bekommen das Präfix "infoplist-".
Eintraege mit shouldTranslate=false (Markennamen, Symbole) und ohne Lokalisierungen entfallen.

Variablen: der n-te Platzhalter (%@ oder %lld, auch %1$@) wird zu $argN. "%%" wird "%". Ein einzelnes "%" ohne
Umwandlung bleibt ein "%". Plural-Varianten werden zu Selektoren über die erste Zahl ($argN des ersten %lld),
Substitutionen (%#@name@) zu Inline-Selektoren über ihr argNum.
"""
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LOC = ROOT / "app/Kadrell/Localization"
OUT = ROOT / "rust/crates/kadrell-i18n"
LANGS = ["de", "en"]
PLURAL_ORDER = ["zero", "one", "two", "few", "many", "other"]

PLACEHOLDER = re.compile(r"%(?:(\d+)\$)?(ll|l)?([@dDiufsxX])|%%|%#@(\w+)@")
TRANSLIT = str.maketrans({"ä": "ae", "ö": "oe", "ü": "ue", "ß": "ss", "Ä": "ae", "Ö": "oe", "Ü": "ue"})


def slug(key: str) -> str:
    text = PLACEHOLDER.sub(" ", key).translate(TRANSLIT).lower()
    text = re.sub(r"[^a-z0-9]+", "-", text).strip("-")
    if len(text) > 48:
        text = text[:48].rsplit("-", 1)[0] if "-" in text[:48] else text[:48]
    if text and not text[0].isalpha():
        text = "m-" + text
    return text


def hex_of(key: str, n: int) -> str:
    return hashlib.sha256(key.encode("utf8")).hexdigest()[:n]


def with_hash(base: str, key: str, prefix: str, n: int) -> str:
    return (base + "-" if base else prefix + "m-") + hex_of(key, n)


def assign_ids(keys: list[str], prefix: str) -> dict[str, str]:
    """Id je Schlüssel, kollisionsfrei (siehe Moduldoku)."""
    base = {k: prefix + slug(k) if slug(k) else "" for k in keys}
    count: dict[str, int] = {}
    for b in base.values():
        count[b] = count.get(b, 0) + 1
    ids = {k: base[k] if base[k] and count[base[k]] == 1 else with_hash(base[k], k, prefix, 6) for k in keys}
    for n in (8, 10, 12):  # Hash-Kollision: verlängern
        seen: dict[str, str] = {}
        for k, i in list(ids.items()):
            if i in seen:
                for kk in (k, seen[i]):
                    ids[kk] = with_hash(base[kk], kk, prefix, n)
            seen[i] = k
    assert len(set(ids.values())) == len(ids), "Id-Kollision"
    return ids


def text_chunk(s: str, lead: bool = False, trail: bool = False) -> str:
    """Literaltext als Fluent-Muster; Sonderzeichen als String-Literal. Leerraum am Anfang/Ende des ganzen Werts
    (lead/trail) würde Fluent abschneiden und wird deshalb ebenfalls als Literal geschrieben."""
    esc = {"\\": "\\\\", "\n": "\\u000A", "\r": "\\u000D", "\t": "\\u0009", '"': '\\"'}
    out = []
    for i, ch in enumerate(s):
        edge = ((lead and i == 0) or (trail and i == len(s) - 1)) and ch == " "
        if ch in "{}\n\r\t" or edge:
            out.append('{ "' + esc.get(ch, ch) + '" }')
        else:
            out.append(ch)
    return "".join(out)


def sub_variants(sub: dict) -> list[tuple[str, str]]:
    plural = sub["variations"]["plural"]
    return [(c, plural[c]["stringUnit"]["value"]) for c in PLURAL_ORDER if c in plural]


def inline_select(sub: dict) -> str:
    n = sub["argNum"]
    lines = ["{ $arg%d ->" % n]
    for cat, text in sub_variants(sub):
        body = ("{ $arg%d }" % n).join(convert(part)[0] for part in text.split("%arg"))  # %arg = die Zahl selbst
        lines.append("        %s[%s] %s" % ("*" if cat == "other" else " ", cat, body))
    return "\n".join(lines) + "\n    }"


def convert(value: str, subs: dict | None = None) -> tuple[str, list[int]]:
    """Wert -> (Fluent-Muster, belegte Variablennummern)."""
    out, pos, seq, used = [], 0, 0, []
    for m in PLACEHOLDER.finditer(value):
        out.append(text_chunk(value[pos:m.start()], lead=pos == 0))
        pos = m.end()
        if m.group(0) == "%%":
            out.append("%")
        elif m.group(4):
            sub = (subs or {})[m.group(4)]
            used.append(sub["argNum"])
            out.append(inline_select(sub))
        else:
            seq += 1
            n = int(m.group(1)) if m.group(1) else seq
            used.append(n)
            out.append("{ $arg%d }" % n)
    out.append(text_chunk(value[pos:], lead=pos == 0, trail=True))
    return "".join(out), used


def plural_var(key: str) -> int:
    """Nummer der Zahl-Variablen, über die der Top-Level-Plural waehlt: erster %lld/%d im Schlüssel."""
    seq = 0
    for m in PLACEHOLDER.finditer(key):
        if m.group(0) == "%%" or m.group(4):
            continue
        seq += 1
        if m.group(3) in "dDiu":
            return int(m.group(1)) if m.group(1) else seq
    return 1


def plural_message(mid: str, key: str, plural: dict) -> str:
    n = plural_var(key)
    lines = ["%s =" % mid, "    { $arg%d ->" % n]
    for cat in PLURAL_ORDER:
        if cat in plural:
            body, _ = convert(plural[cat]["stringUnit"]["value"])
            lines.append("        %s[%s] %s" % ("*" if cat == "other" else " ", cat, body))
    return "\n".join(lines) + "\n    }\n"


def message(mid: str, key: str, loc: dict) -> str:
    """Fluent-Nachricht für eine Sprache."""
    if "variations" in loc:
        return plural_message(mid, key, loc["variations"]["plural"])
    body, _ = convert(loc["stringUnit"]["value"], loc.get("substitutions"))
    return "%s = %s\n" % (mid, body)


def load(path: Path, prefix: str):
    data = json.loads(path.read_text(encoding="utf8"))
    return prefix, {k: v["localizations"] for k, v in data["strings"].items() if v.get("localizations")}


def build():
    sources = [load(LOC / "Localizable.xcstrings", ""), load(LOC / "InfoPlist.xcstrings", "infoplist-")]
    head = "# Erzeugt von rust/tools/xcstrings2ftl.py aus den xcstrings der Mac-App. Nicht von Hand ändern.\n"
    files = {lang: [head] for lang in LANGS}
    catalog = []
    for prefix, entries in sources:
        keys = sorted(entries)
        ids = assign_ids(keys, prefix)
        for key in keys:
            assert set(entries[key]) >= set(LANGS), "Sprache fehlt: %r" % key
            for lang in LANGS:
                files[lang].append("\n" + message(ids[key], key, entries[key][lang]))
            catalog.append((key, ids[key]))
    return {lang: "".join(p) for lang, p in files.items()}, sorted(catalog)


def rust_str(s: str) -> str:
    esc = {"\\": "\\\\", '"': '\\"', "\n": "\\n", "\r": "\\r", "\t": "\\t"}
    return '"' + "".join(esc.get(c, c) for c in s) + '"'


def catalog_rs(catalog) -> str:
    rows = "".join("    (%s, %s),\n" % (rust_str(k), rust_str(i)) for k, i in catalog)
    return ("// Erzeugt von rust/tools/xcstrings2ftl.py. Nicht von Hand ändern.\n"
            "// Deutscher Quelltext (Schlüssel der xcstrings) -> Nachrichten-Id, sortiert nach Schlüssel (Binärsuche).\n\n"
            "#[rustfmt::skip]\npub static KEYS: &[(&str, &str)] = &[\n" + rows + "];\n")


def main() -> int:
    files, catalog = build()
    targets = {OUT / "locales" / lang / "kadrell.ftl": t for lang, t in files.items()}
    targets[OUT / "src/catalog.rs"] = catalog_rs(catalog)
    if "--check" in sys.argv:
        stale = [str(p) for p, t in targets.items() if not p.exists() or p.read_bytes().decode("utf8") != t]
        print("veraltet: " + ", ".join(stale) if stale else "aktuell")
        return 1 if stale else 0
    for p, t in targets.items():
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(t.encode("utf8"))  # LF, ohne Umwandlung
    print("%d Nachrichten" % len(catalog))
    return 0


if __name__ == "__main__":
    sys.exit(main())
