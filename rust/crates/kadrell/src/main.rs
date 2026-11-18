//! Kadrell: Einstiegspunkt. Später verteilt `main` hier die CLI-Befehle (`kadrell <befehl>`, `ext-host`);
//! ohne Befehl startet die App.

fn main() {
    kadrell_term::strip_inherited_env();
    kadrell_app::run();
}
