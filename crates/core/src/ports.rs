//! 业务 crate 使用的异步边界与进度回调。
//!
//! 所有 trait 都不绑定浏览器类型，宿主可选择 IndexedDB、HTTP 或内存存储。

use std::time::Duration;

use async_trait::async_trait;

use crate::Corpus;
use crate::question::Category;
use crate::review::ReviewState;

/// 一次 HTTP 响应
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResponse {
    /// HTTP 状态码
    pub status: u16,
    /// 响应正文
    pub body: Vec<u8>,
}

/// 一个已激活的题库快照
#[derive(Debug, Clone)]
pub struct CorpusSnapshot {
    /// 生成标识
    pub generation: String,
    /// 已校验的题库
    pub corpus: Corpus,
}

/// 发起只读 HTTP 请求
#[async_trait(?Send)]
pub trait Fetcher {
    /// 请求 `path` 并附加查询参数。
    ///
    /// # Errors
    ///
    /// 请求、超时或读取响应失败时返回错误。
    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<FetchResponse, String>;
}

/// 按 generation 暂存、激活和清理题库
#[async_trait(?Send)]
pub trait CorpusStore {
    /// 返回当前活动快照；尚未导入题库时为 `None`
    async fn active(&self) -> Result<Option<CorpusSnapshot>, String>;
    /// 暂存一个 generation 的分类，不改变活动快照
    async fn stage(&self, generation: &str, categories: Vec<Category>) -> Result<(), String>;
    /// 切换活动 generation
    async fn activate(&self, generation: &str) -> Result<(), String>;
    /// 删除指定 generation 的暂存分类
    async fn discard(&self, generation: &str) -> Result<(), String>;
}

/// 读写复盘状态
#[async_trait(?Send)]
pub trait ReviewStore {
    /// 读取复盘状态
    async fn load(&self) -> Result<ReviewState, String>;
    /// 保存复盘状态
    async fn save(&self, state: &ReviewState) -> Result<(), String>;
}

/// 异步等待的时钟边界
#[async_trait(?Send)]
pub trait Clock {
    /// 等待指定时长
    async fn sleep(&self, duration: Duration);
}

/// 同步进度回调；回调必须快速返回
pub trait Progress {
    /// 报告当前阶段
    fn phase(&self, phase: &str);
    /// 报告批次进度
    fn batch(&self, completed: usize, total: usize);
    /// 报告错误文本
    fn error(&self, message: &str);
}
