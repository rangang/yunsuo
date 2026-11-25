//! 宝塔面板 HTTP API 客户端。
//! 认证：request_token = md5(request_time + md5(api_sk))，见 docs.bt.cn/api。

use crate::model::TestResult;
use std::time::{Duration, Instant};

pub struct BtClient {
    base: String,
    sk: String,
    http: reqwest::Client,
}

/// 取 panel_url 的 origin 作为 API 基址（安全入口路径只拦页面登录，API 走根路由）
pub fn normalize_base(panel_url: &str) -> Result<String, String> {
    let u = reqwest::Url::parse(panel_url.trim())
        .map_err(|_| "面板地址无效，请填写完整 URL（如 https://1.2.3.4:8888）".to_string())?;
    if u.scheme() != "http" && u.scheme() != "https" {
        return Err("面板地址必须以 http:// 或 https:// 开头".into());
    }
    let host = u.host_str().ok_or("面板地址缺少主机名")?;
    Ok(match u.port() {
        Some(p) => format!("{}://{}:{}", u.scheme(), host, p),
        None => format!("{}://{}", u.scheme(), host),
    })
}

pub fn sign(sk: &str, time: i64) -> String {
    let sk_md5 = format!("{:x}", md5::compute(sk));
    format!("{:x}", md5::compute(format!("{time}{sk_md5}")))
}

pub fn friendly(msg: &str) -> String {
    if msg.contains("白名单") || msg.contains("IP") {
        format!("{msg}（请到 面板设置 → API 接口，将本机公网 IP 加入白名单）")
    } else if msg.contains("密钥") || msg.contains("token") || msg.contains("签名") || msg.contains("时间") {
        format!("{msg}（请检查 API 密钥，且本机时间与服务器误差小于 60 秒）")
    } else {
        msg.to_string()
    }
}

fn net_err(e: reqwest::Error) -> String {
    let s = e.to_string();
    if s.contains("certificate") {
        format!("TLS 证书校验失败：如面板使用自签证书，请在服务器配置勾选「允许自签证书」（{s}）")
    } else if e.is_connect() {
        format!("无法连接面板：请检查面板地址、端口与防火墙（{s}）")
    } else if e.is_timeout() {
        "面板请求超时".into()
    } else {
        format!("面板请求失败: {s}")
    }
}

impl BtClient {
    pub fn new(panel_url: &str, api_sk: &str, tls_insecure: bool) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(tls_insecure)
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { base: normalize_base(panel_url)?, sk: api_sk.to_string(), http })
    }

    async fn call(&self, module: &str, params: Vec<(&str, String)>) -> Result<serde_json::Value, String> {
        let time = chrono::Utc::now().timestamp();
        let mut form: Vec<(String, String)> =
            params.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        form.push(("request_time".into(), time.to_string()));
        form.push(("request_token".into(), sign(&self.sk, time)));
        let resp = self.http
            .post(format!("{}/{}", self.base, module))
            .form(&form)
            .send().await.map_err(net_err)?;
        let text = resp.text().await.map_err(net_err)?;
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
            let head: String = text.chars().take(120).collect();
            format!("面板返回非 JSON（地址错误或 API 接口未开启？）: {head}")
        })?;
        if v["status"].as_bool() == Some(false) {
            return Err(friendly(v["msg"].as_str().unwrap_or("未知错误")));
        }
        Ok(v)
    }

    pub async fn test_connection(&self) -> TestResult {
        let start = Instant::now();
        match self.call("system", vec![("action", "GetSystemTotal".into())]).await {
            Ok(v) => {
                let version = v["version"].as_str().unwrap_or("未知版本");
                let system = v["system"].as_str().unwrap_or("");
                TestResult {
                    ok: true,
                    latency_ms: start.elapsed().as_millis() as u64,
                    os_info: Some(format!("宝塔 {version} · {system}")),
                    message: "连接成功".into(),
                }
            }
            Err(e) => TestResult {
                ok: false,
                latency_ms: start.elapsed().as_millis() as u64,
                os_info: None,
                message: e,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_known_vector() {
        assert_eq!(sign("abc", 1700000000), "54579377c7f605679096c9e16d049010");
        assert_eq!(sign("", 0), "7aac62340a45d98746c7013bf7043be9");
    }

    #[test]
    fn normalize_bases() {
        assert_eq!(normalize_base("https://1.2.3.4:8888/").unwrap(), "https://1.2.3.4:8888");
        assert_eq!(normalize_base("https://1.2.3.4:8888/abc123").unwrap(), "https://1.2.3.4:8888");
        assert_eq!(normalize_base("http://panel.example.com").unwrap(), "http://panel.example.com");
        assert!(normalize_base("ftp://x").is_err());
        assert!(normalize_base("1.2.3.4:8888").is_err());
    }

    #[test]
    fn friendly_hints() {
        assert!(friendly("您的IP不在白名单中").contains("白名单"));
        assert!(friendly("签名校验失败").contains("60 秒"));
        assert_eq!(friendly("站点不存在"), "站点不存在");
    }
}
