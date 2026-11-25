use serde::{Deserialize, Serialize};

fn default_server_type() -> String {
    "ssh".into()
}

#[derive(Debug, Clone, Serialize)]
pub struct Server {
    pub id: i64,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub key_path: Option<String>,
    pub has_credential: bool,
    pub created_at: String,
    pub server_type: String,
    pub panel_url: Option<String>,
    pub tls_insecure: bool,
}

#[derive(Debug, Deserialize)]
pub struct ServerInput {
    pub id: Option<i64>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub key_path: Option<String>,
    /// 密码或私钥口令；None/空 = 保留已存凭据
    pub secret: Option<String>,
    #[serde(default = "default_server_type")]
    pub server_type: String,
    #[serde(default)]
    pub panel_url: Option<String>,
    #[serde(default)]
    pub tls_insecure: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub local_path: String,
    pub build_command: String,
    pub artifact_path: String,
    pub pre_build_command: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ProjectInput {
    pub id: Option<i64>,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub local_path: String,
    pub build_command: String,
    pub artifact_path: String,
    pub pre_build_command: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TestResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub os_info: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DetectResult {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub build_command: Option<String>,
    pub artifact_path: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeployProfile {
    pub project_id: i64,
    pub server_id: i64,
    pub env_name: String,
    pub remote_dir: String,
    pub site_dir: String,
    pub jar_name: String,
    pub restart_command: String,
    pub health_check_url: String,
    pub backup_keep: i64,
    pub is_prod: bool,
}

#[derive(Debug, Deserialize)]
pub struct ProfileInput {
    pub project_id: i64,
    pub server_id: i64,
    pub env_name: String,
    pub remote_dir: String,
    pub site_dir: String,
    pub jar_name: String,
    pub restart_command: String,
    pub health_check_url: String,
    pub backup_keep: i64,
    pub is_prod: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryRecord {
    pub id: i64,
    pub project_id: i64,
    pub server_id: i64,
    pub env_name: String,
    pub kind: String,
    pub status: String,
    pub started_at: String,
    pub duration_ms: i64,
    pub failed_step: Option<String>,
    pub message: Option<String>,
    pub log_file: Option<String>,
    pub backup_ref: Option<String>,
    pub remote_dir: Option<String>,
}