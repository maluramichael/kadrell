//! Rauchtest: zeigt das Icon 5 s (Wartend-Punkt, danach Arbeitend), schickt eine Benachrichtigung und ein Badge,
//! räumt dann alles wieder weg. Ohne Eingaben; Klicks (falls jemand klickt) werden nur geloggt.
//! Aufruf: `cargo run -p kadrell-tray --example smoke`

use std::time::{Duration, Instant};

use kadrell_tray::model::{build_menu, notification};
use kadrell_tray::{Lang, MenuSession, StatusSummary, Tray, TrayConfig};

struct Stdout;

impl log::Log for Stdout {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        println!("log {}: {}", r.level(), r.args());
    }
    fn flush(&self) {}
}

fn main() {
    let _ = log::set_logger(&Stdout).map(|()| log::set_max_level(log::LevelFilter::Info));
    let tray = Tray::new(TrayConfig { app_id: "de.malura.kadrell.smoke".into(), display_name: "Kadrell Smoke".into(), ..Default::default() });
    let session = MenuSession { key: "s1".into(), group: "Demo".into(), title: "Rauchtest".into(), elapsed: "1 min".into() };
    tray.set_menu(build_menu(&[session], &[], Lang::De));
    tray.set_status(StatusSummary { waiting: 2, working: 1, done: 1, ..Default::default() });
    println!("icon: waiting 2, working 1, done 1");
    tray.set_badge(3);
    tray.notify(notification("s1", "Demo", "Rauchtest", None, true, Lang::De));
    println!("notify: Demo › Rauchtest, wartet auf dich");
    kadrell_tray::play(kadrell_tray::SoundLevel::All, kadrell_tray::Sound::Waiting);
    println!("reduce_motion: {}", kadrell_tray::reduce_motion());

    let start = Instant::now();
    let mut switched = false;
    while start.elapsed() < Duration::from_secs(5) {
        if !switched && start.elapsed() > Duration::from_millis(2500) {
            tray.set_status(StatusSummary { working: 1, ..Default::default() });
            println!("icon: working 1");
            switched = true;
        }
        while let Ok(e) = tray.events().try_recv() {
            println!("event: {e:?}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    drop(tray);
    println!("icon entfernt");
}
