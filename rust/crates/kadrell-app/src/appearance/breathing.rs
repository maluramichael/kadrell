//! Atmender Arbeitspunkt (`UI/BreathingDots.swift`): ein Punkt in der Arbeitsfarbe, der langsam atmet (Größe, Deckkraft
//! und ein weicher Schein, Zyklus 2,8 s, sanft ein- und ausschwingend). Die Phase hängt an der Uhr, nicht am Erscheinen,
//! damit viele arbeitende Sessions als ruhige Fläche wirken statt als Flackern. Bei „Bewegung reduzieren“ steht der Punkt
//! still, voll und mit festem Schein, die Form unterscheidet ihn weiter vom Statuspunkt.
//!
//! Dazu der Randwechsel der Kachel: der Rahmen blendet bei Fokuswechsel in 120 ms über statt zu springen
//! (`CellView.viewWillDraw`).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{Bounds, Hsla, IntoElement, Pixels, Styled, Window, canvas, fill, point, px, size};

/// Ein Atemzug: einatmen und ausatmen, je die Hälfte (Sekunden).
pub const PERIOD: f64 = 2.8;
/// Weicher Schein um den Punkt, in logischen Punkten vor der UI-Größe.
pub const GLOW_RADIUS: f32 = 3.;
/// Dauer des Rahmenwechsels einer Kachel.
pub const BORDER_FADE: Duration = Duration::from_millis(120);

/// Zustand eines Punkts: Größe (1 = wie der Statuspunkt), Deckkraft und Stärke des Scheins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub scale: f32,
    pub opacity: f32,
    pub glow: f32,
}

impl Pose {
    /// Ausgeatmet, der ruhigste Punkt des Zyklus.
    pub const EXHALED: Pose = Pose { scale: 0.86, opacity: 0.75, glow: 0.1 };
    /// Eingeatmet: volle Größe wie der Statuspunkt, Schein am stärksten.
    pub const INHALED: Pose = Pose { scale: 1., opacity: 1., glow: 0.75 };
    /// „Bewegung reduzieren“: steht still, voll, mit festem Schein.
    pub const STILL: Pose = Pose { scale: 1., opacity: 1., glow: 0.55 };
}

/// Zustand zum Zeitpunkt `t` (Sekunden), wie die Animation ihn zeigt: sanft ein- und ausschwingend
/// (`ease-in-ease-out` angenähert als Smoothstep), bei `t = 0` eingeatmet.
pub fn pose_at(t: f64) -> Pose {
    let phase = t.rem_euclid(PERIOD) / PERIOD;
    let half = if phase < 0.5 { phase * 2. } else { 2. - phase * 2. };
    let p = (half * half * (3. - 2. * half)) as f32;
    let (a, b) = (Pose::INHALED, Pose::EXHALED);
    Pose { scale: a.scale + (b.scale - a.scale) * p, opacity: a.opacity + (b.opacity - a.opacity) * p, glow: a.glow + (b.glow - a.glow) * p }
}

/// Pose für die Uhrzeit `t` (Sekunden seit der Epoche) bei gegebener Einstellung „Bewegung reduzieren“.
pub fn pose_for(t: f64, reduce_motion: bool) -> Pose {
    if reduce_motion { Pose::STILL } else { pose_at(t) }
}

fn wall_clock() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0., |d| d.as_secs_f64())
}

/// Wert zwischen `from` und `to` nach `elapsed` von `BORDER_FADE` (linear, danach am Ziel).
pub fn fade(from: Hsla, to: Hsla, elapsed: Duration) -> Hsla {
    let t = (elapsed.as_secs_f32() / BORDER_FADE.as_secs_f32()).clamp(0., 1.);
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    // Farbton über den kurzen Weg ist hier nicht nötig: Rahmenfarben sind aus Schema und Akzent, nah beieinander,
    // gemischt wird im RGB-Raum über Hsla::blend-freie Komponenten.
    let (fr, tr) = (gpui::Rgba::from(from), gpui::Rgba::from(to));
    gpui::Rgba { r: lerp(fr.r, tr.r), g: lerp(fr.g, tr.g), b: lerp(fr.b, tr.b), a: lerp(fr.a, tr.a) }.into()
}

/// Ist „Bewegung reduzieren“ an? Windows: Animationen in den Systemeinstellungen aus. Mit `KADRELL_REDUCE_MOTION=1`
/// bzw. `0` überschreibbar (Tests, Screenshots). Das Ergebnis gilt zwei Sekunden, die Abfrage ist ein Systemaufruf.
pub fn reduce_motion() -> bool {
    use std::sync::Mutex;
    static CACHE: Mutex<Option<(std::time::Instant, bool)>> = Mutex::new(None);
    if let Ok(v) = std::env::var("KADRELL_REDUCE_MOTION") {
        return v == "1";
    }
    let mut cache = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    match *cache {
        Some((at, v)) if at.elapsed() < Duration::from_secs(2) => v,
        _ => {
            let v = system_reduce_motion();
            *cache = Some((std::time::Instant::now(), v));
            v
        }
    }
}

#[cfg(windows)]
fn system_reduce_motion() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::SystemParametersInfoW;
    const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;
    let mut on: i32 = 1;
    // SAFETY: `on` ist ein gültiger BOOL-Zeiger für diese Abfrage, `uiParam` ist 0.
    let ok = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut on).cast(), 0) };
    ok != 0 && on == 0
}

#[cfg(not(windows))]
fn system_reduce_motion() -> bool {
    false
}

/// Der atmende Punkt als Element: `diameter` und `glow` schon mit der UI-Größe multipliziert. Er zeichnet sich selbst
/// und bittet bei jedem Bild um das nächste, solange er sichtbar ist; es braucht kein Neuzeichnen der Ansicht.
pub fn breathing_dot(color: Hsla, diameter: f32, glow: f32) -> impl IntoElement {
    let extra = glow.ceil().max(0.);
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let still = reduce_motion();
            if !still {
                window.request_animation_frame();
            }
            paint_dot(window, bounds, color, diameter, glow, pose_for(wall_clock(), still));
        },
    )
    // Der Schein ragt über den Punkt hinaus: Platz dafür gehört zum Element, die Mitte bleibt die des Statuspunkts.
    .size(px(diameter + 2. * extra))
    .m(px(-extra))
}

fn paint_dot(window: &mut Window, bounds: Bounds<Pixels>, color: Hsla, diameter: f32, glow: f32, pose: Pose) {
    let c = bounds.center();
    let ring = |r: f32, alpha: f32| {
        let b = Bounds::new(point(c.x - px(r), c.y - px(r)), size(px(2. * r), px(2. * r)));
        fill(b, Hsla { a: color.a * alpha, ..color }).corner_radii(px(r))
    };
    let r = diameter / 2. * pose.scale;
    // Schein in drei Stufen nach außen, schwächer je weiter weg (Näherung des weichen Schattens).
    for (i, share) in [1.0f32, 0.6, 0.3].into_iter().enumerate() {
        let spread = glow * pose.scale * (3 - i) as f32 / 3.;
        window.paint_quad(ring(r + spread, pose.glow * 0.35 * share * pose.opacity));
    }
    window.paint_quad(ring(r, pose.opacity));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `testBreathesSlowlyAndInPhase`: ruhig, mindestens 2,5 s je Atemzug.
    #[test]
    fn period_is_calm() {
        const { assert!(PERIOD >= 2.5 && PERIOD <= 3.) };
    }

    /// `testBreathIsShallow`: Deckkraft bleibt hoch, der Punkt wird nie viel kleiner als der Statuspunkt.
    #[test]
    fn breath_is_shallow() {
        const { assert!(Pose::EXHALED.opacity >= 0.7 && Pose::EXHALED.scale >= 0.8) };
        assert_eq!(Pose::INHALED.scale, 1.);
        assert_eq!(pose_at(0.), Pose::INHALED);
        let mid = pose_at(PERIOD / 2.);
        assert!((mid.scale - Pose::EXHALED.scale).abs() < 1e-3);
        assert!((mid.opacity - Pose::EXHALED.opacity).abs() < 1e-3);
        assert!((mid.glow - Pose::EXHALED.glow).abs() < 1e-3);
    }

    /// Alle Punkte atmen im selben Takt: die Phase hängt nur an der Uhr, ein Vielfaches der Periode ist eingeatmet.
    #[test]
    fn phase_follows_the_clock() {
        for k in [0., 1., 7., 1_000_000.] {
            let p = pose_at(k * PERIOD);
            assert!((p.scale - 1.).abs() < 1e-4, "bei {k} Perioden: {p:?}");
        }
        let (a, b) = (pose_at(1.234), pose_at(1.234 + 5. * PERIOD));
        assert!((a.opacity - b.opacity).abs() < 1e-4);
    }

    #[test]
    fn pose_swings_smoothly_and_symmetrically() {
        let quarter = pose_at(PERIOD / 4.);
        assert!(quarter.scale < 1. && quarter.scale > Pose::EXHALED.scale);
        let back = pose_at(PERIOD * 3. / 4.);
        assert!((quarter.scale - back.scale).abs() < 1e-4, "aus- und einatmen spiegeln sich");
        // sanft an den Enden: kurz nach dem Beginn kaum Bewegung
        assert!(pose_at(0.01).scale > 0.999);
    }

    /// `testReduceMotionStandsStillWithGlow`: still, voll, der Schein bleibt.
    #[test]
    fn reduce_motion_stands_still_with_glow() {
        for t in [0., 0.7, 1.4, 2.1] {
            let p = pose_for(t, true);
            assert_eq!(p, Pose::STILL);
            assert_eq!(p.opacity, 1.);
            assert!(p.glow > 0.3, "Schein bleibt: die Form unterscheidet ihn vom Statuspunkt");
        }
        assert_ne!(pose_for(1.4, false), Pose::STILL);
    }

    #[test]
    fn border_fade_runs_120ms() {
        assert_eq!(BORDER_FADE, Duration::from_millis(120));
        let (a, b) = (gpui::rgb(0x000000).into(), gpui::rgb(0xffffff).into());
        assert_eq!(gpui::Rgba::from(fade(a, b, Duration::ZERO)).r, 0.);
        let half = gpui::Rgba::from(fade(a, b, Duration::from_millis(60))).r;
        assert!((half - 0.5).abs() < 1e-3);
        assert_eq!(gpui::Rgba::from(fade(a, b, Duration::from_secs(1))).r, 1.);
    }
}
