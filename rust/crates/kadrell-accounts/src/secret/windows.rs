//! Windows: Live-Eintrag als Datei der `claude`-CLI, Konto-Slots im Windows Credential Manager.

use std::path::PathBuf;

use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW,
    CredWriteW,
};

use super::{LIVE_SERVICE, SecretStore};
use crate::live_file::{read_live_file, write_live_file};

/// Größtes Geheimnis, das der Credential Manager je Eintrag annimmt (`CRED_MAX_CREDENTIAL_BLOB_SIZE`).
const MAX_BLOB: usize = 2560;

pub struct WindowsSecretStore {
    live_file: Option<PathBuf>,
}

impl WindowsSecretStore {
    pub fn new(live_file: Option<PathBuf>) -> Self {
        Self { live_file }
    }
}

impl SecretStore for WindowsSecretStore {
    fn read(&self, service: &str, account: Option<&str>) -> Option<String> {
        if service == LIVE_SERVICE {
            return read_live_file(self.live_file.as_deref()?);
        }
        cred_read(&target(service, account?))
    }

    fn write(&self, service: &str, account: &str, value: &str) -> bool {
        if service == LIVE_SERVICE {
            return self
                .live_file
                .as_deref()
                .is_some_and(|p| write_live_file(p, value));
        }
        cred_write(&target(service, account), account, value)
    }

    fn delete(&self, service: &str, account: &str) {
        // Die Datei der CLI löschen wir nie, auch Swift löscht den Live-Eintrag nicht.
        if service != LIVE_SERVICE {
            cred_delete(&target(service, account));
        }
    }
}

fn target(service: &str, account: &str) -> String {
    format!("{service}/{account}")
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn cred_read(target: &str) -> Option<String> {
    let name = wide(target);
    let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `name` ist nullterminiert; bei Erfolg gehört `cred` uns bis `CredFree`.
    let ok = unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) };
    if ok == 0 || cred.is_null() {
        return None;
    }
    // SAFETY: `cred` ist gültig, Blob und Größe stammen vom Credential Manager.
    let text = unsafe {
        let c = &*cred;
        let bytes = if c.CredentialBlob.is_null() {
            &[][..]
        } else {
            std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize)
        };
        String::from_utf8_lossy(bytes).trim().to_string()
    };
    // SAFETY: `cred` kam von `CredReadW`.
    unsafe { CredFree(cred as *const _) };
    (!text.is_empty()).then_some(text)
}

fn cred_write(target: &str, user: &str, value: &str) -> bool {
    let blob = value.as_bytes();
    if blob.len() > MAX_BLOB {
        log::warn!("Credential Manager: Eintrag zu groß ({} Bytes)", blob.len());
        return false;
    }
    let mut name = wide(target);
    let mut user = wide(user);
    let cred = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: name.as_mut_ptr(),
        Comment: std::ptr::null_mut(),
        LastWritten: windows_sys::Win32::Foundation::FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        },
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_ptr() as *mut u8,
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: std::ptr::null_mut(),
        UserName: user.as_mut_ptr(),
    };
    // SAFETY: alle Zeiger bleiben für die Dauer des Aufrufs gültig, `CredWriteW` kopiert die Daten.
    unsafe { CredWriteW(&cred, 0) != 0 }
}

fn cred_delete(target: &str) {
    let name = wide(target);
    // SAFETY: `name` ist nullterminiert.
    unsafe {
        CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nur mit eigenem, eindeutigem Zieleintrag, nie mit echten Zugangsdaten.
    #[test]
    fn credential_manager_roundtrip() {
        let service = format!("de.malura.kadrell.test.{}", std::process::id());
        let store = WindowsSecretStore::new(None);
        assert_eq!(store.read(&service, Some("a")), None);
        assert!(store.write(&service, "a", "{\"k\":\"v\"}"));
        assert_eq!(
            store.read(&service, Some("a")).as_deref(),
            Some("{\"k\":\"v\"}")
        );
        assert!(store.write(&service, "a", "zwei"));
        assert_eq!(store.read(&service, Some("a")).as_deref(), Some("zwei"));
        store.delete(&service, "a");
        assert_eq!(store.read(&service, Some("a")), None);
    }

    #[test]
    fn oversized_secret_is_refused() {
        let store = WindowsSecretStore::new(None);
        assert!(!store.write("de.malura.kadrell.test.big", "a", &"x".repeat(MAX_BLOB + 1)));
    }

    #[test]
    fn live_service_uses_the_file() {
        let dir =
            std::env::temp_dir().join(format!("kadrell-accounts-winlive-{}", std::process::id()));
        let file = dir.join(".credentials.json");
        let store = WindowsSecretStore::new(Some(file));
        assert_eq!(store.read(LIVE_SERVICE, None), None);
        assert!(store.write(LIVE_SERVICE, "ignored", "{\"claudeAiOauth\":{}}"));
        assert_eq!(
            store.read(LIVE_SERVICE, None).as_deref(),
            Some("{\"claudeAiOauth\":{}}")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
