//! 抓取 member：读登录态 → 探活分流 → 全量抓取 → 写临时目录 → 整体换入 `data/`。
//!
//! 抓取路径只做只读 GET，且不读回旧 `data/`：换入即整份替换，状态另存侧车。

mod client;
mod emit;
mod htmlfmt;
mod model;
mod scalar;
mod verify;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use client::Probe;
use model::Batch;

/// 服务端明确声称的单批上限。
const BATCH_SIZE: usize = 500;
/// 单请求重试次数；超过后本次请求判失败，计入失败率。
const MAX_ATTEMPTS: u32 = 4;
/// 失败率超过此比例即中止，避免继续触碰风控限制。
const FAILURE_LIMIT: f64 = 0.05;

/// `scrape` 的参数。
pub struct Options {
    pub delay: u64,
    pub concurrency: usize,
}

pub fn run(root: &Path, options: Options) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("启动异步运行时失败：{e}"))?;
    runtime.block_on(run_async(root, options))
}

async fn run_async(root: &Path, options: Options) -> Result<(), String> {
    let started = Instant::now();
    let state_path = root.join("auth").join("storage-state.json");
    let text = std::fs::read_to_string(&state_path).map_err(|_| {
        format!(
            "读不到登录态文件：{}。请先运行：mistake-compass login",
            state_path.display()
        )
    })?;
    let cookies = client::cookies_from_state(&text)?;
    let client = client::Client::new(&cookies)?;

    match client.probe().await? {
        Probe::Valid => {}
        Probe::MissingDevice => {
            return Err("登录态缺少设备标识（453）。请先运行：mistake-compass login".to_string());
        }
        Probe::Unauthorized => {
            return Err("登录态已失效（401）。请先运行：mistake-compass login".to_string());
        }
        Probe::Unexpected(code) => {
            return Err(format!(
                "探活返回未预期状态码 {code}。请先运行：mistake-compass login"
            ));
        }
    }

    let delay = Duration::from_millis(options.delay);
    if options.concurrency > 1 {
        println!(
            "注意：并发度 {} 未做真实账号实测，风控风险自担",
            options.concurrency
        );
    }

    println!("正在抓取分类树…");
    let tree = fetch_tree(&client).await?;
    let nodes = model::build_nodes(&tree)?;
    let ids = model::collect_ids(&nodes);
    if ids.is_empty() {
        return Err("分类树里没有错题，无需抓取".to_string());
    }
    let batches_total = ids.len().div_ceil(BATCH_SIZE);
    println!("共 {} 道错题，分 {} 批抓取", ids.len(), batches_total);

    let mut batches: Vec<Batch> = Vec::with_capacity(batches_total);
    let mut requests = 0usize;
    let mut failures = 0usize;
    for (chunk_no, chunk) in ids.chunks(BATCH_SIZE).enumerate() {
        let csv = chunk
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let batch = fetch_batch(&client, &csv, delay, &mut requests, &mut failures).await?;
        batches.push(batch);
        let rate = failures as f64 / requests.max(1) as f64;
        println!(
            "第 {}/{} 批完成（失败 {failures} / {requests} 次请求）",
            chunk_no + 1,
            batches_total
        );
        if rate > FAILURE_LIMIT {
            return Err(format!(
                "失败率 {:.1}% 超过上限 {:.0}%，已中止；`data/` 保持上一次的完整快照",
                rate * 100.0,
                FAILURE_LIMIT * 100.0
            ));
        }
    }

    println!("正在装配并写盘…");
    let records = model::build_records(&ids, &batches)?;
    let staging = Staging::create(root)?;
    let files = emit::write_tree(staging.path(), &nodes, &records)?;
    verify::self_check(staging.path(), &files, records.len())?;
    let bytes: usize = files.iter().map(|file| file.bytes).sum();
    staging.commit()?;

    println!(
        "抓取完成：{} 题 / {} 个分类文件 / {:.2} MB / {:.1} s",
        records.len(),
        files.len(),
        bytes as f64 / 1_048_576.0,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

// ---------------------------------------------------------------- 抓取

async fn fetch_tree(client: &client::Client) -> Result<serde_json::Value, String> {
    let response = client
        .get(
            "/api/xingce/errors/keypoint-tree",
            &[("timeRange", "0".to_string()), ("order", "0".to_string())],
        )
        .await?;
    ensure_success(response.status, "分类树", &response.body)?;
    response.json()
}

/// 一批题的 4 个接口；全部成功才返回。
async fn fetch_batch(
    client: &client::Client,
    csv: &str,
    delay: Duration,
    requests: &mut usize,
    failures: &mut usize,
) -> Result<Batch, String> {
    let solutions = request_json(
        client,
        "/api/xingce/universal/auth/solutions",
        &[("questionIds", csv.to_string()), ("type", "1".to_string())],
        delay,
        requests,
        failures,
    )
    .await?;
    let metas = request_json(
        client,
        "/api/xingce/question/meta",
        &[("ids", csv.to_string())],
        delay,
        requests,
        failures,
    )
    .await?;
    let keypoints = request_json(
        client,
        "/api/xingce/solution/keypoints",
        &[("ids", csv.to_string())],
        delay,
        requests,
        failures,
    )
    .await?;
    let user_answers = request_json(
        client,
        "/api/xingce/user-answers",
        &[("ids", csv.to_string())],
        delay,
        requests,
        failures,
    )
    .await?;

    Batch::from_responses(solutions, metas, keypoints, user_answers)
}

/// 单接口请求：失败按指数退避重试，每次请求都计入失败率。
async fn request_json(
    client: &client::Client,
    path: &str,
    extra: &[(&str, String)],
    delay: Duration,
    requests: &mut usize,
    failures: &mut usize,
) -> Result<serde_json::Value, String> {
    let mut last_error = String::new();
    for attempt in 1..=MAX_ATTEMPTS {
        *requests += 1;
        match client.get(path, extra).await {
            Ok(response) if response.status == 200 => {
                tokio::time::sleep(delay).await;
                return response.json();
            }
            Ok(response) => {
                *failures += 1;
                if response.status == 401 || response.status == 453 {
                    return Err(auth_error(response.status));
                }
                last_error = format!("HTTP {}（{}）", response.status, snippet(&response.body));
            }
            Err(error) => {
                *failures += 1;
                last_error = error;
            }
        }
        if attempt < MAX_ATTEMPTS {
            tokio::time::sleep(delay * attempt * 2).await;
        }
    }
    Err(format!("{path} 连续 {MAX_ATTEMPTS} 次失败：{last_error}"))
}

fn auth_error(status: u16) -> String {
    if status == 453 {
        "登录态缺少设备标识（453）。请先运行：mistake-compass login".to_string()
    } else {
        "登录态已失效（401）。请先运行：mistake-compass login".to_string()
    }
}

fn ensure_success(status: u16, label: &str, body: &[u8]) -> Result<(), String> {
    if status == 200 {
        return Ok(());
    }
    if status == 401 || status == 453 {
        return Err(auth_error(status));
    }
    Err(format!("{label} 请求返回 HTTP {status}：{}", snippet(body)))
}

/// 服务端返回的正文片段：失败时这段文字往往直接说明原因，
/// 只印状态码会让人无从判断（例如容器层与应用层都会返回 400）。
fn snippet(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(&body[..body.len().min(400)]);
    let one_line: String = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let clipped: String = one_line.chars().take(200).collect();
    if clipped.is_empty() {
        "（空正文）".to_string()
    } else {
        clipped
    }
}

// ---------------------------------------------------------------- 换入

/// 临时的写盘目录；只有显式 `commit` 才会替换掉旧 `data/`。
struct Staging {
    path: PathBuf,
    target: PathBuf,
    committed: bool,
}

impl Staging {
    fn create(root: &Path) -> Result<Staging, String> {
        let target = root.join("data");
        let path = root.join("data.tmp");
        std::fs::create_dir_all(root)
            .map_err(|e| format!("创建目录 {} 失败：{e}", root.display()))?;
        if path.exists() {
            std::fs::remove_dir_all(&path)
                .map_err(|e| format!("清理残留的 {} 失败：{e}", path.display()))?;
        }
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("创建临时目录 {} 失败：{e}", path.display()))?;
        Ok(Staging {
            path,
            target,
            committed: false,
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    /// 整体换入：先把旧目录挪开，再把新目录改名到目标位置，最后删掉旧的。
    fn commit(mut self) -> Result<(), String> {
        let backup = self.target.with_file_name("data.old");
        if backup.exists() {
            std::fs::remove_dir_all(&backup)
                .map_err(|e| format!("清理残留的 {} 失败：{e}", backup.display()))?;
        }
        if self.target.exists() {
            std::fs::rename(&self.target, &backup).map_err(|e| {
                format!(
                    "挪开旧数据目录 {} 失败：{e}。数据仍是上一次的完整快照",
                    self.target.display()
                )
            })?;
        }
        if let Err(error) = std::fs::rename(&self.path, &self.target) {
            // 换入失败时把旧目录挪回去，避免留下“既没有旧数据也没有新数据”的状态。
            let _ = std::fs::rename(&backup, &self.target);
            return Err(format!("换入新数据失败：{error}。已恢复上一次的完整快照"));
        }
        if backup.exists() {
            std::fs::remove_dir_all(&backup)
                .map_err(|e| format!("删除旧数据目录 {} 失败：{e}", backup.display()))?;
        }
        self.committed = true;
        Ok(())
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("mc-scrape-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&path).ok();
        std::fs::create_dir_all(&path).expect("创建临时根目录失败");
        path
    }

    #[test]
    fn commit_replaces_previous_snapshot() {
        let root = root("commit");
        let old = root.join("data");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("旧.yaml"), "questions:\n").unwrap();

        let staging = Staging::create(&root).expect("创建临时目录应成功");
        std::fs::write(staging.path().join("新.yaml"), "questions:\n").unwrap();
        staging.commit().expect("换入应成功");

        assert!(root.join("data/新.yaml").exists());
        assert!(!root.join("data/旧.yaml").exists());
        assert!(!root.join("data.old").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn discard_keeps_previous_snapshot() {
        let root = root("discard");
        let old = root.join("data");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("旧.yaml"), "questions:\n").unwrap();

        let staging = Staging::create(&root).expect("创建临时目录应成功");
        std::fs::write(staging.path().join("半途.yaml"), "questions:\n").unwrap();
        let temp = staging.path().to_path_buf();
        drop(staging);

        assert!(!temp.exists(), "丢弃后临时目录应被清掉");
        assert!(root.join("data/旧.yaml").exists(), "旧快照应完好");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn staging_clears_leftover_temp_dir() {
        let root = root("leftover");
        let leftover = root.join("data.tmp");
        std::fs::create_dir_all(&leftover).unwrap();
        std::fs::write(leftover.join("上次的.yaml"), "questions:\n").unwrap();

        let staging = Staging::create(&root).expect("创建临时目录应成功");
        assert!(!leftover.join("上次的.yaml").exists());
        drop(staging);
        std::fs::remove_dir_all(&root).ok();
    }
}
