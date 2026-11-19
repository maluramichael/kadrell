//! Port von `LocalizationTests.swift`, unabhängig vom Zeichnen: jeder deutsche Quelltext, der im Code von
//! `kadrell-app` über `trd!("…")` oder `de!("…")` benutzt wird, steht im Katalog (xcstrings oder Zusatztexte) und hat
//! Deutsch und Englisch. Dazu die Titel, die der Kern dynamisch liefert (Rückfragen, Kürzel-Aktionen).

use std::path::{Path, PathBuf};

use kadrell_core::hotkeys::HotkeyAction;
use kadrell_core::settings::Ask;
use kadrell_i18n::{Arg, I18n, Lang, Language, id_for_de};

/// Alle `.rs`-Dateien unter `dir`.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Liest ein Zeichenketten-Literal ab `start` (Index des öffnenden `"`), mit den üblichen Escapes.
fn read_literal(text: &str, start: usize) -> Option<String> {
    let mut out = String::new();
    let mut chars = text[start + 1..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// Die Literale hinter `trd!(` und `de!(` in einem Quelltext, ohne Kommentarzeilen.
fn literals(source: &str) -> Vec<String> {
    let code: String = source.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    let mut found = Vec::new();
    for marker in ["trd!(", "de!("] {
        let mut from = 0;
        while let Some(at) = code[from..].find(marker) {
            let after = from + at + marker.len();
            from = after;
            let rest = code[after..].trim_start();
            let offset = code.len() - rest.len();
            if rest.starts_with('"') {
                found.extend(read_literal(&code, offset));
            }
        }
    }
    found
}

#[test]
fn every_literal_is_in_the_catalog() {
    let mut files = Vec::new();
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    assert!(files.len() > 20, "Quelldateien nicht gefunden");
    let this = Path::new(file!()).file_name().unwrap().to_owned();
    let mut checked = 0;
    let mut missing = Vec::new();
    for file in files.iter().filter(|f| f.file_name() != Some(&this)) {
        for text in literals(&std::fs::read_to_string(file).unwrap()) {
            checked += 1;
            if id_for_de(&text).is_none() {
                missing.push(format!("{}: {text}", file.file_name().unwrap().to_string_lossy()));
            }
        }
    }
    assert!(checked > 150, "zu wenige Literale gefunden ({checked}), Suche kaputt?");
    assert!(missing.is_empty(), "nicht im Katalog (xcstrings oder extra.ftl):\n{}", missing.join("\n"));
}

/// Jeder Text hat Deutsch und Englisch, ohne Rückfall auf den Schlüssel (Platzhalter bekommen Werte).
fn assert_both_languages(source: &str) {
    let i18n = I18n::new(Language::En);
    let args: Vec<Arg> = (0..source.matches("%@").count() + source.matches("%lld").count()).map(|i| Arg::from(i as i64 + 1)).collect();
    for lang in Lang::ALL {
        let text = i18n.t_de_in(lang, source, &args);
        assert!(!text.is_empty(), "{source}: leerer Text in {}", lang.code());
    }
    assert!(id_for_de(source).is_some(), "{source} fehlt im Katalog");
}

#[test]
fn core_titles_are_translatable() {
    for ask in Ask::ALL {
        assert_both_languages(ask.title_key());
    }
    for action in HotkeyAction::ALL {
        let source = if action.tile_index().is_some() { "Kachel %lld".to_string() } else { action.title_key() };
        assert_both_languages(&source);
    }
}

#[test]
fn hotkey_titles_follow_the_language() {
    let i18n = I18n::new(Language::En);
    let tile = HotkeyAction::Focus3;
    assert_eq!(i18n.t_de_in(Lang::En, "Kachel %lld", &[Arg::from(3)]), "Tile 3");
    assert_eq!(crate::ui::settings::sections::hotkey_title(tile), "Kachel 3");
}

/// Mac-Zeichen für Tasten stehen unter Windows in keinem Text, der durch die Kürzelumwandlung läuft: jeder Quelltext des
/// Codes und jeder Titel einer Aktion, in beiden Sprachen.
#[test]
fn windows_texts_have_no_mac_key_glyphs() {
    let glyphs = ['⌘', '⌥', '⌃', '⇧'];
    let i18n = I18n::new(Language::En);
    let mut files = Vec::new();
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    let mut texts: Vec<String> = files.iter().flat_map(|f| literals(&std::fs::read_to_string(f).unwrap())).collect();
    texts.extend(HotkeyAction::ALL.iter().filter(|a| a.tile_index().is_none()).map(|a| a.title_key()));
    let mut bad = Vec::new();
    for source in texts.iter().filter(|s| id_for_de(s).is_some() && !s.contains('%')) {
        for lang in Lang::ALL {
            let text = crate::lang::localize_shortcuts_for(&i18n.t_de_in(lang, source, &[]), false, lang);
            if text.contains(glyphs) {
                bad.push(format!("{}: {text}", lang.code()));
            }
        }
    }
    assert!(bad.is_empty(), "Mac-Zeichen unter Windows:\n{}", bad.join("\n"));
}

#[test]
fn reader_finds_literals_with_escapes() {
    let found = literals("a trd!(\"Kopieren\") b de!( \"Zeile \\\"x\\\"\" )\n  // trd!(\"Kommentar\")\nc trd!(var)");
    assert_eq!(found, vec!["Kopieren".to_string(), "Zeile \"x\"".to_string()]);
}
