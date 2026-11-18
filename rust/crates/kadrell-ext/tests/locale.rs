//! Die Problemtexte haben Platzhalter und folgen der Sprache zur Laufzeit (eigener Testprozess, weil die Sprache global
//! gilt).

use kadrell_ext::i18n;
use kadrell_ext::manifest;
use kadrell_ext::testutil::{TempDir, make_extension};

#[test]
fn problems_are_translated() {
    let root = TempDir::new("loc");
    make_extension(
        root.path(),
        "apiTooHigh",
        "",
        Some(serde_json::json!({"name": "apiTooHigh", "apiVersion": 2})),
    );
    make_extension(
        root.path(),
        "nameMismatch",
        "",
        Some(serde_json::json!({"name": "anders", "apiVersion": 1})),
    );
    i18n::set_locale("en");
    let problems: Vec<String> = manifest::scan(root.path())
        .into_iter()
        .filter_map(|f| f.problem)
        .collect();
    assert_eq!(
        problems,
        [
            "Needs a newer Kadrell (API 2, supported: 1)",
            "Name in the manifest (“anders”) does not match the folder name"
        ]
    );
    i18n::set_locale("de");
    let problems: Vec<String> = manifest::scan(root.path())
        .into_iter()
        .filter_map(|f| f.problem)
        .collect();
    assert_eq!(
        problems,
        [
            "Braucht neueres Kadrell (API 2, unterstützt: 1)",
            "Name im Manifest („anders“) passt nicht zum Ordnernamen"
        ]
    );
}
