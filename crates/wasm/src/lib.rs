//! WASM 宿主适配：连接浏览器 fetch、计时器、IndexedDB 与页面请求

mod host;
mod sync_runner;
mod ui_host;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

pub use host::{HostClock, HostFetcher, HostProgress};
pub use storage::{ImportSnapshot, IndexedDbStore, build_archive, parse_archive};
pub use sync_runner::run_sync;
pub use ui_host::{handle_request, refresh_ui, render_initial};

/// 把活动题库和复盘状态导出为 ZIP 字节。
///
/// # Errors
///
/// 打开数据库、读取题库或生成 ZIP 失败时转为 `JsValue`。
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

/// 导入 ZIP 备份，并返回导入统计 JSON。
///
/// # Errors
///
/// 数据库打开、备份校验或写入失败时转为 `JsValue`。
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

/// 请求浏览器授予持久化存储权限。
///
/// # Errors
///
/// 浏览器不支持 Storage API 或请求失败时转为 `JsValue`。
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
