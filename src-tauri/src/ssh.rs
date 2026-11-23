//! SSH 连接测试（russh，纯 Rust 实现）。
use crate::model::TestResult;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub enum Auth<'a> {
    Password(&'a str),
    Key {
        path: &'a str,
        passphrase: Option<&'a str>,
    },
}

pub struct TestHandler;

#[async_trait::async_trait]
impl russh::client::Handler for TestHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // M1：信任首次连接的主机密钥；后续版本实现 known_hosts 校验与指纹确认
        Ok(true)
    }
}

pub fn build_auth<'a>(
    auth_type: &'a str,
    key_path: Option<&'a str>,
    secret: Option<&'a str>,
) -> Result<Auth<'a>, String> {
    match auth_type {
        "password" => match secret {
            Some(s) if !s.is_empty() => Ok(Auth::Password(s)),
            _ => Err("缺少密码：请先在服务器配置中填写并保存密码".into()),
        },
        "key" => match key_path {
            Some(p) if !p.is_empty() => Ok(Auth::Key {
                path: p,
                passphrase: secret.filter(|s| !s.is_empty()),
            }),
            _ => Err("缺少私钥文件路径".into()),
        },
        other => Err(format!("未知认证方式: {other}")),
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// 建立 SSH 会话并完成认证（连接测试与部署流水线共用）。
pub async fn connect_and_auth(
    host: &str,
    port: u16,
    username: &str,
    auth: Auth<'_>,
) -> Result<russh::client::Handle<TestHandler>, BoxError> {
    let config = Arc::new(russh::client::Config::default());
    let mut session = russh::client::connect(config, (host, port), TestHandler).await?;
    let authed = match auth {
        Auth::Password(pw) => session.authenticate_password(username, pw).await?,
        Auth::Key { path, passphrase } => {
            let key = russh_keys::load_secret_key(std::path::Path::new(path), passphrase)?;
            session
                .authenticate_publickey(username, Arc::new(key))
                .await?
        }
    };
    if !authed {
        return Err("认证失败：用户名、密码或私钥不正确".into());
    }
    Ok(session)
}

async fn run(host: &str, port: u16, username: &str, auth: Auth<'_>) -> Result<Option<String>, BoxError> {
    let session = connect_and_auth(host, port, username, auth).await?;

    // 附带获取系统信息（失败不影响连接测试结果）
    let mut os_info = None;
    if let Ok(mut channel) = session.channel_open_session().await {
        if channel.exec(true, "uname -sr").await.is_ok() {
            let mut buf: Vec<u8> = Vec::new();
            while let Some(msg) = channel.wait().await {
                match msg {
                    russh::ChannelMsg::Data { data } => buf.extend_from_slice(&data),
                    russh::ChannelMsg::Eof | russh::ChannelMsg::Close => break,
                    _ => {}
                }
            }
            let s = String::from_utf8_lossy(&buf).trim().to_string();
            if !s.is_empty() {
                os_info = Some(s);
            }
        }
    }

    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "test complete", "")
        .await;
    Ok(os_info)
}

/// 远程执行脚本，收集输出与退出码（部署/回滚/健康检查共用）。
pub async fn exec_script(
    session: &russh::client::Handle<TestHandler>,
    script: &str,
    timeout_secs: u64,
) -> Result<(i32, String), BoxError> {
    let fut = async {
        let mut channel = session.channel_open_session().await?;
        channel.exec(true, script).await?;
        let mut out: Vec<u8> = Vec::new();
        let mut code: i32 = -1;
        while let Some(msg) = channel.wait().await {
            match msg {
                russh::ChannelMsg::Data { data } => out.extend_from_slice(&data),
                russh::ChannelMsg::ExtendedData { data, ext: 1 } => out.extend_from_slice(&data),
                russh::ChannelMsg::ExitStatus { exit_status } => code = exit_status as i32,
                russh::ChannelMsg::Close => break,
                _ => {}
            }
        }
        Ok::<_, BoxError>((code, String::from_utf8_lossy(&out).into_owned()))
    };
    match tokio::time::timeout(Duration::from_secs(timeout_secs), fut).await {
        Ok(r) => r,
        Err(_) => Err(format!("远程脚本执行超时（{timeout_secs} 秒）").into()),
    }
}

pub async fn test_connection(host: &str, port: u16, username: &str, auth: Auth<'_>) -> TestResult {
    let start = Instant::now();
    match tokio::time::timeout(Duration::from_secs(15), run(host, port, username, auth)).await {
        Ok(Ok(os_info)) => TestResult {
            ok: true,
            latency_ms: start.elapsed().as_millis() as u64,
            os_info,
            message: "连接成功".into(),
        },
        Ok(Err(e)) => TestResult {
            ok: false,
            latency_ms: start.elapsed().as_millis() as u64,
            os_info: None,
            message: format!("{e}"),
        },
        Err(_) => TestResult {
            ok: false,
            latency_ms: start.elapsed().as_millis() as u64,
            os_info: None,
            message: "连接超时（15 秒）：请检查主机地址、端口与网络".into(),
        },
    }
}