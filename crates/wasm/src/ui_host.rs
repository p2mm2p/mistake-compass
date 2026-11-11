use std::cell::RefCell;

use wasm_bindgen::prelude::*;

use mc_core::ports::{CorpusStore, ReviewStore};
use storage::IndexedDbStore;
use ui::{UiResponse, UiState};

thread_local! {
    static UI_STATE: RefCell<Option<UiState>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
pub async fn render_initial() -> Result<String, JsValue> {
    initial_payload()
        .await
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub async fn refresh_ui() -> Result<String, JsValue> {
    initial_payload()
        .await
        .map_err(|error| JsValue::from_str(&error))
}

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
