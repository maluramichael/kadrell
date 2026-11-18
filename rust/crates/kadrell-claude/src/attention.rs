//! Was dich auf eine Session aufmerksam macht, für alle Fenster gemeinsam: Marke „neu“, Badge und Hüpfen, Klang,
//! Ansage, Systembenachrichtigung, Hook `session-focus` und die einmaligen Tipps (Swift: `App/AttentionTracker.swift`,
//! dazu `Feedback.transitions`). Die Zustandsmaschine liefert statt Closure-Aufrufen eine Liste von `Effect`s; die
//! Oberfläche führt sie aus und liefert die Texte (zweisprachig).

use crate::agent::Status;
use std::collections::{HashMap, HashSet};

/// Sessions, die seit dem letzten Stand zu warten begonnen haben bzw. vom Arbeiten in den Leerlauf gewechselt sind
/// (`Feedback.transitions`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transitions {
    pub waiting: HashSet<String>,
    pub done: HashSet<String>,
}

pub fn transitions(old: &HashMap<String, Status>, new: &HashMap<String, Status>) -> Transitions {
    let mut t = Transitions::default();
    for (id, s) in new {
        let Some(o) = old.get(id).filter(|o| *o != s) else { continue };
        if *s == Status::Waiting {
            t.waiting.insert(id.clone());
        }
        if *o == Status::Running && *s == Status::Idle {
            t.done.insert(id.clone());
        }
    }
    t
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Waiting,
    Done,
}

/// Einmaliger Tipp in der Leiste.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TipText {
    /// „Gelb heißt: Claude wartet auf dich. ⌥N springt zur nächsten wartenden Session.“
    WaitingMeansYellow,
    Other(String),
}

/// Der Tipp verschwindet nach 12 s von selbst oder per Klick darauf (`AttentionTracker::clear_tip`).
pub const TIP_SECONDS: u64 = 12;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Zahl der wartenden und ungesehenen Sessions (0 = Badge weg).
    SetBadge(usize),
    /// Kurz im Dock hüpfen, ohne Benachrichtigungsrechte.
    RequestAttention,
    Play(Sound),
    /// Systembenachrichtigung; `waiting` false heißt fertig.
    Notify { key: String, waiting: bool },
    /// Ansage für VoiceOver o. ä.: „<Titel> wartet“ bzw. „<Titel> fertig“.
    Announce { key: String, waiting: bool },
    /// Hook `session-focus` für diese Session.
    FireFocusHook(String),
    Tip(Option<TipText>),
}

/// Wo die Marken und die Tipp-Flags liegen (Profil-Einstellungen): `sessions.unseen` als Liste, `tip.waiting.shown`.
pub trait AttentionStore {
    fn get_list(&self, key: &str) -> Vec<String>;
    fn set_list(&mut self, key: &str, value: &[String]);
    fn get_bool(&self, key: &str) -> bool;
    fn set_bool(&mut self, key: &str, value: bool);
}

pub const UNSEEN_KEY: &str = "sessions.unseen";
pub const WAITING_TIP_KEY: &str = "tip.waiting.shown";

/// Was der Tracker von einer Session wissen muss.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttentionSession {
    pub id: String,
    pub active_worktree: Option<String>,
}

/// Eingaben eines Abgleichs (`update`).
pub struct UpdateInput<'a> {
    pub sessions: &'a HashMap<String, AttentionSession>,
    /// Wartende Sessions in Baumreihenfolge.
    pub waiting: &'a [String],
    /// Status angehängter Sessions.
    pub statuses: &'a HashMap<String, Status>,
    pub focused: Option<&'a str>,
    /// Sichtbare Kacheln; nil: nur die Fokus-Kachel zählt als angeschaut.
    pub looking: Option<&'a HashSet<String>>,
    /// Das aktuelle Fenster ist vorn.
    pub is_key: bool,
}

pub struct AttentionTracker<S: AttentionStore> {
    store: S,
    unseen: HashSet<String>,
    /// Einmaliger Tipp in der Leiste, siehe `show_tip_once`.
    pub tip: Option<TipText>,
    last_waiting_ids: HashSet<String>,
    last_statuses: HashMap<String, Status>,
    hook_focus: Option<String>,
    hook_focus_worktree: Option<String>,
    /// Erst wenn claude aufgelöst ist, feuert ein Hook.
    pub focus_hook_enabled: bool,
}

impl<S: AttentionStore> AttentionTracker<S> {
    pub fn new(store: S) -> Self {
        let unseen = store.get_list(UNSEEN_KEY).into_iter().collect();
        AttentionTracker {
            store,
            unseen,
            tip: None,
            last_waiting_ids: HashSet::new(),
            last_statuses: HashMap::new(),
            hook_focus: None,
            hook_focus_worktree: None,
            focus_hook_enabled: false,
        }
    }

    /// Fertig gewordene oder wartende Sessions, die du noch nicht angesehen hast. Weg erst, wenn du sie anklickst oder
    /// fokussierst; übersteht einen Neustart.
    pub fn unseen(&self) -> &HashSet<String> {
        &self.unseen
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    fn set_unseen(&mut self, next: HashSet<String>) {
        if next != self.unseen {
            let mut list: Vec<String> = next.iter().cloned().collect();
            list.sort();
            self.store.set_list(UNSEEN_KEY, &list);
            self.unseen = next;
        }
    }

    /// true, wenn die Marke da war: dann muss die Anzeige nachziehen.
    pub fn mark_seen(&mut self, key: &str) -> bool {
        let mut next = self.unseen.clone();
        let had = next.remove(key);
        self.set_unseen(next);
        had
    }

    /// Marken zu Sessions, die es nicht mehr gibt, fallen weg.
    pub fn prune(&mut self, sessions: &HashMap<String, AttentionSession>) {
        let next = self.unseen.iter().filter(|k| sessions.contains_key(*k)).cloned().collect();
        self.set_unseen(next);
    }

    /// Abgleich nach jeder Änderung. Liefert die Übergänge (der Baum lässt sie aufblitzen) und die Effekte in der
    /// Reihenfolge, in der Swift sie auslöst.
    pub fn update(&mut self, input: &UpdateInput) -> (Transitions, Vec<Effect>) {
        let mut effects = Vec::new();
        let looking: HashSet<String> = input.looking.cloned().unwrap_or_else(|| input.focused.map(String::from).into_iter().collect());
        self.fire_hook_if_focus_changed(input.focused.and_then(|f| input.sessions.get(f)), &mut effects);
        let waiting_set: HashSet<String> = input.waiting.iter().cloned().collect();
        // Wartend oder fertig, aber noch nicht angesehen: beides zusammen, sonst verschwinden fertige Sessions aus dem Badge.
        effects.push(Effect::SetBadge(waiting_set.union(&self.unseen).count()));
        // Neu dazugekommene wartende Session, Fenster nicht im Vordergrund: kurz im Dock hüpfen.
        if !waiting_set.is_subset(&self.last_waiting_ids) && !input.is_key {
            effects.push(Effect::RequestAttention);
        }
        self.last_waiting_ids = waiting_set;
        let changed = transitions(&self.last_statuses, input.statuses);
        self.last_statuses = input.statuses.clone();
        let is_key = input.is_key;
        self.handle_transitions(&changed, input.statuses, &|id| !is_key || !looking.contains(id), &mut effects);
        // Was gerade sichtbar auf der Arbeitsfläche liegt, ist gesehen: auch eine ältere Marke fällt weg.
        if is_key && !self.unseen.is_disjoint(&looking) {
            let next = self.unseen.difference(&looking).cloned().collect();
            self.set_unseen(next);
        }
        (changed, effects)
    }

    fn fire_hook_if_focus_changed(&mut self, session: Option<&AttentionSession>, effects: &mut Vec<Effect>) {
        let Some(s) = session.filter(|_| self.focus_hook_enabled) else { return };
        if self.hook_focus.as_deref() == Some(&s.id) && self.hook_focus_worktree == s.active_worktree {
            return;
        }
        self.hook_focus = Some(s.id.clone());
        self.hook_focus_worktree = s.active_worktree.clone();
        effects.push(Effect::FireFocusHook(s.id.clone()));
    }

    /// Klang und Marke „neu“ nur für das, was man gerade nicht sieht (`not_looking`): andere Session oder Fenster im Hintergrund.
    fn handle_transitions(&mut self, changed: &Transitions, statuses: &HashMap<String, Status>, not_looking: &dyn Fn(&str) -> bool, effects: &mut Vec<Effect>) {
        // Einmaliger Tipp bei der allerersten wartenden Session überhaupt: sonst bleibt Gelb unerklärt.
        if !changed.waiting.is_empty() {
            self.show_tip_once(WAITING_TIP_KEY, TipText::WaitingMeansYellow, effects);
        }
        // Ansagen für alle, nicht nur für unbeobachtete: VoiceOver bekommt sonst nichts vom Kernnutzen der App mit.
        for (ids, waiting) in [(&changed.waiting, true), (&changed.done, false)] {
            for key in sorted(ids) {
                effects.push(Effect::Announce { key, waiting });
            }
        }
        if changed.waiting.iter().any(|k| not_looking(k)) {
            effects.push(Effect::Play(Sound::Waiting));
        } else if changed.done.iter().any(|k| not_looking(k)) {
            effects.push(Effect::Play(Sound::Done));
        }
        for (ids, waiting) in [(&changed.waiting, true), (&changed.done, false)] {
            for key in sorted(ids).into_iter().filter(|k| not_looking(k)) {
                effects.push(Effect::Notify { key, waiting });
            }
        }
        let fresh: HashSet<String> = changed.waiting.union(&changed.done).filter(|k| not_looking(k)).cloned().collect();
        let mut next: HashSet<String> = self.unseen.union(&fresh).cloned().collect();
        // Wieder am Arbeiten: die Marke gilt der letzten Antwort, nicht der laufenden.
        next.retain(|k| statuses.get(k) != Some(&Status::Running));
        self.set_unseen(next);
    }

    /// Tipp nur beim ersten Mal je `key`, danach nie wieder.
    pub fn show_tip_once_now(&mut self, key: &str, text: TipText) -> Vec<Effect> {
        let mut effects = Vec::new();
        self.show_tip_once(key, text, &mut effects);
        effects
    }

    fn show_tip_once(&mut self, key: &str, text: TipText, effects: &mut Vec<Effect>) {
        if self.store.get_bool(key) {
            return;
        }
        self.store.set_bool(key, true);
        self.tip = Some(text.clone());
        effects.push(Effect::Tip(Some(text)));
    }

    /// Nach `TIP_SECONDS` oder per Klick: der Tipp verschwindet, aber nur wenn es noch derselbe ist.
    pub fn clear_tip(&mut self, only_if: Option<&TipText>) -> Option<Effect> {
        if only_if.is_some() && self.tip.as_ref() != only_if {
            return None;
        }
        self.tip.take().map(|_| Effect::Tip(None))
    }
}

fn sorted(ids: &HashSet<String>) -> Vec<String> {
    let mut v: Vec<String> = ids.iter().cloned().collect();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MemStore {
        lists: HashMap<String, Vec<String>>,
        bools: HashMap<String, bool>,
    }

    impl AttentionStore for MemStore {
        fn get_list(&self, key: &str) -> Vec<String> {
            self.lists.get(key).cloned().unwrap_or_default()
        }
        fn set_list(&mut self, key: &str, value: &[String]) {
            self.lists.insert(key.into(), value.to_vec());
        }
        fn get_bool(&self, key: &str) -> bool {
            self.bools.get(key).copied().unwrap_or(false)
        }
        fn set_bool(&mut self, key: &str, value: bool) {
            self.bools.insert(key.into(), value);
        }
    }

    fn sessions(ids: &[&str]) -> HashMap<String, AttentionSession> {
        ids.iter().map(|i| (i.to_string(), AttentionSession { id: i.to_string(), active_worktree: None })).collect()
    }

    fn st(pairs: &[(&str, Status)]) -> HashMap<String, Status> {
        pairs.iter().map(|(k, s)| (k.to_string(), *s)).collect()
    }

    fn set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn step(t: &mut AttentionTracker<MemStore>, s: &HashMap<String, AttentionSession>, waiting: &[&str], statuses: &[(&str, Status)], focused: Option<&str>, is_key: bool) -> (Transitions, Vec<Effect>) {
        let w: Vec<String> = waiting.iter().map(|s| s.to_string()).collect();
        t.update(&UpdateInput { sessions: s, waiting: &w, statuses: &st(statuses), focused, looking: None, is_key })
    }

    use Status::{Idle, Running, Waiting};

    #[test]
    fn waiting_in_background_marks_new_plays_and_bounces() {
        let mut t = AttentionTracker::new(MemStore::default());
        let s = sessions(&["a", "b"]);
        step(&mut t, &s, &[], &[("a", Running), ("b", Running)], Some("b"), true);
        let (changed, fx) = step(&mut t, &s, &["a"], &[("a", Waiting), ("b", Running)], Some("b"), true);
        assert_eq!(changed.waiting, set(&["a"]));
        assert_eq!(t.unseen(), &set(&["a"]));
        assert!(fx.contains(&Effect::Play(Sound::Waiting)));
        assert!(fx.contains(&Effect::Notify { key: "a".into(), waiting: true }));
        assert!(!fx.contains(&Effect::RequestAttention), "Fenster ist vorn: kein Dock-Hüpfen");
        assert!(fx.contains(&Effect::SetBadge(1)));
        assert!(fx.contains(&Effect::Tip(Some(TipText::WaitingMeansYellow))), "erste wartende Session zeigt den Tipp");
        assert_eq!(t.store().get_list(UNSEEN_KEY), ["a"], "Marke übersteht einen Neustart");
    }

    #[test]
    fn focused_session_in_key_window_stays_quiet() {
        let mut t = AttentionTracker::new(MemStore::default());
        let s = sessions(&["a"]);
        step(&mut t, &s, &[], &[("a", Running)], Some("a"), true);
        let (changed, fx) = step(&mut t, &s, &[], &[("a", Idle)], Some("a"), true);
        assert_eq!(changed.done, set(&["a"]));
        assert!(t.unseen().is_empty());
        assert!(!fx.iter().any(|e| matches!(e, Effect::Play(_) | Effect::Notify { .. })));
    }

    #[test]
    fn background_window_counts_as_not_looking_and_bounces() {
        let mut t = AttentionTracker::new(MemStore::default());
        let s = sessions(&["a"]);
        step(&mut t, &s, &[], &[("a", Running)], Some("a"), false);
        let (_, fx) = step(&mut t, &s, &["a"], &[("a", Waiting)], Some("a"), false);
        assert_eq!(t.unseen(), &set(&["a"]));
        assert_eq!(fx.iter().filter(|e| **e == Effect::RequestAttention).count(), 1);
    }

    #[test]
    fn running_again_clears_mark_and_mark_seen() {
        let mut t = AttentionTracker::new(MemStore::default());
        let s = sessions(&["a", "b"]);
        step(&mut t, &s, &[], &[("a", Running), ("b", Running)], None, true);
        let (_, fx) = step(&mut t, &s, &[], &[("a", Idle), ("b", Idle)], None, true);
        assert_eq!(t.unseen(), &set(&["a", "b"]));
        assert_eq!(fx.iter().filter(|e| matches!(e, Effect::Play(Sound::Done))).count(), 1);
        step(&mut t, &s, &[], &[("a", Running), ("b", Idle)], None, true);
        assert_eq!(t.unseen(), &set(&["b"]), "wieder am Arbeiten: Marke weg");
        assert!(t.mark_seen("b"));
        assert!(!t.mark_seen("b"));
        assert!(t.unseen().is_empty());
    }

    fn look(t: &mut AttentionTracker<MemStore>, s: &HashMap<String, AttentionSession>, statuses: &[(&str, Status)], focused: Option<&str>, looking: &[&str], is_key: bool) -> Vec<Effect> {
        let looking = set(looking);
        t.update(&UpdateInput { sessions: s, waiting: &[], statuses: &st(statuses), focused, looking: Some(&looking), is_key }).1
    }

    #[test]
    fn visible_tiles_count_as_seen() {
        let mut t = AttentionTracker::new(MemStore::default());
        let s = sessions(&["a", "b"]);
        look(&mut t, &s, &[("a", Running), ("b", Running)], Some("a"), &["a", "b"], true);
        // b ist sichtbar, aber nicht fokussiert: fertig werden markiert keine neue Marke.
        let fx = look(&mut t, &s, &[("a", Running), ("b", Idle)], Some("a"), &["a", "b"], true);
        assert!(t.unseen().is_empty(), "sichtbare Kachel bekommt kein NEU");
        assert!(!fx.iter().any(|e| matches!(e, Effect::Play(_))));
    }

    #[test]
    fn bringing_unseen_into_view_clears_mark() {
        let mut t = AttentionTracker::new(MemStore::default());
        let s = sessions(&["a"]);
        look(&mut t, &s, &[("a", Running)], None, &[], false);
        look(&mut t, &s, &[("a", Idle)], None, &[], false);
        assert_eq!(t.unseen(), &set(&["a"]));
        // Jetzt in den Blick geholt: Marke fällt weg, ohne dass sich der Status ändert.
        look(&mut t, &s, &[("a", Idle)], Some("a"), &["a"], true);
        assert!(t.unseen().is_empty(), "in den Blick geholt = gesehen");
    }

    #[test]
    fn prune_and_persisted_start() {
        let mut store = MemStore::default();
        store.set_list(UNSEEN_KEY, &["a".to_string(), "gone".to_string()]);
        let mut t = AttentionTracker::new(store);
        assert_eq!(t.unseen(), &set(&["a", "gone"]));
        t.prune(&sessions(&["a"]));
        assert_eq!(t.unseen(), &set(&["a"]));
        assert_eq!(t.store().get_list(UNSEEN_KEY), ["a"]);
    }

    #[test]
    fn tip_only_once() {
        let mut t = AttentionTracker::new(MemStore::default());
        assert_eq!(t.show_tip_once_now("tip.test", TipText::Other("x".into())), [Effect::Tip(Some(TipText::Other("x".into())))]);
        assert_eq!(t.clear_tip(None), Some(Effect::Tip(None)));
        assert!(t.show_tip_once_now("tip.test", TipText::Other("x".into())).is_empty());
        assert_eq!(t.tip, None);
    }

    #[test]
    fn clear_tip_only_if_it_is_still_the_same() {
        let mut t = AttentionTracker::new(MemStore::default());
        t.show_tip_once_now("a", TipText::Other("x".into()));
        assert_eq!(t.clear_tip(Some(&TipText::Other("y".into()))), None);
        assert_eq!(t.clear_tip(Some(&TipText::Other("x".into()))), Some(Effect::Tip(None)));
    }

    #[test]
    fn focus_hook_fires_only_on_change() {
        let mut t = AttentionTracker::new(MemStore::default());
        let s = sessions(&["a", "b"]);
        let mut fired = Vec::new();
        step(&mut t, &s, &[], &[], Some("a"), true);
        t.focus_hook_enabled = true;
        for f in ["a", "a", "b"] {
            for e in step(&mut t, &s, &[], &[], Some(f), true).1 {
                if let Effect::FireFocusHook(k) = e {
                    fired.push(k);
                }
            }
        }
        assert_eq!(fired, ["a", "b"], "vor claude kein Hook, danach nur bei Wechsel");
    }

    #[test]
    fn focus_hook_fires_again_when_active_worktree_changes() {
        let mut t = AttentionTracker::new(MemStore::default());
        t.focus_hook_enabled = true;
        let mut s = sessions(&["a"]);
        let count = |t: &mut AttentionTracker<MemStore>, s: &HashMap<String, AttentionSession>| step(t, s, &[], &[], Some("a"), true).1.iter().filter(|e| matches!(e, Effect::FireFocusHook(_))).count();
        assert_eq!(count(&mut t, &s), 1);
        assert_eq!(count(&mut t, &s), 0);
        s.get_mut("a").unwrap().active_worktree = Some("/wt".into());
        assert_eq!(count(&mut t, &s), 1);
    }

    #[test]
    fn transitions_rules() {
        let old = st(&[("a", Running), ("b", Idle), ("c", Running)]);
        let new = st(&[("a", Idle), ("b", Waiting), ("c", Running), ("d", Waiting)]);
        let t = transitions(&old, &new);
        assert_eq!(t.waiting, set(&["b"]), "neue Sessions ohne Vorstand zählen nicht");
        assert_eq!(t.done, set(&["a"]));
    }
}
