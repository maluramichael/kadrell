//! Rechte Seite der Leiste wie `StatusBarView.swift`: Konto-Taste, „N trennen“, „N warten“, Nutzungswerte als flache
//! Module und das schwarze Status-Display mit Lampe und Uhr. Die Teile entstehen von rechts nach links mit geschätzter
//! Breite (die Schrift ist dicktengleich: Zeichen mal 0,6 der Schriftgröße); `BarPlan::fit` lässt in fester Reihenfolge
//! Teile weg, bis alles neben die linken Tasten passt.
//!
//! Nicht übernommen: das Hochzählen der Werte beim Wechsel (0,5 s).

use std::rc::Rc;

use chrono::Local;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Rgba, Role,
    StatefulInteractiveElement, Styled, Window, div, hsla, px,
};
use kadrell_i18n::trd;

use super::bar::ExtStatus;
use super::bar_keys::{Flash, Press, Region, press_key};
use super::chrome::{screen_face, screen_ink, screen_muted, v_seam};
use super::pill::{Pill, PillKind};
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
    pub on_account: Press,
    pub on_detach: Press,
    pub on_waiting: Press,
    /// Einmaliger Tipp (zweite Session, Bedeutung von Gelb): Modul „Tipp  ×“, ein Klick räumt ihn weg.
    pub tip: Option<String>,
    pub on_tip: Handler,
    /// claude ist älter als getestet: Modul „claude alt · claude update“, ein Klick kopiert den Befehl.
    pub version_warning: Option<String>,
    pub on_version: Handler,
    /// Letzte Ansage („<Titel> wartet“) für Bildschirmleser, als Beschreibung am Display.
    pub announcement: Option<String>,
    /// Gedrückte Taste (80 ms eingesunken) und wie man sie meldet.
    pub pressed: Option<&'static str>,
    pub flash: Flash,
    /// Statuseinträge der Extensions, links neben den Nutzungswerten (der erste ganz links).
    pub ext: Vec<ExtStatus>,
    /// Neuere Version bereit (`misc::update`): dezente Taste „<Version> verfügbar“.
    pub update: Option<UpdatePill>,
}

/// Die Update-Taste: Versionsnummer und was ein Klick tut (Changelog und Download-Link zeigen).
#[derive(Clone)]
pub struct UpdatePill {
    pub version: String,
    pub on_click: Handler,
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

/// Ein Teil eines Moduls: Stücke in einer Farbe ohne Abstand dazwischen („25%/“ gedämpft, „32%“ farbig).
type Group = Vec<(String, Hsla)>;

/// Schrift eines flachen Moduls.
#[derive(Clone, Copy)]
struct Face {
    size: f32,
    bold: bool,
}

const USAGE_FACE: Face = Face { size: 11.5, bold: false };

fn group_width(g: &Group, size: f32) -> f32 {
    g.iter().map(|(s, _)| text_width(s, size)).sum()
}

fn group_el(g: Group) -> impl IntoElement {
    g.into_iter().fold(div().flex().flex_none(), |row, (s, c)| row.child(div().text_color(c).child(s)))
}

/// Flaches Modul mit Fuge rechts: Teile (Kürzel gedämpft, Wert farbig), 5 px Abstand dazwischen.
fn module(id: impl Into<ElementId>, groups: Vec<Group>, tip: Option<String>, click: Option<&Handler>, t: &Theme) -> Part {
    module_with(id, groups, USAGE_FACE, tip, click, t)
}

fn module_with(id: impl Into<ElementId>, groups: Vec<Group>, face: Face, tip: Option<String>, click: Option<&Handler>, t: &Theme) -> Part {
    let inner: f32 = groups.iter().map(|g| group_width(g, face.size)).sum::<f32>() + 5. * groups.len().saturating_sub(1) as f32;
    let row = div().flex().items_center().h_full().px(px(10.)).gap(px(5.)).text_size(px(face.size)).whitespace_nowrap();
    let row = if face.bold { row.font_weight(FontWeight::BOLD) } else { row };
    let row = groups.into_iter().fold(row, |r, g| r.child(group_el(g)));
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

/// Links der Tasten: Tipp, dann die Versionswarnung (`noticeModuleParts`).
fn notice_parts(b: &BarCtl, plan: &BarPlan, t: &Theme) -> Vec<Part> {
    let mut parts = Vec::new();
    if let Some(tip) = b.tip.as_ref().filter(|_| plan.tip) {
        let text = vec![vec![(format!("{tip}  ×"), t.waiting)]];
        let region = Region::new(trd!("Tipp"), Some(tip.clone()));
        parts.push(module_with("tip", text, Face { size: 10.5, bold: false }, Some(region.tooltip()), Some(&b.on_tip), t));
    }
    if let Some(warning) = b.version_warning.as_ref().filter(|_| plan.version) {
        let text = vec![vec![(trd!("claude alt · claude update"), t.waiting)]];
        let region = Region::new(trd!("Ältere claude-Version"), Some(warning.clone()));
        parts.push(module_with("claude-version", text, Face { size: 10.5, bold: true }, Some(region.tooltip()), Some(&b.on_version), t));
    }
    parts
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
            // Der Plan-Stand vor dem Wert („25%/“) bleibt gedämpft, nur der Wert trägt die Ampelfarbe.
            let mut shown = Vec::new();
            if !prefix.is_empty() {
                shown.push((prefix, t.muted));
            }
            shown.push((value, color));
            let id = ids[match row.key { UsageKey::Fable => 0, UsageKey::Weekly => 1, UsageKey::Session => 2 }];
            module(id, vec![vec![(row.key.name().to_string(), t.muted)], shown], Some(usage_tip(&row, b)), None, t)
        })
        .collect()
}

// MARK: Tasten

/// Taste der rechten Seite: feuert beim Drücken, Trefferfläche über die ganze Höhe, Tooltip „Label: Wert“.
fn key_part(id: &'static str, label: String, fill: Option<Hsla>, region: Region, press: &Press, b: &BarCtl, t: &Theme) -> Part {
    let width = text_width(&label, 10.) + 18. + KEY_GAP;
    let mut pill = Pill::new(id, label);
    match fill {
        Some(f) => pill = pill.fill(f),
        None if b.pressed == Some(id) => pill = pill.kind(PillKind::Active).fill(t.surface),
        None => {}
    }
    Part { width, el: press_key(id, id, &region, pill, press, &b.flash).into_any_element() }
}

/// Die Update-Taste hat ihren eigenen Klick (`misc::update`), sie feuert beim Loslassen.
fn update_part(u: &UpdatePill) -> Part {
    let label = trd!("%@ verfügbar", u.version.as_str());
    let width = text_width(&label, 10.) + 18. + KEY_GAP;
    let h = u.on_click.clone();
    let pill = Pill::new("update", label).tooltip_text(Region::new(trd!("Update verfügbar"), Some(u.version.clone())).tooltip()).on_click(move |e, w, c| h(e, w, c));
    Part { width, el: pill.into_any_element() }
}

fn waiting_region(b: &BarCtl) -> Region {
    Region::new(trd!("Wartende Sessions"), Some(b.waiting.to_string()))
}

fn detach_region(b: &BarCtl) -> Region {
    Region::new(trd!("Fertige Sessions trennen"), Some(b.detachable.to_string()))
}

/// Titel des aktiven Kontos, ohne Konto die Aufforderung.
fn account_title(b: &BarCtl) -> String {
    b.view.active_title().unwrap_or_else(|| trd!("Konto wählen"))
}

fn account_region(b: &BarCtl) -> Region {
    Region::new(trd!("Aktives Konto"), Some(account_title(b)))
}

/// Konto-Taste: aktiver Account, bei Platzmangel nur die ersten zehn Zeichen. Ohne Konten keine Taste.
fn account_part(b: &BarCtl, short: bool, t: &Theme) -> Option<Part> {
    if b.view.menu.is_empty() {
        return None;
    }
    let title = account_title(b);
    let limit = if short { 10 } else { 26 };
    let shown = if title.chars().count() > limit { format!("{}…", title.chars().take(limit - 1).collect::<String>()) } else { title };
    Some(key_part("account", shown, None, account_region(b), &b.on_account, b, t))
}

/// Von rechts: „N warten“, „N trennen“, Update und Konto, jeweils nur wenn es etwas zu tun gibt.
fn key_parts(b: &BarCtl, plan: &BarPlan, t: &Theme) -> Vec<Part> {
    let mut parts = Vec::new();
    if b.waiting > 0 {
        parts.push(key_part("waiting", trd!("%lld warten", b.waiting), Some(t.waiting), waiting_region(b), &b.on_waiting, b, t));
    }
    if b.detachable > 0 {
        parts.push(key_part("detach", trd!("%lld trennen", b.detachable), Some(t.idle), detach_region(b), &b.on_detach, b, t));
    }
    if let Some(u) = b.update.as_ref().filter(|_| plan.update) {
        parts.push(update_part(u));
    }
    if plan.account {
        parts.extend(account_part(b, plan.short_account, t));
    }
    parts
}

/// Trefferflächen der rechten Seite in der Reihenfolge des Zeichnens (von rechts), am Ende das Display „Status“.
pub fn right_regions(b: &BarCtl, plan: &BarPlan) -> Vec<Region> {
    let mut regions = Vec::new();
    if b.waiting > 0 {
        regions.push(waiting_region(b));
    }
    if b.detachable > 0 {
        regions.push(detach_region(b));
    }
    if let Some(u) = b.update.as_ref().filter(|_| plan.update) {
        regions.push(Region::new(trd!("Update verfügbar"), Some(u.version.clone())));
    }
    if plan.account && !b.view.menu.is_empty() {
        regions.push(account_region(b));
    }
    regions.push(Region::new(trd!("Status"), Some(spoken_status(b, plan))));
    regions
}

// MARK: Display

fn luminance(c: Hsla) -> f32 {
    let c: Rgba = c.into();
    let lin = |v: f32| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Fehlerrot, so weit Richtung Weiß gemischt, dass es auf dem schwarzen Display lesbar ist (mindestens 4,5:1),
/// wie `StatusBarView.screenError`.
pub fn screen_error(t: &Theme) -> Hsla {
    let (face, error): (Hsla, Rgba) = (screen_face(t), t.error.into());
    let mixed = |k: f32| -> Hsla { Rgba { r: error.r + (1. - error.r) * k, g: error.g + (1. - error.g) * k, b: error.b + (1. - error.b) * k, a: 1. }.into() };
    (0..=20).map(|i| mixed(i as f32 * 0.05)).find(|c| contrast(*c, face) >= 4.5).unwrap_or_else(|| hsla(0., 0., 1., 1.))
}

/// Was Tooltip und Bildschirmleser vom Display bekommen: die Langform bzw. die Fehlermeldung, dazu die Nutzungswerte,
/// die aus Platzmangel fehlen.
fn spoken_status(b: &BarCtl, plan: &BarPlan) -> String {
    format!("{}{}", b.error.clone().unwrap_or_else(|| logic::status_spoken(&b.counts)), dropped_usage(b, plan))
}

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
    let ink = if b.error.is_some() { screen_error(t) } else { screen_ink() };
    let lamp = if b.error.is_some() { t.error } else { lamp_color(logic::lamp(&b.counts), t) };
    let spoken = spoken_status(b, plan);
    let label = div().flex_none().w(px(text_w)).overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(ink).font_weight(FontWeight::BOLD).child(text);
    let mut el = div()
        .id("status-display")
        .role(Role::Status)
        .aria_label(trd!("Status"))
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .overflow_hidden()
        .h(px(20.))
        .w(px(w))
        .px(px(10.))
        .rounded(px(5.))
        .bg(screen_face(t))
        .text_size(px(10.5))
        .child(div().absolute().top_0().left_0().right_0().h(px(1.)).bg(hsla(0., 0., 1., 0.06)))
        .child(div().size(px(7.)).flex_none().rounded_full().bg(lamp).mr(px(6.)))
        .child(label)
        .child(div().flex_1().min_w(px(if text_w > 0. { 8. } else { 0. })))
        .child(div().flex_none().text_color(screen_muted()).child(clock.to_string()))
        .tooltip(tooltip::text(spoken.clone()));
    el = el.aria_description(b.announcement.clone().map_or(spoken.clone(), |a| format!("{spoken}. {a}")));
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
    parts.extend(notice_parts(b, plan, t));
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
            module(("ext-status", i), vec![vec![(e.text.clone(), e.dot.unwrap_or(t.fg))]], Some(tip), e.click.as_ref(), t)
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
