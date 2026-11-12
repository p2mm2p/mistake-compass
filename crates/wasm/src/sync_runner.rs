//! 把同步状态机接到浏览器宿主与 IndexedDB

use std::time::Duration;

use storage::IndexedDbStore;
use sync::state::{SyncOptions, run};
use wasm_bindgen::prelude::*;

use crate::host::{HostClock, HostFetcher, HostProgress};

/// 从浏览器抓取题库并切换到 `generation` 对应的快照。
///
/// # Errors
///
/// 数据库打开、同步状态机或快照切换失败时转为 `JsValue`。
#[wasm_bindgen]
pub async fn run_sync(generation: &str) -> Result<String, JsValue> {
    let store = IndexedDbStore::open()
        .await
        .map_err(|error| JsValue::from_str(&error))?;
    let options = SyncOptions {
        generation: generation.to_string(),
        delay: Duration::from_millis(800),
        ..SyncOptions::default()
    };
    let report = run(
        &HostFetcher::new("tiku.fenbi.com"),
        &HostClock,
        &HostProgress,
        &store,
        &options,
    )
    .await
    .map_err(|error| JsValue::from_str(&error.to_string()))?;
    Ok(serde_json::json!({
        "questions": report.questions,
        "categories": report.categories,
        "batches": report.batches,
        "requests": report.requests,
        "failures": report.failures
    })
    .to_string())
}
