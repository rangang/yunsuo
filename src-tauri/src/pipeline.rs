//! 部署流水线引擎（M3：prepare → build → package → connect → upload → deploy → verify）。
//! 每步状态 / 日志 / 进度通过 Tauri event 推给前端；日志落盘 + 历史入库；支持取消与回滚。
use crate::model::{DeployProfile, Project, Server};
use crate::{builder, scripts, ssh, store};
use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone)]
pub struct DeployControl {
    pub cancel: Arc<AtomicBool>,
    pub child_pid: Arc<Mutex<Option<u32>>>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeployEvent {
    Step { project_id: i64, step: String, status: String, detail: Option<String> },
    Log { project_id: i64, line: String, level: String },
    Progress { project_id: i64, uploaded: u64, total: u64, speed_bps: u64 },
    Finished { project_id: i64, ok: bool, cancelled: bool, message: String, duration_ms: u64 },
}

const EVENT: &str = "deploy-event";
const STEP_ORDER: [&str; 8] = ["prepare", "build", "package", "connect", "upload", "deploy", "verify", "cleanup"];

/// 从设置表读取的流水线行为参数
pub struct RunSettings {
    pub ssh_timeout: u64,
    pub upload_retry: u64,
    pub auto_health_check: bool,
    pub log_masking: bool,
    pub quiet_start: u32,
    pub quiet_end: u32,
    pub maven_path: String,
}

impl RunSettings {
    pub fn load(conn: &Connection) -> Self {
        Self {
            ssh_timeout: store::get_setting(conn, "ssh_timeout", "10").parse().unwrap_or(10),
            upload_retry: store::get_setting(conn, "upload_retry", "2").parse().unwrap_or(2),
            auto_health_check: store::get_setting(conn, "auto_health_check", "1") == "1",
            log_masking: store::get_setting(conn, "log_masking", "1") == "1",
            quiet_start: store::get_setting(conn, "quiet_start", "0").parse().unwrap_or(0),
            quiet_end: store::get_setting(conn, "quiet_end", "0").parse().unwrap_or(0),
            maven_path: store::get_setting(conn, "maven_path", ""),
        }
    }
    fn in_quiet_hours(&self) -> bool {
        if self.quiet_start == self.quiet_end {
            return false;
        }
        let h = chrono::Local::now().format("%H").to_string().parse::<u32>().unwrap_or(0);
        if self.quiet_start < self.quiet_end {
            h >= self.quiet_start && h < self.quiet_end
        } else {
            h >= self.quiet_start || h < self.quiet_end
        }
    }
}

/// 事件广播 + 日志收集（落盘用，敏感信息脱敏）
struct Ctx {
    app: AppHandle,
    pid: i64,
    lines: Mutex<Vec<String>>,
    mask: Option<String>,
}

impl Ctx {
    fn new(app: AppHandle, pid: i64, mask: Option<String>) -> Self {
        Self { app, pid, lines: Mutex::new(Vec::new()), mask }
    }
    fn step(&self, name: &str, status: &str, detail: Option<String>) {
        let _ = self.app.emit(EVENT, DeployEvent::Step {
            project_id: self.pid, step: name.into(), status: status.into(), detail,
        });
    }
    fn log(&self, line: impl Into<String>, level: &str) {
        let mut line = line.into();
        if let Some(secret) = &self.mask {
            if !secret.is_empty() && line.contains(secret.as_str()) {
                line = line.replace(secret.as_str(), "***");
            }
        }
        let ts = chrono::Local::now().format("%H:%M:%S");
        self.lines.lock().unwrap().push(format!("[{ts}] {line}"));
        let _ = self.app.emit(EVENT, DeployEvent::Log { project_id: self.pid, line, level: level.into() });
    }
    fn progress(&self, uploaded: u64, total: u64, speed_bps: u64) {
        let _ = self.app.emit(EVENT, DeployEvent::Progress { project_id: self.pid, uploaded, total, speed_bps });
    }
}

#[async_trait::async_trait]
trait Transport: Send + Sync {
    async fn exec(&self, ctx: &Ctx, script: &str, timeout_secs: u64, cancel: &AtomicBool) -> Result<(i32, String), String>;
    async fn upload(&self, ctx: &Ctx, local_file: &Path, upload_name: &str, remote_dir: &str, ts: &str, total: u64, cancel: &Arc<AtomicBool>) -> Result<String, String>;
    async fn close(&self);
}

struct SshTransport {
    session: russh::client::Handle<ssh::TestHandler>,
}

#[async_trait::async_trait]
impl Transport for SshTransport {
    async fn exec(&self, _ctx: &Ctx, script: &str, timeout_secs: u64, _cancel: &AtomicBool) -> Result<(i32, String), String> {
        ssh::exec_script(&self.session, script, timeout_secs).await.map_err(|e| e.to_string())
    }
    async fn upload(&self, ctx: &Ctx, local_file: &Path, upload_name: &str, remote_dir: &str, ts: &str, total: u64, cancel: &Arc<AtomicBool>) -> Result<String, String> {
        upload_sftp(ctx, &self.session, local_file, upload_name, remote_dir, ts, total, cancel).await
    }
    async fn close(&self) {
        let _ = self.session.disconnect(russh::Disconnect::ByApplication, "finished", "").await;
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) { Err("已取消".into()) } else { Ok(()) }
}

fn mb(bytes: u64) -> String { format!("{:.1} MB", bytes as f64 / 1048576.0) }

fn step_index(name: &str) -> usize {
    STEP_ORDER.iter().position(|s| *s == name).unwrap_or(0)
}

/// 收尾：写日志文件 + 历史记录 + 桌面通知 + Finished 事件
#[allow(clippy::too_many_arguments)]
fn finish(
    ctx: &Ctx,
    db: &Arc<Mutex<Connection>>,
    log_dir: &Path,
    project_id: i64,
    project_name: &str,
    server_id: i64,
    env_name: &str,
    kind: &str,
    started_at: &str,
    start: Instant,
    result: Result<String, String>,
    current_step: &str,
    log_name: &str,
    backup_ref: Option<&str>,
    remote_dir: &str,
) {
    let duration_ms = start.elapsed().as_millis() as u64;
    let (ok, cancelled, message) = match result {
        Ok(msg) => (true, false, msg),
        Err(e) if e == "已取消" => (false, true, "任务已取消".to_string()),
        Err(e) => (false, false, e),
    };
    let log_file = format!("{log_name}.log");
    let _ = std::fs::write(log_dir.join(&log_file), ctx.lines.lock().unwrap().join("\n"));
    let status = if ok { "success" } else if cancelled { "cancelled" } else { "failed" };
    let failed_step = if ok || cancelled { None } else { Some(current_step) };
    let notify_on = {
        let conn = db.lock().unwrap();
        store::insert_record(
            &conn, project_id, server_id, env_name, kind, status, started_at,
            duration_ms as i64, failed_step, &message, &log_file, backup_ref, remote_dir,
        ).ok();
        let key = if ok { "notify_success" } else { "notify_failure" };
        let settings = RunSettings::load(&conn);
        !cancelled && !settings.in_quiet_hours() && store::get_setting(&conn, key, "1") == "1"
    };
    if notify_on {
        use tauri_plugin_notification::NotificationExt;
        let action = if kind == "rollback" { "回滚" } else { "部署" };
        let title = format!("{project_name} {action}{}", if ok { "成功" } else { "失败" });
        let _ = ctx.app.notification().builder()
            .title(title)
            .body(format!("{message}（耗时 {} 秒）", duration_ms / 1000))
            .show();
    }
    ctx.log(message.clone(), if ok { "ok" } else if cancelled { "stderr" } else { "error" });
    let _ = ctx.app.emit(EVENT, DeployEvent::Finished {
        project_id, ok, cancelled, message, duration_ms,
    });
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    app: AppHandle,
    db: Arc<Mutex<Connection>>,
    log_dir: PathBuf,
    project: Project,
    server: Server,
    secret: Option<String>,
    profile: DeployProfile,
    cancel: Arc<AtomicBool>,
    child_pid: Arc<Mutex<Option<u32>>>,
) {
    let ctx = Ctx::new(app, project.id, None);
    let start = Instant::now();
    let started_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let settings = RunSettings::load(&db.lock().unwrap());
    let ctx = Ctx { mask: if settings.log_masking { secret.clone() } else { None }, ..ctx };
    let mut current_step = "prepare".to_string();
    let result = run_inner(
        &ctx, &project, &server, secret.as_deref(), &profile, &cancel, &child_pid, &ts, &mut current_step, &settings,
    ).await;
    let backup_ref = if step_index(&current_step) >= step_index("deploy") { Some(ts.as_str()) } else { None };
    finish(
        &ctx, &db, &log_dir, project.id, &project.name, server.id, &profile.env_name,
        "deploy", &started_at, start, result, &current_step, &format!("deploy_{ts}"), backup_ref, &profile.remote_dir,
    );
}

#[allow(clippy::too_many_arguments)]
async fn run_inner(
    ctx: &Ctx,
    project: &Project,
    server: &Server,
    secret: Option<&str>,
    profile: &DeployProfile,
    cancel: &Arc<AtomicBool>,
    child_pid: &Arc<Mutex<Option<u32>>>,
    ts: &str,
    current_step: &mut String,
    settings: &RunSettings,
) -> Result<String, String> {
    let local_dir = PathBuf::from(&project.local_path);
    let remote_dir = &profile.remote_dir;

    // ---- 1. prepare ----
    ctx.step("prepare", "running", None);
    if !local_dir.is_dir() {
        ctx.step("prepare", "failed", None);
        return Err(format!("项目路径不存在: {}", project.local_path));
    }
    ctx.step("prepare", "done", None);
    check_cancel(cancel)?;

    // ---- 2. build ----
    *current_step = "build".into();
    ctx.step("build", "running", None);
    if let Some(hook) = project.pre_build_command.as_deref().filter(|s| !s.trim().is_empty()) {
        let (hook_cmd, _) = builder::prepare_command(hook, &settings.maven_path).map_err(|e| {
            ctx.step("build", "failed", None);
            e
        })?;
        ctx.log(format!("▶ 构建前钩子: {hook_cmd}"), "info");
        let code = builder::run_streaming(&local_dir, &hook_cmd, cancel.clone(), child_pid.clone(), |line, err| {
            ctx.log(line, if err { "stderr" } else { "stdout" });
        }).await?;
        if code != 0 {
            ctx.step("build", "failed", Some(format!("退出码 {code}")));
            return Err(format!("构建前钩子失败，退出码 {code}"));
        }
    }
    check_cancel(cancel)?;
    let (build_cmd, resolved_maven) = builder::prepare_command(&project.build_command, &settings.maven_path).map_err(|e| {
        ctx.step("build", "failed", None);
        e
    })?;
    if let Some(p) = resolved_maven {
        ctx.log(format!("已定位 Maven: {p}"), "info");
    }
    ctx.log(format!("▶ 构建命令: {build_cmd}"), "info");
    let build_start = Instant::now();
    let code = builder::run_streaming(&local_dir, &build_cmd, cancel.clone(), child_pid.clone(), |line, err| {
        ctx.log(line, if err { "stderr" } else { "stdout" });
    }).await?;
    if code != 0 {
        ctx.step("build", "failed", Some(format!("退出码 {code}")));
        return Err(format!("构建失败，退出码 {code}"));
    }
    ctx.step("build", "done", Some(format!("{:.1}s", build_start.elapsed().as_secs_f64())));
    check_cancel(cancel)?;

    // ---- 3. package ----
    *current_step = "package".into();
    ctx.step("package", "running", None);
    let tmp_dir = std::env::temp_dir().join("deploypilot").join(ts);
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let (artifact_file, upload_name) = if project.kind == "frontend" {
        let dist = local_dir.join(&project.artifact_path);
        if !dist.is_dir() {
            ctx.step("package", "failed", None);
            return Err(format!("产物目录不存在: {}（构建是否成功？）", dist.display()));
        }
        let zip_path = tmp_dir.join("dist.zip");
        let (dist2, zip2) = (dist.clone(), zip_path.clone());
        let size = tokio::task::spawn_blocking(move || zip_dir(&dist2, &zip2))
            .await.map_err(|e| e.to_string())??;
        ctx.log(format!("dist → dist.zip（{}）", mb(size)), "info");
        (zip_path, "dist.zip".to_string())
    } else {
        let pattern = local_dir.join(&project.artifact_path).to_string_lossy().replace('\\', "/");
        let mut candidates: Vec<PathBuf> = glob::glob(&pattern)
            .map_err(|e| format!("产物路径模式无效: {e}"))?
            .filter_map(|r| r.ok())
            .filter(|p| {
                let n = p.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
                p.is_file() && !n.contains("sources") && !n.contains("javadoc")
                    && !n.starts_with("original") && !n.ends_with(".plain.jar")
            })
            .collect();
        candidates.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        let jar = candidates.pop().ok_or_else(|| format!("未找到产物 jar（模式: {pattern}）"))?;
        let name = jar.file_name().unwrap().to_string_lossy().to_string();
        let size = std::fs::metadata(&jar).map_err(|e| e.to_string())?.len();
        ctx.log(format!("定位产物 {name}（{}）", mb(size)), "info");
        (jar, name)
    };
    let total = std::fs::metadata(&artifact_file).map_err(|e| e.to_string())?.len();
    ctx.step("package", "done", Some(mb(total)));
    check_cancel(cancel)?;

    // ---- 4. connect ----
    *current_step = "connect".into();
    ctx.step("connect", "running", None);
    ctx.log(format!("▶ 连接服务器 {}@{}:{}", server.username, server.host, server.port), "info");
    let auth = ssh::build_auth(&server.auth_type, server.key_path.as_deref(), secret)?;
    let session = tokio::time::timeout(
        std::time::Duration::from_secs(settings.ssh_timeout),
        ssh::connect_and_auth(&server.host, server.port, &server.username, auth),
    )
    .await
    .map_err(|_| format!("SSH 连接超时（{} 秒）", settings.ssh_timeout))
    .and_then(|r| r.map_err(|e| format!("SSH 连接失败: {e}")))
    .map_err(|e| {
        ctx.step("connect", "failed", None);
        e
    })?;
    ctx.step("connect", "done", None);
    let transport = SshTransport { session };

    // connect 之后的所有步骤共用会话，结束时统一断开
    let result = async {
        check_cancel(cancel)?;
        // ---- 5. upload ----
        *current_step = "upload".into();
        ctx.step("upload", "running", None);
        let mut attempt = 0u64;
        let remote_path = loop {
            match transport.upload(ctx, &artifact_file, &upload_name, remote_dir, ts, total, cancel).await {
                Ok(p) => break p,
                Err(e) if e == "已取消" => return Err(e),
                Err(e) => {
                    if attempt >= settings.upload_retry {
                        ctx.step("upload", "failed", None);
                        return Err(format!("上传失败（已重试 {attempt} 次）: {e}"));
                    }
                    attempt += 1;
                    ctx.log(format!("上传中断: {e} — 第 {attempt}/{} 次重试…", settings.upload_retry), "stderr");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    check_cancel(cancel)?;
                }
            }
        };
        ctx.step("upload", "done", Some(mb(total)));
        ctx.log(format!("上传完成，大小校验一致: {remote_path}"), "info");
        check_cancel(cancel)?;

        // ---- 6. deploy ----
        *current_step = "deploy".into();
        ctx.step("deploy", "running", None);
        ctx.log("▶ 远程部署：备份旧版 → 原子替换 → 重启服务", "info");
        let script = scripts::deploy_script(project, profile, ts, &upload_name);
        let (code, output) = transport.exec(ctx, &script, 180, cancel)
            .await.map_err(|e| format!("远程部署执行失败: {e}"))?;
        for line in output.lines().filter(|l| !l.trim().is_empty()) {
            ctx.log(line, "stdout");
        }
        if code != 0 || !output.contains("DEPLOY_OK") {
            ctx.step("deploy", "failed", Some(format!("退出码 {code}")));
            let hint = if output.contains("RESTART_FAILED") {
                "产物已替换，但重启命令失败：请检查服务器上的服务日志，或回滚恢复旧版本。"
            } else {
                "旧版本已备份，可一键回滚。"
            };
            return Err(format!("远程部署失败（退出码 {code}）。{hint}"));
        }
        ctx.step("deploy", "done", Some("备份+切换+重启".into()));
        check_cancel(cancel)?;

        // ---- 7. verify ----
        *current_step = "verify".into();
        let url = profile.health_check_url.trim();
        if !settings.auto_health_check {
            ctx.step("verify", "done", Some("已在设置中关闭".into()));
        } else if url.is_empty() {
            ctx.step("verify", "done", Some("未配置，跳过".into()));
        } else {
            ctx.step("verify", "running", None);
            ctx.log(format!("▶ 健康检查: {url}"), "info");
            let (code, output) = transport.exec(ctx, &scripts::health_check_script(url), 20, cancel)
                .await.map_err(|e| format!("健康检查执行失败: {e}"))?;
            let http = output.trim().to_string();
            if code == 127 {
                ctx.log("服务器未安装 curl，健康检查跳过", "stderr");
                ctx.step("verify", "done", Some("无 curl，跳过".into()));
            } else if code == 0 && http.parse::<u16>().map(|c| (200..400).contains(&c)).unwrap_or(false) {
                ctx.log(format!("HTTP {http} OK"), "info");
                ctx.step("verify", "done", Some(format!("HTTP {http}")));
            } else {
                ctx.step("verify", "failed", Some(format!("HTTP {http}")));
                return Err(format!("健康检查失败（HTTP {http}，curl 退出码 {code}）。建议回滚到上一版本。"));
            }
        }
        Ok(format!("部署成功：{} 已上线，备份 {ts} 可回滚。", project.name))
    }.await;

    transport.close().await;
    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}

async fn upload_sftp(
    ctx: &Ctx,
    session: &russh::client::Handle<ssh::TestHandler>,
    local_file: &Path,
    upload_name: &str,
    remote_dir: &str,
    ts: &str,
    total: u64,
    cancel: &Arc<AtomicBool>,
) -> Result<String, String> {
    let channel = session.channel_open_session().await.map_err(|e| e.to_string())?;
    channel.request_subsystem(true, "sftp").await.map_err(|e| e.to_string())?;
    let sftp = russh_sftp::client::SftpSession::new(channel.into_stream())
        .await.map_err(|e| format!("SFTP 会话建立失败: {e}"))?;

    let base = format!("{}/.deploy_tmp/{}", remote_dir.trim_end_matches('/'), ts);
    let mut acc = String::new();
    for part in base.trim_start_matches('/').split('/') {
        acc = format!("{acc}/{part}");
        let _ = sftp.create_dir(&acc).await;
    }
    let remote_path = format!("{base}/{upload_name}");
    ctx.log(format!("SFTP → {remote_path}"), "info");

    let mut local = tokio::fs::File::open(local_file).await.map_err(|e| e.to_string())?;
    let mut remote = sftp.create(&remote_path).await.map_err(|e| format!("创建远程文件失败: {e}"))?;

    let mut buf = vec![0u8; 256 * 1024];
    let mut uploaded: u64 = 0;
    let mut last_emit = Instant::now();
    let mut last_bytes: u64 = 0;
    loop {
        check_cancel(cancel).map_err(|e| {
            ctx.step("upload", "failed", Some("已取消".into()));
            e
        })?;
        let n = local.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 { break; }
        remote.write_all(&buf[..n]).await.map_err(|e| format!("上传中断: {e}"))?;
        uploaded += n as u64;
        if last_emit.elapsed().as_millis() >= 150 || uploaded == total {
            let speed = ((uploaded - last_bytes) as f64 / last_emit.elapsed().as_secs_f64().max(0.001)) as u64;
            ctx.progress(uploaded, total, speed);
            last_emit = Instant::now();
            last_bytes = uploaded;
        }
    }
    remote.shutdown().await.map_err(|e| e.to_string())?;

    let meta = sftp.metadata(&remote_path).await.map_err(|e| e.to_string())?;
    let remote_size = meta.size.unwrap_or(0);
    if remote_size != total {
        ctx.step("upload", "failed", None);
        return Err(format!("上传校验失败：本地 {total} 字节，远程 {remote_size} 字节"));
    }
    Ok(remote_path)
}

/// 回滚运行器：connect → rollback（恢复备份 + 重启）
#[allow(clippy::too_many_arguments)]
pub async fn run_rollback(
    app: AppHandle,
    db: Arc<Mutex<Connection>>,
    log_dir: PathBuf,
    project: Project,
    server: Server,
    secret: Option<String>,
    profile: DeployProfile,
    backup_ts: Option<String>,
    cancel: Arc<AtomicBool>,
) {
    let ctx = Ctx::new(app, project.id, None);
    let start = Instant::now();
    let started_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let log_ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let settings = RunSettings::load(&db.lock().unwrap());
    let ctx = Ctx { mask: if settings.log_masking { secret.clone() } else { None }, ..ctx };
    let mut current_step = "connect".to_string();

    let result = async {
        check_cancel(&cancel)?;
        ctx.step("connect", "running", None);
        ctx.log(format!("▶ 连接服务器 {}@{}:{}", server.username, server.host, server.port), "info");
        let auth = ssh::build_auth(&server.auth_type, server.key_path.as_deref(), secret.as_deref())?;
        let session = tokio::time::timeout(
            std::time::Duration::from_secs(settings.ssh_timeout),
            ssh::connect_and_auth(&server.host, server.port, &server.username, auth),
        )
        .await
        .map_err(|_| format!("SSH 连接超时（{} 秒）", settings.ssh_timeout))
        .and_then(|r| r.map_err(|e| format!("SSH 连接失败: {e}")))?;
        ctx.step("connect", "done", None);
        let transport = SshTransport { session };

        current_step = "rollback".into();
        ctx.step("rollback", "running", None);
        let target = backup_ts.clone().unwrap_or_else(|| "最新备份".into());
        ctx.log(format!("▶ 回滚到 {target}：恢复备份 → 重启服务"), "info");
        let script = scripts::rollback_script(&project, &profile, backup_ts.as_deref());
        let (code, output) = transport.exec(&ctx, &script, 180, &cancel)
            .await.map_err(|e| format!("回滚执行失败: {e}"))?;
        for line in output.lines().filter(|l| !l.trim().is_empty()) {
            ctx.log(line, "stdout");
        }
        transport.close().await;
        if output.contains("NO_BACKUP") {
            ctx.step("rollback", "failed", None);
            return Err("没有可用的备份，无法回滚".into());
        }
        if code != 0 || !output.contains("DEPLOY_OK") {
            ctx.step("rollback", "failed", Some(format!("退出码 {code}")));
            let hint = if output.contains("RESTART_FAILED") { "备份已恢复，但重启命令失败，请检查服务器日志。" } else { "请查看服务端日志。" };
            return Err(format!("回滚失败（退出码 {code}）。{hint}"));
        }
        ctx.step("rollback", "done", None);
        Ok(format!("已回滚到 {target} 并重启服务。"))
    }.await;

    finish(
        &ctx, &db, &log_dir, project.id, &project.name, server.id, &profile.env_name,
        "rollback", &started_at, start, result, &current_step, &format!("rollback_{log_ts}"), backup_ts.as_deref(), &profile.remote_dir,
    );
}

/// 递归压缩目录（阻塞任务，在 spawn_blocking 中执行）
fn zip_dir(src: &Path, dest: &Path) -> Result<u64, String> {
    let file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut zw = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut stack = vec![src.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(src).unwrap().to_string_lossy().replace('\\', "/");
                zw.start_file(rel, opts).map_err(|e| e.to_string())?;
                let mut f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
                std::io::copy(&mut f, &mut zw).map_err(|e| e.to_string())?;
            }
        }
    }
    zw.finish().map_err(|e| e.to_string())?;
    Ok(std::fs::metadata(dest).map_err(|e| e.to_string())?.len())
}