//! Lage und Größe der Fenster (`MainWindowController.init`): Fenster 0 hat die Autosave-Lage `KadrellMain`, weitere
//! `KadrellMain.N`. GPUI hat kein Frame-Autosave, deshalb steht der Rahmen unter `NSWindow Frame KadrellMain[.N]` in den
//! Einstellungen (vier Zahlen x, y, Breite, Höhe). Ohne gespeicherte Lage ist ein Fenster mittig, ein weiteres wird
//! gegen das letzte versetzt (`cascadeTopLeft`). Alles rein rechnerisch, ohne GPUI testbar; `watch` speichert die Lage
//! eines offenen Fensters (verzögert, denn beim Ziehen kommen viele Änderungen).

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui::{AppContext as _, Bounds, Context, Pixels, Window, WindowBounds, point, px, size};
use serde_json::{Value, json};

use super::model::suffix;
use crate::state::attention::patch_setting;

/// Anfangsgröße des Fensters und kleinste erlaubte Größe (`contentRect`, `minSize`).
pub const DEFAULT_SIZE: (f32, f32) = (1400., 900.);
pub const MIN_SIZE: (f32, f32) = (800., 500.);
/// Versatz eines neuen Fensters gegen das letzte (`cascadeTopLeft`).
pub const CASCADE_STEP: f32 = 25.;
/// So lange muss die Lage ruhig bleiben, bevor sie gespeichert wird.
const SAVE_DELAY: Duration = Duration::from_millis(400);

/// Rechteck in Pixeln des Bildschirms: Ursprung oben links.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Frame {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Frame {
        Frame { x, y, w, h }
    }

    fn centered_in(display: Frame, w: f32, h: f32) -> Frame {
        Frame::new(display.x + (display.w - w) / 2., display.y + (display.h - h) / 2., w, h)
    }
}

/// Name der Lage eines Fensters (`frameName`).
pub fn frame_name(index: usize) -> String {
    format!("KadrellMain{}", suffix(index))
}

/// Schlüssel in den Einstellungen.
pub fn frame_key(index: usize) -> String {
    format!("NSWindow Frame {}", frame_name(index))
}

pub fn encode(frame: Frame) -> Value {
    json!([frame.x, frame.y, frame.w, frame.h])
}

/// Gespeicherte Lage; Unbrauchbares (falsche Form, keine Zahlen, kleiner als das Minimum) ergibt `None`.
pub fn decode(value: Option<&Value>) -> Option<Frame> {
    let list = value?.as_array()?;
    let n: Vec<f32> = list.iter().map(|v| v.as_f64().map(|f| f as f32)).collect::<Option<_>>()?;
    let [x, y, w, h] = n.as_slice() else { return None };
    (x.is_finite() && y.is_finite() && *w >= MIN_SIZE.0 && *h >= MIN_SIZE.1).then(|| Frame::new(*x, *y, *w, *h))
}

/// Größe auf den Bildschirm begrenzt, nie unter dem Minimum.
fn fit_size(w: f32, h: f32, display: Frame) -> (f32, f32) {
    (w.min(display.w).max(MIN_SIZE.0.min(display.w)), h.min(display.h).max(MIN_SIZE.1.min(display.h)))
}

/// Rahmen ganz auf den Bildschirm schieben (ein abgesteckter Zweitbildschirm darf kein unerreichbares Fenster hinterlassen).
pub fn fit(frame: Frame, display: Frame) -> Frame {
    let (w, h) = fit_size(frame.w, frame.h, display);
    let x = frame.x.clamp(display.x, (display.x + display.w - w).max(display.x));
    let y = frame.y.clamp(display.y, (display.y + display.h - h).max(display.y));
    Frame::new(x, y, w, h)
}

/// Der Bildschirm, auf dem der Rahmen am meisten liegt; ohne Überschneidung der erste.
pub fn display_for(frame: Frame, displays: &[Frame]) -> Option<Frame> {
    let overlap = |d: &Frame| {
        let w = (frame.x + frame.w).min(d.x + d.w) - frame.x.max(d.x);
        let h = (frame.y + frame.h).min(d.y + d.h) - frame.y.max(d.y);
        if w > 0. && h > 0. { w * h } else { 0. }
    };
    displays.iter().copied().max_by(|a, b| overlap(a).total_cmp(&overlap(b))).or_else(|| displays.first().copied())
}

/// Lage beim Öffnen. Fenster 0: gespeicherte Größe (sonst 1400x900), mittig. Weitere: die gespeicherte Lage, sonst
/// mittig und gegen `cascade` (das zuletzt geöffnete Fenster) versetzt.
pub fn initial(index: usize, saved: Option<Frame>, cascade: Option<Frame>, displays: &[Frame]) -> Option<Frame> {
    let primary = *displays.first()?;
    let (w, h) = saved.map_or(DEFAULT_SIZE, |f| (f.w, f.h));
    if index == 0 {
        let (w, h) = fit_size(w, h, primary);
        return Some(Frame::centered_in(primary, w, h));
    }
    if let Some(f) = saved {
        return Some(fit(f, display_for(f, displays).unwrap_or(primary)));
    }
    let (w, h) = fit_size(w, h, primary);
    let mut frame = Frame::centered_in(primary, w, h);
    if let Some(from) = cascade {
        frame.x = from.x + CASCADE_STEP;
        frame.y = from.y + CASCADE_STEP;
        frame = fit(frame, display_for(from, displays).unwrap_or(primary));
    }
    Some(frame)
}

fn to_frame(b: Bounds<Pixels>) -> Frame {
    Frame::new(b.origin.x.as_f32(), b.origin.y.as_f32(), b.size.width.as_f32(), b.size.height.as_f32())
}

fn to_bounds(f: Frame) -> Bounds<Pixels> {
    Bounds::new(point(px(f.x), px(f.y)), size(px(f.w), px(f.h)))
}

/// Die Bildschirme in Pixeln, der Hauptbildschirm zuerst.
fn displays(cx: &gpui::App) -> Vec<Frame> {
    let mut all: Vec<Frame> = Vec::new();
    if let Some(d) = cx.primary_display() {
        all.push(to_frame(d.visible_bounds()));
    }
    for d in cx.displays() {
        let f = to_frame(d.visible_bounds());
        if !all.contains(&f) {
            all.push(f);
        }
    }
    all
}

/// Fensterrahmen für Fenster `index`: aus den Einstellungen, mittig oder versetzt; `None` ohne Bildschirm (dann gilt
/// die Wahl von GPUI). `KADRELL_DEV_SIZE` (`ui::overlay::window_size`) bleibt für Screenshots gültig, solange nichts
/// gespeichert ist.
pub fn bounds_for(index: usize, saved: Option<&Value>, cascade: Option<Frame>, cx: &gpui::App) -> Option<Bounds<Pixels>> {
    let saved = decode(saved);
    let displays = displays(cx);
    let frame = match (saved, std::env::var_os("KADRELL_DEV_SIZE")) {
        (None, Some(_)) => {
            let s = crate::ui::overlay::window_size();
            let primary = *displays.first()?;
            let (w, h) = fit_size(s.width.as_f32(), s.height.as_f32(), primary);
            Frame::centered_in(primary, w, h)
        }
        _ => initial(index, saved, cascade, &displays)?,
    };
    Some(to_bounds(frame))
}

/// Rahmen eines offenen Fensters, zum Versetzen des nächsten.
pub fn frame_of(window: &Window) -> Frame {
    to_frame(window.bounds())
}

/// Rahmen des Hauptfensters beim Start; ohne Bildschirmangaben mittig auf dem Hauptbildschirm (Wahl von GPUI).
pub fn main_bounds(settings: &kadrell_core::Settings, cx: &gpui::App) -> Bounds<Pixels> {
    bounds_for(0, settings.get(&frame_key(0)), None, cx).unwrap_or_else(|| Bounds::centered(None, crate::ui::overlay::window_size(), cx))
}

/// Rahmen des Fensters, gegen das ein neues versetzt aufgeht: das zuletzt geöffnete weitere Fenster, sonst das Hauptfenster.
pub fn cascade_source(shell: &gpui::Entity<crate::Shell>, cx: &mut gpui::App) -> Option<Frame> {
    let (last, main) = {
        let sh = shell.read(cx);
        (sh.extras.last().map(|(_, handle)| *handle), sh.main_window)
    };
    cx.update_window(last.or(main)?, |_, window, _| frame_of(window)).ok()
}

/// Speichert die Lage des Fensters bei jeder Änderung (verzögert, damit das Ziehen nicht jede Bewegung schreibt).
/// Maximiert und Vollbild ändern die gespeicherte Lage nicht: danach kommt das Fenster in der Größe davor wieder.
pub fn watch<V: 'static>(window: &mut Window, cx: &mut Context<V>, settings_path: PathBuf, index: usize) {
    let generation = Rc::new(Cell::new(0u64));
    cx.observe_window_bounds(window, move |_, window, cx| {
        let WindowBounds::Windowed(bounds) = window.window_bounds() else { return };
        let frame = to_frame(bounds);
        generation.set(generation.get() + 1);
        let (mine, current) = (generation.get(), generation.clone());
        let path = settings_path.clone();
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            if current.get() == mine {
                patch_setting(&path, &frame_key(index), encode(frame));
            }
        })
        .detach();
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Frame = Frame { x: 0., y: 0., w: 1920., h: 1080. };

    #[test]
    fn names_and_keys_follow_the_mac_app() {
        assert_eq!(frame_name(0), "KadrellMain");
        assert_eq!(frame_name(1), "KadrellMain.2");
        assert_eq!(frame_key(2), "NSWindow Frame KadrellMain.3");
    }

    #[test]
    fn frames_round_trip_and_reject_garbage() {
        let f = Frame::new(10., 20., 1000., 700.);
        assert_eq!(decode(Some(&encode(f))), Some(f));
        assert_eq!(decode(None), None);
        assert_eq!(decode(Some(&json!("x"))), None);
        assert_eq!(decode(Some(&json!([1, 2, 3]))), None);
        assert_eq!(decode(Some(&json!([1, 2, 100, 100]))), None, "kleiner als das Minimum");
        assert_eq!(decode(Some(&json!([1, "a", 1000, 700]))), None);
    }

    #[test]
    fn window_zero_is_centered_with_the_saved_size() {
        let f = initial(0, None, None, &[D]).unwrap();
        assert_eq!(f, Frame::new(260., 90., 1400., 900.));
        let saved = Frame::new(5., 5., 1000., 600.);
        assert_eq!(initial(0, Some(saved), None, &[D]).unwrap(), Frame::new(460., 240., 1000., 600.), "Größe gespeichert, Lage mittig");
    }

    #[test]
    fn size_is_limited_to_the_display() {
        let small = Frame::new(0., 0., 1280., 800.);
        let f = initial(0, None, None, &[small]).unwrap();
        assert_eq!((f.w, f.h), (1280., 800.));
        assert_eq!((f.x, f.y), (0., 0.));
    }

    #[test]
    fn extra_window_without_a_saved_frame_cascades() {
        let main = Frame::new(260., 90., 1400., 900.);
        let f = initial(1, None, Some(main), &[D]).unwrap();
        assert_eq!((f.x, f.y), (285., 115.));
        assert_eq!((f.w, f.h), (1400., 900.));
        let centered = initial(1, None, None, &[D]).unwrap();
        assert_eq!((centered.x, centered.y), (260., 90.), "ohne Fenster zum Versetzen mittig");
    }

    #[test]
    fn cascade_stays_on_the_display() {
        let main = Frame::new(500., 180., 1400., 900.);
        let f = initial(1, None, Some(main), &[D]).unwrap();
        assert_eq!((f.x, f.y), (520., 180.), "rechts und unten bleibt es auf dem Bildschirm");
    }

    #[test]
    fn extra_window_uses_its_saved_frame_and_moves_back_onto_a_display() {
        let saved = Frame::new(100., 100., 1000., 700.);
        assert_eq!(initial(1, Some(saved), None, &[D]).unwrap(), saved);
        let gone = Frame::new(4000., 100., 1000., 700.);
        assert_eq!(initial(1, Some(gone), None, &[D]).unwrap(), Frame::new(920., 100., 1000., 700.));
    }

    #[test]
    fn display_with_the_most_overlap_wins() {
        let right = Frame::new(1920., 0., 1920., 1080.);
        let f = Frame::new(1800., 0., 1000., 700.);
        assert_eq!(display_for(f, &[D, right]), Some(right));
        assert_eq!(display_for(f, &[]), None);
    }

    #[test]
    fn no_display_means_no_frame() {
        assert_eq!(initial(0, None, None, &[]), None);
    }
}
