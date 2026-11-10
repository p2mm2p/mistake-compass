//! 纯视图路由：输入方法、路径和表单正文，输出 HTML 与状态变更标记。

pub mod data;
pub mod review;
pub mod routes;
pub mod view;

pub use routes::{UiResponse, UiState, handle};
