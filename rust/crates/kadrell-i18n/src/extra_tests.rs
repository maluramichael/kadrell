//! Tests der Zusatztexte der Rust-App (`locales/{de,en}/extra.ftl`, `extra.rs`).

use crate::tests::vars_of_pattern;
use crate::*;
use fluent_syntax::ast;
use std::collections::{BTreeMap, BTreeSet};

fn extra_messages(lang: Lang) -> BTreeMap<String, BTreeSet<String>> {
    let resource = match fluent_syntax::parser::parse(extra_ftl_source(lang)) {
        Ok(r) => r,
        Err((_, errors)) => panic!("extra.ftl ({}) hat Syntaxfehler: {errors:?}", lang.code()),
    };
    let mut map = BTreeMap::new();
    for entry in &resource.body {
        if let ast::Entry::Message(m) = entry {
            let mut vars = BTreeSet::new();
            vars_of_pattern(m.value.as_ref().expect("Nachricht ohne Wert"), &mut vars);
            assert!(map.insert(m.id.name.to_string(), vars).is_none(), "doppelte Id {} in extra.ftl", m.id.name);
        }
    }
    map
}

/// Anzahl der Platzhalter `%@` und `%lld` im Schlüssel.
fn placeholders(key: &str) -> usize {
    key.matches("%@").count() + key.matches("%lld").count()
}

/// Der Schlüssel mit eingesetzten Werten `A0`, `A1`, ... für die Platzhalter, der Reihe nach.
fn filled(key: &str) -> String {
    let mut out = String::new();
    let mut rest = key;
    let mut n = 0;
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        let len = if rest[at..].starts_with("%@") { 2 } else { 4 };
        out.push_str(&format!("A{n}"));
        n += 1;
        rest = &rest[at + len..];
    }
    out.push_str(rest);
    out
}

#[test]
fn extra_locales_have_same_ids_and_variables() {
    let (de, en) = (extra_messages(Lang::De), extra_messages(Lang::En));
    assert_eq!(de.keys().collect::<Vec<_>>(), en.keys().collect::<Vec<_>>());
    for (id, vars) in &de {
        assert_eq!(vars, &en[id], "Variablen unterschiedlich bei {id}");
    }
}

#[test]
fn extra_catalog_is_sorted_unique_and_apart_from_the_xcstrings() {
    let keys = extra_catalog();
    assert!(!keys.is_empty());
    assert!(keys.windows(2).all(|w| w[0].0.as_bytes() < w[1].0.as_bytes()), "extra nicht nach Schlüssel sortiert");
    let de = extra_messages(Lang::De);
    assert_eq!(keys.len(), de.len());
    let main_ids: BTreeSet<_> = catalog().iter().map(|(_, id)| *id).collect();
    for (key, id) in keys {
        assert!(de.contains_key(*id), "Id {id} fehlt in extra.ftl");
        assert!(!main_ids.contains(id), "Id {id} gibt es schon in kadrell.ftl");
        assert!(catalog().binary_search_by(|(k, _)| (*k).cmp(key)).is_err(), "{key} steht schon in den xcstrings");
        assert_eq!(de[*id].len(), placeholders(key), "Platzhalter von {key} und Variablen der Nachricht passen nicht");
    }
}

#[test]
fn extra_german_text_is_its_own_source() {
    let i18n = I18n::new(Language::De);
    for (key, _) in extra_catalog() {
        let args: Vec<Arg> = (0..placeholders(key)).map(|i| Arg::from(format!("A{i}"))).collect();
        assert_eq!(i18n.t_de_in(Lang::De, key, &args), filled(key), "{key}");
    }
}

#[test]
fn extra_texts_resolve_in_both_languages() {
    let i18n = I18n::new(Language::En);
    for (key, id) in extra_catalog() {
        for lang in Lang::ALL {
            assert!(i18n.has(lang, id), "{id} fehlt in {}", lang.code());
        }
        assert_eq!(id_for_de(key), Some(*id));
    }
    assert_eq!(i18n.t_de("Kachel zoomen"), "Zoom tile");
    assert_eq!(i18n.t_de_args("Verschieben nach: %@", &[Arg::from("Work")]), "Move to: Work");
    assert_eq!(i18n.t_de_in(Lang::De, "Kachel zoomen", &[]), "Kachel zoomen");
    assert_eq!(i18n.t_de_in(Lang::En, "Gruppe umbenennen", &[]), "Rename group");
}

#[test]
fn explicit_language_does_not_touch_the_setting() {
    let i18n = I18n::new(Language::En);
    assert_eq!(i18n.t_de_in(Lang::De, "Kopieren", &[]), "Kopieren");
    assert_eq!(i18n.lang(), Lang::En);
    assert_eq!(i18n.t_de("Kopieren"), "Copy");
}
