use std::time::Duration;

use sync::state::{SyncOptions, run};
use wasm_bindgen::prelude::*;

use crate::host::{HostClock, HostFetcher, HostProgress};
use crate::idb::IndexedDbStore;

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
