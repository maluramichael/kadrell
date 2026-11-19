//! SIGTERM, SIGINT und SIGHUP (Unix): die App beendet sich wie bei Quit. Prozesse der Kacheln enden, der Steuerkanal
//! verschwindet, ein Wegwerfprofil räumt sich weg. Ein zweites Signal beendet sofort (`128 + Signal`), falls das
//! saubere Beenden hängt.
//!
//! Der Handler schreibt nur ein Byte in eine Pipe (das ist signalsicher); ein Thread liest sie und reicht das Signal
//! an die Oberfläche weiter. Signale werden gefangen, nicht blockiert: so erben die Kindprozesse (Claude, Shells)
//! weder eine Signalmaske noch den Handler, und beide Pipe-Enden haben `FD_CLOEXEC`.

use gpui::App;

/// Signale, auf die die App wie auf Quit reagiert.
#[cfg(unix)]
pub const QUIT_SIGNALS: [libc::c_int; 3] = [libc::SIGTERM, libc::SIGINT, libc::SIGHUP];

/// Empfänger der gefangenen Signale (Nummern). Unter Windows gibt es keinen.
pub type Signals = Option<async_channel::Receiver<i32>>;

/// Fängt die Quit-Signale. Früh aufrufen, bevor das Fenster steht: ein Signal beim Start ist dann nicht tödlich,
/// sondern wartet, bis die Oberfläche läuft.
#[cfg(unix)]
pub fn install_quit_signals() -> Signals {
    match unix::install(&QUIT_SIGNALS) {
        Ok(rx) => Some(rx),
        Err(e) => {
            eprintln!("kadrell: Signale nicht abgefangen: {e}");
            None
        }
    }
}

#[cfg(not(unix))]
pub fn install_quit_signals() -> Signals {
    None
}

/// Jedes Signal läuft wie Quit über die Rückfrage (`quit::on_signal`): mit laufenden Prozessen fragt die App, ein
/// Signal mitten im bestätigten Beenden beendet sofort (`128 + Signal`), falls das saubere Beenden hängt.
pub fn quit_on(cx: &mut App, signals: Signals) {
    let Some(rx) = signals else { return };
    cx.spawn(async move |cx| {
        while let Ok(sig) = rx.recv().await {
            cx.update(|cx| crate::quit::on_signal(cx, sig));
        }
    })
    .detach();
}

#[cfg(unix)]
pub(crate) mod unix {
    use std::io;
    use std::sync::atomic::{AtomicI32, Ordering};

    /// Schreibende Pipe-Seite für den Handler (-1: nicht installiert).
    static WRITE_FD: AtomicI32 = AtomicI32::new(-1);

    extern "C" fn on_signal(sig: libc::c_int) {
        let fd = WRITE_FD.load(Ordering::Relaxed);
        if fd >= 0 {
            let byte = sig as u8;
            // SAFETY: `write` ist signalsicher, `byte` lebt auf dem Stack. Ein volles oder geschlossenes Rohr ist egal.
            unsafe { libc::write(fd, (&raw const byte).cast(), 1) };
        }
    }

    /// Pipe mit `FD_CLOEXEC` auf beiden Enden, das schreibende nicht blockierend (der Handler darf nie warten).
    fn pipe() -> io::Result<(i32, i32)> {
        let mut fds = [0; 2];
        // SAFETY: `fds` hat Platz für zwei Deskriptoren.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        for (fd, nonblocking) in [(fds[0], false), (fds[1], true)] {
            // SAFETY: gültige, eben geöffnete Deskriptoren.
            unsafe {
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                if nonblocking {
                    let flags = libc::fcntl(fd, libc::F_GETFL);
                    libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
            }
        }
        Ok((fds[0], fds[1]))
    }

    fn catch(sig: libc::c_int) -> io::Result<()> {
        // SAFETY: `sigaction` mit vollständig initialisierter Struktur und einem Handler, der nur `write` ruft.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as usize;
            action.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            if libc::sigaction(sig, &action, std::ptr::null_mut()) != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    /// Liest die Pipe und reicht jedes Signal weiter, bis der Empfänger weg ist.
    fn forward(read_fd: i32, tx: async_channel::Sender<i32>) {
        let mut byte = 0u8;
        loop {
            // SAFETY: `byte` ist ein gültiger Puffer für ein Byte.
            let n = unsafe { libc::read(read_fd, (&raw mut byte).cast(), 1) };
            match n {
                1 if tx.send_blocking(i32::from(byte)).is_err() => return,
                1 => {}
                0 => return,
                _ if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted => {}
                _ => return,
            }
        }
    }

    /// Fängt `signals` und liefert ihre Nummern. Nur einmal je Prozess sinnvoll (der Handler ist global).
    pub fn install(signals: &[libc::c_int]) -> io::Result<async_channel::Receiver<i32>> {
        let (read_fd, write_fd) = pipe()?;
        WRITE_FD.store(write_fd, Ordering::Relaxed);
        let (tx, rx) = async_channel::unbounded();
        std::thread::Builder::new().name("kadrell-signals".into()).spawn(move || forward(read_fd, tx))?;
        signals.iter().try_for_each(|s| catch(*s))?;
        Ok(rx)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn fd_is_cloexec(fd: i32) -> bool {
            // SAFETY: `F_GETFD` liest nur Flags.
            unsafe { libc::fcntl(fd, libc::F_GETFD) & libc::FD_CLOEXEC != 0 }
        }

        /// Ein Test für alles, weil der Handler prozessweit gilt. Benutzt SIGUSR1, nicht die Quit-Signale.
        #[test]
        fn caught_signals_arrive_in_order_and_pipe_does_not_leak_into_children() {
            let rx = install(&[libc::SIGUSR1]).expect("Handler");
            let write_fd = WRITE_FD.load(Ordering::Relaxed);
            assert!(fd_is_cloexec(write_fd), "schreibendes Ende erbt ein Kindprozess");
            // SAFETY: eigener Prozess, SIGUSR1 ist gefangen.
            unsafe { libc::raise(libc::SIGUSR1) };
            assert_eq!(rx.recv_blocking().unwrap(), libc::SIGUSR1);
            for _ in 0..3 {
                // SAFETY: wie oben.
                unsafe { libc::raise(libc::SIGUSR1) };
            }
            for _ in 0..3 {
                assert_eq!(rx.recv_blocking().unwrap(), libc::SIGUSR1);
            }
        }

        #[test]
        fn quit_signals_are_the_three_expected() {
            assert_eq!(super::super::QUIT_SIGNALS, [libc::SIGTERM, libc::SIGINT, libc::SIGHUP]);
        }
    }
}
