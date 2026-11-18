//! Kleine Makros für Einstellungen und Aufzählungen mit Rohwert (`rawValue` wie in Swift).

/// Aufzählung mit Rohwerten als Text: `ALL`, `raw()`, `from_raw()` und [`crate::settings::RawEnum`].
macro_rules! raw_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $var:ident = $raw:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vmeta])* $var),+
        }

        impl $name {
            /// Alle Fälle in der Reihenfolge der Deklaration.
            pub const ALL: &'static [$name] = &[$($name::$var),+];

            /// Wert, wie er in Dateien und Einstellungen steht.
            pub fn raw(self) -> &'static str {
                match self { $($name::$var => $raw),+ }
            }

            /// Fall zu einem Rohwert, `None` bei Unbekanntem.
            pub fn from_raw(raw: &str) -> Option<$name> {
                match raw { $($raw => Some($name::$var),)+ _ => None }
            }
        }

        impl $crate::settings::RawEnum for $name {
            fn from_raw(raw: &str) -> Option<Self> { $name::from_raw(raw) }
            fn raw(self) -> &'static str { $name::raw(self) }
        }
    };
}

/// Zwei Methoden für eine Einstellung vom Typ `bool`: Lesen (mit Standardwert) und Schreiben.
macro_rules! bool_setting {
    ($(#[$doc:meta])* $get:ident, $set:ident, $key:literal, $default:expr) => {
        $(#[$doc])*
        pub fn $get(&self) -> bool {
            self.bool_or($key, $default)
        }

        pub fn $set(&mut self, value: bool) {
            self.set($key, serde_json::Value::Bool(value));
        }
    };
}

/// Wie `bool_setting`, für Text. Ein Wert anderen Typs gilt als nicht gesetzt.
macro_rules! string_setting {
    ($(#[$doc:meta])* $get:ident, $set:ident, $key:literal, $default:expr) => {
        $(#[$doc])*
        pub fn $get(&self) -> String {
            self.string_or($key, $default)
        }

        pub fn $set(&mut self, value: &str) {
            self.set($key, serde_json::Value::String(value.to_string()));
        }
    };
}

/// Wie `bool_setting`, für Kommazahlen ohne Bereichsprüfung.
macro_rules! f64_setting {
    ($(#[$doc:meta])* $get:ident, $set:ident, $key:literal, $default:expr) => {
        $(#[$doc])*
        pub fn $get(&self) -> f64 {
            self.f64_or($key, $default)
        }

        pub fn $set(&mut self, value: f64) {
            self.set_f64($key, value);
        }
    };
}

/// Wie `bool_setting`, für eine Aufzählung mit Rohwert; Unbekanntes fällt auf den Standard zurück.
macro_rules! enum_setting {
    ($(#[$doc:meta])* $get:ident, $set:ident, $key:literal, $ty:ty, $default:expr) => {
        $(#[$doc])*
        pub fn $get(&self) -> $ty {
            self.enum_or($key, $default)
        }

        pub fn $set(&mut self, value: $ty) {
            self.set($key, serde_json::Value::String(crate::settings::RawEnum::raw(value).to_string()));
        }
    };
}

/// Wie `bool_setting`, für eine Liste von Text.
macro_rules! strings_setting {
    ($(#[$doc:meta])* $get:ident, $set:ident, $key:literal) => {
        $(#[$doc])*
        pub fn $get(&self) -> Vec<String> {
            self.strings_or($key)
        }

        pub fn $set(&mut self, value: &[String]) {
            self.set($key, serde_json::json!(value));
        }
    };
}
