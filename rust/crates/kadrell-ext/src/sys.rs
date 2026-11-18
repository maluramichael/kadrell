//! Plattformteil für Kindprozesse: Prozessbäume gemeinsam beenden (Job Objects unter Windows, Prozessgruppen unter Unix)
//! und der Protokollkanal des Helpers (Kopie des ursprünglichen stdout, fd 1 zeigt danach auf stderr).

use std::fs::File;
use std::process::{Child, Command};

/// Ein Prozess samt seiner Nachkommen, der sich gemeinsam beenden lässt.
///
/// Unter Windows ein Job Object, unter Unix die Prozessgruppe des Kindes (der Befehl muss mit `configure` gestartet
/// sein). Mit `kill_on_close` stirbt der Baum, sobald der Wächter fallen gelassen wird (auch beim Ende der App).
pub struct KillGroup {
    #[cfg(windows)]
    job: win::JobHandle,
    #[cfg(unix)]
    pgid: i32,
}

/// Vorbereitung eines Befehls: kein Konsolenfenster (Windows), eigene Prozessgruppe (Unix).
pub fn configure(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(not(any(windows, unix)))]
    let _ = cmd;
}

impl KillGroup {
    /// Nimmt ein frisch gestartetes Kind in die Gruppe auf. `None`, wenn das System es nicht erlaubt.
    pub fn attach(child: &Child, kill_on_close: bool) -> Option<KillGroup> {
        #[cfg(windows)]
        {
            let job = win::JobHandle::create(kill_on_close)?;
            job.assign_child(child).then_some(KillGroup { job })
        }
        #[cfg(unix)]
        {
            let _ = kill_on_close;
            Some(KillGroup {
                pgid: i32::try_from(child.id()).ok()?,
            })
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = (child, kill_on_close);
            None
        }
    }

    /// Hart beenden (SIGKILL, TerminateJobObject).
    pub fn kill(&self) {
        #[cfg(windows)]
        self.job.terminate(1);
        #[cfg(unix)]
        // SAFETY: killpg mit einer Gruppen-id, die wir selbst angelegt haben.
        unsafe {
            libc::killpg(self.pgid, libc::SIGKILL);
        }
    }

    /// Höflich bitten (SIGTERM). Windows kennt nichts Vergleichbares für Konsolenlose Prozesse, dort geschieht nichts.
    pub fn terminate(&self) {
        #[cfg(unix)]
        // SAFETY: wie `kill`.
        unsafe {
            libc::killpg(self.pgid, libc::SIGTERM);
        }
    }
}

/// Lässt den Helfer samt allen Kindprozessen mit seinem Ende sterben (EOF, `os.exit`, Absturz, Kill von außen).
/// Windows: der Helfer tritt einem Job Object mit KILL_ON_JOB_CLOSE bei, Kinder erben es. Unix: Kinder aus
/// `kadrell.exec` liegen in eigenen Gruppen und stehen in einer Liste, die `kill_exec_children` beendet.
pub fn die_with_children() {
    #[cfg(windows)]
    if let Some(job) = win::JobHandle::create(true) {
        job.assign_current_process();
        // Das Handle bleibt bis zum Prozessende offen; sein Schließen durch Windows löst den Kill aus.
        std::mem::forget(job);
    }
}

#[cfg(unix)]
static EXEC_GROUPS: std::sync::Mutex<Vec<i32>> = std::sync::Mutex::new(Vec::new());

/// Merkt eine Prozessgruppe eines `kadrell.exec`-Kindes, damit das Ende des Helfers sie mitnimmt.
pub fn track_exec(_group: &KillGroup) {
    #[cfg(unix)]
    EXEC_GROUPS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(_group.pgid);
}

pub fn untrack_exec(_group: &KillGroup) {
    #[cfg(unix)]
    EXEC_GROUPS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|g| *g != _group.pgid);
}

/// Prozess-ids aller Nachkommen von `pid` (Kinder, Enkel, …), über die Eltern-Beziehungen der Prozessliste.
pub fn descendants(pid: u32) -> Vec<u32> {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let Ok(mut system) = System::new() else {
        return Vec::new();
    };
    system.refresh_processes(ProcessesToUpdate::All, true);
    let mut found: Vec<u32> = Vec::new();
    let mut queue = vec![pid];
    while let Some(parent) = queue.pop() {
        for (id, p) in system.processes() {
            let id = id.as_u32();
            if p.parent() == Some(Pid::from_u32(parent)) && id != pid && !found.contains(&id) {
                found.push(id);
                queue.push(id);
            }
        }
    }
    found
}

/// SIGKILL an alle Nachkommen von `pid`, die Prozessgruppen eigener Kinder eingeschlossen. Ein über `kadrell.exec`
/// gestartetes Kind liegt in einer eigenen Gruppe, ein Signal an die Gruppe des Helfers erreicht es nicht, und stirbt
/// der Helfer, laufen die Kinder weiter. Darum vor jedem Signal an einen Helfer aufrufen, auch vor dem sanften: danach
/// hängen die Kinder nicht mehr an ihm.
pub fn kill_descendants(pid: u32) {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let ids = descendants(pid);
    if ids.is_empty() {
        return;
    }
    let Ok(mut system) = System::new() else {
        return;
    };
    let pids: Vec<Pid> = ids.iter().map(|i| Pid::from_u32(*i)).collect();
    system.refresh_processes(ProcessesToUpdate::Some(&pids), true);
    for p in pids {
        if let Some(process) = system.process(p) {
            process.kill();
        }
    }
}

/// Beendet alle noch laufenden `kadrell.exec`-Kinder; vor jedem Ende des Helfers aufzurufen. Unter Windows erledigt das
/// Job Object das beim Prozessende von selbst.
pub fn kill_exec_children() {
    #[cfg(unix)]
    for g in EXEC_GROUPS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .drain(..)
    {
        // SAFETY: Gruppen-ids stammen von unseren eigenen Kindern.
        unsafe {
            libc::killpg(g, libc::SIGKILL);
        }
    }
}

/// Protokollkanal des Helpers: eine nicht vererbbare Kopie des ursprünglichen stdout. Danach zeigt fd 1 (auch das
/// Handle von Rust und das der C-Laufzeit, die Lua für `io.write` benutzt) auf stderr, damit ein verirrter Schreiber
/// das Protokoll nicht zerschießt, sondern im Log der Extension landet.
pub fn take_protocol_stdout() -> std::io::Result<File> {
    #[cfg(windows)]
    {
        win::take_protocol_stdout()
    }
    #[cfg(unix)]
    {
        unix_take_protocol_stdout()
    }
    #[cfg(not(any(windows, unix)))]
    {
        Err(std::io::Error::other("plattform nicht unterstützt"))
    }
}

#[cfg(unix)]
fn unix_take_protocol_stdout() -> std::io::Result<File> {
    use std::os::fd::FromRawFd;
    // close-on-exec: Prozesse aus kadrell.exec dürfen den Protokollkanal nicht erben.
    // SAFETY: fcntl/dup2 auf die Standard-fds des eigenen Prozesses.
    unsafe {
        let copy = libc::fcntl(1, libc::F_DUPFD_CLOEXEC, 0);
        if copy < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::dup2(2, 1) < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(File::from_raw_fd(copy))
    }
}

#[cfg(windows)]
mod win {
    use std::fs::File;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::process::Child;

    use windows_sys::Win32::Foundation::{
        CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    unsafe extern "C" {
        fn _dup2(fd1: i32, fd2: i32) -> i32;
    }

    pub struct JobHandle(HANDLE);

    // SAFETY: ein Job-Handle darf von jedem Thread benutzt werden.
    unsafe impl Send for JobHandle {}
    unsafe impl Sync for JobHandle {}

    impl JobHandle {
        pub fn create(kill_on_close: bool) -> Option<JobHandle> {
            // SAFETY: Aufrufe der Win32-API mit gültigen Argumenten; das Handle gehört danach `JobHandle`.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return None;
                }
                let handle = JobHandle(job);
                if kill_on_close {
                    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                    let ok = SetInformationJobObject(
                        job,
                        JobObjectExtendedLimitInformation,
                        std::ptr::addr_of!(info).cast(),
                        std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                    );
                    if ok == 0 {
                        return None;
                    }
                }
                Some(handle)
            }
        }

        pub fn assign_child(&self, child: &Child) -> bool {
            // SAFETY: das Prozess-Handle des Kindes lebt, solange `child` lebt.
            unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) != 0 }
        }

        pub fn assign_current_process(&self) -> bool {
            // SAFETY: GetCurrentProcess liefert ein Pseudo-Handle.
            unsafe { AssignProcessToJobObject(self.0, GetCurrentProcess()) != 0 }
        }

        pub fn terminate(&self, code: u32) {
            // SAFETY: gültiges Job-Handle.
            unsafe {
                TerminateJobObject(self.0, code);
            }
        }
    }

    impl Drop for JobHandle {
        fn drop(&mut self) {
            // SAFETY: wir besitzen das Handle.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    pub fn take_protocol_stdout() -> std::io::Result<File> {
        // SAFETY: Standard-Handles des eigenen Prozesses, Kopie ohne Vererbung.
        unsafe {
            let out = GetStdHandle(STD_OUTPUT_HANDLE);
            if out.is_null() || out == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::other("kein stdout"));
            }
            let mut copy: HANDLE = std::ptr::null_mut();
            let me = GetCurrentProcess();
            if DuplicateHandle(me, out, me, &mut copy, 0, 0, DUPLICATE_SAME_ACCESS) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            // Erst die C-Laufzeit (Lua schreibt über fd 1), danach das Win32-Handle (Rust, Kindprozesse).
            if _dup2(2, 1) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let err = GetStdHandle(STD_ERROR_HANDLE);
            if !err.is_null() && err != INVALID_HANDLE_VALUE {
                SetStdHandle(STD_OUTPUT_HANDLE, err);
            }
            Ok(File::from_raw_handle(copy))
        }
    }
}
