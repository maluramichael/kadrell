//! Rechte Seite der Leiste wie `StatusBarView.swift`: Konto-Taste, „N trennen“, „N warten“, Nutzungswerte als flache
//! Module und das schwarze Status-Display mit Lampe und Uhr. Die Teile entstehen von rechts nach links mit geschätzter
//! Breite (die Schrift ist dicktengleich: Zeichen mal 0,6 der Schriftgröße); `BarPlan::fit` lässt in fester Reihenfolge
//! Teile weg, bis alles neben die linken Tasten passt.
//!
//! Nicht übernommen: das Hochzählen der Werte beim Wechsel (0,5 s).

use std::rc::Rc;

use chrono::Local;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled, Window, div, px,
};
use kadrell_i18n::trd;

use super::bar::ExtStatus;
use super::chrome::{screen_face, screen_ink, screen_muted, v_seam};
use super::pill::Pill;
use super::tooltip;
use crate::theme::Theme;
use crate::usage::hub::View;
use crate::usage::logic::{self, BarPlan, Lamp, StatusCounts, Tone, UsageKey, UsageShown};

pub type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

const KEY_GAP: f32 = 4.;
const EDGE: f32 = 6.;
const SEAM: f32 = 2.;
/// Breite eines Zeichens relativ zur Schriftgröße (JetBrains Mono).
const ADVANCE: f32 = 0.6;

/// Alles, was die rechte Seite braucht; die Shell baut es bei jedem Zeichnen neu.
#[derive(Clone)]
pub struct BarCtl {
    pub view: View,
    pub now: f64,
    pub shown: UsageShown,
    pub counts: StatusCounts,
    /// Fertige Claude-Sessions mit Prozess: Zahl auf der Taste „N trennen“.
    pub detachable: usize,
    pub waiting: usize,
    /// Fehlermeldung, die statt des Status im Display steht.
    pub error: Option<String>,
    pub lang: kadrell_accounts::Lang,
    /// Breite des Fensters und geschätzte Breite der linken Tasten.
    pub width: f32,
    pub left: f32,
    pub on_account: Handler,
    pub on_detach: Handler,
    pub on_waiting: Handler,
    /// Statuseinträge der Extensions, links neben den Nutzungswerten (der erste ganz links).
    pub ext: Vec<ExtStatus>,
}

fn text_width(s: &str, size: f32) -> f32 {
    s.chars().count() as f32 * size * ADVANCE
}

/// Ein Teil der rechten Seite: Breite samt Abstand zum linken Nachbarn und das Element dazu.
struct Part {
    width: f32,
    el: AnyElement,
}

fn tone_color(tone: Tone, t: &Theme) -> Hsla {
    match tone {
        Tone::Good => t.idle,
        Tone::Warn => t.waiting,
        Tone::Bad => t.error,
    }
}

// MARK: Nutzungswerte

struct UsageRow {
    key: UsageKey,
    pct: Option<i32>,
    plan: Option<i32>,
    reset: Option<f64>,
    label: String,
}

/// Nutzungswerte von rechts: Fable-Woche, 7 Tage, 5 Stunden, soweit die Einstellungen sie zeigen.
fn usage_rows(b: &BarCtl) -> Vec<UsageRow> {
    let u = &b.view.usage;
    let rows = [
        UsageRow { key: UsageKey::Fable, pct: u.fable, plan: None, reset: u.weekly_resets, label: trd!("Fable-Kontingent") },
        UsageRow { key: UsageKey::Weekly, pct: u.weekly, plan: u.weekly_plan(b.now), reset: u.weekly_resets, label: trd!("Claude-Nutzung der letzten 7 Tage") },
        UsageRow { key: UsageKey::Session, pct: u.session, plan: None, reset: u.session_resets, label: trd!("Claude-Nutzung der letzten 5 Stunden") },
    ];
    rows.into_iter().filter(|r| b.shown.allows(r.key, b.view.show_session)).collect()
}

/// Wert des Moduls: Plan-Stand vorweg („25%/32%“), dann der Prozentwert.
fn value_text(row: &UsageRow) -> (String, String) {
    let plan = row.plan.filter(|_| row.pct.is_some()).map(|p| format!("{p}%/")).unwrap_or_default();
    (plan, row.pct.map_or_else(|| "–%".to_string(), |v| format!("{v}%")))
}

fn usage_tip(row: &UsageRow, b: &BarCtl) -> String {
    let base = match (row.pct, row.plan) {
        (None, _) => trd!("%@: Nutzung nicht abrufbar", row.label.as_str()),
        (Some(p), None) => trd!("%@: %lld %% verbraucht", row.label.as_str(), p),
        (Some(p), Some(plan)) => trd!("%@: %lld %% verbraucht, %lld %% nach Plan", row.label.as_str(), p, plan),
    };
    format!("{base}{}{}", reset_tip(row.reset, b), stale_tip(b))
}

fn stale_tip(b: &BarCtl) -> String {
    match logic::stale_minutes(b.view.measured_at, b.now) {
        Some(m) => format!(" · {}", trd!("Stand von vor %lld min", m)),
        None => String::new(),
    }
}

/// Reset-Zeit im Tooltip: absolute Uhrzeit plus relativ, leer wenn kein Reset-Zeitpunkt bekannt ist.
fn reset_tip(resets: Option<f64>, b: &BarCtl) -> String {
    let Some(at) = resets.filter(|r| *r > b.now) else { return String::new() };
    let clock = kadrell_accounts::reset_clock(at, b.now, &Local, b.lang);
    format!(" · {}", trd!("Reset %@ (%@)", clock.as_str(), relative(at - b.now, b.lang).as_str()))
}

/// „in 2 Std.“, „in 35 Min.“, „in 3 Tg.“ (abgekürzt wie `RelativeDateTimeFormatter`).
pub fn relative(secs: f64, lang: kadrell_accounts::Lang) -> String {
    let (n, unit) = if secs >= 86_400. {
        ((secs / 86_400.).round() as i64, 2)
    } else if secs >= 3600. {
        ((secs / 3600.).round() as i64, 1)
    } else {
        ((secs / 60.).round().max(1.) as i64, 0)
    };
    match (lang, unit) {
        (kadrell_accounts::Lang::De, 0) => format!("in {n} Min."),
        (kadrell_accounts::Lang::De, 1) => format!("in {n} Std."),
        (kadrell_accounts::Lang::De, _) => format!("in {n} Tg."),
        (_, 0) => format!("in {n} min."),
        (_, 1) => format!("in {n} hr."),
        (_, _) => format!("in {n} days"),
    }
}

/// Flaches Modul mit Fuge rechts: Text in Teilen (Kürzel gedämpft, Wert farbig).
fn module(id: impl Into<ElementId>, spans: Vec<(String, Hsla)>, tip: Option<String>, click: Option<&Handler>, t: &Theme) -> Part {
    let inner: f32 = spans.iter().map(|(s, _)| text_width(s, 11.5)).sum::<f32>() + 5. * spans.len().saturating_sub(1) as f32;
    let row = div().flex().items_center().h_full().px(px(10.)).gap(px(5.)).text_size(px(11.5)).whitespace_nowrap();
    let row = spans.into_iter().fold(row, |r, (s, c)| r.child(div().text_color(c).child(s)));
    let mut el = div().id(id).flex().flex_none().items_center().h_full().child(row).child(v_seam(t));
    if let Some(tip) = tip {
        el = el.tooltip(tooltip::text(tip));
    }
    if let Some(h) = click {
        let h = h.clone();
        el = el.cursor_pointer().on_click(move |e, w, c| h(e, w, c));
    }
    Part { width: SEAM + 20. + inner, el: el.into_any_element() }
}

fn usage_parts(b: &BarCtl, plan: &BarPlan, t: &Theme) -> Vec<Part> {
    let stale = logic::stale_minutes(b.view.measured_at, b.now).is_some();
    let ids = ["usage-fable", "usage-7d", "usage-5h"];
    usage_rows(b)
        .into_iter()
        .filter(|r| plan.shows(r.key))
        .map(|row| {
            let (prefix, value) = value_text(&row);
            let color = match row.pct {
                Some(v) if !stale => tone_color(logic::usage_tone(v, row.plan), t),
                _ => t.muted,
            };
            let mut spans = vec![(row.key.name().to_string(), t.muted)];
            if prefix.is_empty() {
                spans.push((value, color));
            } else {
                spans.push((format!("{prefix}{value}"), color));
            }
            let id = ids[match row.key { UsageKey::Fable => 0, UsageKey::Weekly => 1, UsageKey::Session => 2 }];
            module(id, spans, Some(usage_tip(&row, b)), None, t)
        })
        .collect()
}

// MARK: Tasten

fn key_part(id: &'static str, label: String, fill: Option<Hsla>, tip: String, handler: &Handler) -> Part {
    let width = text_width(&label, 10.) + 18. + KEY_GAP;
    let h = handler.clone();
    let mut pill = Pill::new(id, label).tooltip_text(tip).on_click(move |e, w, c| h(e, w, c));
    if let Some(f) = fill {
        pill = pill.fill(f);
    }
    Part { width, el: pill.into_any_element() }
}

/// Konto-Taste: aktiver Account, bei Platzmangel nur die ersten zehn Zeichen. Ohne Konten keine Taste.
fn account_part(b: &BarCtl, short: bool) -> Option<Part> {
    if b.view.menu.is_empty() {
        return None;
    }
    let title = b.view.active_title().unwrap_or_else(|| trd!("Konto wählen"));
    let limit = if short { 10 } else { 26 };
    let shown = if title.chars().count() > limit { format!("{}…", title.chars().take(limit - 1).collect::<String>()) } else { title.clone() };
    let tip = format!("{}: {title}", trd!("Aktives Konto"));
    Some(key_part("account", shown, None, tip, &b.on_account))
}

/// Von rechts: „N warten“, „N trennen“ und das Konto, jeweils nur wenn es etwas zu tun gibt.
fn key_parts(b: &BarCtl, plan: &BarPlan, t: &Theme) -> Vec<Part> {
    let mut parts = Vec::new();
    if b.waiting > 0 {
        let tip = format!("{}: {}", trd!("Wartende Sessions"), b.waiting);
        parts.push(key_part("waiting", trd!("%lld warten", b.waiting), Some(t.waiting), tip, &b.on_waiting));
    }
    if b.detachable > 0 {
        let tip = format!("{}: {}", trd!("Fertige Sessions trennen"), b.detachable);
        parts.push(key_part("detach", trd!("%lld trennen", b.detachable), Some(t.idle), tip, &b.on_detach));
    }
    if plan.account {
        parts.extend(account_part(b, plan.short_account));
    }
    parts
}

// MARK: Display

fn lamp_color(lamp: Lamp, t: &Theme) -> Hsla {
    match lamp {
        Lamp::Waiting => t.waiting,
        Lamp::Error => t.error,
        Lamp::Ok => t.idle,
        Lamp::Off => screen_muted(),
    }
}

/// Nutzungswerte, die aus Platzmangel nicht in der Leiste stehen, für den Tooltip des Displays („ · 5h 40%“).
fn dropped_usage(b: &BarCtl, plan: &BarPlan) -> String {
    let rows = usage_rows(b);
    rows.iter().rev().filter(|r| !plan.shows(r.key)).map(|r| format!(" · {} {}", r.key.name(), r.pct.map_or("–%".to_string(), |v| format!("{v}%")))).collect()
}

fn display_part(b: &BarCtl, plan: &BarPlan, clock: &str, t: &Theme) -> Part {
    let text = b.error.clone().unwrap_or_else(|| logic::status_text(&b.counts, plan.compact));
    let text_w = (text_width(&text, 10.5) - plan.shrink).max(0.);
    let time_w = text_width(clock, 10.5);
    let w = 10. + 7. + 6. + text_w + if text_w > 0. { 8. } else { 0. } + time_w + 10.;
    let ink = if b.error.is_some() { t.error_text } else { screen_ink() };
    let spoken = format!("{}{}", b.error.clone().unwrap_or_else(|| logic::status_spoken(&b.counts)), dropped_usage(b, plan));
    let label = div().flex_none().w(px(text_w)).overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(ink).font_weight(FontWeight::BOLD).child(text);
    let el = div()
        .id("status-display")
        .flex()
        .flex_none()
        .items_center()
        .h(px(20.))
        .w(px(w))
        .px(px(10.))
        .rounded(px(5.))
        .bg(screen_face(t))
        .text_size(px(10.5))
        .child(div().size(px(7.)).flex_none().rounded_full().bg(lamp_color(logic::lamp(&b.counts), t)).mr(px(6.)))
        .child(label)
        .child(div().flex_1().min_w(px(if text_w > 0. { 8. } else { 0. })))
        .child(div().flex_none().text_color(screen_muted()).child(clock.to_string()))
        .tooltip(tooltip::text(spoken));
    Part { width: w + EDGE, el: el.into_any_element() }
}

// MARK: Zusammenbau

fn spacer(width: f32) -> Part {
    Part { width, el: div().flex_none().w(px(width)).into_any_element() }
}

/// Alle Teile von rechts nach links nach `plan`: Display, Tasten, Rand, Nutzung.
fn parts(b: &BarCtl, plan: &BarPlan, clock: &str, t: &Theme) -> Vec<Part> {
    let mut parts = vec![display_part(b, plan, clock, t), spacer(EDGE - KEY_GAP)];
    let keys = key_parts(b, plan, t);
    let had_keys = !keys.is_empty();
    parts.extend(keys);
    if !had_keys {
        parts.pop();
    }
    parts.extend(usage_parts(b, plan, t));
    parts.extend(ext_parts(b, plan, t));
    parts
}

/// Einträge der Extensions, der erste ganz links: es bleiben die `plan.extensions` rechten.
fn ext_parts(b: &BarCtl, plan: &BarPlan, t: &Theme) -> Vec<Part> {
    let skip = b.ext.len().saturating_sub(plan.extensions);
    b.ext
        .iter()
        .enumerate()
        .skip(skip)
        .rev()
        .map(|(i, e)| {
            let tip = format!("{}: {}", e.name, e.text);
            module(("ext-status", i), vec![(e.text.clone(), e.dot.unwrap_or(t.fg))], Some(tip), e.click.as_ref(), t)
        })
        .collect()
}

fn total(parts: &[Part]) -> f32 {
    parts.iter().map(|p| p.width).sum()
}

/// Der vollständigste Plan, der neben die linken Tasten passt.
pub fn plan_for(b: &BarCtl, clock: &str, t: &Theme) -> BarPlan {
    let room = b.width - b.left - 16. - EDGE;
    BarPlan::fit(b.ext.len(), room, |p| total(&parts(b, p, clock, t)))
}

/// Die rechte Seite, von links nach rechts gesetzt (die Teile entstehen von rechts).
pub fn right_side(clock: &str, b: &BarCtl, t: &Theme) -> impl IntoElement + use<> {
    let plan = plan_for(b, clock, t);
    let mut parts = parts(b, &plan, clock, t);
    parts.reverse();
    let row = div().flex().flex_none().items_center().h_full().gap(px(0.));
    let row = parts.into_iter().fold(row, |r, p| r.child(p.el));
    div().flex().flex_none().items_center().h_full().pr(px(EDGE)).child(row)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_time_is_abbreviated() {
        use kadrell_accounts::Lang;
        assert_eq!(relative(35. * 60., Lang::De), "in 35 Min.");
        assert_eq!(relative(2. * 3600., Lang::De), "in 2 Std.");
        assert_eq!(relative(3. * 86_400., Lang::En), "in 3 days");
    }

    #[test]
    fn text_width_counts_characters_not_bytes() {
        assert_eq!(text_width("äöü", 10.), 18.);
    }
}
