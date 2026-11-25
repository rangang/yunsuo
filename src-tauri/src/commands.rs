use crate::model::{
    DeployProfile, DetectResult, HistoryRecord, ProfileInput, Project, ProjectInput, Server,
    ServerInput, TestResult,
};
use crate::pipeline::DeployControl;
use crate::{bt, builder, ssh, store, vault};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub deploy: Arc<Mutex<Option<DeployControl>>>,
    pub log_dir: PathBuf,
}

#[tauri::command]
pub fn list_servers(state: State<AppState>) -> Result<Vec<Server>, String> {
    let db = state.db.lock().unwrap();
    store::list_servers(&db)
}

#[tauri::command]
pub fn save_server(state: State<AppState>, input: ServerInput) -> Result<Server, String> {
    let db = state.db.lock().unwrap();
    let id = match input.id {
        Some(id) => {
            store::update_server(&db, id, &input)?;
            id
        }
        None => store::insert_server(&db, &input)?,
    };
    if let Some(secret) = input.secret.as_deref() {
        if !secret.is_empty() {
            vault::set_secret(id, secret)?;
            store::set_has_credential(&db, id, true)?;
        }
    }
    store::get_server(&db, id)
}

#[tauri::command]
pub fn delete_server(state: State<AppState>, id: i64) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    store::delete_server(&db, id)?;
    vault::delete_secret(id)
}

#[tauri::command]
pub async fn test_server(state: State<'_, AppState>, id: i64) -> Result<TestResult, String> {
    let (server, secret) = {
        let db = state.db.lock().unwrap();
        let server = store::get_server(&db, id)?;
        let secret = vault::get_secret(id)?;
        (server, secret)
    };
    if server.server_type == "bt" {
        let url = server.panel_url.as_deref().filter(|s| !s.trim().is_empty())
            .ok_or("未配置面板地址")?;
        let sk = secret.as_deref().filter(|s| !s.is_empty())
            .ok_or("未保存 API 密钥")?;
        let client = bt::BtClient::new(url, sk, server.tls_insecure)?;
        return Ok(client.test_connection().await);
    }
    let auth = ssh::build_auth(
        &server.auth_type,
        server.key_path.as_deref(),
        secret.as_deref(),
    )?;
    Ok(ssh::test_connection(&server.host, server.port, &server.username, auth).await)
}

#[tauri::command]
pub async fn test_server_draft(input: ServerInput) -> Result<TestResult, String> {
    if input.server_type == "bt" {
        let url = input.panel_url.as_deref().filter(|s| !s.trim().is_empty())
            .ok_or("未配置面板地址")?;
        let sk = input.secret.as_deref().filter(|s| !s.is_empty())
            .ok_or("请填写 API 密钥")?;
        let client = bt::BtClient::new(url, sk, input.tls_insecure)?;
        return Ok(client.test_connection().await);
    }
    let auth = ssh::build_auth(
        &input.auth_type,
        input.key_path.as_deref(),
        input.secret.as_deref(),
    )?;
    Ok(ssh::test_connection(&input.host, input.port, &input.username, auth).await)
}

#[tauri::command]
pub fn list_projects(state: State<AppState>) -> Result<Vec<Project>, String> {
    let db = state.db.lock().unwrap();
    store::list_projects(&db)
}

#[tauri::command]
pub fn save_project(state: State<AppState>, input: ProjectInput) -> Result<Project, String> {
    let db = state.db.lock().unwrap();
    store::save_project(&db, &input)
}

#[tauri::command]
pub fn delete_project(state: State<AppState>, id: i64) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    store::delete_project(&db, id)
}

#[tauri::command]
pub fn detect_project(path: String) -> DetectResult {
    let p = std::path::Path::new(&path);
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string());
    if p.join("package.json").exists() {
        DetectResult {
            kind: Some("frontend".into()),
            build_command: Some("npm run build".into()),
            artifact_path: Some("dist".into()),
            name,
        }
    } else if p.join("pom.xml").exists() {
        DetectResult {
            kind: Some("java".into()),
            build_command: Some("mvn -DskipTests package".into()),
            artifact_path: Some("target/*.jar".into()),
            name,
        }
    } else if p.join("build.gradle").exists() || p.join("build.gradle.kts").exists() {
        DetectResult {
            kind: Some("java".into()),
            build_command: Some("gradle build -x test".into()),
            artifact_path: Some("build/libs/*.jar".into()),
            name,
        }
    } else {
        DetectResult {
            kind: None,
            build_command: None,
            artifact_path: None,
            name,
        }
    }
}

#[tauri::command]
pub fn get_profile(
    state: State<AppState>,
    project_id: i64,
    server_id: i64,
    env_name: String,
) -> Result<Option<DeployProfile>, String> {
    let db = state.db.lock().unwrap();
    store::get_profile(&db, project_id, server_id, &env_name)
}

#[tauri::command]
pub fn save_profile(state: State<AppState>, input: ProfileInput) -> Result<DeployProfile, String> {
    let site = input.site_dir.trim().to_string();
    if site.is_empty()
        || !site.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err("站点文件夹名只能包含字母、数字、. _ -（如 current、www、my-site）".into());
    }
    let mut input = input;
    input.site_dir = site;
    let jar = input.jar_name.trim().to_string();
    if !jar.ends_with(".jar")
        || jar.len() < 5
        || !jar.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err("jar 文件名必须以 .jar 结尾，且只能包含字母、数字、. _ -（如 yjl-system.jar）".into());
    }
    input.jar_name = jar;
    let db = state.db.lock().unwrap();
    store::save_profile(&db, &input)
}

#[tauri::command]
pub fn list_environments(state: State<AppState>, project_id: i64, server_id: i64) -> Result<Vec<String>, String> {
    let db = state.db.lock().unwrap();
    store::list_environments(&db, project_id, server_id)
}

#[tauri::command]
pub fn get_setting(state: State<AppState>, key: String, default: Option<String>) -> Result<String, String> {
    let db = state.db.lock().unwrap();
    Ok(store::get_setting(&db, &key, &default.unwrap_or_else(|| "1".into())))
}

#[tauri::command]
pub fn set_setting(state: State<AppState>, key: String, value: String) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    store::set_setting(&db, &key, &value)
}

#[tauri::command]
pub fn save_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("写入文件失败: {e}"))
}

#[tauri::command]
pub fn open_log_dir(state: State<AppState>) -> Result<(), String> {
    #[cfg(windows)]
    let program = "explorer.exe";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program)
        .arg(&state.log_dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cleanup_old_logs(state: State<AppState>, days: u64) -> Result<u64, String> {
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(days * 86400);
    let mut removed = 0u64;
    let entries = std::fs::read_dir(&state.log_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "log").unwrap_or(false) {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if modified < cutoff && std::fs::remove_file(&path).is_ok() {
                        removed += 1;
                    }
                }
            }
        }
    }
    Ok(removed)
}

#[tauri::command]
pub fn list_history(state: State<AppState>, project_id: Option<i64>) -> Result<Vec<HistoryRecord>, String> {
    let db = state.db.lock().unwrap();
    store::list_records(&db, project_id)
}

#[tauri::command]
pub fn get_history_log(state: State<AppState>, id: i64) -> Result<String, String> {
    let record = {
        let db = state.db.lock().unwrap();
        store::get_record(&db, id)?
    };
    let file = record.log_file.ok_or("该记录没有日志文件")?;
    std::fs::read_to_string(state.log_dir.join(file)).map_err(|e| format!("读取日志失败: {e}"))
}

/// 读取项目+服务器+凭据+部署配置（部署/回滚共用）
fn load_deploy_context(
    state: &State<AppState>,
    project_id: i64,
    server_id: i64,
    env_name: &str,
) -> Result<(Project, Server, Option<String>, DeployProfile), String> {
    let db = state.db.lock().unwrap();
    let project = store::get_project(&db, project_id)?;
    let server = store::get_server(&db, server_id)?;
    let secret = vault::get_secret(server_id)?;
    let profile = store::get_profile(&db, project_id, server_id, env_name)?
        .ok_or("请先在该环境下填写远程目录等部署配置")?;
    Ok((project, server, secret, profile))
}

fn acquire_control(state: &State<AppState>) -> Result<DeployControl, String> {
    let mut active = state.deploy.lock().unwrap();
    if active.is_some() {
        return Err("已有部署/回滚任务进行中，请等待完成或先取消".into());
    }
    let c = DeployControl {
        cancel: Arc::new(AtomicBool::new(false)),
        child_pid: Arc::new(Mutex::new(None)),
    };
    *active = Some(c.clone());
    Ok(c)
}

#[tauri::command]
pub async fn start_deploy(
    state: State<'_, AppState>,
    app: AppHandle,
    project_id: i64,
    server_id: i64,
    env_name: String,
    remote_dir: String,
) -> Result<(), String> {
    let remote_dir = remote_dir.trim().to_string();
    if !remote_dir.starts_with('/') || remote_dir.len() < 2 {
        return Err("远程目录必须是绝对路径（以 / 开头）".into());
    }
    let (project, server, secret, mut profile) = load_deploy_context(&state, project_id, server_id, &env_name)?;
    if project.kind == "java" && profile.restart_command.trim().is_empty() {
        return Err("Java 项目必须配置重启命令（如 sudo systemctl restart 服务名），请在「部署配置」中填写".into());
    }
    profile.remote_dir = remote_dir;
    {
        let db = state.db.lock().unwrap();
        store::save_profile(&db, &ProfileInput {
            project_id,
            server_id,
            env_name: profile.env_name.clone(),
            remote_dir: profile.remote_dir.clone(),
            site_dir: profile.site_dir.clone(),
            jar_name: profile.jar_name.clone(),
            restart_command: profile.restart_command.clone(),
            health_check_url: profile.health_check_url.clone(),
            backup_keep: profile.backup_keep,
            is_prod: profile.is_prod,
        })?;
    }
    let control = acquire_control(&state)?;
    let slot = state.deploy.clone();
    let db = state.db.clone();
    let log_dir = state.log_dir.clone();
    tokio::spawn(async move {
        crate::pipeline::run(
            app, db, log_dir, project, server, secret, profile,
            control.cancel, control.child_pid,
        ).await;
        *slot.lock().unwrap() = None;
    });
    Ok(())
}

#[tauri::command]
pub async fn rollback(
    state: State<'_, AppState>,
    app: AppHandle,
    project_id: i64,
    server_id: i64,
    env_name: String,
    backup_ts: Option<String>,
) -> Result<(), String> {
    let (project, server, secret, profile) = load_deploy_context(&state, project_id, server_id, &env_name)?;
    let control = acquire_control(&state)?;
    let slot = state.deploy.clone();
    let db = state.db.clone();
    let log_dir = state.log_dir.clone();
    tokio::spawn(async move {
        crate::pipeline::run_rollback(
            app, db, log_dir, project, server, secret, profile,
            backup_ts, control.cancel,
        ).await;
        *slot.lock().unwrap() = None;
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_deploy(state: State<AppState>) -> Result<(), String> {
    let active = state.deploy.lock().unwrap();
    match active.as_ref() {
        Some(c) => {
            c.cancel.store(true, Ordering::Relaxed);
            if let Some(pid) = *c.child_pid.lock().unwrap() {
                builder::kill_tree(pid);
            }
            Ok(())
        }
        None => Err("当前没有进行中的部署任务".into()),
    }
}