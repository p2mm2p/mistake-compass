//! IndexedDB 持久化与 ZIP 备份格式。
//!
//! 浏览器存储与备份读写只在 `IndexedDbStore` 的导入导出方法汇合。

mod backup;
mod idb;

pub use backup::{ImportSnapshot, build_archive, parse_archive};
pub use idb::IndexedDbStore;
