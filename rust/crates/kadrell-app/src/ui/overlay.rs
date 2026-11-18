//! Overlay-Rahmen wie `OverlayPanel.swift`: Abdunklung über der Inhaltsfläche des Fensters (ohne Titelleiste),
//! mittiger Dialog mit Breite und Höhe aus der Fläche abzüglich Rand (`OverlayGeometry.margin` = 24), fensterfüllend für
//! Einstellungen und Statistik. Die Fläche (`Area`) setzt die Shell in jedem Frame (`sync_area`), alles Weitere wird bei
//! jedem Zeichnen daraus gerechnet: ein Overlay folgt so jeder Größenänderung sofort, nie ein einmal berechneter Rahmen.
//! Werte stammen aus `app/Kadrell/Theme+Instrument.swift`. Echten Weichzeichner gibt GPUI nicht her, deshalb nur die
//! Abdunklung.

use std::cell::Cell;

use gpui::{
    AbsoluteLength, App, BoxShadow, ClickEvent, Div, Hsla, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, Pixels, SharedString, Size, Styled, Window, div, hsla, point, px, size,
};

use super::chrome::{h_seam, seam_dark, seam_light};
use super::instrument::{self, KeyRole};
use crate::theme::Theme;

/// Dialogradius der Mac-App.
pub const DIALOG_RADIUS: f32 = 14.;
/// Abstand zentrierter Dialoge und der Palette zum Rand der Inhaltsfläche (`OverlayGeometry.margin`).
pub const MARGIN: f32 = 24.;
/// Tiefe des festen Schattens unter erhabenen Flächen (`Instrument.shadowDepth`).
pub const SHADOW_DEPTH: f32 = 3.;

/// Inhaltsfläche des Fensters (ohne eigene Titelleiste), UI-Größe und Gerätemaßstab: Grundlage aller Overlay-Maße.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub width: f32,
    pub height: f32,
    /// Abstand der Fläche zur Fensteroberkante (eigene Titelleiste unter Windows, am Mac 0).
    pub top: f32,
    /// UI-Größe als Faktor (`Theme.scale`).
    pub scale: f32,
    /// Geräte-Pixel je logischem Punkt (`window.scale_factor()`), für `hairline` und `snapped`.
    pub device: f32,
}

impl Default for Area {
    fn default() -> Area {
        Area { width: 1400., height: 868., top: 32., scale: 1., device: 1. }
    }
}

/// Rechteck in Fensterkoordinaten.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl Area {
    /// Höchstbreite eines zentrierten Dialogs: Fläche minus Rand beidseits (`overlayMaxWidth`).
    pub fn max_width(&self) -> f32 {
        (self.width - 2. * MARGIN).max(0.)
    }

    /// Höchsthöhe eines zentrierten Dialogs: Fläche minus Rand oben und unten.
    pub fn max_height(&self) -> f32 {
        (self.height - 2. * MARGIN).max(0.)
    }

    /// Dialogbreite wie `overlayWidth(_:)`: logische Breite mal UI-Größe, nie breiter als die Fläche minus Rand.
    pub fn dialog_width(&self, logical: f32) -> f32 {
        (logical * self.scale).min(self.max_width()).max(0.)
    }

    /// Dialoghöhe: logische Höhe mal UI-Größe, samt Schatten nicht höher als die Fläche minus Rand.
    pub fn dialog_height(&self, logical: f32) -> f32 {
        (logical * self.scale).min(self.max_height() - SHADOW_DEPTH * self.scale).max(0.)
    }

    /// Schmaler Rand des fensterfüllenden Dialogs: ein halbes Prozent der Breite je Seite, mindestens 6 (`inset(of:)`).
    pub fn inset(&self) -> f32 {
        (self.width * 0.005).round().max(6.)
    }

    /// Fensterfüllend: seitlich und unten der Rand, oben bündig unter der Titelleiste (`fullWindow(in:)`).
    pub fn full_window(&self) -> Frame {
        let inset = self.inset();
        Frame { left: inset, top: 0., width: (self.width - 2. * inset).max(0.), height: (self.height - inset).max(0.) }
    }

    /// Weg der Einfahrt in Pixeln: Höhe des Panels samt Luft oben und unten (`maxHeight + 2 * topInset`).
    pub fn slide_travel(&self) -> f32 {
        self.full_window().height + 2. * self.inset()
    }

    /// Palette: volle Größe 640 x 420 plus Schattentiefe, im kleinen Fenster auf Fläche minus Rand verkleinert.
    pub fn palette(&self) -> Frame {
        let s = self.scale;
        let full_w = (640. * s).round();
        let full_h = (420. * s).round() + (SHADOW_DEPTH * s).round();
        let width = full_w.min(self.max_width());
        let height = full_h.min(self.max_height());
        let top = (0.12 * self.height).min(self.height - MARGIN - height).max(MARGIN.min(0.12 * self.height));
        Frame { left: ((self.width - width) / 2.).max(0.), top: top.max(0.), width, height }
    }

    /// Ein Gerätepixel in Punkten (`Instrument.hairline`).
    pub fn hairline(&self) -> f32 {
        1. / self.device.max(0.01)
    }

    /// Rastet eine Position aufs Gerätepixel ein (`Instrument.snapped`).
    pub fn snapped(&self, v: f32) -> f32 {
        (v * self.device).round() / self.device.max(0.01)
    }
}

thread_local! {
    static AREA: Cell<Area> = Cell::new(Area::default());
}

/// Setzt die Fläche, die alle Overlay-Maße dieses Threads (der Oberfläche) lesen.
pub fn set_area(area: Area) {
    AREA.with(|a| a.set(area));
}

pub fn area() -> Area {
    AREA.with(Cell::get)
}

/// UI-Größe der Overlays (`Theme.scale`).
pub fn scale() -> f32 {
    area().scale
}

/// Ein Gerätepixel in Punkten.
pub fn hairline() -> f32 {
    area().hairline()
}

/// Höhe der eigenen Titelleiste: Windows zeichnet sie selbst, der Mac nutzt die des Systems.
pub fn title_bar_height() -> f32 {
    if cfg!(target_os = "windows") { super::menubar::TITLE_HEIGHT } else { 0. }
}

/// Liest Fenstergröße, UI-Größe und Gerätemaßstab und setzt daraus die Fläche. Die Shell ruft das in jedem Frame, bevor
/// die Overlays zeichnen.
pub fn sync_area(window: &Window, cx: &App) {
    super::overlay_queue::note_launch();
    let view = window.viewport_size();
    let top = title_bar_height();
    set_area(Area {
        width: view.width.as_f32(),
        height: (view.height.as_f32() - top).max(0.),
        top,
        scale: crate::appearance::current(cx).scale,
        device: window.scale_factor(),
    });
}

/// Setzt alle vier Rahmenbreiten (GPUI kennt nur ganze Stufen, die Fuge braucht ein Gerätepixel).
pub fn border_px<E: Styled>(mut el: E, width: f32) -> E {
    let w = AbsoluteLength::Pixels(px(width));
    let b = &mut el.style().border_widths;
    b.top = Some(w);
    b.right = Some(w);
    b.bottom = Some(w);
    b.left = Some(w);
    el
}

/// Dialogfläche: die hellste Schemafläche (`Theme.raised`).
pub fn raised(t: &Theme) -> Hsla {
    t.raised()
}

/// Eingelassenes Feld (`Theme.well`): in dunklen Schemata `panel` 72 % zu Schwarz gemischt, sonst `bg`.
pub fn well(t: &Theme) -> Hsla {
    t.well()
}

/// Feste Breite der Dialoge wie `overlayWidth(_:)`: mit der UI-Größe skaliert, nie breiter als die Fläche minus
/// Rand.
pub fn overlay_width(d: Div, width: f32) -> Div {
    d.w(px(area().dialog_width(width)))
}

/// Höhe nach Inhalt, höchstens `height` (skaliert) und samt Schatten nie mehr als die Fläche minus Rand: ein Dialog ist so
/// hoch wie sein Inhalt (`preferredContentSize`), nicht so hoch wie seine Obergrenze.
pub fn overlay_height(d: Div, height: f32) -> Div {
    d.max_h(px(area().dialog_height(height)))
}

fn dim() -> Hsla {
    hsla(0., 0., 0., 0.3)
}

/// Abdunklung über der Inhaltsfläche (unter der Titelleiste), schluckt Maus und Rad und ruft `on_close` bei Klick
/// daneben auf. Der Dialog darin schluckt seine Klicks selbst (`dialog_frame`).
pub fn backdrop(on_close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static) -> Div {
    div()
        .absolute()
        .top(px(area().top))
        .left_0()
        .right_0()
        .bottom_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(dim())
        .occlude()
        .on_mouse_down(MouseButton::Left, on_close)
}

/// Fensterfüllend (Einstellungen, Statistik): die Fläche liegt bündig unter der Titelleiste, seitlich und unten mit
/// `Area::inset` Abstand. Das Kind sollte `full_window_frame` sein.
pub fn backdrop_full(on_close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static) -> Div {
    let a = area();
    let f = a.full_window();
    // Das Panel liegt bündig unter der Titelleiste und schneidet ab, was über seiner Oberkante steht; die Luft oben
    // (`topInset`) liegt als Abstand im Inneren, so kommt die Fläche beim Einfahren unter der Titelleiste hervor.
    let panel = div()
        .absolute()
        .top(px(a.top))
        .left_0()
        .right_0()
        .bottom_0()
        .overflow_hidden()
        .flex()
        .flex_col()
        .pl(px(f.left))
        .pr(px(f.left))
        .pt(px(a.inset()))
        .pb(px(a.inset()))
        .bg(dim());
    if super::overlay_anim::leaving() {
        // Fährt hinaus: keine Klicks mehr im Dialog, sie gehen ans Hauptfenster (`ignoresMouseEvents`).
        return panel;
    }
    panel.occlude().on_mouse_down(MouseButton::Left, on_close)
}

/// Farbe des festen Schattens: Schwarz, in dunklen Schemata 0,5, in hellen 0,16 (`Instrument.dropShadow`).
pub fn shadow_color(t: &Theme) -> Hsla {
    hsla(0., 0., 0., if t.dark { 0.5 } else { 0.16 })
}

fn drop_shadow(t: &Theme) -> BoxShadow {
    BoxShadow {
        color: shadow_color(t),
        offset: point(px(0.), px(SHADOW_DEPTH * scale())),
        blur_radius: px(0.),
        spread_radius: px(0.),
        inset: false,
    }
}

/// Fugenring: außen eine dunkle, direkt innen eine helle Haarlinie (`Instrument.seamRing`), ohne Auswirkung auf das
/// Layout (liegt absolut im Rahmen).
pub fn inner_ring(t: &Theme, radius: f32) -> Div {
    let h = hairline();
    border_px(div().absolute().top_0().left_0().right_0().bottom_0().rounded(px((radius - h).max(0.))), h).border_color(seam_light(t))
}

/// Erhabene Dialogfläche mit festem Schatten nach unten und geschlossenem Fugenring (außen dunkel, innen hell). Bewusst
/// ohne Beschneiden: Fokusringe am Rand bleiben sichtbar. Schrift 12 in `fg` wie `dialogFrame()`.
pub fn dialog_frame(t: &Theme) -> Div {
    let a = area();
    let radius = DIALOG_RADIUS;
    border_px(div(), a.hairline())
        .relative()
        .flex()
        .flex_col()
        .max_h(px((a.max_height() - SHADOW_DEPTH * a.scale).max(0.)))
        .rounded(px(radius))
        .bg(raised(t))
        .border_color(seam_dark(t))
        .shadow(vec![drop_shadow(t)])
        .text_size(px(12. * a.scale))
        .text_color(t.fg)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(inner_ring(t, radius))
}

/// Fensterfüllende Dialogfläche (`Placement.fullWindow`): füllt, was `backdrop_full` ihr lässt.
pub fn full_window_frame(t: &Theme) -> Div {
    dialog_frame(t).flex_1().min_h_0().max_h_full()
}

/// Titelzeile in Versalien (`dialogTitle()`): gesperrt, gedämpft, links, Rand 16, oben 12. Der Text kommt in Versalien an.
pub fn dialog_title(text: impl Into<SharedString>, t: &Theme) -> Div {
    let s = scale();
    div().flex().flex_none().w_full().px(px(16. * s)).pt(px(12. * s)).child(instrument::caps_label(&text.into(), false, t.muted))
}

/// Taste im Instrument-Look in der Rolle Primär (Akzent): Kante als Dicke, darauf die Kappe.
pub fn key_button(label: impl Into<SharedString>, t: &Theme) -> Div {
    instrument::key(label.into(), KeyRole::Primary, false, true, t)
}

/// Zeigt die Haupttaste in der Fußzeile: Beschriftung, zwei Leerzeichen, `⏎`.
pub fn primary_label(button: &str) -> String {
    format!("{button}  ⏎")
}

/// Fußzeile (`DialogFootBar`): Fuge oben, Hinweis links (11, gedämpft), Haupttaste rechts, Abstand 12, Rand 16/10.
/// `button` ist die Beschriftung ohne `⏎`; sie bekommt `  ⏎` angehängt, es sei denn sie enthält es schon.
pub fn dialog_foot(
    hint: impl Into<SharedString>,
    button: impl Into<SharedString>,
    t: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    dialog_foot_with(hint, button, KeyRole::Primary, t, on_click)
}

/// Wie `dialog_foot`, mit Rolle der Haupttaste (destruktive Rückfragen: `Destructive`).
pub fn dialog_foot_with(
    hint: impl Into<SharedString>,
    button: impl Into<SharedString>,
    role: KeyRole,
    t: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let label = foot_label(&button.into());
    foot_bar(hint, instrument::key_button_with("dialog-primary", label, role, false, t, on_click), t)
}

/// Wie `dialog_foot_with`, mit Tastaturfokus: die Haupttaste ist ein Tab-Stopp mit Ring (`kbdFocusRing`).
pub fn dialog_foot_in(
    hint: impl Into<SharedString>,
    button: impl Into<SharedString>,
    role: KeyRole,
    t: &Theme,
    window: &mut Window,
    cx: &mut App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let label = foot_label(&button.into());
    foot_bar(hint, instrument::key_focusable("dialog-primary", label, role, false, t, window, cx, on_click), t)
}

fn foot_bar(hint: impl Into<SharedString>, key: impl IntoElement, t: &Theme) -> Div {
    let s = scale();
    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(h_seam(t))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12. * s))
                .px(px(16. * s))
                .py(px(10. * s))
                .text_size(px(11. * s))
                .text_color(t.muted)
                .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().child(hint.into()))
                .child(key),
        )
}

/// Beschriftung der Haupttaste: `<Taste>  ⏎` mit zwei Leerzeichen. Ein schon angehängtes `⏎` wird auf zwei Leerzeichen
/// gebracht, ein Kürzel wie `⌘⏎` oder `Strg+Enter` (destruktive Rückfrage) bleibt stehen.
pub fn foot_label(button: &str) -> String {
    if button.ends_with("⌘⏎") || button.ends_with("Enter") {
        return button.to_string();
    }
    match button.strip_suffix('⏎') {
        Some(head) => primary_label(head.trim_end()),
        None => primary_label(button),
    }
}

/// Fenstergröße beim Start: `KADRELL_DEV_SIZE=1000x700` für Screenshots, sonst 1400 x 900.
pub fn window_size() -> Size<Pixels> {
    let parsed = std::env::var("KADRELL_DEV_SIZE").ok().and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?))
    });
    let (w, h) = parsed.unwrap_or((1400., 900.));
    size(px(w), px(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(w: f32, h: f32) -> Area {
        Area { width: w, height: h, top: 32., scale: 1., device: 1. }
    }

    /// `testCenteredDialogFollowsResizeAndMove`: die Maße kommen jedes Mal aus der aktuellen Fläche, der Dialog bleibt
    /// mittig (Flex), also genügt: Breite und Höhe ändern sich mit der Fläche, ohne Zustand.
    #[test]
    fn centered_dialog_follows_resize() {
        let big = window(1400., 900.);
        let small = window(900., 600.);
        assert_eq!(big.dialog_width(400.), 400.);
        assert_eq!(small.dialog_width(1000.), 900. - 48.);
        assert!(big.dialog_width(1000.) > small.dialog_width(1000.));
    }

    /// `testCenteredDialogStaysInsideWindow`: zu breit und zu hoch bleibt samt Rand im Fenster und wächst wieder.
    #[test]
    fn centered_dialog_stays_inside_window() {
        let a = window(1400., 900.);
        assert_eq!(a.dialog_width(1000.), 1000.);
        assert!(a.dialog_height(2000.) <= 900. - 2. * MARGIN);
        let small = window(800., 500.);
        assert_eq!(small.dialog_width(1000.), 800. - 2. * MARGIN);
        assert!(small.dialog_height(2000.) + SHADOW_DEPTH <= 500. - 2. * MARGIN + 0.5);
        assert_eq!(window(1600., 900.).dialog_width(1000.), 1000.);
    }

    #[test]
    fn dialog_width_scales_with_ui_size_but_stays_inside() {
        let a = Area { scale: 1.5, ..window(1400., 900.) };
        assert_eq!(a.dialog_width(400.), 600.);
        assert_eq!(a.dialog_width(2000.), 1400. - 48.);
        assert_eq!(window(30., 30.).dialog_width(400.), 0.);
    }

    /// `testFullWindowFillsWindowAndFollowsImmediately`: rund 99 % der Breite, oben bündig, schmaler Rand seitlich/unten.
    #[test]
    fn full_window_fills_window() {
        let a = window(1400., 900.);
        let f = a.full_window();
        assert_eq!(a.inset(), 7.);
        assert!((f.width / a.width - 0.99).abs() < 0.002);
        assert_eq!(f.top, 0.);
        assert_eq!(f.left, 7.);
        assert_eq!(f.height, 900. - 7.);
        let narrow = window(900., 600.);
        assert_eq!(narrow.inset(), 6., "mindestens 6");
        assert_eq!(narrow.full_window().width, 900. - 12.);
    }

    #[test]
    fn slide_travel_covers_the_panel_and_its_gaps() {
        let a = window(1400., 900.);
        assert_eq!(a.slide_travel(), 900. - 7. + 14.);
    }

    /// `testSettingsShrinkWithWindow`: die Fläche schrumpft mit dem Fenster, nie breiter als die Fläche.
    #[test]
    fn full_window_shrinks_with_window() {
        let wide = window(2000., 1100.).full_window();
        let narrow = window(900., 700.).full_window();
        assert!(narrow.width < wide.width);
        assert!(narrow.left + narrow.width <= 900.);
    }

    /// `testPaletteFollowsWindow`: oben bei 12 % der Höhe, mittig, im kleinen Fenster Fläche minus Rand.
    #[test]
    fn palette_follows_window() {
        let a = window(1400., 900.);
        let p = a.palette();
        assert_eq!(p.width, 640.);
        assert_eq!(p.height, 420. + 3.);
        assert!((p.top - 0.12 * 900.).abs() < 0.5);
        assert!((p.left * 2. + p.width - 1400.).abs() < 1.);
        let small = window(500., 400.);
        let q = small.palette();
        assert_eq!(q.width, 500. - 2. * MARGIN);
        assert_eq!(q.height, 400. - 2. * MARGIN);
        assert!(q.top >= MARGIN - 0.5 || q.top + q.height <= 400.);
        assert!(q.top + q.height <= 400. - MARGIN + 0.5);
    }

    #[test]
    fn hairline_is_one_device_pixel() {
        let a = Area { device: 2., ..window(100., 100.) };
        assert_eq!(a.hairline(), 0.5);
        assert_eq!(a.snapped(10.3), 10.5);
        assert_eq!(window(100., 100.).snapped(10.4), 10.);
    }

    #[test]
    fn area_is_per_thread_and_readable() {
        set_area(Area { scale: 2., ..window(500., 300.) });
        assert_eq!(scale(), 2.);
        assert_eq!(area().width, 500.);
    }

    #[test]
    fn foot_label_has_two_spaces_before_return() {
        assert_eq!(foot_label("Starten"), "Starten  ⏎");
        assert_eq!(foot_label("Starten ⏎"), "Starten  ⏎");
        assert_eq!(foot_label("Starten  ⏎"), "Starten  ⏎");
        assert_eq!(foot_label("Löschen  ⌘⏎"), "Löschen  ⌘⏎");
        assert_eq!(foot_label("Löschen  Strg+Enter"), "Löschen  Strg+Enter");
    }

    #[test]
    fn shadow_is_lighter_in_light_schemes() {
        let dark = crate::theme::Theme::mocha();
        let light = crate::theme::Theme::latte();
        assert_eq!(shadow_color(&dark).a, 0.5);
        assert_eq!(shadow_color(&light).a, 0.16);
    }
}
