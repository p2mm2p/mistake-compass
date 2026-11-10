pub use mc_core::{Entry, ReviewState, State};

pub type Review = ReviewState;

#[cfg(test)]
pub fn load_review(root: &std::path::Path) -> Result<ReviewState, String> {
    let path = root.join("review/state.yaml");
    if !path.exists() {
        return Ok(ReviewState::default());
    }
    ReviewState::from_yaml(&std::fs::read_to_string(path).map_err(|error| error.to_string())?)
}
