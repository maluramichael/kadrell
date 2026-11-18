//! Ollama.app im Hintergrund starten (Swift: `OllamaLauncher.swift`). Nur auf dem Mac: dort gibt es die App, die den
//! Dienst mitbringt. Unter Windows und Linux startet Kadrell keinen Ollama-Dienst (der Nutzer startet ihn selbst oder
//! stellt die Adresse eines laufenden ein).

/// `true`, wenn die App gestartet wurde (der Dienst braucht dann noch einen Moment).
#[cfg(target_os = "macos")]
pub fn launch_ollama_app() -> bool {
    const APP: &str = "/Applications/Ollama.app";
    std::path::Path::new(APP).exists() && std::process::Command::new("/usr/bin/open").args(["-g", "-a", APP]).spawn().is_ok()
}

#[cfg(not(target_os = "macos"))]
pub fn launch_ollama_app() -> bool {
    false
}
