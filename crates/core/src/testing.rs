use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use async_trait::async_trait;

use crate::ports::{
    Clock, CorpusSnapshot, CorpusStore, FetchResponse, Fetcher, Progress, ReviewStore,
};
use crate::question::Category;
use crate::review::ReviewState;

pub struct MemoryFetcher {
    responses: RefCell<VecDeque<Result<FetchResponse, String>>>,
    calls: RefCell<Vec<String>>,
}

impl MemoryFetcher {
    pub fn new(responses: Vec<Result<FetchResponse, String>>) -> MemoryFetcher {
        MemoryFetcher {
            responses: RefCell::new(responses.into()),
            calls: RefCell::new(Vec::new()),
        }
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

#[async_trait(?Send)]
impl Fetcher for MemoryFetcher {
    async fn get(&self, path: &str, _query: &[(&str, String)]) -> Result<FetchResponse, String> {
        self.calls.borrow_mut().push(path.to_string());
        self.responses
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Err(format!("没有为 {path} 准备响应")))
    }
}

pub struct MemoryCorpusStore {
    active: RefCell<Option<CorpusSnapshot>>,
    staged: RefCell<BTreeMap<String, Vec<Category>>>,
}

impl MemoryCorpusStore {
    pub fn new(active: Option<CorpusSnapshot>) -> MemoryCorpusStore {
        MemoryCorpusStore {
            active: RefCell::new(active),
            staged: RefCell::new(BTreeMap::new()),
        }
    }
}

#[async_trait(?Send)]
impl CorpusStore for MemoryCorpusStore {
    async fn active(&self) -> Result<Option<CorpusSnapshot>, String> {
        Ok(self.active.borrow().clone())
    }

    async fn stage(&self, generation: &str, categories: Vec<Category>) -> Result<(), String> {
        self.staged
            .borrow_mut()
            .insert(generation.to_string(), categories);
        Ok(())
    }

    async fn activate(&self, generation: &str) -> Result<(), String> {
        let categories = self
            .staged
            .borrow_mut()
            .remove(generation)
            .ok_or_else(|| format!("没有暂存 generation {generation}"))?;
        let corpus = crate::Corpus::new(categories)?;
        *self.active.borrow_mut() = Some(CorpusSnapshot {
            generation: generation.to_string(),
            corpus,
        });
        Ok(())
    }

    async fn discard(&self, generation: &str) -> Result<(), String> {
        self.staged.borrow_mut().remove(generation);
        Ok(())
    }
}

pub struct MemoryReviewStore {
    state: RefCell<ReviewState>,
}

impl MemoryReviewStore {
    pub fn new(state: ReviewState) -> MemoryReviewStore {
        MemoryReviewStore {
            state: RefCell::new(state),
        }
    }

    pub fn state(&self) -> ReviewState {
        self.state.borrow().clone()
    }
}

#[async_trait(?Send)]
impl ReviewStore for MemoryReviewStore {
    async fn load(&self) -> Result<ReviewState, String> {
        Ok(self.state.borrow().clone())
    }

    async fn save(&self, state: &ReviewState) -> Result<(), String> {
        *self.state.borrow_mut() = state.clone();
        Ok(())
    }
}

#[derive(Default)]
pub struct MemoryClock {
    sleeps: RefCell<Vec<Duration>>,
}

impl MemoryClock {
    pub fn sleeps(&self) -> Vec<Duration> {
        self.sleeps.borrow().clone()
    }
}

#[async_trait(?Send)]
impl Clock for MemoryClock {
    async fn sleep(&self, duration: Duration) {
        self.sleeps.borrow_mut().push(duration);
    }
}

#[derive(Default)]
pub struct MemoryProgress {
    events: RefCell<Vec<String>>,
}

impl MemoryProgress {
    pub fn events(&self) -> Vec<String> {
        self.events.borrow().clone()
    }
}

impl Progress for MemoryProgress {
    fn phase(&self, phase: &str) {
        self.events.borrow_mut().push(format!("phase:{phase}"));
    }

    fn batch(&self, completed: usize, total: usize) {
        self.events
            .borrow_mut()
            .push(format!("batch:{completed}/{total}"));
    }

    fn error(&self, message: &str) {
        self.events.borrow_mut().push(format!("error:{message}"));
    }
}

pub struct NoopProgress;

impl Progress for NoopProgress {
    fn phase(&self, _phase: &str) {}
    fn batch(&self, _completed: usize, _total: usize) {}
    fn error(&self, _message: &str) {}
}
