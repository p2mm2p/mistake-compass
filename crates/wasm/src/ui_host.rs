//! 页面请求适配与 UI 状态缓存。
//!
//! `wasm_bindgen` 每次调用会新建栈帧，交互状态放在线程局部变量中。

use std::cell::RefCell;

use wasm_bindgen::prelude::*;

use mc_core::ports::{CorpusStore, ReviewStore};
use storage::IndexedDbStore;
use ui::{UiResponse, UiState};

thread_local! {
    static UI_STATE: RefCell<Option<UiState>> = const { RefCell::new(None) };
}

/// 读取活动题库与复盘状态，初始化 UI 状态并返回页面 HTML。
///
/// 没有题库时返回 `hasCorpus=false`。
///
/// # Errors
///
/// IndexedDB 读取或页面渲染失败时转为 `JsValue`。
#[wasm_bindgen]
pub async fn render_initial() -> Result<String, JsValue> {
    initial_payload()
        .await
        .map_err(|error| JsValue::from_str(&error))
}

/// 重新读取活动题库与复盘状态，替换缓存的 UI 状态。
///
/// # Errors
///
/// IndexedDB 读取或页面渲染失败时转为 `JsValue`。
#[wasm_bindgen]
pub async fn refresh_ui() -> Result<String, JsValue> {
    initial_payload()
        .await
        .map_err(|error| JsValue::from_str(&error))
}

/// 分发页面请求，并在复盘状态变化时写回 IndexedDB。
///
/// # Errors
///
/// UI 状态未初始化、数据库写入失败时转为 `JsValue`。
#[wasm_bindgen]
pub async fn handle_request(method: &str, target: &str, body: &str) -> Result<String, JsValue> {
    let (response, review) = UI_STATE
        .with(|slot| {
            let mut slot = slot.borrow_mut();
            let state = slot.as_mut().ok_or_else(|| "UI 状态未初始化".to_string())?;
            let response = state.handle(method, target, body.as_bytes(), &now());
            let review = response.persist_review.then(|| state.review.clone());
            Ok::<_, String>((response, review))
        })
        .map_err(|error| JsValue::from_str(&error))?;

    if let Some(review) = review {
        let store = IndexedDbStore::open()
            .await
            .map_err(|error| JsValue::from_str(&error))?;
        store
            .save(&review)
            .await
            .map_err(|error| JsValue::from_str(&error))?;
    }

    Ok(json_response(response))
}

async fn initial_payload() -> Result<String, String> {
    let store = IndexedDbStore::open().await?;
    let Some(active) = store.active().await? else {
        UI_STATE.with(|slot| *slot.borrow_mut() = None);
        return Ok(serde_json::json!({
            "hasCorpus": false,
            "html": ""
        })
        .to_string());
    };
    let review = store.load().await?;
    let mut state = UiState::new(active.corpus, review);
    let response = state.handle("GET", "/", b"", &now());
    UI_STATE.with(|slot| *slot.borrow_mut() = Some(state));
    Ok(serde_json::json!({
        "hasCorpus": true,
        "html": response.html
    })
    .to_string())
}

fn json_response(response: UiResponse) -> String {
    serde_json::json!({
        "status": response.status,
        "html": response.html
    })
    .to_string()
}

fn now() -> String {
    js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
}
