//! 登录 member：拉起可见浏览器让用户自己登录，再用 CDP 读出 cookie 写成 storageState。
//!
//! 登录态文件是与其他 member 之间唯一的交接物，因此这里只负责把浏览器里的 cookie
//! 如实落盘，不做任何续期、合并或过滤。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::network::Cookie;
use futures::StreamExt;

/// 人工登录时打开的页面。
const PORTAL: &str = "https://www.fenbi.com/";
/// 探活接口：按状态码区分“有效 / 凭据失效 / 缺设备标识”。
const PROBE: &str = "https://login.fenbi.com/api/users/current";
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const STEP_TIMEOUT: Duration = Duration::from_secs(60);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(120);

/// `login` 的参数。
pub struct Options {
    /// 现存登录态仍有效时也强制重登。
    pub force: bool,
    /// 等待人工登录的秒数上限。
    pub timeout: u64,
}

pub fn run(root: &Path, options: Options) -> Result<(), String> {
    let state_path = root.join("auth").join("storage-state.json");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("启动异步运行时失败：{e}"))?;
    runtime.block_on(run_async(&state_path, options))
}

async fn run_async(state_path: &Path, options: Options) -> Result<(), String> {
    if !options.force {
        match probe_existing(state_path).await {
            Some(Outcome::Valid) => {
                println!("登录态仍有效，跳过登录（用 --force 可强制重新登录）");
                return Ok(());
            }
            Some(Outcome::MissingDevice) => {
                println!("现有登录态缺少设备标识（HTTP 453），需要重新登录");
            }
            Some(Outcome::Unauthorized) => {
                println!("现有登录态已失效（HTTP 401），需要重新登录");
            }
            Some(Outcome::Unexpected(code)) => {
                println!("现有登录态探活返回 HTTP {code}，继续重新登录");
            }
            Some(Outcome::Failed(reason)) => {
                println!("现有登录态探活失败（{reason}），继续重新登录");
            }
            None => println!("还没有登录态文件，开始登录"),
        }
    }

    let edge = find_browser()?;
    let profile = TempProfile::new()?;
    let cookies = capture_cookies(&edge, profile.path(), options.timeout).await?;
    profile.remove();
    let (cookies, skipped) = retain_auth_cookies(cookies);
    write_state(state_path, &cookies)?;
    if skipped > 0 {
        println!("已忽略 {skipped} 条与鉴权无关的 cookie");
    }
    println!(
        "已写出 {} 条 cookie 到 {}",
        cookies.len(),
        state_path.display()
    );
    Ok(())
}

// ---------------------------------------------------------------- 探活

/// 现有登录态的探活结果。
enum Outcome {
    Valid,
    MissingDevice,
    Unauthorized,
    Unexpected(u16),
    Failed(String),
}

/// 登录态文件不存在时返回 `None`；存在就按服务端状态码分流。
async fn probe_existing(state_path: &Path) -> Option<Outcome> {
    let text = std::fs::read_to_string(state_path).ok()?;
    let state: serde_json::Value = serde_json::from_str(&text).ok()?;
    let header = cookie_header(&state)?;
    if header.is_empty() {
        return Some(Outcome::Unauthorized);
    }
    let client = match reqwest::Client::builder().timeout(STEP_TIMEOUT).build() {
        Ok(client) => client,
        Err(error) => return Some(Outcome::Failed(error.to_string())),
    };
    match client.get(PROBE).header("Cookie", header).send().await {
        Ok(response) => Some(match response.status().as_u16() {
            200 => Outcome::Valid,
            453 => Outcome::MissingDevice,
            401 => Outcome::Unauthorized,
            code => Outcome::Unexpected(code),
        }),
        Err(error) => Some(Outcome::Failed(error.to_string())),
    }
}

/// 把 storageState 里的 cookie 拼成一个请求头；没有 cookie 时返回空串。
fn cookie_header(state: &serde_json::Value) -> Option<String> {
    let list = state.get("cookies")?.as_array()?;
    let pairs: Vec<String> = list
        .iter()
        .filter_map(|cookie| {
            let name = cookie.get("name")?.as_str()?;
            let value = cookie.get("value")?.as_str()?;
            (!name.is_empty()).then(|| format!("{name}={value}"))
        })
        .collect();
    Some(pairs.join("; "))
}

// ---------------------------------------------------------------- 登录

/// 拉起可见浏览器、打开粉笔、轮询到登录态后读出全部 cookie。
async fn capture_cookies(
    edge: &Path,
    profile: &Path,
    timeout_secs: u64,
) -> Result<Vec<Cookie>, String> {
    let config = BrowserConfig::builder()
        .chrome_executable(edge)
        .user_data_dir(profile)
        .with_head()
        .window_size(1280, 900)
        // 默认会注入 800x600 的视口覆盖，页面缩在窗口一角，登录按钮点不到。
        .viewport(None)
        .build()
        .map_err(|e| format!("构建浏览器配置失败：{e}"))?;

    let (browser, mut handler) = tokio::time::timeout(LAUNCH_TIMEOUT, Browser::launch(config))
        .await
        .map_err(|_| format!("启动浏览器超时（{} 秒）", LAUNCH_TIMEOUT.as_secs()))?
        .map_err(|e| format!("启动浏览器失败：{e}"))?;
    let driver = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    println!("已打开浏览器，请在窗口里完成登录；最长等待 {timeout_secs} 秒");
    let result = wait_for_login(&browser, timeout_secs).await;
    let cookies = match result {
        Ok(cookies) => cookies,
        Err(error) => {
            close(browser).await;
            let _ = tokio::time::timeout(Duration::from_secs(5), driver).await;
            return Err(error);
        }
    };

    // 先关浏览器再导出：证书与密钥的进程锁已释放。
    close(browser).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), driver).await;
    Ok(cookies)
}

async fn wait_for_login(browser: &Browser, timeout_secs: u64) -> Result<Vec<Cookie>, String> {
    browser
        .new_page(PORTAL)
        .await
        .map_err(|e| format!("打开 {PORTAL} 失败：{e}"))?;
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let cookies = browser
            .get_cookies()
            .await
            .map_err(|e| format!("读取浏览器 cookie 失败（浏览器可能已被关闭）：{e}"))?;
        if is_authenticated(&cookies) {
            println!("检测到登录态（device_id + persistent/sess）");
            return Ok(cookies);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "等待 {timeout_secs} 秒仍未检测到登录态，未写出登录态文件"
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// 鉴权需要设备标识，外加长期凭据与会话凭据二选一。
fn is_authenticated(cookies: &[Cookie]) -> bool {
    let pairs: Vec<(&str, &str)> = cookies
        .iter()
        .map(|cookie| (cookie.name.as_str(), cookie.value.as_str()))
        .collect();
    has_credentials(&pairs)
}

/// 参与鉴权的 cookie。
///
/// 登录态文件只存这几个：浏览器（尤其 Edge）会把自家遥测与首启动页的 cookie
/// 一并写进 cookie 库，实测一次登录能带出二十多条与粉笔无关的条目。这些条目对鉴权无用，
/// 却会让后续请求头膨胀到服务端拒收，因此在这里就挡掉，而不是留到发送时再过滤。
const AUTH_COOKIES: [&str; 5] = ["device_id", "persistent", "sess", "userid", "sid"];

/// 只保留鉴权 cookie，返回（保留下来的、被忽略的条数）。
///
/// 同名 cookie 可能出现在不同域下，按下发顺序只保留最后一条，
/// 与浏览器“后者生效”的读法一致。
fn retain_auth_cookies(cookies: Vec<Cookie>) -> (Vec<Cookie>, usize) {
    let total = cookies.len();
    let mut kept: Vec<Cookie> = Vec::with_capacity(AUTH_COOKIES.len());
    for cookie in cookies {
        if !is_auth_cookie(&cookie.name) {
            continue;
        }
        // 同名去重：后出现的替换先前的。
        match kept.iter().position(|item| item.name == cookie.name) {
            Some(at) => kept[at] = cookie,
            None => kept.push(cookie),
        }
    }
    let skipped = total - kept.len();
    (kept, skipped)
}

/// 是否是参与鉴权的 cookie。
fn is_auth_cookie(name: &str) -> bool {
    AUTH_COOKIES.contains(&name)
}

/// 判定一组 cookie 是否构成有效登录态。
fn has_credentials(cookies: &[(&str, &str)]) -> bool {
    let present = |name: &str| {
        cookies
            .iter()
            .any(|(key, value)| *key == name && !value.is_empty())
    };
    present("device_id") && (present("persistent") || present("sess"))
}

async fn close(mut browser: Browser) {
    if let Err(error) = browser.close().await {
        eprintln!("关闭浏览器失败（可能已退出）：{error}");
    }
}

// ---------------------------------------------------------------- 收尾

/// 原子写出登录态：先写同目录临时文件，再整体替换。
fn write_state(path: &Path, cookies: &[Cookie]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录 {} 失败：{e}", parent.display()))?;
    }
    // origins 恒为空：鉴权只依赖 cookie。
    let doc = serde_json::json!({ "cookies": cookies, "origins": [] });
    let text = serde_json::to_vec_pretty(&doc).map_err(|e| format!("序列化登录态失败：{e}"))?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, &text).map_err(|e| format!("写 {} 失败：{e}", temp.display()))?;
    std::fs::rename(&temp, path).map_err(|e| format!("替换 {} 失败：{e}", path.display()))?;
    Ok(())
}

/// 用完即弃的浏览器 profile 目录；离开作用域或显式调用都会删除。
struct TempProfile {
    path: PathBuf,
}

impl TempProfile {
    fn new() -> Result<TempProfile, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            "mistake-compass-login-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("创建临时 profile {} 失败：{e}", path.display()))?;
        Ok(TempProfile { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn remove(&self) {
        if let Err(error) = std::fs::remove_dir_all(&self.path) {
            eprintln!("清理临时 profile {} 失败：{error}", self.path.display());
        }
    }
}

impl Drop for TempProfile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 优先用本机实测过的安装路径，退回 chromiumoxide 自己的探测。
fn find_browser() -> Result<PathBuf, String> {
    for candidate in bundled_candidates() {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    match chromiumoxide::detection::default_executable(
        chromiumoxide::detection::DetectionOptions::default(),
    ) {
        Ok(path) if path.exists() => Ok(path),
        _ => Err(format!(
            "找不到 Edge / Chrome，已试过：{}。请安装 Edge 后重试",
            bundled_candidates()
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("、")
        )),
    }
}

fn bundled_candidates() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "macos")]
    {
        paths.push("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge".into());
        paths.push("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into());
    }
    #[cfg(target_os = "windows")]
    {
        paths.push(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe".into());
        paths.push(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe".into());
    }
    #[cfg(target_os = "linux")]
    {
        paths.push("/usr/bin/microsoft-edge".into());
        paths.push("/usr/bin/google-chrome".into());
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_alone_is_not_enough() {
        assert!(!has_credentials(&[("device_id", "v1_x")]));
    }

    #[test]
    fn persistent_or_sess_pairs_with_device_id() {
        assert!(has_credentials(&[
            ("device_id", "v1_x"),
            ("persistent", "p")
        ]));
        assert!(has_credentials(&[("device_id", "v1_x"), ("sess", "s")]));
    }

    #[test]
    fn empty_values_do_not_count() {
        assert!(!has_credentials(&[
            ("device_id", "v1_x"),
            ("persistent", "")
        ]));
        assert!(!has_credentials(&[("device_id", ""), ("persistent", "p")]));
    }

    #[test]
    fn cookie_header_joins_name_value_pairs() {
        let state = serde_json::json!({
            "cookies": [
                {"name": "device_id", "value": "v1_x"},
                {"name": "persistent", "value": "p"},
            ],
            "origins": [],
        });
        assert_eq!(
            cookie_header(&state).as_deref(),
            Some("device_id=v1_x; persistent=p")
        );
    }

    #[test]
    fn only_auth_cookies_are_kept() {
        // 实测：Edge 会写二十多条 msn.cn 之类的无关 cookie，这些 cookie 不该进登录态文件。
        for name in AUTH_COOKIES {
            assert!(is_auth_cookie(name), "{name} 应被保留");
        }
        for name in [
            "sensorsdata2015jssdkcross",
            "App-User",
            "MUID",
            "MUIDB",
            "_EDGE_S",
            "_EDGE_V",
            "_C_Auth",
            "EdgeAnidEligible",
            "USRLOC",
            "__rubyUX",
            "msnup",
            "pglt-edgeChromium-dhp",
            "MicrosoftApplicationsTelemetryDeviceId",
            "ai_session",
            "acw_tc",
            "SERVERID",
            "SERVERCORSID",
            "sajssdk_2015_cross_new_user",
            "Hm_lvt_e7351028cde0d0ccb9ccdbe5fe531683",
        ] {
            assert!(!is_auth_cookie(name), "{name} 应被忽略");
        }
    }

    /// 这份名单取自 2026-10-12 真实登录态里出现过的全部无关 cookie：
    /// 只要有一个被误判成“保留”，登录态文件就会重新带上噪声。
    #[test]
    fn every_unrelated_cookie_from_a_real_state_is_rejected() {
        let real = [
            "App-User",
            "EdgeAnidEligible",
            "MUID",
            "MUIDB",
            "MicrosoftApplicationsTelemetryDeviceId",
            "SERVERCORSID",
            "SERVERID",
            "USRLOC",
            "_C_Auth",
            "_EDGE_S",
            "_EDGE_V",
            "__rubyUX",
            "acw_tc",
            "ai_session",
            "msnup",
            "pglt-edgeChromium-dhp",
            "sajssdk_2015_cross_new_user",
            "sensorsdata2015jssdkcross",
        ];
        let kept: Vec<&str> = real
            .iter()
            .copied()
            .filter(|name| is_auth_cookie(name))
            .collect();
        assert!(kept.is_empty(), "这些都不该被保留，却被保留：{kept:?}");
    }
}
