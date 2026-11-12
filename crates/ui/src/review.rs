//! 页面层复用的复盘类型与测试期状态载入

pub use mc_core::{Entry, ReviewState, State};

/// 页面层使用的复盘状态别名
pub type Review = ReviewState;

/// 从测试根目录读取 `review/state.yaml`；文件不存在时返回空状态。
///
/// # Errors
///
/// 文件读取或 YAML 解析失败时返回错误。
#[cfg(test)]
pub fn load_review(root: &std::path::Path) -> Result<ReviewState, String> {
    let path = root.join("review/state.yaml");
    if !path.exists() {
        return Ok(ReviewState::default());
    }
    ReviewState::from_yaml(&std::fs::read_to_string(path).map_err(|error| error.to_string())?)
}
