//! 端口的内存测试替身。
//!
//! 这些类型记录调用顺序与事件，供状态机和视图测试断言。

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use async_trait::async_trait;

use crate::ports::{
    Clock, CorpusSnapshot, CorpusStore, FetchResponse, Fetcher, Progress, ReviewStore,
};
use crate::question::Category;
use crate::review::ReviewState;

/// 按队列返回预设响应的 `Fetcher` 替身
pub struct MemoryFetcher {
    responses: RefCell<VecDeque<Result<FetchResponse, String>>>,
    calls: RefCell<Vec<String>>,
}

impl MemoryFetcher {
    /// 用预设响应创建；响应耗尽后返回错误
    pub fn new(responses: Vec<Result<FetchResponse, String>>) -> MemoryFetcher {
        MemoryFetcher {
            responses: RefCell::new(responses.into()),
            calls: RefCell::new(Vec::new()),
        }
    }

    /// 按调用顺序返回请求路径
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

/// 内存题库存储
pub struct MemoryCorpusStore {
    active: RefCell<Option<CorpusSnapshot>>,
    staged: RefCell<BTreeMap<String, Vec<Category>>>,
}

impl MemoryCorpusStore {
    /// 创建带可选活动快照的存储
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

/// 内存复盘存储
pub struct MemoryReviewStore {
    state: RefCell<ReviewState>,
}

impl MemoryReviewStore {
    /// 用初始状态创建存储
    pub fn new(state: ReviewState) -> MemoryReviewStore {
        MemoryReviewStore {
            state: RefCell::new(state),
        }
    }

    /// 返回当前复盘状态副本
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

/// 记录每次 sleep 时长的时钟替身
#[derive(Default)]
pub struct MemoryClock {
    sleeps: RefCell<Vec<Duration>>,
}

impl MemoryClock {
    /// 返回记录的 sleep 时长
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

/// 记录进度事件文本的替身
#[derive(Default)]
pub struct MemoryProgress {
    events: RefCell<Vec<String>>,
}

impl MemoryProgress {
    /// 返回记录的事件，格式为 `phase:...`、`batch:...` 或 `error:...`
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

/// 忽略所有进度事件
pub struct NoopProgress;

impl Progress for NoopProgress {
    fn phase(&self, _phase: &str) {}
    fn batch(&self, _completed: usize, _total: usize) {}
    fn error(&self, _message: &str) {}
}
