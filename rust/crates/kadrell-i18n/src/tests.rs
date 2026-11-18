use super::*;
use fluent_syntax::ast;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn app_catalog(name: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../app/Kadrell/Localization").join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

fn parse(lang: Lang) -> ast::Resource<&'static str> {
    match fluent_syntax::parser::parse(ftl_source(lang)) {
        Ok(r) => r,
        Err((_, errors)) => panic!("{} hat Syntaxfehler: {errors:?}", lang.code()),
    }
}

fn vars_of_expression(expr: &ast::Expression<&str>, out: &mut BTreeSet<String>) {
    match expr {
        ast::Expression::Inline(inline) => vars_of_inline(inline, out),
        ast::Expression::Select { selector, variants } => {
            vars_of_inline(selector, out);
            for v in variants {
                vars_of_pattern(&v.value, out);
            }
        }
    }
}

fn vars_of_inline(inline: &ast::InlineExpression<&str>, out: &mut BTreeSet<String>) {
    match inline {
        ast::InlineExpression::VariableReference { id } => {
            out.insert(id.name.to_string());
        }
        ast::InlineExpression::Placeable { expression } => vars_of_expression(expression, out),
        _ => {}
    }
}

fn vars_of_pattern(pattern: &ast::Pattern<&str>, out: &mut BTreeSet<String>) {
    for el in &pattern.elements {
        if let ast::PatternElement::Placeable { expression } = el {
            vars_of_expression(expression, out);
        }
    }
}

/// Id -> Menge der verwendeten Variablen.
fn messages(lang: Lang) -> BTreeMap<String, BTreeSet<String>> {
    let mut map = BTreeMap::new();
    for entry in &parse(lang).body {
        if let ast::Entry::Message(m) = entry {
            let mut vars = BTreeSet::new();
            vars_of_pattern(m.value.as_ref().expect("Nachricht ohne Wert"), &mut vars);
            assert!(map.insert(m.id.name.to_string(), vars).is_none(), "doppelte Id {}", m.id.name);
        }
    }
    map
}

#[test]
fn both_locales_have_same_ids_and_variables() {
    let (de, en) = (messages(Lang::De), messages(Lang::En));
    let only_de: Vec<_> = de.keys().filter(|k| !en.contains_key(*k)).collect();
    let only_en: Vec<_> = en.keys().filter(|k| !de.contains_key(*k)).collect();
    assert!(only_de.is_empty() && only_en.is_empty(), "nur de: {only_de:?}, nur en: {only_en:?}");
    for (id, vars) in &de {
        assert_eq!(vars, &en[id], "Variablen unterschiedlich bei {id}");
    }
}

#[test]
fn catalog_matches_locales() {
    let de = messages(Lang::De);
    let keys = catalog();
    assert_eq!(keys.len(), de.len());
    assert!(keys.windows(2).all(|w| w[0].0 < w[1].0), "catalog nicht nach Schlüssel sortiert");
    let ids: BTreeSet<_> = keys.iter().map(|(_, id)| *id).collect();
    assert_eq!(ids.len(), keys.len(), "Ids nicht eindeutig");
    for (_, id) in keys {
        assert!(de.contains_key(*id), "Id {id} fehlt in den .ftl-Dateien");
        assert!(id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && id.starts_with(|c: char| c.is_ascii_lowercase()));
    }
}

/// Port von LocalizationTests.swift: jeder übersetzbare Schlüssel hat Deutsch und Englisch, keine Lücke.
#[test]
fn every_xcstrings_key_has_both_languages() {
    let mut checked = 0;
    for file in ["Localizable.xcstrings", "InfoPlist.xcstrings"] {
        let strings = app_catalog(file)["strings"].as_object().unwrap().clone();
        for (key, entry) in &strings {
            let Some(locs) = entry.get("localizations") else {
                assert_eq!(entry["shouldTranslate"], false, "{key}: weder Übersetzung noch shouldTranslate=false");
                continue;
            };
            for lang in ["de", "en"] {
                assert!(has_text(&locs[lang]), "{key}: Text für {lang} fehlt");
            }
            assert!(catalog().binary_search_by(|(k, _)| (*k).cmp(key.as_str())).is_ok(), "{key} fehlt im catalog (Konverter laufen lassen)");
            checked += 1;
        }
    }
    assert_eq!(checked, catalog().len());
}

fn has_text(loc: &serde_json::Value) -> bool {
    if let Some(v) = loc["stringUnit"]["value"].as_str() {
        return !v.is_empty();
    }
    loc["variations"]["plural"].as_object().is_some_and(|p| !p.is_empty() && p.values().all(has_text))
}

#[test]
fn every_german_source_resolves_in_both_languages() {
    let i18n = I18n::new(Language::En);
    for (source, id) in catalog() {
        for lang in Lang::ALL {
            assert!(i18n.has(lang, id), "{id} fehlt in {}", lang.code());
        }
        assert_eq!(id_for_de(source), Some(*id));
    }
}

#[test]
fn language_switches_live() {
    let i18n = I18n::new(Language::En);
    assert_eq!(i18n.t_de("Kopieren"), "Copy");
    assert_eq!(i18n.lang(), Lang::En);
    let before = i18n.generation();
    i18n.set_language(Language::De);
    assert_eq!(i18n.t_de("Kopieren"), "Kopieren");
    assert_eq!(i18n.language(), Language::De);
    assert!(i18n.generation() > before);
    i18n.set_language(Language::System);
    assert_eq!(i18n.language(), Language::System);
    assert_eq!(i18n.lang(), system_language());
}

#[test]
fn id_lookup_matches_german_lookup() {
    let i18n = I18n::new(Language::En);
    let id = id_for_de("Kopieren").unwrap();
    assert_eq!(i18n.t(id), "Copy");
}

#[test]
fn positional_and_named_arguments() {
    let i18n = I18n::new(Language::En);
    let key = "%@: %lld %% verbraucht";
    assert_eq!(i18n.t_de_args(key, &[Arg::from("Work"), Arg::from(42)]), "Work: 42% used");
    i18n.set_language(Language::De);
    assert_eq!(i18n.t_de_args(key, &[Arg::from("Work"), Arg::from(42)]), "Work: 42 % verbraucht");
    let id = id_for_de(key).unwrap();
    assert_eq!(i18n.t_args(id, &[("arg1", Arg::from("A")), ("arg2", Arg::from(7))]), "A: 7 % verbraucht");
}

#[test]
fn plural_variations_pick_by_number() {
    let i18n = I18n::new(Language::En);
    let key = "Kadrell neu starten? %lld Session(s) arbeiten gerade!";
    assert_eq!(i18n.t_de_args(key, &[Arg::from(1)]), "Restart Kadrell? 1 session is working right now!");
    assert_eq!(i18n.t_de_args(key, &[Arg::from(3)]), "Restart Kadrell? 3 sessions are working right now!");
    assert_eq!(i18n.t_de_args(key, &[Arg::from(0)]), "Restart Kadrell? 0 sessions are working right now!");
    i18n.set_language(Language::De);
    assert_eq!(i18n.t_de_args(key, &[Arg::from(1)]), "Kadrell neu starten? 1 Session(s) arbeiten gerade!");
    // Deutsch mit eigener Plural-Variante
    assert_eq!(i18n.t_de_args("%lld ARBEITEN", &[Arg::from(1)]), "1 ARBEITET");
    assert_eq!(i18n.t_de_args("%lld ARBEITEN", &[Arg::from(2)]), "2 ARBEITEN");
}

#[test]
fn substitution_becomes_inline_selector() {
    let i18n = I18n::new(Language::En);
    let key = "Gruppe „%@“ mit %lld Session(s) schließen?";
    assert_eq!(i18n.t_de_args(key, &[Arg::from("Work"), Arg::from(1)]), "Close group “Work” with 1 session?");
    assert_eq!(i18n.t_de_args(key, &[Arg::from("Work"), Arg::from(5)]), "Close group “Work” with 5 sessions?");
}

#[test]
fn large_numbers_have_no_grouping() {
    let i18n = I18n::new(Language::En);
    let key = "%lld %%";
    assert_eq!(i18n.t_de_args(key, &[Arg::from(1234567)]), "1234567 %");
}

#[test]
fn missing_translation_falls_back_to_german_without_panic() {
    let i18n = I18n {
        de: build_bundle(Lang::De, "nur-de = Hallo\nbeide = Hallo { $arg1 }\n"),
        en: build_bundle(Lang::En, "beide = Hello { $arg1 }\n"),
        setting: AtomicU8::new(Language::En.as_u8()),
        resolved: AtomicU8::new(Lang::En.as_u8()),
        generation: AtomicU64::new(0),
        reported: Mutex::new(HashSet::new()),
    };
    assert_eq!(i18n.t("nur-de"), "Hallo");
    assert_eq!(i18n.t_args("beide", &[("arg1", Arg::from("Welt"))]), "Hello Welt");
    assert_eq!(i18n.t("gibt-es-nicht"), "gibt-es-nicht");
    assert_eq!(i18n.t_de("Ein Text, den es nicht gibt"), "Ein Text, den es nicht gibt");
    // je Id nur einmal gemeldet
    assert_eq!(i18n.reported.lock().unwrap().len(), 3);
    i18n.t("nur-de");
    assert_eq!(i18n.reported.lock().unwrap().len(), 3);
}

#[test]
fn missing_argument_does_not_panic() {
    let i18n = I18n::new(Language::En);
    let _ = i18n.t_de("%@: %lld %% verbraucht");
}

#[test]
fn macros_use_global_instance() {
    global().set_language(Language::En);
    assert_eq!(trd!("Kopieren"), "Copy");
    assert_eq!(tr!(id_for_de("Kopieren").unwrap()), "Copy");
    assert_eq!(trd!("%lld %%", 5), "5 %");
    let id = id_for_de("%lld %%").unwrap();
    assert_eq!(tr!(id, arg1 = 9), "9 %");
    global().set_language(Language::De);
    assert_eq!(trd!("Kopieren"), "Kopieren");
}

#[test]
fn switching_is_thread_safe() {
    let i18n = std::sync::Arc::new(I18n::new(Language::De));
    let workers: Vec<_> = (0..4)
        .map(|n| {
            let i18n = i18n.clone();
            std::thread::spawn(move || {
                for i in 0..200 {
                    if n == 0 {
                        i18n.set_language(if i % 2 == 0 { Language::En } else { Language::De });
                    }
                    let text = i18n.t_de("Kopieren");
                    assert!(text == "Copy" || text == "Kopieren");
                }
            })
        })
        .collect();
    for w in workers {
        w.join().unwrap();
    }
}

#[test]
fn language_setting_roundtrip() {
    for l in Language::ALL {
        assert_eq!(Language::parse(l.as_str()), Some(l));
    }
    assert_eq!(Language::parse("fr"), None);
    assert_eq!(Lang::from_tag("en_US.UTF-8"), Some(Lang::En));
    assert_eq!(Lang::from_tag("fr"), None);
}
