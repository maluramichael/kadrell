//! Konten, Nutzung und Statistik-Reihen von Kadrell, ohne GUI und ohne eigenen Thread.
//!
//! Port von `AccountService`, `AccountToken`, `UsageService`, `UsageHistory`, `StatsSeries`, `Keychain` und `Stats`
//! der Swift-App. Alle Zeitpunkte sind Sekunden seit der Unix-Epoche (`f64`, in Swift `Date`), `now` wird immer
//! übergeben, damit alles ohne Uhr testbar bleibt. Netz und Geheimnisse stecken hinter [`HttpClient`] und
//! [`SecretStore`]; die Dienste sind synchron (`&mut self` ersetzt das `exclusive`/`busy` aus Swift) und laufen
//! im Aufrufer auf einem Arbeitsthread.
//!
//! Anbindung an `kadrell-core` (Welle 2), die lokalen Typen entsprechen diesen Einstellungen:
//! - [`AccountsState`] liegt als JSON unter `accounts.index` (Konten) und `accounts.active` (aktive Id).
//! - [`AccountSettings`]: `autoswitch.enabled` (Default aus), `autoswitch.threshold` (50..=99, Default 90),
//!   `autoswitch.onPace` (Default aus), `temporary_profile` aus `Profile::is_temporary`.
//! - `autoswitch.notice` und `autoswitch.paceWarning` (beide Default an) wertet die Oberfläche aus, siehe
//!   [`AccountEvent::AutoSwitched`] und [`PaceWatch`].
//! - [`Stats`] liest und schreibt über [`KeyValueStore`] die Schlüssel `stats.*` der Profil-Einstellungen.
//! - Der Verlauf liegt in `usage-history.jsonl` im Profilordner ([`UsageHistory`]).
//! - [`Lang`] folgt der Einstellung `language`; die Fluent-Texte ersetzen später die kleinen Texte in `text.rs`.

mod account;
mod balance;
mod history;
mod http;
mod live_file;
mod oauth;
mod pace;
mod secret;
mod stats;
mod stats_series;
pub mod testing;
mod text;
mod usage;

pub use account::{
    Account, AccountEvent, AccountService, AccountSettings, AccountsState, CachedUsage, MenuItem,
    NO_ACCOUNT_ID, Profile, current_user_name, fetch_profile, parse_profile, usage_detail,
};
pub use balance::{
    HYSTERESIS, SwitchPolicy, best_pace_target, best_target, cached_used, is_due, pace_headroom,
    session_used, switch_target,
};
pub use history::{UsageHistory, UsageSample};
pub use http::{HttpClient, HttpError, HttpRequest, HttpResponse, Method, UreqClient};
pub use live_file::{live_credentials_path, read_live_file, write_live_file};
pub use oauth::{
    Obj, TOKEN_URL, applying, needs_refresh, oauth_from, object_from, refresh_request, string_from,
};
pub use pace::{PaceWarning, PaceWatch};
pub use secret::{
    LIVE_SERVICE, MemorySecretStore, STORE_SERVICE, SecretStore, default_secret_store,
};
pub use stats::{KeyValueStore, MemoryKeyValues, Stats, StatsKey};
pub use stats_series::{
    AccountLimits, Cal, DayMark, LimitKind, LimitPoint, LimitSource, LimitStats, ProjectCount,
    ProjectSort, PromptBar, PromptEvent, PromptSource, PromptStats, StatsInput, StatsListAction,
    StatsRange, StatsSnapshot, StatsView,
};
pub use text::Lang;
pub use usage::{FetchResult, Outcome, Usage, UsageService, fetch, live_token, reset_clock};
