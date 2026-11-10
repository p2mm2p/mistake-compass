use std::time::Duration;

use async_trait::async_trait;

use crate::Corpus;
use crate::question::Category;
use crate::review::ReviewState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct CorpusSnapshot {
    pub generation: String,
    pub corpus: Corpus,
}

#[async_trait(?Send)]
pub trait Fetcher {
    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<FetchResponse, String>;
}

#[async_trait(?Send)]
pub trait CorpusStore {
    async fn active(&self) -> Result<Option<CorpusSnapshot>, String>;
    async fn stage(&self, generation: &str, categories: Vec<Category>) -> Result<(), String>;
    async fn activate(&self, generation: &str) -> Result<(), String>;
    async fn discard(&self, generation: &str) -> Result<(), String>;
}

#[async_trait(?Send)]
pub trait ReviewStore {
    async fn load(&self) -> Result<ReviewState, String>;
    async fn save(&self, state: &ReviewState) -> Result<(), String>;
}

#[async_trait(?Send)]
pub trait Clock {
    async fn sleep(&self, duration: Duration);
}

pub trait Progress {
    fn phase(&self, phase: &str);
    fn batch(&self, completed: usize, total: usize);
    fn error(&self, message: &str);
}
