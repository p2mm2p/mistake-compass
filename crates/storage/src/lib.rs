//! IndexedDB 持久化与 ZIP 备份格式。

mod backup;
mod idb;

pub use backup::{ImportSnapshot, build_archive, parse_archive};
pub use idb::IndexedDbStore;
