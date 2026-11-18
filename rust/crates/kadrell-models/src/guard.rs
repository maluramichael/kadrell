//! Ein Kindprozess, der mit Kadrell endet, samt seinen Kindern (der Router von `llama-server` startet je Modell einen
//! eigenen Prozess). Windows: Job Object mit `KILL_ON_JOB_CLOSE`, das Handle gehört dem Guard und damit dem Prozess von
//! Kadrell: stirbt Kadrell, auch durch Absturz oder Beenden im Task-Manager, räumt Windows den ganzen Baum ab. Unix:
//! eigene Prozessgruppe, `stop` beendet die Gruppe.

use std::collections::BTreeMap;
use std::process::{Child, Command, Stdio};

pub struct ProcessGuard {
    child: Child,
    #[cfg(windows)]
    job: isize,
}

impl ProcessGuard {
    /// Startet `program` ohne Fenster, ohne Ein- und Ausgabe, mit genau dieser Umgebung.
    pub fn spawn(program: &str, args: &[String], env: &BTreeMap<String, String>) -> std::io::Result<ProcessGuard> {
        let mut cmd = Command::new(program);
        cmd.args(args).env_clear().envs(env).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        prepare(&mut cmd);
        let child = cmd.spawn()?;
        #[cfg(windows)]
        {
            let job = job::assign(&child);
            Ok(ProcessGuard { child, job })
        }
        #[cfg(not(windows))]
        Ok(ProcessGuard { child })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Läuft der Prozess noch?
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Beendet den Prozess und alle seine Kinder und wartet auf das Ende des Prozesses.
    pub fn stop(&mut self) {
        #[cfg(windows)]
        job::terminate(self.job);
        #[cfg(unix)]
        unix::terminate_group(self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        self.stop();
        #[cfg(windows)]
        job::close(self.job);
    }
}

#[cfg(windows)]
fn prepare(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(unix)]
fn prepare(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
}

#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };

    /// Neues Job Object mit `KILL_ON_JOB_CLOSE` und den Prozess darin. 0 heißt: ging nicht (der Prozess läuft trotzdem,
    /// nur ohne die Absicherung gegen einen Absturz von Kadrell).
    pub fn assign(child: &Child) -> isize {
        // SAFETY: reine Win32-Aufrufe mit gültigen Zeigern auf lokale Strukturen; das Handle wird nur hier und in
        // `terminate`/`close` benutzt.
        unsafe {
            let job: HANDLE = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return 0;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
            let set = SetInformationJobObject(job, JobObjectExtendedLimitInformation, std::ptr::addr_of!(info).cast(), size);
            if set == 0 || AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) == 0 {
                CloseHandle(job);
                return 0;
            }
            job as isize
        }
    }

    pub fn terminate(job: isize) {
        if job != 0 {
            // SAFETY: `job` ist ein von `assign` geliefertes, noch offenes Handle.
            unsafe { TerminateJobObject(job as HANDLE, 1) };
        }
    }

    pub fn close(job: isize) {
        if job != 0 {
            // SAFETY: wie oben; danach wird das Handle nicht mehr benutzt (Drop).
            unsafe { CloseHandle(job as HANDLE) };
        }
    }
}

/// Beendet Prozess und Gruppe, wenn `pid` noch ein Programm namens `name` ist (Unix; nicht auf Windows getestet).
#[cfg(unix)]
pub fn terminate_if_named(pid: u32, name: &str) {
    let out = Command::new("ps").args(["-p", &pid.to_string(), "-o", "comm="]).output();
    let comm = out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    if comm.ends_with(name) {
        unix::terminate_group(pid);
    }
}

#[cfg(unix)]
mod unix {
    /// SIGTERM an die ganze Prozessgruppe (der Kindprozess ist ihr Anführer).
    pub fn terminate_group(pid: u32) {
        // SAFETY: `kill` mit negativer pid ist ein reiner Systemaufruf ohne Speicherzugriff.
        unsafe { libc::kill(-(pid as i32), libc::SIGTERM) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long_running() -> (String, Vec<String>) {
        if cfg!(windows) {
            ("ping".into(), vec!["-n".into(), "60".into(), "127.0.0.1".into()])
        } else {
            ("sleep".into(), vec!["60".into()])
        }
    }

    fn env() -> BTreeMap<String, String> {
        std::env::vars().collect()
    }

    #[test]
    fn stop_ends_the_process_and_drop_does_too() {
        let (program, args) = long_running();
        let mut g = ProcessGuard::spawn(&program, &args, &env()).unwrap();
        assert!(g.is_running());
        assert!(g.pid() > 0);
        g.stop();
        assert!(!g.is_running());
    }

    #[test]
    fn missing_program_is_an_error() {
        assert!(ProcessGuard::spawn("kadrell-gibt-es-nicht", &[], &env()).is_err());
    }
}
