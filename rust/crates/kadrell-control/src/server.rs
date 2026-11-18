//! Der Server des Steuerkanals in der laufenden App: lauscht, liest je Verbindung eine Zeile JSON, reicht sie samt
//! pid des Clients an den [`Service`] und schreibt die Antwort zurück. Kein eigener Daemon: der Server ist ein
//! Thread im App-Prozess.
//!
//! - Unix (macOS, Linux): Unix-Socket im Profilordner mit Rechten 0600. Nur der eigene Benutzer kommt durch
//!   (Dateirechte plus `getpeereid` bzw. `SO_PEERCRED`), die pid des Clients kommt von `LOCAL_PEERPID` (macOS)
//!   bzw. `SO_PEERCRED` (Linux).
//! - Windows: TCP auf 127.0.0.1 mit zufälligem Port und Token in der Endpunktdatei. Die pid des Clients folgt aus
//!   der TCP-Tabelle des Systems (`GetExtendedTcpTable`): der Besitzer der Verbindung vom Client-Port zum
//!   Server-Port. Ohne passendes Token oder bekannte pid antwortet der Server nicht.
//!
//! Der Dienst läuft auf dem Verbindungs-Thread, darf also blockieren. Wer den Zustand auf einem anderen Thread
//! hält (GPUI), wechselt im Dienst dorthin und wartet auf das Ergebnis; [`crate::exec::Controller`] löst das für
//! einen geteilten Handler hinter einem Mutex.

use crate::protocol::{ControlRequest, ControlResponse, read_line, write_line};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

/// Beantwortet eine Anfrage; das zweite Argument ist die pid des Clients, `None` wenn sie unbekannt ist.
pub type Service = Arc<dyn Fn(ControlRequest, Option<u32>) -> ControlResponse + Send + Sync + 'static>;

/// Der Thread der Annahmeschleife und eine Funktion, die ihn aus `accept` aufweckt.
type Listening = (JoinHandle<()>, Box<dyn Fn() + Send + Sync>);

/// So lange darf ein Client brauchen, bis seine Zeile da ist. Er schickt sie sofort.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Der laufende Server. Beim Fallenlassen oder mit [`ControlServer::stop`] endet er und räumt auf.
pub struct ControlServer {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl ControlServer {
    /// Startet den Server. `path` ist der Socket (Unix) bzw. die Endpunktdatei (Windows); ein Rest eines
    /// abgestürzten Laufs wird ersetzt (eine zweite Instanz hält der Profil-Lock vorher fern).
    pub fn start(path: &Path, service: Service) -> io::Result<ControlServer> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (thread, wake) = transport::listen(path, service, stop.clone())?;
        Ok(ControlServer { path: path.to_path_buf(), stop, thread: Some(thread), wake })
    }

    /// Socket bzw. Endpunktdatei.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Beendet den Server und entfernt Socket bzw. Endpunktdatei. Laufende Antworten werden noch zu Ende geschrieben.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        (self.wake)();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Eine Verbindung: Zeile lesen, prüfen, Dienst rufen, antworten. `token` nur auf Windows.
fn serve_connection<S: Read + Write>(mut stream: S, peer_pid: u32, token: Option<&str>, service: &Service) {
    let Some(line) = read_line(&mut stream) else { return };
    let response = match serde_json::from_slice::<ControlRequest>(&line) {
        Ok(req) if token.is_some_and(|t| t != req.token) => return,
        Ok(req) => service(req, Some(peer_pid)),
        Err(_) => ControlResponse::fail("ungültige Anfrage"),
    };
    let _ = write_line(&mut stream, &response);
}

#[cfg(unix)]
mod transport {
    use super::*;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};

    pub fn listen(path: &Path, service: Service, stop: Arc<AtomicBool>) -> io::Result<Listening> {
        let _ = std::fs::remove_file(path);
        // Der Socket entsteht nie mit weiteren Rechten als 0600: umask um das Binden, danach chmod zur Sicherheit.
        // SAFETY: `umask` ist immer aufrufbar; die Maske wird sofort zurückgesetzt.
        let old = unsafe { libc::umask(0o077) };
        let bound = UnixListener::bind(path);
        unsafe { libc::umask(old) };
        let listener = bound.map_err(|e| io::Error::new(e.kind(), format!("bind {}: {e}", path.display())))?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let wake_path = path.to_path_buf();
        let thread = std::thread::spawn(move || accept_loop(&listener, &service, &stop));
        let wake = Box::new(move || {
            let _ = UnixStream::connect(&wake_path);
        });
        Ok((thread, wake))
    }

    fn accept_loop(listener: &UnixListener, service: &Service, stop: &AtomicBool) {
        for stream in listener.incoming() {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = stream else { continue };
            let service = service.clone();
            std::thread::spawn(move || answer(stream, &service));
        }
    }

    fn answer(stream: UnixStream, service: &Service) {
        let Some(peer) = peer_credentials(stream.as_raw_fd()) else { return };
        // SAFETY: `geteuid` hat keine Voraussetzungen.
        if peer.uid != unsafe { libc::geteuid() } {
            return;
        }
        let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
        serve_connection(&stream, peer.pid, None, service);
    }

    /// Benutzer und Prozess am anderen Ende der Verbindung.
    pub struct Peer {
        pub uid: libc::uid_t,
        pub pid: u32,
    }

    #[cfg(target_os = "macos")]
    pub fn peer_credentials(fd: libc::c_int) -> Option<Peer> {
        let (mut uid, mut gid) = (0, 0);
        // SAFETY: `fd` ist ein offener Socket, die Zeiger zeigen auf lokale Variablen passender Größe.
        let creds = unsafe { libc::getpeereid(fd, &mut uid, &mut gid) };
        let mut pid: libc::pid_t = 0;
        let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
        let got = unsafe { libc::getsockopt(fd, libc::SOL_LOCAL, libc::LOCAL_PEERPID, std::ptr::addr_of_mut!(pid).cast(), &mut len) };
        (creds == 0 && got == 0 && pid > 0).then_some(Peer { uid, pid: pid as u32 })
    }

    #[cfg(target_os = "linux")]
    pub fn peer_credentials(fd: libc::c_int) -> Option<Peer> {
        let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: `fd` ist ein offener Socket, `cred` ist eine `ucred` und `len` ihre Größe.
        let got = unsafe { libc::getsockopt(fd, libc::SOL_SOCKET, libc::SO_PEERCRED, std::ptr::addr_of_mut!(cred).cast(), &mut len) };
        (got == 0 && cred.pid > 0).then_some(Peer { uid: cred.uid, pid: cred.pid as u32 })
    }

    /// Andere Unix-Systeme: ohne pid des Clients gibt es keinen Aufrufer, die Verbindung wird abgewiesen.
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    pub fn peer_credentials(_fd: libc::c_int) -> Option<Peer> {
        None
    }
}

#[cfg(windows)]
mod transport {
    use super::*;
    use crate::endpoint::Endpoint;
    use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};

    pub fn listen(path: &Path, service: Service, stop: Arc<AtomicBool>) -> io::Result<Listening> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let token = Endpoint::new_token();
        Endpoint { port, token: token.clone(), pid: std::process::id() }.write(path)?;
        let thread = std::thread::spawn(move || accept_loop(&listener, port, &token, &service, &stop));
        let wake = Box::new(move || {
            let _ = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(1));
        });
        Ok((thread, wake))
    }

    fn accept_loop(listener: &TcpListener, port: u16, token: &str, service: &Service, stop: &AtomicBool) {
        for stream in listener.incoming() {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = stream else { continue };
            let (service, token) = (service.clone(), token.to_string());
            std::thread::spawn(move || answer(stream, port, &token, &service));
        }
    }

    fn answer(stream: TcpStream, port: u16, token: &str, service: &Service) {
        let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
        let Some(pid) = stream.peer_addr().ok().and_then(|peer| owner_pid(peer, port)) else { return };
        serve_connection(&stream, pid, Some(token), service);
    }

    /// pid des Prozesses, dem die Verbindung vom `client`-Socket zum Server-Port gehört.
    fn owner_pid(client: SocketAddr, server_port: u16) -> Option<u32> {
        if !client.ip().is_loopback() {
            return None;
        }
        (0..3).find_map(|attempt| {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(10));
            }
            tcp_owner(client.port(), server_port)
        })
    }

    /// Sucht in der TCP-Tabelle (IPv4) die Verbindung 127.0.0.1:`local` zu 127.0.0.1:`remote` und gibt ihren Besitzer.
    fn tcp_owner(local: u16, remote: u16) -> Option<u32> {
        use windows_sys::Win32::NetworkManagement::IpHelper::{GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_CONNECTIONS};
        use windows_sys::Win32::Networking::WinSock::AF_INET;

        let mut size = 0u32;
        // SAFETY: erster Aufruf ohne Puffer, nur um die Größe zu erfahren (ERROR_INSUFFICIENT_BUFFER).
        unsafe { GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 0, AF_INET.into(), TCP_TABLE_OWNER_PID_CONNECTIONS, 0) };
        // u64-Puffer, damit die Struktur ausgerichtet liegt. Die Tabelle kann zwischen den Aufrufen wachsen: Reserve.
        let mut buf = vec![0u64; (size as usize).div_ceil(8) + 64];
        let mut size = (buf.len() * 8) as u32;
        // SAFETY: `buf` ist groß genug (`size` Bytes) und für `MIB_TCPTABLE_OWNER_PID` ausgerichtet.
        let rc = unsafe { GetExtendedTcpTable(buf.as_mut_ptr().cast(), &mut size, 0, AF_INET.into(), TCP_TABLE_OWNER_PID_CONNECTIONS, 0) };
        if rc != 0 {
            return None;
        }
        let table = buf.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
        // SAFETY: die API hat `dwNumEntries` Zeilen hinter dem Zähler in `buf` abgelegt.
        let rows = unsafe {
            let first = std::ptr::addr_of!((*table).table).cast::<MIB_TCPROW_OWNER_PID>();
            std::slice::from_raw_parts(first, (*table).dwNumEntries as usize)
        };
        let loopback = u32::from_ne_bytes([127, 0, 0, 1]);
        // Die Ports stehen in Netzwerk-Byte-Reihenfolge in den unteren 16 Bit.
        rows.iter()
            .find(|r| r.dwLocalAddr == loopback && r.dwRemoteAddr == loopback && u16::from_be(r.dwLocalPort as u16) == local && u16::from_be(r.dwRemotePort as u16) == remote)
            .map(|r| r.dwOwningPid)
    }
}

#[cfg(not(any(unix, windows)))]
mod transport {
    use super::*;

    pub fn listen(_path: &Path, _service: Service, _stop: Arc<AtomicBool>) -> io::Result<Listening> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Steuerkanal: Plattform nicht unterstützt"))
    }
}
