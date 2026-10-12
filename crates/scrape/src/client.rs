//! 只读 HTTP 客户端：把错题接口的公共参数与 cookie 头收在一处。
//!
//! 服务端不校验 Origin / Referer / User-Agent（实测），鉴权只认 cookie；
//! 因此这里只带 cookie 与接口要求的公共查询参数。

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::Value;

const HOST: &str = "https://tiku.fenbi.com";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// 探活结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    Valid,
    /// 缺设备标识（服务端 453）。
    MissingDevice,
    /// 登录凭据失效（服务端 401）。
    Unauthorized,
    Unexpected(u16),
}

/// 一次请求的结果。
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Response {
    /// 状态码为 200 时解析 JSON。
    pub fn json(&self) -> Result<Value, String> {
        serde_json::from_slice(&self.body).map_err(|e| format!("响应不是合法 JSON：{e}"))
    }
}

/// 错题接口客户端。
pub struct Client {
    http: reqwest::Client,
    cookie: String,
    device_id: String,
}

impl Client {
    /// 从 storageState 的 cookie 列表构造；缺 `device_id` 时仍构造，由探活负责报错。
    pub fn new(cookies: &[(String, String)]) -> Result<Client, String> {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| format!("构造 HTTP 客户端失败：{e}"))?;
        let cookie = cookies
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        let device_id = cookies
            .iter()
            .rev()
            .find(|(name, _)| name == "device_id")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        Ok(Client {
            http,
            cookie,
            device_id,
        })
    }

    /// 探活：按状态码区分有效、缺设备标识、凭据失效。
    pub async fn probe(&self) -> Result<Probe, String> {
        let response = self
            .http
            .get("https://login.fenbi.com/api/users/current")
            .header("Cookie", &self.cookie)
            .send()
            .await
            .map_err(|e| format!("探活请求失败：{e}"))?;
        Ok(match response.status().as_u16() {
            200 => Probe::Valid,
            453 => Probe::MissingDevice,
            401 => Probe::Unauthorized,
            code => Probe::Unexpected(code),
        })
    }

    /// 打一次错题接口；`extra` 是接口自己的查询参数。
    pub async fn get(&self, path: &str, extra: &[(&str, String)]) -> Result<Response, String> {
        let mut url = reqwest::Url::parse(&format!("{HOST}{path}"))
            .map_err(|e| format!("拼接口地址失败：{e}"))?;
        {
            let mut query = url.query_pairs_mut();
            for (key, value) in common_params(&self.device_id) {
                query.append_pair(key, &value);
            }
            for (key, value) in extra {
                query.append_pair(key, value);
            }
        }
        let response = self
            .http
            .get(url)
            .header("Cookie", &self.cookie)
            .header("Accept", "application/json, text/plain, */*")
            .send()
            .await
            .map_err(|e| format!("请求失败：{e}"))?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|e| format!("读取响应正文失败：{e}"))?
            .to_vec();
        Ok(Response { status, body })
    }
}

/// 前端 SPA 给所有请求附加的公共参数（含设备标识）。
fn common_params(device_id: &str) -> Vec<(&'static str, String)> {
    vec![
        ("kav", "131".to_string()),
        ("av", "134".to_string()),
        ("hav", "128".to_string()),
        ("app", "web".to_string()),
        ("version", "3.0.0.0".to_string()),
        ("gav", "2".to_string()),
        ("apcId", "0".to_string()),
        ("deviceId", device_id.to_string()),
    ]
}

/// 从 storageState 里取 cookie；同名取靠后的一条，与浏览器“后者生效”一致。
///
/// 这里不做筛选：登录态文件由 `login` 负责写干净，抓取侧只如实转发文件内容。
/// 请求头一旦超出服务端上限，由请求失败照常报错。
pub fn cookies_from_state(text: &str) -> Result<Vec<(String, String)>, String> {
    let state: Value =
        serde_json::from_str(text).map_err(|e| format!("登录态文件不是合法 JSON：{e}"))?;
    let list = state
        .get("cookies")
        .and_then(Value::as_array)
        .ok_or_else(|| "登录态文件里没有 cookies".to_string())?;
    let mut cookies: BTreeMap<String, String> = BTreeMap::new();
    for cookie in list {
        let Some(name) = cookie.get("name").and_then(Value::as_str) else {
            continue;
        };
        let value = cookie
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !name.is_empty() {
            cookies.insert(name.to_string(), value.to_string());
        }
    }
    Ok(cookies.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_duplicate_cookie_wins() {
        let state = r#"{"cookies":[
            {"name":"device_id","value":"first"},
            {"name":"device_id","value":"second"}
        ]}"#;
        let cookies = cookies_from_state(state).expect("应能读出 cookie");
        assert_eq!(
            cookies,
            vec![("device_id".to_string(), "second".to_string())]
        );
    }

    #[test]
    fn state_without_cookies_is_rejected() {
        assert!(cookies_from_state("{}").is_err());
    }

    #[test]
    fn every_cookie_in_the_state_file_is_forwarded() {
        // 登录态文件由 login 写干净；抓取侧不做筛选，只如实转发。
        let state = r#"{"cookies":[
            {"name":"device_id","value":"v1_x"},
            {"name":"persistent","value":"p"},
            {"name":"SERVERID","value":"lb"}
        ]}"#;
        let cookies = cookies_from_state(state).expect("应能读出 cookie");
        // 结果按键排序（BTreeMap），顺序本身不影响请求头语义。
        assert_eq!(
            cookies,
            vec![
                ("SERVERID".to_string(), "lb".to_string()),
                ("device_id".to_string(), "v1_x".to_string()),
                ("persistent".to_string(), "p".to_string()),
            ]
        );
    }

    #[test]
    fn common_params_carry_device_id() {
        let params = common_params("v1_abc");
        assert!(params.contains(&("deviceId", "v1_abc".to_string())));
    }
}
