use std::collections::HashSet;
use std::fmt;
use std::time::Duration;

use mc_core::ports::{Clock, CorpusStore, FetchResponse, Fetcher, Progress};
use mc_core::{Category, Corpus, Question};
use serde_json::Value;

use crate::model::{Batch, Node, build_nodes, build_records, collect_ids, pick_node};

pub struct SyncOptions {
    pub generation: String,
    pub delay: Duration,
    pub batch_size: usize,
    pub max_attempts: u32,
    pub failure_limit: f64,
}

impl Default for SyncOptions {
    fn default() -> SyncOptions {
        SyncOptions {
            generation: "g1".to_string(),
            delay: Duration::from_millis(800),
            batch_size: 500,
            max_attempts: 4,
            failure_limit: 0.05,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyncReport {
    pub questions: usize,
    pub categories: usize,
    pub batches: usize,
    pub requests: usize,
    pub failures: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncError {
    Auth(u16),
    Empty,
    FailureRate { rate: f64, limit: f64 },
    Fetch(String),
    Protocol(String),
    Store(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::Auth(401) => write!(formatter, "登录态已失效，请在粉笔网页重新登录"),
            SyncError::Auth(453) => {
                write!(formatter, "登录态缺少设备标识，请在粉笔网页重新登录")
            }
            SyncError::Auth(code) => write!(formatter, "登录失败：HTTP {code}"),
            SyncError::Empty => write!(formatter, "分类树里没有错题，无需抓取"),
            SyncError::FailureRate { rate, limit } => write!(
                formatter,
                "失败率 {:.1}% 超过上限 {:.1}%，已中止",
                rate * 100.0,
                limit * 100.0
            ),
            SyncError::Fetch(error) => write!(formatter, "抓取失败：{error}"),
            SyncError::Protocol(error) => write!(formatter, "接口响应异常：{error}"),
            SyncError::Store(error) => write!(formatter, "保存题库失败：{error}"),
        }
    }
}

impl std::error::Error for SyncError {}

pub async fn run<F, K, P, S>(
    fetcher: &F,
    clock: &K,
    progress: &P,
    store: &S,
    options: &SyncOptions,
) -> Result<SyncReport, SyncError>
where
    F: Fetcher,
    K: Clock,
    P: Progress,
    S: CorpusStore,
{
    match run_inner(fetcher, clock, progress, store, options).await {
        Ok(report) => Ok(report),
        Err(error) => {
            progress.error(&error.to_string());
            Err(error)
        }
    }
}

async fn run_inner<F, K, P, S>(
    fetcher: &F,
    clock: &K,
    progress: &P,
    store: &S,
    options: &SyncOptions,
) -> Result<SyncReport, SyncError>
where
    F: Fetcher,
    K: Clock,
    P: Progress,
    S: CorpusStore,
{
    let mut requests = 0usize;
    let mut failures = 0usize;

    progress.phase("分类树");
    let mut tree_query = common_params();
    tree_query.push(("timeRange", "0".to_string()));
    tree_query.push(("order", "0".to_string()));
    let tree = request_json(
        fetcher,
        clock,
        "/api/xingce/errors/keypoint-tree",
        &tree_query,
        options,
        &mut requests,
        &mut failures,
    )
    .await?;
    let nodes = build_nodes(&tree).map_err(SyncError::Protocol)?;
    let ids = collect_ids(&nodes);
    if ids.is_empty() {
        return Err(SyncError::Empty);
    }

    let batch_size = options.batch_size.max(1);
    let batches_total = ids.len().div_ceil(batch_size);
    let mut batches = Vec::with_capacity(batches_total);
    for (batch_no, chunk) in ids.chunks(batch_size).enumerate() {
        progress.phase("题目批次");
        let csv = chunk
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let batch =
            fetch_batch(fetcher, clock, &csv, options, &mut requests, &mut failures).await?;
        batches.push(batch);
        progress.batch(batch_no + 1, batches_total);
        let rate = failures as f64 / requests.max(1) as f64;
        if rate > options.failure_limit {
            return Err(SyncError::FailureRate {
                rate,
                limit: options.failure_limit,
            });
        }
    }

    progress.phase("装配题库");
    let records = build_records(&ids, &batches).map_err(SyncError::Protocol)?;
    let categories = group_categories(&nodes, &records)?;
    Corpus::new(categories.clone()).map_err(SyncError::Protocol)?;
    store
        .stage(&options.generation, categories)
        .await
        .map_err(SyncError::Store)?;
    progress.phase("切换快照");
    store
        .activate(&options.generation)
        .await
        .map_err(SyncError::Store)?;

    let category_count = records
        .iter()
        .filter_map(|record| pick_node(record.fb, &nodes))
        .collect::<HashSet<_>>()
        .len();
    Ok(SyncReport {
        questions: records.len(),
        categories: category_count,
        batches: batches_total,
        requests,
        failures,
    })
}

async fn fetch_batch<F, K>(
    fetcher: &F,
    clock: &K,
    csv: &str,
    options: &SyncOptions,
    requests: &mut usize,
    failures: &mut usize,
) -> Result<Batch, SyncError>
where
    F: Fetcher,
    K: Clock,
{
    let solutions = request_json(
        fetcher,
        clock,
        "/api/xingce/universal/auth/solutions",
        &[("questionIds", csv.to_string()), ("type", "1".to_string())],
        options,
        requests,
        failures,
    )
    .await?;
    let metas = request_json(
        fetcher,
        clock,
        "/api/xingce/question/meta",
        &[("ids", csv.to_string())],
        options,
        requests,
        failures,
    )
    .await?;
    let keypoints = request_json(
        fetcher,
        clock,
        "/api/xingce/solution/keypoints",
        &[("ids", csv.to_string())],
        options,
        requests,
        failures,
    )
    .await?;
    let user_answers = request_json(
        fetcher,
        clock,
        "/api/xingce/user-answers",
        &[("ids", csv.to_string())],
        options,
        requests,
        failures,
    )
    .await?;
    Batch::from_responses(solutions, metas, keypoints, user_answers).map_err(SyncError::Protocol)
}

async fn request_json<F, K>(
    fetcher: &F,
    clock: &K,
    path: &str,
    extra: &[(&str, String)],
    options: &SyncOptions,
    requests: &mut usize,
    failures: &mut usize,
) -> Result<Value, SyncError>
where
    F: Fetcher,
    K: Clock,
{
    let mut query = common_params();
    query.extend(extra.iter().cloned());
    let mut last_error = String::new();
    for attempt in 1..=options.max_attempts.max(1) {
        *requests += 1;
        match fetcher.get(path, &query).await {
            Ok(FetchResponse { status: 200, body }) => {
                let value = serde_json::from_slice(&body).map_err(|error| {
                    SyncError::Protocol(format!("{path} 不是合法 JSON：{error}"))
                })?;
                clock.sleep(options.delay).await;
                return Ok(value);
            }
            Ok(FetchResponse { status, body }) => {
                if status == 401 || status == 453 {
                    return Err(SyncError::Auth(status));
                }
                *failures += 1;
                last_error = format!(
                    "HTTP {status}（{}）",
                    String::from_utf8_lossy(&body[..body.len().min(120)])
                );
            }
            Err(error) => {
                *failures += 1;
                last_error = error;
            }
        }
        if attempt < options.max_attempts {
            clock.sleep(options.delay * attempt * 2).await;
        }
    }
    Err(SyncError::Fetch(format!(
        "{path} 连续 {} 次失败：{last_error}",
        options.max_attempts.max(1)
    )))
}

fn common_params() -> Vec<(&'static str, String)> {
    [
        ("kav", "131"),
        ("av", "134"),
        ("hav", "128"),
        ("app", "web"),
        ("version", "3.0.0.0"),
        ("gav", "2"),
        ("apcId", "0"),
    ]
    .into_iter()
    .map(|(key, value)| (key, value.to_string()))
    .collect()
}

fn group_categories(nodes: &[Node], records: &[Question]) -> Result<Vec<Category>, SyncError> {
    let mut grouped: Vec<Vec<Question>> = vec![Vec::new(); nodes.len()];
    for record in records {
        let node = pick_node(record.fb, nodes)
            .ok_or_else(|| SyncError::Protocol(format!("题 {} 没有分类归属", record.fb)))?;
        grouped[node].push(record.clone());
    }
    let mut categories = Vec::new();
    for (node, questions) in nodes.iter().zip(grouped) {
        if questions.is_empty() {
            continue;
        }
        categories.push(Category {
            path: node.path.join("/"),
            questions,
        });
    }
    categories.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(categories)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use mc_core::ports::{FetchResponse, Fetcher};
    use mc_core::testing::{MemoryClock, MemoryCorpusStore, MemoryFetcher, MemoryProgress};
    use serde_json::json;

    fn response(value: Value) -> Result<FetchResponse, String> {
        Ok(FetchResponse {
            status: 200,
            body: serde_json::to_vec(&value).unwrap(),
        })
    }

    fn responses() -> Vec<Result<FetchResponse, String>> {
        vec![
            response(json!([
                {"name": "甲", "questionIds": [10], "children": []}
            ])),
            response(json!({
                "solutions": [{
                    "id": 10,
                    "type": 1,
                    "difficulty": 4,
                    "content": "<p>题干</p>",
                    "accessories": [{"options": ["甲", "乙"]}],
                    "correctAnswer": {"choice": "1"},
                    "solution": "<p>解析</p>"
                }],
                "materials": []
            })),
            response(json!([{"id": 10, "correctRatio": 50.0}])),
            response(json!([[]])),
            response(json!([{"questionId": 10, "answer": {"choice": "0"}}])),
        ]
    }

    fn options() -> SyncOptions {
        SyncOptions {
            generation: "g-test".into(),
            delay: Duration::from_millis(1),
            ..SyncOptions::default()
        }
    }

    #[test]
    fn successful_sync_stages_and_activates_snapshot() {
        block_on(async {
            let fetcher = MemoryFetcher::new(responses());
            let clock = MemoryClock::default();
            let progress = MemoryProgress::default();
            let store = MemoryCorpusStore::new(None);
            let report = run(&fetcher, &clock, &progress, &store, &options())
                .await
                .expect("抓取应成功");
            assert_eq!(report.questions, 1);
            assert_eq!(report.categories, 1);
            assert_eq!(report.batches, 1);
            let active = store.active().await.unwrap().expect("应有快照");
            assert_eq!(active.generation, "g-test");
            assert_eq!(active.corpus.total_questions(), 1);
            assert!(progress.events().contains(&"batch:1/1".to_string()));
        });
    }

    #[test]
    fn retries_then_succeeds() {
        block_on(async {
            let mut planned = responses();
            planned.insert(0, Err("临时失败".into()));
            let fetcher = MemoryFetcher::new(planned);
            let clock = MemoryClock::default();
            let progress = MemoryProgress::default();
            let store = MemoryCorpusStore::new(None);
            let mut options = options();
            options.failure_limit = 1.0;
            run(&fetcher, &clock, &progress, &store, &options)
                .await
                .expect("重试应成功");
            assert!(clock.sleeps().len() >= 2);
            assert_eq!(fetcher.calls().len(), 6);
        });
    }

    #[test]
    fn failure_rate_aborts_before_activation() {
        block_on(async {
            let mut planned = responses();
            planned.insert(0, Err("临时失败".into()));
            let fetcher = MemoryFetcher::new(planned);
            let clock = MemoryClock::default();
            let progress = MemoryProgress::default();
            let store = MemoryCorpusStore::new(None);
            let error = run(&fetcher, &clock, &progress, &store, &options())
                .await
                .expect_err("失败率超过上限应中止");
            assert!(matches!(error, SyncError::FailureRate { .. }), "{error}");
            assert!(store.active().await.unwrap().is_none());
            assert!(
                progress
                    .events()
                    .iter()
                    .any(|event| event.starts_with("error:"))
            );
        });
    }

    #[test]
    fn auth_status_stops_immediately() {
        block_on(async {
            let fetcher = MemoryFetcher::new(vec![Ok(FetchResponse {
                status: 401,
                body: Vec::new(),
            })]);
            let clock = MemoryClock::default();
            let progress = MemoryProgress::default();
            let store = MemoryCorpusStore::new(None);
            let error = run(&fetcher, &clock, &progress, &store, &options())
                .await
                .expect_err("401 应停止");
            assert_eq!(error, SyncError::Auth(401));
            assert_eq!(fetcher.calls().len(), 1);
        });
    }

    #[test]
    fn fake_fetcher_type_is_usable() {
        fn assert_fetcher<T: Fetcher>(_: &T) {}
        let fetcher = MemoryFetcher::new(Vec::new());
        assert_fetcher(&fetcher);
    }
}
