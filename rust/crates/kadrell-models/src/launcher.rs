//! Ollama im Hintergrund starten (Swift: `OllamaLauncher.swift`). Mac: `/Applications/Ollama.app` mit `open -g`.
//! Windows: die Tray-App `ollama app.exe` aus dem Installationsordner des Ollama-Installers
//! (`%LOCALAPPDATA%\Programs\Ollama`, Ordner auf dieser Maschine geprüft; das Starten selbst ist nicht verifiziert,
//! Annahme: sie bringt den Dienst auf 127.0.0.1:11434 mit wie die Mac-App). Linux: nichts, dort läuft Ollama als
//! Systemdienst.

/// `true`, wenn die App gestartet wurde (der Dienst braucht dann noch einen Moment).
#[cfg(target_os = "macos")]
pub fn launch_ollama_app() -> bool {
    const APP: &str = "/Applications/Ollama.app";
    std::path::Path::new(APP).exists() && std::process::Command::new("/usr/bin/open").args(["-g", "-a", APP]).spawn().is_ok()
}

#[cfg(windows)]
pub fn launch_ollama_app() -> bool {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let Some(exe) = windows_app_path(std::env::var("LOCALAPPDATA").ok().as_deref()).filter(|p| p.is_file()) else { return false };
    std::process::Command::new(exe).creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW).spawn().is_ok()
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn launch_ollama_app() -> bool {
    false
}

/// Wo der Windows-Installer die Tray-App ablegt; ohne `LOCALAPPDATA` nirgends.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_app_path(local_app_data: Option<&str>) -> Option<std::path::PathBuf> {
    let base = local_app_data.filter(|b| !b.is_empty())?;
    Some(std::path::Path::new(base).join("Programs").join("Ollama").join("ollama app.exe"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_app_lives_in_the_installer_folder() {
        let p = windows_app_path(Some("C:/Users/x/AppData/Local")).unwrap();
        assert!(p.ends_with("Programs/Ollama/ollama app.exe"));
        assert_eq!(windows_app_path(None), None);
        assert_eq!(windows_app_path(Some("")), None);
    }
}
