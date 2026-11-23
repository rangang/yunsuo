//! 本地构建执行器：cmd /C 执行命令，流式收集输出（UTF-8/GBK 自适应），支持取消并杀进程树。
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

static MAVEN_CACHE: OnceLock<Option<String>> = OnceLock::new();

fn is_mvn(p: PathBuf) -> Option<String> {
    if p.is_file() { Some(p.to_string_lossy().into_owned()) } else { None }
}

/// 扫描常见位置寻找 mvn.cmd（IDEA 内置 Maven / 独立安装 / Toolbox）
fn scan_maven() -> Option<String> {
    for var in ["MAVEN_HOME", "M2_HOME"] {
        if let Ok(home) = std::env::var(var) {
            if let Some(p) = is_mvn(Path::new(&home).join("bin").join("mvn.cmd")) {
                return Some(p);
            }
        }
    }
    let user = std::env::var("USERPROFILE").unwrap_or_default();
    let roots = [
        r"C:\Program Files\JetBrains".to_string(),
        r"D:\Program Files\JetBrains".to_string(),
        r"D:\software".to_string(),
        r"D:\soft".to_string(),
        r"C:\Program Files".to_string(),
        format!(r"{user}\AppData\Local\JetBrains\Toolbox\apps"),
        format!(r"{user}\AppData\Local\Programs"),
    ];
    for root in &roots {
        let entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.starts_with("apache-maven") {
                if let Some(p) = is_mvn(dir.join("bin").join("mvn.cmd")) {
                    return Some(p);
                }
            }
            // JetBrains 容器目录则展开一层
            let candidates: Vec<PathBuf> = if name == "jetbrains" {
                std::fs::read_dir(&dir)
                    .map(|rd| rd.flatten().map(|e| e.path()).collect())
                    .unwrap_or_default()
            } else {
                vec![dir.clone()]
            };
            for c in candidates {
                for plugin in ["maven-plugin", "maven"] {
                    for ver in ["maven3", "maven4"] {
                        if let Some(p) = is_mvn(c.join("plugins").join(plugin).join("lib").join(ver).join("bin").join("mvn.cmd")) {
                            return Some(p);
                        }
                    }
                }
                // Toolbox 布局：ch-0/<build>/plugins/...
                let ch = c.join("ch-0");
                if ch.is_dir() {
                    if let Ok(rd) = std::fs::read_dir(&ch) {
                        for b in rd.flatten() {
                            for plugin in ["maven-plugin", "maven"] {
                                if let Some(p) = is_mvn(b.path().join("plugins").join(plugin).join("lib").join("maven3").join("bin").join("mvn.cmd")) {
                                    return Some(p);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// 解析 Maven：Some(完整路径) 表示需要替换命令中的 mvn；None 表示 PATH 中可直接用
pub fn resolve_maven(explicit: &str) -> Result<Option<String>, String> {
    let explicit = explicit.trim();
    if !explicit.is_empty() {
        if Path::new(explicit).is_file() {
            return Ok(Some(explicit.to_string()));
        }
        return Err(format!("设置的 Maven 路径不存在: {explicit}"));
    }
    if let Ok(out) = std::process::Command::new("where.exe").arg("mvn").output() {
        if out.status.success() {
            return Ok(None);
        }
    }
    if let Some(found) = MAVEN_CACHE.get_or_init(scan_maven) {
        return Ok(Some(found.clone()));
    }
    Err("未找到 Maven：请安装 Maven 并加入 PATH，或在「设置 → 部署默认值 → Maven 路径」填写 mvn.cmd 完整路径（支持 IDEA 内置 Maven）".into())
}

/// 命令以 mvn 开头时，替换为解析到的完整路径；返回 (新命令, 解析到的路径)
pub fn prepare_command(cmd: &str, maven_setting: &str) -> Result<(String, Option<String>), String> {
    let trimmed = cmd.trim_start();
    let first = trimmed.split_whitespace().next().unwrap_or("");
    if first.eq_ignore_ascii_case("mvn") || first.eq_ignore_ascii_case("mvn.cmd") {
        if let Some(path) = resolve_maven(maven_setting)? {
            let rest = &trimmed[first.len()..];
            return Ok((format!("\"{path}\"{rest}"), Some(path)));
        }
    }
    Ok((cmd.to_string(), None))
}

pub fn decode_line(bytes: &[u8]) -> String {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::GBK.decode(bytes).0.into_owned(),
    }
}

/// 结束进程树（Windows：taskkill /T；cmd /C 的子进程如 node 不会随父进程退出）
pub fn kill_tree(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

async fn pump(
    reader: impl tokio::io::AsyncRead + Unpin,
    is_stderr: bool,
    cancel: Arc<AtomicBool>,
    tx: mpsc::UnboundedSender<(String, bool)>,
) -> Result<(), String> {
    let mut reader = BufReader::new(reader);
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let n = tokio::select! {
            r = reader.read_until(b'\n', &mut buf) => r.map_err(|e| e.to_string())?,
            _ = tokio::time::sleep(std::time::Duration::from_millis(120)) => {
                if cancel.load(Ordering::Relaxed) { return Err("已取消".into()); }
                continue;
            }
        };
        if n == 0 {
            return Ok(());
        }
        let line = decode_line(&buf);
        if !line.is_empty() {
            let _ = tx.send((line, is_stderr));
        }
        if cancel.load(Ordering::Relaxed) {
            return Err("已取消".into());
        }
    }
}

/// 在 dir 下执行 cmd，逐行回调输出；返回退出码。取消时杀进程树并返回 Err。
pub async fn run_streaming(
    dir: &Path,
    cmd: &str,
    cancel: Arc<AtomicBool>,
    child_pid: Arc<Mutex<Option<u32>>>,
    mut on_line: impl FnMut(String, bool),
) -> Result<i32, String> {
    let mut command = Command::new("cmd");
    #[cfg(windows)]
    {
        // 原样传给 cmd /C 并整体包一层引号：
        // 避免 Rust 默认参数转义把引号变成 \"（cmd 不识别），同时兼容带空格路径与 && 等复合命令
        command.raw_arg(format!("/C \"{cmd}\""));
    }
    #[cfg(not(windows))]
    {
        command.args(["/C", cmd]);
    }
    let mut child = command
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("无法启动命令「{cmd}」: {e}"))?;

    *child_pid.lock().unwrap() = child.id();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (tx, mut rx) = mpsc::unbounded_channel::<(String, bool)>();
    let tx2 = tx.clone();
    let cancel2 = cancel.clone();
    let drain = async {
        while let Some((line, is_stderr)) = rx.recv().await {
            on_line(line, is_stderr);
        }
    };
    let (r1, r2, _) = tokio::join!(
        pump(stdout, false, cancel.clone(), tx),
        pump(stderr, true, cancel2, tx2),
        drain
    );

    let result = async {
        r1?;
        r2?;
        let status = child.wait().await.map_err(|e| e.to_string())?;
        Ok(status.code().unwrap_or(-1))
    }
    .await;

    *child_pid.lock().unwrap() = None;
    if cancel.load(Ordering::Relaxed) {
        if let Some(pid) = child.id() {
            kill_tree(pid);
        }
        let _ = child.wait().await;
        return Err("已取消".into());
    }
    result
}