//! Unter Windows das App-Icon in `kadrell.exe` einbetten (`windows/kadrell.rc`). Andere Ziele: nichts.

fn main() {
    println!("cargo:rerun-if-changed=windows/kadrell.rc");
    println!("cargo:rerun-if-changed=windows/kadrell.ico");
    // Ziel, nicht Host: `#[cfg(windows)]` würde in build.rs den Build-Rechner prüfen.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("windows/kadrell.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("App-Icon einbetten");
    }
}
