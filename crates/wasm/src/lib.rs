mod backup;
mod host;
mod idb;
mod sync_runner;
mod ui_host;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

pub use backup::{ImportSnapshot, build_archive, parse_archive};
pub use host::{HostClock, HostFetcher, HostProgress};
pub use idb::IndexedDbStore;
pub use sync_runner::run_sync;
pub use ui_host::{handle_request, refresh_ui, render_initial};

#[wasm_bindgen]
pub async fn export_backup() -> Result<js_sys::Uint8Array, JsValue> {
    let store = IndexedDbStore::open()
        .await
        .map_err(|error| JsValue::from_str(&error))?;
    let bytes = store
        .export_archive()
        .await
        .map_err(|error| JsValue::from_str(&error))?;
    Ok(js_sys::Uint8Array::from(bytes.as_slice()))
}

#[wasm_bindgen]
pub async fn import_backup(bytes: Vec<u8>) -> Result<String, JsValue> {
    let store = IndexedDbStore::open()
        .await
        .map_err(|error| JsValue::from_str(&error))?;
    let snapshot = store
        .import_archive(&bytes)
        .await
        .map_err(|error| JsValue::from_str(&error))?;
    Ok(serde_json::json!({
        "formatVersion": snapshot.format_version,
        "questions": snapshot.question_count,
        "categories": snapshot.categories.len()
    })
    .to_string())
}

#[wasm_bindgen]
pub async fn request_persistent_storage() -> Result<bool, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("没有 window"))?;
    let storage = window.navigator().storage();
    let value = JsFuture::from(
        storage
            .persist()
            .map_err(|error| JsValue::from_str(&format!("请求持久化失败：{error:?}")))?,
    )
    .await
    .map_err(|error| JsValue::from_str(&format!("请求持久化失败：{error:?}")))?;
    Ok(value.as_bool().unwrap_or(false))
}
