//! 浏览器宿主适配：fetch、计时器和进度桥。
//!
//! 通过 `globalThis.__mcExtensionHost.fetch` 与 `globalThis.__mcProgress` 连接扩展脚本。

use std::time::Duration;

use async_trait::async_trait;
use gloo_timers::future::TimeoutFuture;
use js_sys::{Array, Reflect};
use mc_core::ports::{Clock, FetchResponse, Fetcher, Progress};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(js_namespace = ["globalThis", "__mcExtensionHost"])]
extern "C" {
    fn fetch(host: &str, path: &str, query: JsValue) -> js_sys::Promise;
}

/// 通过扩展脚本代理请求的 `Fetcher` 适配器
pub struct HostFetcher {
    host: String,
}

impl HostFetcher {
    /// 用目标主机名创建。
    ///
    /// `host` 传给扩展脚本，由浏览器侧发起请求。
    pub fn new(host: impl Into<String>) -> HostFetcher {
        HostFetcher { host: host.into() }
    }
}

#[async_trait(?Send)]
impl Fetcher for HostFetcher {
    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<FetchResponse, String> {
        let pairs = Array::new();
        for (key, value) in query {
            let pair = Array::new();
            pair.push(&JsValue::from_str(key));
            pair.push(&JsValue::from_str(value));
            pairs.push(&pair);
        }
        let value = JsFuture::from(fetch(&self.host, path, pairs.into()))
            .await
            .map_err(|error| format!("浏览器 fetch 失败：{error:?}"))?;
        let status = Reflect::get(&value, &JsValue::from_str("status"))
            .ok()
            .and_then(|value| value.as_f64())
            .ok_or_else(|| "fetch 响应缺 status".to_string())? as u16;
        let body = Reflect::get(&value, &JsValue::from_str("body"))
            .ok()
            .and_then(|value| value.as_string())
            .unwrap_or_default()
            .into_bytes();
        Ok(FetchResponse { status, body })
    }
}

/// 用浏览器定时器等待的时钟适配器
pub struct HostClock;

#[async_trait(?Send)]
impl Clock for HostClock {
    async fn sleep(&self, duration: Duration) {
        let millis = duration.as_millis().min(u32::MAX as u128) as u32;
        TimeoutFuture::new(millis).await;
    }
}

/// 把进度事件转成 JSON 并调用 `globalThis.__mcProgress`
pub struct HostProgress;

impl HostProgress {
    fn emit(&self, event: serde_json::Value) {
        let global = js_sys::global();
        let Ok(callback) = Reflect::get(&global, &JsValue::from_str("__mcProgress")) else {
            return;
        };
        let Ok(callback) = callback.dyn_into::<js_sys::Function>() else {
            return;
        };
        let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(&event.to_string()));
    }
}

impl Progress for HostProgress {
    fn phase(&self, phase: &str) {
        self.emit(serde_json::json!({ "kind": "phase", "value": phase }));
    }

    fn batch(&self, completed: usize, total: usize) {
        self.emit(serde_json::json!({
            "kind": "batch",
            "completed": completed,
            "total": total
        }));
    }

    fn error(&self, message: &str) {
        self.emit(serde_json::json!({ "kind": "error", "message": message }));
    }
}
