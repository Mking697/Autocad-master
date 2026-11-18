//! Autosave storage over `window.localStorage` ([`cadcraft_engine::autosave::StorageHooks`]):
//! the web build has no filesystem, so this replaces the desktop app's autosave folder and lock
//! file. `localStorage` only holds strings, so bytes travel as base64 (the same codec `RECOVER`
//! already uses for its `data:` form, `cmd::file::base64_encode`/`base64_decode`).

use cadcraft_engine::autosave::StorageHooks;
use cadcraft_engine::cmd::file::{base64_decode, base64_encode};
use web_sys::js_sys;

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn now_unix() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}

fn get(key: &str) -> Option<Vec<u8>> {
    let raw = local_storage()?.get_item(key).ok().flatten()?;
    base64_decode(&raw)
}

fn set(key: &str, bytes: &[u8]) -> Result<(), String> {
    let storage = local_storage().ok_or("no window.localStorage")?;
    storage.set_item(key, &base64_encode(bytes)).map_err(|e| e.as_string().unwrap_or_else(|| format!("{e:?}")))
}

fn remove(key: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(key);
    }
}

fn keys() -> Vec<String> {
    let Some(storage) = local_storage() else { return Vec::new() };
    let len = storage.length().unwrap_or(0);
    (0..len).filter_map(|i| storage.key(i).ok().flatten()).collect()
}

/// Install the `localStorage` autosave hooks. Call once at start, before
/// [`cadcraft_ui_egui::CadApp::enable_web_autosave`].
pub fn install() {
    cadcraft_engine::autosave::set_storage(StorageHooks { now_unix, get, set, remove, keys });
}

/// A fresh id for this page load: tells this tab's own autosave keys apart from a previous load's
/// leftovers (offered for recovery). The web has no lock file, just a label matching
/// `is_instance_id`'s `<digits>-<hex>` shape (unix seconds, at most 10 digits for centuries yet,
/// then a random tail so two tabs loading in the same second don't collide).
pub fn instance_id() -> String {
    let secs = now_unix();
    let rand = (js_sys::Math::random() * 1e15) as u64;
    format!("{secs}-{rand:x}")
}
