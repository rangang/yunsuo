//! 本地构建执行器：Windows 用 cmd /C（UTF-8/GBK 自适应），macOS/Linux 用 sh -c（UTF-8）。
//! 流式收集输出，支持取消并结束整棵进程树。
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

static MAVEN_CACHE: OnceLock<Option<String>> = OnceLock::new();

#[cfg(windows)]
const MVN_BIN: &str = "mvn.cmd";
#[cfg(not(windows))]
const MVN_BIN: &str = "mvn";

fn is_mvn(p: PathBuf) -> Option<String> {
    if p.is_file() { Some(p.to_string_lossy().into_owned()) } else { None }
}

/// 在 IDEA 安装目录（容器目录）下探测内置 Maven
fn scan_idea_dir(c: &Path) -> Option<String> {
    for plugin in ["maven-plugin", "maven"] {
        for ver in ["maven3", "maven4"] {
            // Windows: <idea>/plugins/...；macOS: <Idea.app>/Contents/plugins/...
            if let Some(p) = is_mvn(c.join("plugins").join(plugin).join("lib").join(ver).join("bin").join(MVN_BIN)) {
                return Some(p);
            }
            if let Some(p) = is_mvn(c.join("Contents").join("plugins").join(plugin).join("lib").join(ver).join("bin").join(MVN_BIN)) {
                return Some(p);
            }
        }
    }
    // Toolbox 布局：ch-0/<build>/[Idea.app/Contents/]plugins/...
    let ch = c.join("ch-0");
    if ch.is_dir() {
        if let Ok(rd) = std::fs::read_dir(&ch) {
            for b in rd.flatten() {
                if let Some(p) = scan_idea_dir(&b.path()) {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// 扫描常见位置寻找 Maven（IDEA 内置 / 独立安装 / Toolbox / Homebrew）
fn scan_maven() -> Option<String> {
    for var in ["MAVEN_HOME", "M2_HOME"] {
        if let Ok(home) = std::env::var(var) {
            if let Some(p) = is_mvn(Path::new(&home).join("bin").join(MVN_BIN)) {
                return Some(p);
            }
        }
    }
    #[cfg(windows)]
    let roots: Vec<String> = {
        let user = std::env::var("USERPROFILE").unwrap_or_default();
        vec![
            r"C:\Program Files\JetBrains".to_string(),
            r"D:\Program Files\JetBrains".to_string(),
            r"D:\software".to_string(),
            r"D:\soft".to_string(),
            r"C:\Program Files".to_string(),
            format!(r"{user}\AppData\Local\JetBrains\Toolbox\apps"),
            format!(r"{user}\AppData\Local\Programs"),
        ]
    };
    #[cfg(not(windows))]
    let roots: Vec<String> = {
        let home = std::env::var("HOME").unwrap_or_default();
        vec![
            "/opt/homebrew/bin".to_string(),
            "/usr/local/bin".to_string(),
            "/opt/maven/bin".to_string(),
            format!("{home}/Applications"),
            "/Applications".to_string(),
            format!("{home}/Library/Application Support/JetBrains/Toolbox/apps"),
            format!("{home}/.local/share/JetBrains/Toolbox/apps"),
        ]
    };
    for root in &roots {
        // Homebrew 等 bin 目录直接含 mvn
        if let Some(p) = is_mvn(Path::new(root).join(MVN_BIN)) {
            return Some(p);
        }
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
                if let Some(p) = is_mvn(dir.join("bin").join(MVN_BIN)) {
                    return Some(p);
                }
            }
            // macOS: IntelliJ IDEA*.app
            #[cfg(not(windows))]
            if name.contains("intellij") && name.ends_with(".app") {
                if let Some(p) = scan_idea_dir(&dir) {
                    return Some(p);
                }
            }
            // JetBrains 容器目录则展开一层
            let candidates: Vec<PathBuf> = if name == "jetbrains" || name == "apps" {
                std::fs::read_dir(&dir)
                    .map(|rd| rd.flatten().map(|e| e.path()).collect())
                    .unwrap_or_default()
            } else {
                vec![dir.clone()]
            };
            for c in candidates {
                if let Some(p) = scan_idea_dir(&c) {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// PATH 中是否可直接使用 mvn
fn mvn_in_path() -> bool {
    #[cfg(windows)]
    let probe = std::process::Command::new("where.exe").arg("mvn").output();
    #[cfg(not(windows))]
    let probe = std::process::Command::new("which").arg("mvn").output();
    probe.map(|o| o.status.success()).unwrap_or(false)
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
    if mvn_in_path() {
        return Ok(None);
    }
    if let Some(found) = MAVEN_CACHE.get_or_init(scan_maven) {
        return Ok(Some(found.clone()));
    }
    Err("未找到 Maven：请安装 Maven 并加入 PATH，或在「设置 → 部署默认值 → Maven 路径」填写 mvn 可执行文件完整路径（支持 IDEA 内置 Maven）".into())
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
        // 中文 Windows 控制台常见 GBK；macOS/Linux 终端统一 UTF-8
        #[cfg(windows)]
        Err(_) => encoding_rs::GBK.decode(bytes).0.into_owned(),
        #[cfg(not(windows))]
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// 结束进程树：Windows 用 taskkill /T；unix 杀整个进程组（spawn 时已设 process_group）
pub fn kill_tree(pid: u32) {
    #[cfg(windows)]
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &format!("-{pid}")])
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
    #[cfg(windows)]
    let mut command = {
        let mut c = Command::new("cmd");
        // 原样传给 cmd /C 并整体包一层引号：
        // 避免 Rust 默认参数转义把引号变成 \"（cmd 不识别），同时兼容带空格路径与 && 等复合命令
        c.raw_arg(format!("/C \"{cmd}\""));
        c
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        // 独立进程组：取消时可整组结束（sh 的子进程如 node 不会随父进程退出）
        c.process_group(0);
        c
    };
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