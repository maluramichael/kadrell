//! Aufbau der Statistik-Seite: Kopf mit Zeitraum, Limits je Konto, Prompts mit Projektliste, Zähler und Fuß.
//! Beschriftung der Achsen und Tooltips sind Elemente über den gezeichneten Flächen (`draw`).

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, Context, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, Window, div,
    prelude::*, px, relative,
};
use kadrell_accounts::{AccountLimits, LimitKind, LimitPoint, LimitStats, ProjectSort, PromptStats, StatsRange, StatsSnapshot};
use kadrell_i18n::trd;

use super::draw::{self, END_PAD, Hover, OnHover, Style};
use super::format::{self, count_ticks};
use super::{Counters, StatsView, ui_lang};
use crate::theme::Theme;
use crate::ui::settings::controls::measure;
use crate::ui::theme;

const LIMIT_HEIGHT: f32 = 110.;
const PROMPT_HEIGHT: f32 = 190.;
const Y_LABEL_WIDTH: f32 = 38.;
/// Reicht die Breite nicht für Chart und Liste nebeneinander, steht die Liste darunter.
const SIDE_BY_SIDE: f32 = 720.;
const PROMPT_CHART: usize = 1000;

fn range_label(range: StatsRange) -> String {
    match range {
        StatsRange::Day => trd!("24 Std."),
        StatsRange::Week => trd!("7 Tage"),
        StatsRange::Month => trd!("30 Tage"),
        StatsRange::Year => trd!("1 Jahr"),
    }
}

fn sort_label(sort: ProjectSort) -> String {
    match sort {
        ProjectSort::Count => trd!("Anzahl"),
        ProjectSort::Name => trd!("Name"),
        ProjectSort::Recent => trd!("Zuletzt"),
    }
}

fn kind_label(kind: LimitKind) -> String {
    match kind {
        LimitKind::Session => trd!("5 Stunden"),
        LimitKind::Weekly => trd!("7 Tage"),
        LimitKind::Fable => "Fable".to_string(),
    }
}

/// Zahl mit Tausendertrenner der Sprache.
pub fn number(n: i64, lang: kadrell_accounts::Lang) -> String {
    let sep = if lang == kadrell_accounts::Lang::De { '.' } else { ',' };
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

fn section_title(text: String, t: &Theme) -> impl IntoElement {
    div().text_size(px(11.)).font_weight(FontWeight::BOLD).text_color(t.accent_text).child(text)
}

fn swatch(color: Hsla) -> impl IntoElement {
    div().size(px(8.)).flex_none().rounded(px(2.)).bg(color)
}

fn muted(text: impl Into<gpui::SharedString>, size: f32, t: &Theme) -> gpui::Div {
    div().text_size(px(size)).text_color(t.muted).child(text.into())
}

/// Eingelassene Gruppe wie `instrumentGroup()`.
fn group(t: &Theme) -> gpui::Div {
    // Fläche `bg`, Radius 9, geschlossener Fugenring (außen dunkel, innen hell).
    let r = crate::ui::instrument::GROUP_RADIUS * crate::ui::overlay::scale();
    crate::ui::overlay::border_px(div().relative().rounded(px(r)).bg(t.bg), crate::ui::overlay::hairline())
        .border_color(crate::ui::chrome::seam_dark(t))
        .child(crate::ui::overlay::inner_ring(t, r))
}

impl StatsView {
    fn on_hover(&self, cx: &mut Context<Self>) -> OnHover {
        let view = cx.entity();
        Rc::new(move |h, app| view.update(app, |s, cx| s.set_hover(h, cx)))
    }

    pub(super) fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let buttons = StatsRange::ALL.iter().map(|&range| {
            let selected = self.range == range;
            div()
                .id(("range", range.raw() as usize))
                .px(px(12.))
                .py(px(5.))
                .rounded(px(5.))
                .text_size(px(12.))
                .cursor_pointer()
                .border(px(1.5))
                .border_color(gpui::transparent_black())
                .when(selected, |d| d.bg(t.line).border_color(t.fg))
                .child(range_label(range))
                .on_click(cx.listener(move |this, _, _, cx| this.set_range(range, cx)))
        });
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(16.))
            .px(px(16.))
            .pt(px(14.))
            .pb(px(12.))
            .child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).child(trd!("Statistik")))
            .child(div().flex().gap(px(4.)).children(buttons))
    }

    pub(super) fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .px(px(16.))
            .py(px(10.))
            .text_size(px(11.))
            .text_color(t.muted)
            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(trd!("1 bis 4 oder ←/→ Zeitraum · ↑/↓ Projekt · Leertaste filtert · ⌫ alle · s sortiert")))
            .child(div().flex_none().pl(px(12.)).child(self.loader.close_hint.clone()))
    }

    /// Höchstbreite des Panels (`overlayMaxWidth` im fensterfüllenden Dialog), nicht die Breite nach dem Innenabstand.
    fn avail_width(_window: &Window) -> f32 {
        crate::ui::overlay::area().full_window().width
    }

    // MARK: Limits

    pub(super) fn limits_section(&self, snap: &Arc<StatsSnapshot>, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let t = theme(cx);
        let legend = LimitKind::ALL.iter().map(|&k| {
            div().flex().items_center().gap(px(4.)).child(swatch(draw::limit_color(k, &t))).child(muted(kind_label(k), 11., &t))
        });
        let head = div()
            .flex()
            .items_center()
            .gap(px(14.))
            .child(section_title(trd!("LIMITS"), &t))
            .child(muted(trd!("Verbraucht in Prozent"), 11., &t))
            .children(legend);
        let columns = limit_columns(Self::avail_width(window), crate::ui::overlay::scale(), snap.limits.len());
        let cards: Vec<AnyElement> = (0..snap.limits.len()).map(|i| self.limit_card(snap, i, cx)).collect();
        let mut grid: Vec<AnyElement> = Vec::new();
        for chunk in cards.chunks_mut_vec(columns) {
            grid.push(div().flex().gap(px(12.)).children(chunk.into_iter().map(|c| div().flex_1().min_w_0().child(c))).into_any_element());
        }
        div().flex().flex_col().gap(px(8.)).child(head).child(div().flex().flex_col().gap(px(12.)).children(grid)).into_any_element()
    }

    fn limit_card(&self, snap: &Arc<StatsSnapshot>, idx: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = theme(cx);
        let account = &snap.limits[idx];
        let head = div()
            .flex()
            .items_baseline()
            .gap(px(8.))
            .child(div().text_size(px(12.)).font_weight(FontWeight::BOLD).whitespace_nowrap().child(account.title.clone()))
            .child(div().flex_1())
            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(11.)).text_color(t.muted).child(account.detail.clone()));
        let body = if account.is_empty() {
            div().flex().items_center().justify_center().h(px(LIMIT_HEIGHT)).text_size(px(12.)).text_color(t.muted).child(trd!("Noch keine Messwerte in diesem Zeitraum.")).into_any_element()
        } else {
            self.limit_chart(snap, idx, account, cx)
        };
        group(&t).p(px(10.)).flex().flex_col().gap(px(6.)).child(head).child(body).into_any_element()
    }

    fn limit_chart(&self, snap: &Arc<StatsSnapshot>, idx: usize, account: &AccountLimits, cx: &mut Context<Self>) -> AnyElement {
        let t = theme(cx);
        let lang = ui_lang();
        let y_labels: Vec<(f32, String)> = [(0., "100 %"), (0.5, "50 %"), (1., "0 %")].iter().map(|(f, s)| (*f, s.to_string())).collect();
        let tooltip = self.hover.filter(|h| h.chart == idx).and_then(|h| limit_tooltip(account, snap, h, lang, &t).map(|c| (h, c)));
        let canvas = draw::limit_chart(snap.clone(), idx, self.hover, Style::new(&t), self.on_hover(cx));
        chart_frame(snap, canvas, &y_labels, LIMIT_HEIGHT, true, tooltip, lang, &t)
    }

    // MARK: Prompts

    pub(super) fn prompts_section(&self, snap: &Arc<StatsSnapshot>, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let t = theme(cx);
        let lang = ui_lang();
        let head = div()
            .flex()
            .items_baseline()
            .gap(px(14.))
            .child(section_title(trd!("PROMPTS"), &t))
            .child(div().text_size(px(14.)).font_weight(FontWeight::BOLD).child(number(snap.prompt_total as i64, lang)))
            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(11.)).text_color(t.muted).child(trd!("Aus den Transcripts, auch von Sessions außerhalb von Kadrell.")));
        let body = if snap.bars.is_empty() {
            let text = if self.reading { trd!("Transcripts werden gelesen …") } else { trd!("Keine Prompts in diesem Zeitraum.") };
            div().flex().items_center().justify_center().h(px(PROMPT_HEIGHT)).text_size(px(12.)).text_color(t.muted).child(text).into_any_element()
        } else if side_by_side(Self::avail_width(window), crate::ui::overlay::scale()) {
            div().flex().gap(px(16.)).child(div().flex_1().min_w_0().child(self.prompt_chart(snap, cx))).child(div().w(px(240.)).flex_none().child(self.project_list(snap, cx))).into_any_element()
        } else {
            div().flex().flex_col().gap(px(8.)).child(self.prompt_chart(snap, cx)).child(self.project_list(snap, cx)).into_any_element()
        };
        div().flex().flex_col().gap(px(8.)).child(head).child(body).into_any_element()
    }

    fn prompt_chart(&self, snap: &Arc<StatsSnapshot>, cx: &mut Context<Self>) -> AnyElement {
        let t = theme(cx);
        let lang = ui_lang();
        let max = draw::prompt_max(snap);
        let y_labels: Vec<(f32, String)> = count_ticks(max).iter().map(|v| (1. - *v as f32 / max as f32, v.to_string())).collect();
        let tooltip = self.hover.filter(|h| h.chart == PROMPT_CHART).and_then(|h| prompt_tooltip(snap, h, lang, &t).map(|c| (h, c)));
        let canvas = draw::prompt_chart(snap.clone(), PROMPT_CHART, self.hover, Style::new(&t), self.on_hover(cx));
        chart_frame(snap, canvas, &y_labels, PROMPT_HEIGHT, false, tooltip, lang, &t)
    }

    /// Die Tabelle zum Chart: alle Projekte des Zeitraums, scrollbar. ↑/↓ bewegt den Cursor, Leertaste oder Klick
    /// wählt Projekte für den Chart, s sortiert.
    fn project_list(&self, snap: &Arc<StatsSnapshot>, cx: &mut Context<Self>) -> AnyElement {
        let t = theme(cx);
        let lang = ui_lang();
        let head = div().flex().items_center().gap(px(4.)).px(px(12.)).text_size(px(11.)).text_color(t.muted).child(trd!("Sortiert:")).child(div().text_color(t.fg).child(sort_label(snap.sort))).child(div().flex_1());
        let head = head.children((!snap.filter.is_empty()).then(|| div().text_color(t.accent_text).child(trd!("%lld gewählt", snap.filter.len()))));
        let rows = snap.projects.iter().enumerate().map(|(i, p)| {
            let color = if p.included { t.fg } else { t.muted };
            let chip = if p.included { draw::project_color(p.slot, &t) } else { t.muted.opacity(0.4) };
            let name = p.name.clone();
            div()
                .id(("project", i))
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(12.))
                .py(px(4.))
                .rounded(px(5.))
                .cursor_pointer()
                .text_color(color)
                .border(px(1.5))
                .border_color(gpui::transparent_black())
                .when(self.cursor.as_deref() == Some(p.name.as_str()), |d| d.bg(t.surface).border_color(t.fg))
                .child(swatch(chip))
                .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(12.)).child(p.name.clone()))
                .child(div().flex_none().text_size(px(12.)).font_weight(FontWeight::BOLD).child(number(p.count as i64, lang)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.cursor = Some(name.clone());
                    this.toggle(&name, cx);
                    cx.notify();
                }))
        });
        let list = div().id("project-list").flex().flex_col().max_h(px(PROMPT_HEIGHT)).overflow_y_scroll().track_scroll(&self.list_scroll).children(rows);
        div().flex().flex_col().gap(px(4.)).child(head).child(list).into_any_element()
    }

    // MARK: Zähler

    fn counter_tiles(&self, lang: kadrell_accounts::Lang, t: &Theme) -> Vec<AnyElement> {
        let c: &Counters = &self.counters;
        vec![
            tile(c.messages, trd!("Nachrichten geschickt"), None, lang, t),
            tile(c.sessions, trd!("Sessions geöffnet"), None, lang, t),
            tile(c.terminals, trd!("Terminals geöffnet"), None, lang, t),
            tile(c.switches, trd!("Kachelwechsel"), None, lang, t),
            tile(c.record, trd!("Meiste Sessions gleichzeitig"), c.record_date.map(|d| format::long_date(d, lang)), lang, t),
        ]
    }

    /// `ViewThatFits(in: .horizontal)`: passt die Reihe in voller Breite in den Platz, steht sie in einer Reihe, sonst
    /// in zwei Spalten. Breiten aus dem letzten Frame; ändern sie sich, folgt noch ein Frame.
    pub(super) fn counters_in_one_row(&mut self, window: &mut Window) -> bool {
        let (natural, offered) = {
            let geo = self.geometry.borrow();
            (geo.get(ROW_KEY).map(|b| f32::from(b.size.width)), geo.get(AREA_KEY).map(|b| f32::from(b.size.width)))
        };
        let basis = (natural.unwrap_or(0.), offered.unwrap_or(0.));
        if self.counters_basis.is_none_or(|old| (old.0 - basis.0).abs() >= 0.5 || (old.1 - basis.1).abs() >= 0.5) {
            window.request_animation_frame();
        }
        self.counters_basis = Some(basis);
        fits_one_row(natural, offered)
    }

    pub(super) fn counters_section(&self, lang: kadrell_accounts::Lang, one_row: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = theme(cx);
        let since = match self.counters.since {
            Some(s) => format!("{} {}", trd!("Seit"), format::long_date(s, lang)),
            None => trd!("Noch nichts gezählt."),
        };
        let layout = if one_row {
            div().flex().items_start().gap_x(px(24.)).children(self.counter_tiles(lang, &t))
        } else {
            two_columns(self.counter_tiles(lang, &t))
        };
        // Die unsichtbare Reihe in natürlicher Breite misst, ob sie in den angebotenen Platz passt.
        // Breiter Rahmen, damit die Reihe darin nicht auf den Platz schrumpft; gemessen wird die Reihe selbst.
        let row = div().relative().flex_none().flex().items_start().gap_x(px(24.)).children(self.counter_tiles(lang, &t)).child(measure(&self.geometry, ROW_KEY.to_string()));
        let probe = div().absolute().top_0().left_0().w(px(4000.)).invisible().flex().child(row);
        let area = div().relative().w_full().child(layout).child(probe).child(measure(&self.geometry, AREA_KEY.to_string()));
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().flex().items_baseline().gap(px(8.)).child(section_title(trd!("GESAMT"), &t)).child(muted(since, 11., &t)))
            .child(area)
            .child(muted(trd!("Gezählt wird, solange Kadrell läuft."), 11., &t))
            .into_any_element()
    }
}

const ROW_KEY: &str = "counters-row";
const AREA_KEY: &str = "counters-area";

/// Passt die Reihe in natürlicher Breite in den angebotenen Platz? Ohne Messung zwei Spalten (die Reihe kann nicht überlaufen).
pub fn fits_one_row(natural: Option<f32>, offered: Option<f32>) -> bool {
    matches!((natural, offered), (Some(n), Some(o)) if n > 0. && n <= o)
}

/// Zwei Spalten wie das `Grid` der Mac-App: Kacheln 1, 3, 5 links, 2 und 4 rechts, Abstand 24 und 10.
fn two_columns(tiles: Vec<AnyElement>) -> gpui::Div {
    let (mut left, mut right): (Vec<AnyElement>, Vec<AnyElement>) = (Vec::new(), Vec::new());
    for (i, tile) in tiles.into_iter().enumerate() {
        if i % 2 == 0 { left.push(tile) } else { right.push(tile) }
    }
    let column = |tiles: Vec<AnyElement>| div().flex().flex_col().items_start().gap_y(px(10.)).children(tiles);
    div().flex().items_start().gap_x(px(24.)).child(column(left)).child(column(right))
}

/// So viele Spalten, wie breit genug sind (380 je Spalte, mit der UI-Größe skaliert), nie mehr als Konten und mindestens eine.
pub fn limit_columns(width: f32, scale: f32, accounts: usize) -> usize {
    let fit = (width / (380. * scale)).max(0.) as usize;
    fit.min(accounts).max(1)
}

/// Chart und Liste nebeneinander, sobald die Breite reicht (`sideBySideWidth * Theme.scale`).
pub fn side_by_side(width: f32, scale: f32) -> bool {
    width >= SIDE_BY_SIDE * scale
}

fn tile(value: i64, label: String, date: Option<String>, lang: kadrell_accounts::Lang, t: &Theme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(div().text_size(px(16.)).font_weight(FontWeight::BOLD).child(number(value, lang)))
        .child(muted(label, 12., t).whitespace_nowrap())
        .children(date.map(|d| div().text_size(px(11.)).text_color(t.accent_text).child(d)))
        .into_any_element()
}

// MARK: Rahmen und Tooltips

/// Chart mit Beschriftung: links die Werte, oben die Tagesdaten, unten die Zeitachse, darüber der Tooltip.
#[allow(clippy::too_many_arguments)]
fn chart_frame(
    snap: &StatsSnapshot,
    canvas: gpui::Canvas<()>,
    y_labels: &[(f32, String)],
    height: f32,
    day_labels: bool,
    tooltip: Option<(Hover, AnyElement)>,
    lang: kadrell_accounts::Lang,
    t: &Theme,
) -> AnyElement {
    let span = (snap.now - snap.from).max(1.);
    let frac = |at: f64| ((at - snap.from) / span) as f32;
    let y_col = div().relative().flex_none().w(px(Y_LABEL_WIDTH)).h(px(height)).children(y_labels.iter().map(|(f, text)| {
        div().absolute().right(px(6.)).top(px(3. + f * (height - 6.) - 7.)).text_size(px(10.)).text_color(t.muted).child(text.clone())
    }));
    let days = snap.days.iter().filter(|d| day_labels && d.labeled).map(|d| {
        let x = frac(d.date);
        let right = x > 0.7;
        let mut label = div().absolute().top(px(1.)).px(px(3.)).whitespace_nowrap().text_size(px(10.)).font_weight(FontWeight::BOLD).text_color(t.muted).bg(t.bg).child(format::day_label(d.date, lang));
        label = if right { label.right(relative(1. - x)) } else { label.left(relative(x)) };
        label
    });
    let tooltip_el = tooltip.map(|(h, content)| {
        let wrap = div().absolute().top(px(0.));
        let wrap = if h.x > h.width * 0.6 { wrap.right(px(h.width - h.x + 8.)) } else { wrap.left(px(h.x + 8.)) };
        wrap.child(content)
    });
    let plot = div()
        .relative()
        .flex_1()
        .min_w_0()
        .h(px(height))
        .child(div().absolute().inset_0().child(canvas.size_full()))
        .child(div().absolute().inset_0().pr(px(END_PAD)).child(div().relative().size_full().children(days)))
        .children(tooltip_el);
    let ticks = format::ticks(snap.range, snap.from, snap.now, lang).into_iter().map(|tick| {
        div().absolute().left(relative(frac(tick.at))).whitespace_nowrap().text_size(px(10.)).text_color(t.muted).child(tick.label)
    });
    let axis = div().relative().h(px(14.)).ml(px(Y_LABEL_WIDTH)).pr(px(END_PAD)).child(div().relative().size_full().children(ticks));
    div().flex().flex_col().gap(px(2.)).child(div().flex().child(y_col).child(plot)).child(axis).into_any_element()
}

fn tooltip_box(t: &Theme) -> gpui::Div {
    div().flex().flex_col().gap(px(2.)).px(px(8.)).py(px(5.)).rounded(px(4.)).bg(t.surface).text_size(px(11.)).text_color(t.fg).whitespace_nowrap()
}

/// Je Limit der Messwert am nächsten bei `at`. Liegt keiner in der Nähe, fehlt das Limit im Tooltip.
fn nearest(account: &AccountLimits, at: f64, tolerance: f64) -> Vec<&LimitPoint> {
    LimitKind::ALL
        .iter()
        .filter_map(|&kind| {
            account
                .points
                .iter()
                .filter(|p| p.kind == kind)
                .min_by(|a, b| (a.date - at).abs().total_cmp(&(b.date - at).abs()))
                .filter(|p| (p.date - at).abs() <= tolerance)
        })
        .collect()
}

fn limit_tooltip(account: &AccountLimits, snap: &StatsSnapshot, h: Hover, lang: kadrell_accounts::Lang, t: &Theme) -> Option<AnyElement> {
    let at = h.time(snap.from, snap.now)?;
    let tolerance = LimitStats::MIN_GAP.max(snap.range.span() / LimitStats::MAX_POINTS);
    let rows = nearest(account, at, tolerance).into_iter().map(|p| {
        div().flex().items_center().gap(px(4.)).child(swatch(draw::limit_color(p.kind, t))).child(kind_label(p.kind)).child(format!("{} %", p.value))
    });
    Some(tooltip_box(t).child(format::date_time(at, lang, true)).children(rows).into_any_element())
}

fn prompt_tooltip(snap: &StatsSnapshot, h: Hover, lang: kadrell_accounts::Lang, t: &Theme) -> Option<AnyElement> {
    let at = h.time(snap.from, snap.now)?;
    let parts: Vec<_> = snap.bars.iter().filter(|b| b.start <= at && at < b.end).collect();
    let first = parts.first()?;
    let total: usize = parts.iter().map(|b| b.count()).sum();
    let rows = parts.iter().rev().map(|b| {
        let name = if b.project == PromptStats::OTHER { trd!("Andere") } else { b.project.clone() };
        div().flex().items_center().gap(px(4.)).child(swatch(draw::project_color(b.slot, t))).child(name).child(number(b.count() as i64, lang))
    });
    Some(
        tooltip_box(t)
            .child(format::bar_time(first.start, snap.range, lang))
            .child(div().font_weight(FontWeight::BOLD).child(trd!("%lld Prompts", total)))
            .children(rows)
            .into_any_element(),
    )
}

/// `Vec::chunks` für eigene Elemente (die nicht `Clone` sind).
trait ChunksOwned<T> {
    fn chunks_mut_vec(self, n: usize) -> Vec<Vec<T>>;
}

impl<T> ChunksOwned<T> for Vec<T> {
    fn chunks_mut_vec(self, n: usize) -> Vec<Vec<T>> {
        let mut out: Vec<Vec<T>> = Vec::new();
        for (i, item) in self.into_iter().enumerate() {
            if i % n.max(1) == 0 {
                out.push(Vec::new());
            }
            if let Some(last) = out.last_mut() {
                last.push(item);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadrell_accounts::Lang;

    #[test]
    fn numbers_group_by_language() {
        assert_eq!(number(1_234_567, Lang::De), "1.234.567");
        assert_eq!(number(1_234_567, Lang::En), "1,234,567");
        assert_eq!(number(12, Lang::De), "12");
        assert_eq!(number(1000, Lang::En), "1,000");
        assert_eq!(number(0, Lang::De), "0");
    }

    #[test]
    fn counters_stay_in_one_row_only_when_the_measured_row_fits() {
        assert!(fits_one_row(Some(900.), Some(900.)));
        assert!(fits_one_row(Some(600.), Some(900.)));
        assert!(!fits_one_row(Some(901.), Some(900.)));
        assert!(!fits_one_row(None, Some(900.)));
        assert!(!fits_one_row(Some(600.), None));
        assert!(!fits_one_row(Some(0.), Some(900.)));
    }

    /// `StatsCharts.swift`: `max(1, min(Int(maxWidth / (380 * scale)), accounts))`.
    #[test]
    fn limit_columns_follow_the_width_but_never_exceed_the_accounts() {
        assert_eq!(limit_columns(1000., 1., 5), 2);
        assert_eq!(limit_columns(1140., 1., 5), 3);
        assert_eq!(limit_columns(1140., 1., 2), 2);
        assert_eq!(limit_columns(300., 1., 5), 1);
        assert_eq!(limit_columns(1000., 1.3, 5), 2);
        assert_eq!(limit_columns(1000., 1., 0), 1);
    }

    #[test]
    fn prompts_go_side_by_side_from_720_scaled() {
        assert!(side_by_side(720., 1.));
        assert!(!side_by_side(719., 1.));
        assert!(!side_by_side(800., 1.3));
        assert!(side_by_side(936., 1.3));
    }

    #[test]
    fn chunks_fill_rows() {
        let rows = vec![1, 2, 3, 4, 5].chunks_mut_vec(2);
        assert_eq!(rows, vec![vec![1, 2], vec![3, 4], vec![5]]);
    }
}
