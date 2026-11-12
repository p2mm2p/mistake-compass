//! 错题抓取状态机、上游响应装配与 YAML 发射。
//!
//! 网络、时钟和存储通过 `mc_core::ports` 注入，纯逻辑集中在 `model`。

pub mod emit;
mod htmlfmt;
mod model;
mod scalar;
pub mod state;

pub use state::{SyncError, SyncOptions, SyncReport, run as run_sync};
