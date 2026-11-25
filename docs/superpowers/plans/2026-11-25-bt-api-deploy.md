# 宝塔 API 部署模式 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 为云梭新增「宝塔面板 HTTP API」服务器类型，无 SSH 凭据也能完成与 SSH 模式语义一致的部署/回滚/健康检查。

**架构：** 新增 Rust 模块 `bt.rs`（宝塔 HTTP 客户端，双原语：UploadFile 上传 + 计划任务执行 shell）；`pipeline.rs` 引入 `Transport` trait，SSH 与宝塔为两个实现；`scripts.rs` 部署脚本零改动。规格：`docs/superpowers/specs/2026-11-25-bt-api-deploy-design.md`。

**技术栈：** Tauri 2 / Rust（新增 reqwest 0.12 rustls + multipart、md5 0.7）/ Vue 3 + TS / rusqlite。

**工作目录约定：** 仓库根 = `D:\RG_Projects\dabao-bushu\deploypilot`（git 仓库，main 分支）。`cargo` 命令在 `src-tauri/` 下执行；`npm` 命令在仓库根执行。Shell 为 Git Bash。`cargo test` 依赖前端产物 `dist/`（已存在，勿删）。

---

## 文件结构

| 文件 | 职责 | 动作 |
|---|---|---|
| src-tauri/Cargo.toml | 依赖 | 修改：+ reqwest、md5 |
| src-tauri/src/model.rs | Server/ServerInput 结构 | 修改：+ server_type/panel_url/tls_insecure |
| src-tauri/src/store.rs | SQLite 迁移与 CRUD | 修改：server 表 3 新列 + 单测 |
| src-tauri/src/bt.rs | 宝塔 API 客户端（签名/call/exec/upload 原语/test） | 创建 |
| src-tauri/src/main.rs | 模块注册 | 修改：mod bt |
| src-tauri/src/pipeline.rs | Transport trait + SshTransport/BtTransport + connect 分发 | 修改 |
| src-tauri/src/commands.rs | test_server/test_server_draft 按类型分发 | 修改 |
| src/ipc/types.ts | 前端类型 | 修改 |
| src/ipc/index.ts | 浏览器 mock | 修改 |
| src/components/ServerModal.vue | 连接方式切换 + 宝塔表单 | 修改 |
| src/views/ServersView.vue | 类型徽标 + 宝塔展示 | 修改 |
| src/components/ProfileModal.vue | 宝塔默认重启命令 + 提示 | 修改 |
| README.md | 功能说明 | 修改 |

不改动：`scripts.rs`、`ssh.rs`、`vault.rs`、`builder.rs`。

---

### 任务 1：数据层（model + store 迁移 + 单测）

**文件：**
- 修改：`src-tauri/src/model.rs:3-27`
- 修改：`src-tauri/src/store.rs`（init 迁移数组、row_to_server、SERVER_COLS、insert_server、update_server）
- 测试：`src-tauri/src/store.rs` 内新增 `#[cfg(test)] mod tests`

- [ ] **步骤 1：编写失败的测试**

在 `src-tauri/src/store.rs` 文件末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn bt_input() -> ServerInput {
        ServerInput {
            id: None,
            name: "宝塔面板".into(),
            host: "1.2.3.4".into(),
            port: 0,
            username: "".into(),
            auth_type: "apikey".into(),
            key_path: None,
            secret: None,
            server_type: "bt".into(),
            panel_url: Some("https://1.2.3.4:8888".into()),
            tls_insecure: true,
        }
    }

    #[test]
    fn bt_server_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let id = insert_server(&conn, &bt_input()).unwrap();
        set_has_credential(&conn, id, true).unwrap();
        let s = get_server(&conn, id).unwrap();
        assert_eq!(s.server_type, "bt");
        assert_eq!(s.panel_url.as_deref(), Some("https://1.2.3.4:8888"));
        assert!(s.tls_insecure);
        assert!(s.has_credential);
        let mut upd = bt_input();
        upd.tls_insecure = false;
        update_server(&conn, id, &upd).unwrap();
        assert!(!get_server(&conn, id).unwrap().tls_insecure);
    }

    #[test]
    fn legacy_ssh_server_gets_defaults() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE server (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL, host TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                username TEXT NOT NULL, auth_type TEXT NOT NULL,
                key_path TEXT,
                has_credential INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
            );
            INSERT INTO server (name, host, port, username, auth_type)
                VALUES ('old', '10.0.0.1', 22, 'root', 'password');",
        )
        .unwrap();
        init(&conn).unwrap();
        let s = get_server(&conn, 1).unwrap();
        assert_eq!(s.server_type, "ssh");
        assert_eq!(s.panel_url, None);
        assert!(!s.tls_insecure);
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test store::tests`
预期：编译错误 `missing field server_type` / `no field server_type`（ServerInput/Server 尚无新字段）。

- [ ] **步骤 3：实现 model.rs**

`src-tauri/src/model.rs`：在 `use serde...` 之后加默认值函数，并给两个结构体加字段。

Server（Serialize）在 `pub created_at: String,` 之后追加：

```rust
    pub server_type: String,
    pub panel_url: Option<String>,
    pub tls_insecure: bool,
```

ServerInput（Deserialize）在 `pub secret: Option<String>,` 之后追加：

```rust
    #[serde(default = "default_server_type")]
    pub server_type: String,
    #[serde(default)]
    pub panel_url: Option<String>,
    #[serde(default)]
    pub tls_insecure: bool,
```

文件顶部（use 之后）加：

```rust
fn default_server_type() -> String {
    "ssh".into()
}
```

- [ ] **步骤 4：实现 store.rs**

init 的迁移数组（`for col in [` 内）末尾追加 3 行：

```rust
        "ALTER TABLE server ADD COLUMN server_type TEXT NOT NULL DEFAULT 'ssh'",
        "ALTER TABLE server ADD COLUMN panel_url TEXT",
        "ALTER TABLE server ADD COLUMN tls_insecure INTEGER NOT NULL DEFAULT 0",
```

`row_to_server` 在 `created_at: row.get(8)?,` 之后追加（新列在 SELECT 末尾，下标 9/10/11）：

```rust
        server_type: row.get(9)?,
        panel_url: row.get(10)?,
        tls_insecure: row.get::<_, i64>(11)? != 0,
```

`SERVER_COLS` 改为：

```rust
const SERVER_COLS: &str =
    "id, name, host, port, username, auth_type, key_path, has_credential, created_at, server_type, panel_url, tls_insecure";
```

`insert_server` 改为：

```rust
pub fn insert_server(conn: &Connection, input: &ServerInput) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO server (name, host, port, username, auth_type, key_path, server_type, panel_url, tls_insecure)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![input.name, input.host, input.port as i64, input.username, input.auth_type, input.key_path,
                input.server_type, input.panel_url, input.tls_insecure as i64],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}
```

`update_server` 改为：

```rust
pub fn update_server(conn: &Connection, id: i64, input: &ServerInput) -> Result<(), String> {
    let n = conn.execute(
        "UPDATE server SET name=?1, host=?2, port=?3, username=?4, auth_type=?5, key_path=?6,
                           server_type=?7, panel_url=?8, tls_insecure=?9 WHERE id=?10",
        params![input.name, input.host, input.port as i64, input.username, input.auth_type, input.key_path,
                input.server_type, input.panel_url, input.tls_insecure as i64, id],
    )
    .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("服务器不存在 (id={id})"));
    }
    Ok(())
}
```

- [ ] **步骤 5：运行测试验证通过**

运行：`cd src-tauri && cargo test store::tests`
预期：2 个测试 PASS。再跑 `cargo check` 确认全 crate 编译（pipeline/commands 只读 Server 已有字段，不受影响）。

- [ ] **步骤 6：Commit**

```bash
git add src-tauri/src/model.rs src-tauri/src/store.rs
git commit -m "feat: server 表支持宝塔类型（server_type/panel_url/tls_insecure）"
```

---

### 任务 2：bt.rs 基础（依赖 + 签名 + 规范化 + call + 连接测试）

**文件：**
- 修改：`src-tauri/Cargo.toml`
- 修改：`src-tauri/src/main.rs:3-10`（mod 列表）
- 创建：`src-tauri/src/bt.rs`

- [ ] **步骤 1：添加依赖与模块注册**

`src-tauri/Cargo.toml` `[dependencies]` 末尾（`chrono = "0.4"` 之后）追加：

```toml
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "multipart"] }
md5 = "0.7"
```

`src-tauri/src/main.rs` 的 mod 列表按字母序插入：

```rust
mod bt;
```

（放在 `mod builder;` 之前。）

- [ ] **步骤 2：编写失败的测试**

创建 `src-tauri/src/bt.rs`，先只写测试与最小骨架之外的纯函数签名占位是不允许的——直接写完整测试文件内容如下（实现下一步补齐，本步结束时文件含测试 + 空实现签名以外的内容见步骤 3；为让测试先失败，本步仅写入测试模块与 `pub fn` 的 `todo!()`  bodies）：

```rust
//! 宝塔面板 HTTP API 客户端。
//! 认证：request_token = md5(request_time + md5(api_sk))，见 docs.bt.cn/api。

use std::time::{Duration, Instant};

pub fn normalize_base(_panel_url: &str) -> Result<String, String> {
    todo!()
}

pub fn sign(_sk: &str, _time: i64) -> String {
    todo!()
}

pub fn friendly(_msg: &str) -> String {
    todo!()
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
```

- [ ] **步骤 3：运行测试验证失败**

运行：`cd src-tauri && cargo test bt::tests`
预期：3 个测试 FAIL（panic `not yet implemented`）。

- [ ] **步骤 4：实现基础客户端**

将 `bt.rs` 中三个 `todo!()` 函数替换为以下完整实现，并在文件顶部补齐 use 与结构体（测试模块保留不动）：

```rust
use crate::model::TestResult;
use std::sync::atomic::{AtomicBool, Ordering};
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
```

注意：文件顶部原有的 `use std::time::{Duration, Instant};` 保留一份即可（合并进上面的 use 区）；`AtomicBool/Ordering` 在本任务暂未使用，任务 3 会用到，先不加（避免 unused 警告）——即本步 use 区为：

```rust
use crate::model::TestResult;
use std::time::{Duration, Instant};
```

- [ ] **步骤 5：运行测试验证通过**

运行：`cd src-tauri && cargo test bt::tests && cargo check`
预期：3 个测试 PASS，全 crate 编译通过。

- [ ] **步骤 6：Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/main.rs src-tauri/src/bt.rs
git commit -m "feat: 宝塔 API 客户端基础（签名/地址规范化/请求封装/连接测试）"
```

---

### 任务 3：bt.rs 执行通道（计划任务 exec）

**文件：**
- 修改：`src-tauri/src/bt.rs`
- 测试：`bt.rs` 内 `tests` 模块追加

- [ ] **步骤 1：编写失败的测试**

在 `bt.rs` 的 `mod tests` 内追加：

```rust
    #[test]
    fn wrap_contains_markers() {
        let w = wrap_script("echo hi");
        assert!(w.starts_with("echo YUNSUO_BEGIN\n(\n"));
        assert!(w.ends_with(")\necho \"YUNSUO_EXIT=$?\"\n"));
    }

    #[test]
    fn parse_exit_normal() {
        let text = "★[2026-11-25 12:00:00]\nYUNSUO_BEGIN\nhello\nDEPLOY_OK\nYUNSUO_EXIT=0\n----footer";
        let (code, out) = parse_exit(text).unwrap();
        assert_eq!(code, 0);
        assert_eq!(out, "hello\nDEPLOY_OK");
    }

    #[test]
    fn parse_exit_failure_code() {
        let text = "YUNSUO_BEGIN\nboom\nYUNSUO_EXIT=3";
        let (code, out) = parse_exit(text).unwrap();
        assert_eq!(code, 3);
        assert_eq!(out, "boom");
    }

    #[test]
    fn parse_exit_missing_marker() {
        assert!(parse_exit("still running").is_none());
        assert!(parse_exit("YUNSUO_BEGIN\npartial").is_none());
    }

    #[test]
    fn parse_exit_ignores_noise_before_begin() {
        let text = "YUNSUO_EXIT=9\nYUNSUO_BEGIN\nok\nYUNSUO_EXIT=0";
        let (code, out) = parse_exit(text).unwrap();
        assert_eq!(code, 0);
        assert_eq!(out, "ok");
    }
```

并在测试模块外追加 `todo!()` 骨架：

```rust
pub fn wrap_script(_script: &str) -> String {
    todo!()
}

pub fn parse_exit(_log: &str) -> Option<(i32, String)> {
    todo!()
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test bt::tests`
预期：新增 5 个测试 FAIL（`not yet implemented`），旧 3 个仍 PASS。

- [ ] **步骤 3：实现纯函数与 exec 编排**

替换两个 `todo!()`：

```rust
/// 子 shell 隔离原脚本的 set -e / exit；首尾标记用于从面板日志中截取脚本输出与退出码
pub fn wrap_script(script: &str) -> String {
    format!("echo YUNSUO_BEGIN\n(\n{}\n)\necho \"YUNSUO_EXIT=$?\"\n", script.trim_end())
}

pub fn parse_exit(log: &str) -> Option<(i32, String)> {
    let mut inside = false;
    let mut lines: Vec<&str> = Vec::new();
    for line in log.lines() {
        let t = line.trim();
        if !inside {
            if t == "YUNSUO_BEGIN" {
                inside = true;
            }
            continue;
        }
        if let Some(rest) = t.strip_prefix("YUNSUO_EXIT=") {
            let code: i32 = rest.trim().parse().ok()?;
            return Some((code, lines.join("\n")));
        }
        lines.push(line);
    }
    None
}
```

在 `impl BtClient` 内追加（`test_connection` 之后）：

```rust
    /// 经计划任务执行 shell：AddCrontab → set_execute_script → 轮询 GetLogs → DelCrontab（必删）
    pub async fn exec_script(
        &self,
        script: &str,
        timeout_secs: u64,
        cancel: &AtomicBool,
    ) -> Result<ExecOutcome, String> {
        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S%.3f");
        let add = self.call("crontab", vec![
            ("action", "AddCrontab".into()),
            ("name", format!("yunsuo-{ts}")),
            ("type", "minute-n".into()),
            ("where1", "30".into()),
            ("sType", "toShell".into()),
            ("sBody", wrap_script(script)),
        ]).await?;
        let id = add["id"].as_i64().ok_or("面板未返回计划任务 id")?;
        let run = self.run_task(id, timeout_secs, cancel).await;
        let cleanup_warning = match self.call("crontab", vec![
            ("action", "DelCrontab".into()),
            ("id", id.to_string()),
        ]).await {
            Ok(_) => None,
            Err(e) => Some(format!("计划任务删除失败（id={id}）：{e}，可到面板计划任务手动删除")),
        };
        let (code, output) = run?;
        Ok(ExecOutcome { code, output, cleanup_warning })
    }

    async fn run_task(&self, id: i64, timeout_secs: u64, cancel: &AtomicBool) -> Result<(i32, String), String> {
        self.call("crontab", vec![
            ("action", "set_execute_script".into()),
            ("id", id.to_string()),
        ]).await?;
        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("已取消".into());
            }
            if Instant::now() >= deadline {
                return Err(format!("远程执行超时（{timeout_secs} 秒），可能面板计划任务服务未运行"));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
            let v = self.call("crontab", vec![
                ("action", "GetLogs".into()),
                ("id", id.to_string()),
            ]).await?;
            if let Some(r) = parse_exit(v["msg"].as_str().unwrap_or("")) {
                return Ok(r);
            }
        }
    }
```

在结构体区（`BtClient` 定义之后）追加：

```rust
pub struct ExecOutcome {
    pub code: i32,
    pub output: String,
    pub cleanup_warning: Option<String>,
}
```

use 区补上：

```rust
use std::sync::atomic::{AtomicBool, Ordering};
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test bt::tests && cargo check`
预期：8 个测试 PASS，编译无警告。

- [ ] **步骤 5：Commit**

```bash
git add src-tauri/src/bt.rs
git commit -m "feat: 宝塔执行通道——计划任务包装脚本与退出码解析"
```

---

### 任务 4：bt.rs 上传原语（分块命名/合并脚本/UploadFile）

**文件：**
- 修改：`src-tauri/src/bt.rs`
- 测试：`bt.rs` 内 `tests` 模块追加

- [ ] **步骤 1：编写失败的测试**

`mod tests` 内追加：

```rust
    #[test]
    fn part_names() {
        assert_eq!(part_name("app.jar", 0), "app.jar.part0001");
        assert_eq!(part_name("app.jar", 12), "app.jar.part0013");
    }

    #[test]
    fn merge_script_quotes() {
        let s = merge_script(
            "/opt/my app/.deploy_tmp/t1",
            "app.jar",
            &["app.jar.part0001".into(), "app.jar.part0002".into()],
        );
        assert!(s.starts_with("cd '/opt/my app/.deploy_tmp/t1' && cat 'app.jar.part0001' 'app.jar.part0002' > 'app.jar'"));
        assert!(s.contains("rm -f 'app.jar.part0001' 'app.jar.part0002'"));
        assert!(s.ends_with("wc -c < 'app.jar'"));
    }

    #[test]
    fn quote_escapes_single_quote() {
        assert_eq!(sh_quote("a'b"), "'a'\\''b'");
    }
```

测试模块外追加骨架：

```rust
pub fn sh_quote(_s: &str) -> String {
    todo!()
}

pub fn part_name(_upload_name: &str, _index: u64) -> String {
    todo!()
}

pub fn merge_script(_base: &str, _name: &str, _parts: &[String]) -> String {
    todo!()
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test bt::tests`
预期：新增 3 个 FAIL，旧 8 个 PASS。

- [ ] **步骤 3：实现**

替换三个 `todo!()`：

```rust
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub fn part_name(upload_name: &str, index: u64) -> String {
    format!("{upload_name}.part{:04}", index + 1)
}

pub fn merge_script(base: &str, name: &str, parts: &[String]) -> String {
    let list = parts.iter().map(|p| sh_quote(p)).collect::<Vec<_>>().join(" ");
    format!(
        "cd {} && cat {} > {} && rm -f {} && wc -c < {}",
        sh_quote(base), list, sh_quote(name), list, sh_quote(name)
    )
}
```

`impl BtClient` 内追加：

```rust
    /// 单次 UploadFile（multipart：path + zunfile）。大文件分块编排见 pipeline 的 BtTransport。
    pub async fn upload_file(&self, data: &[u8], remote_dir: &str, name: &str) -> Result<(), String> {
        let time = chrono::Utc::now().timestamp();
        let part = reqwest::multipart::Part::bytes(data.to_vec())
            .file_name(name.to_string())
            .mime_str("application/octet-stream")
            .map_err(|e| e.to_string())?;
        let form = reqwest::multipart::Form::new()
            .text("action", "UploadFile")
            .text("path", remote_dir.to_string())
            .text("request_time", time.to_string())
            .text("request_token", sign(&self.sk, time))
            .part("zunfile", part);
        let resp = self.http
            .post(format!("{}/files", self.base))
            .multipart(form)
            .timeout(Duration::from_secs(600))
            .send().await.map_err(net_err)?;
        let text = resp.text().await.map_err(net_err)?;
        let v: serde_json::Value = serde_json::from_str(&text)
            .map_err(|_| "面板上传返回非 JSON".to_string())?;
        if v["status"].as_bool() == Some(false) {
            return Err(friendly(v["msg"].as_str().unwrap_or("上传失败")));
        }
        Ok(())
    }
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test bt::tests && cargo check`
预期：11 个测试 PASS。

- [ ] **步骤 5：Commit**

```bash
git add src-tauri/src/bt.rs
git commit -m "feat: 宝塔上传原语（UploadFile/分块命名/合并脚本）"
```

---

### 任务 5：pipeline 传输层抽象（SSH 行为保持重构）

**文件：**
- 修改：`src-tauri/src/pipeline.rs`（Ctx 之后加 trait/impl；run_inner 约 295-392 行；run_rollback 约 473-509 行）

本任务不引入宝塔逻辑，只把 SSH 会话包进 `Transport`，为任务 6 的分发铺路。行为必须与重构前完全一致。

- [ ] **步骤 1：添加 trait 与 SshTransport**

在 `pipeline.rs` 的 `impl Ctx { ... }` 块之后、`fn check_cancel` 之前插入：

```rust
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
```

- [ ] **步骤 2：run_inner 改用 transport**

connect 段：在 `ctx.step("connect", "done", None);`（约 311 行）之后插入一行：

```rust
    let transport = SshTransport { session };
```

随后 async 块内 3 处替换：

1. upload 循环内：
   `upload_sftp(ctx, &session, &artifact_file, &upload_name, remote_dir, ts, total, cancel).await`
   → `transport.upload(ctx, &artifact_file, &upload_name, remote_dir, ts, total, cancel).await`
2. deploy 段：
   `ssh::exec_script(&session, &script, 180)` → `transport.exec(ctx, &script, 180, cancel)`
   （其后的 `.await.map_err(|e| format!("远程部署执行失败: {e}"))?` 保留不变）
3. verify 段：
   `ssh::exec_script(&session, &scripts::health_check_script(url), 20)` → `transport.exec(ctx, &scripts::health_check_script(url), 20, cancel)`
   （其后 `.await.map_err(...)` 保留）

async 块结束后：
`let _ = session.disconnect(russh::Disconnect::ByApplication, "deploy finished", "").await;`
→ `transport.close().await;`

- [ ] **步骤 3：run_rollback 改用 transport**

`ctx.step("connect", "done", None);`（约 485 行）之后插入：

```rust
        let transport = SshTransport { session };
```

替换：
1. `ssh::exec_script(&session, &script, 180)` → `transport.exec(&ctx, &script, 180, &cancel)`（其后 `.await.map_err(|e| format!("回滚执行失败: {e}"))?` 保留）
2. `let _ = session.disconnect(russh::Disconnect::ByApplication, "rollback finished", "").await;` → `transport.close().await;`

- [ ] **步骤 4：编译 + 全量测试**

运行：`cd src-tauri && cargo check && cargo test`
预期：编译通过（无 unused 警告），任务 1-4 的 13 个测试全部 PASS。

- [ ] **步骤 5：浏览器 mock 冒烟（SSH 路径 UI 无回归）**

运行：仓库根 `npm run dev`，浏览器打开提示的本地地址 → 服务器管理页应正常显示 2 条 mock 服务器；项目页发起一次 mock 部署，7 步事件流完整走到「部署成功」。确认后 Ctrl+C 停止。

- [ ] **步骤 6：Commit**

```bash
git add src-tauri/src/pipeline.rs
git commit -m "refactor: 流水线引入 Transport 传输层抽象（SSH 实现，行为不变）"
```

---

### 任务 6：BtTransport + connect 分发 + 命令分发

**文件：**
- 修改：`src-tauri/src/pipeline.rs`
- 修改：`src-tauri/src/commands.rs:51-75`

- [ ] **步骤 1：pipeline 引入 bt 与 BtTransport**

`pipeline.rs` 顶部 `use crate::{builder, scripts, ssh, store};` 改为：

```rust
use crate::{bt, builder, scripts, ssh, store};
```

在 `SshTransport` 的 impl 块之后插入：

```rust
struct BtTransport {
    client: bt::BtClient,
    retry: u64,
}

#[async_trait::async_trait]
impl Transport for BtTransport {
    async fn exec(&self, ctx: &Ctx, script: &str, timeout_secs: u64, cancel: &AtomicBool) -> Result<(i32, String), String> {
        let o = self.client.exec_script(script, timeout_secs, cancel).await?;
        if let Some(w) = o.cleanup_warning {
            ctx.log(w, "stderr");
        }
        Ok((o.code, o.output))
    }

    async fn upload(&self, ctx: &Ctx, local_file: &Path, upload_name: &str, remote_dir: &str, ts: &str, total: u64, cancel: &Arc<AtomicBool>) -> Result<String, String> {
        const CHUNK: u64 = 8 * 1024 * 1024;
        let base = format!("{}/.deploy_tmp/{}", remote_dir.trim_end_matches('/'), ts);
        let mkdir = self.client.exec_script(&format!("mkdir -p {}", bt::sh_quote(&base)), 30, cancel).await?;
        if mkdir.code != 0 {
            return Err(format!("创建远程临时目录失败: {}", mkdir.output));
        }
        let remote_path = format!("{base}/{upload_name}");
        ctx.log(format!("宝塔 API → {remote_path}"), "info");

        let parts = (total + CHUNK - 1) / CHUNK;
        let mut file = tokio::fs::File::open(local_file).await.map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; CHUNK as usize];
        let mut names: Vec<String> = Vec::new();
        let mut uploaded: u64 = 0;
        let mut last_emit = Instant::now();
        let mut last_bytes: u64 = 0;
        for i in 0..parts {
            check_cancel(cancel)?;
            let want = CHUNK.min(total - uploaded) as usize;
            tokio::io::AsyncReadExt::read_exact(&mut file, &mut buf[..want]).await.map_err(|e| e.to_string())?;
            let name = if parts == 1 { upload_name.to_string() } else { bt::part_name(upload_name, i) };
            let mut attempt = 0u64;
            loop {
                match self.client.upload_file(&buf[..want], &base, &name).await {
                    Ok(()) => break,
                    Err(e) => {
                        if attempt >= self.retry {
                            return Err(format!("上传失败（分块 {}/{parts}，已重试 {attempt} 次）: {e}", i + 1));
                        }
                        attempt += 1;
                        ctx.log(format!("分块上传中断: {e} — 第 {attempt}/{} 次重试…", self.retry), "stderr");
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        check_cancel(cancel)?;
                    }
                }
            }
            names.push(name);
            uploaded += want as u64;
            let speed = ((uploaded - last_bytes) as f64 / last_emit.elapsed().as_secs_f64().max(0.001)) as u64;
            ctx.progress(uploaded, total, speed);
            last_emit = Instant::now();
            last_bytes = uploaded;
        }

        let verify_script = if parts == 1 {
            format!("wc -c < {}", bt::sh_quote(&remote_path))
        } else {
            bt::merge_script(&base, upload_name, &names)
        };
        let v = self.client.exec_script(&verify_script, 120, cancel).await?;
        let size: u64 = v.output.trim().parse().unwrap_or(0);
        if v.code != 0 || size != total {
            return Err(format!("上传校验失败：本地 {total} 字节，远程 {size} 字节"));
        }
        Ok(remote_path)
    }

    async fn close(&self) {}
}

async fn connect_transport(
    ctx: &Ctx,
    server: &Server,
    secret: Option<&str>,
    settings: &RunSettings,
) -> Result<Box<dyn Transport>, String> {
    if server.server_type == "bt" {
        let url = server.panel_url.as_deref().filter(|s| !s.trim().is_empty())
            .ok_or("宝塔服务器未配置面板地址")?;
        let sk = secret.filter(|s| !s.is_empty())
            .ok_or("未保存宝塔 API 密钥，请编辑服务器填写")?;
        ctx.log(format!("▶ 连接宝塔面板 {url}"), "info");
        let client = bt::BtClient::new(url, sk, server.tls_insecure)?;
        let t = client.test_connection().await;
        if !t.ok {
            return Err(format!("宝塔面板连接失败: {}", t.message));
        }
        ctx.log(format!("面板连接成功 · {}ms · {}", t.latency_ms, t.os_info.unwrap_or_default()), "info");
        Ok(Box::new(BtTransport { client, retry: settings.upload_retry }))
    } else {
        ctx.log(format!("▶ 连接服务器 {}@{}:{}", server.username, server.host, server.port), "info");
        let auth = ssh::build_auth(&server.auth_type, server.key_path.as_deref(), secret)?;
        let session = tokio::time::timeout(
            std::time::Duration::from_secs(settings.ssh_timeout),
            ssh::connect_and_auth(&server.host, server.port, &server.username, auth),
        )
        .await
        .map_err(|_| format!("SSH 连接超时（{} 秒）", settings.ssh_timeout))
        .and_then(|r| r.map_err(|e| format!("SSH 连接失败: {e}")))?;
        Ok(Box::new(SshTransport { session }))
    }
}
```

- [ ] **步骤 2：run_inner connect 段替换为分发**

将 run_inner 中从 `ctx.log(format!("▶ 连接服务器 ...` 到 `ctx.step("connect", "done", None);` 的整段（含 `let auth = ...`、`let session = ...`、`let transport = SshTransport { session };`）替换为：

```rust
    let transport = connect_transport(ctx, server, secret, settings).await.map_err(|e| {
        ctx.step("connect", "failed", None);
        e
    })?;
    ctx.step("connect", "done", None);
```

（保留其前的 `*current_step = "connect".into(); ctx.step("connect", "running", None);`。）

- [ ] **步骤 3：run_rollback connect 段替换为分发**

将 run_rollback 中从 `ctx.log(format!("▶ 连接服务器 ...` 到 `let transport = SshTransport { session };` 的整段替换为：

```rust
        let transport = connect_transport(&ctx, &server, secret.as_deref(), &settings).await?;
```

（保留其前的 `ctx.step("connect", "running", None);` 与其后的 `ctx.step("connect", "done", None);`。）

- [ ] **步骤 4：commands.rs 测试连接分发**

`commands.rs` 顶部 `use crate::{builder, ssh, store, vault};` 改为：

```rust
use crate::{bt, builder, ssh, store, vault};
```

`test_server` 中，取到 `(server, secret)` 之后、`let auth = ssh::build_auth(...)` 之前插入：

```rust
    if server.server_type == "bt" {
        let url = server.panel_url.as_deref().filter(|s| !s.trim().is_empty())
            .ok_or("未配置面板地址")?;
        let sk = secret.as_deref().filter(|s| !s.is_empty())
            .ok_or("未保存 API 密钥")?;
        let client = bt::BtClient::new(url, sk, server.tls_insecure)?;
        return Ok(client.test_connection().await);
    }
```

`test_server_draft` 开头（`let auth = ...` 之前）插入：

```rust
    if input.server_type == "bt" {
        let url = input.panel_url.as_deref().filter(|s| !s.trim().is_empty())
            .ok_or("未配置面板地址")?;
        let sk = input.secret.as_deref().filter(|s| !s.is_empty())
            .ok_or("请填写 API 密钥")?;
        let client = bt::BtClient::new(url, sk, input.tls_insecure)?;
        return Ok(client.test_connection().await);
    }
```

- [ ] **步骤 5：编译 + 全量测试**

运行：`cd src-tauri && cargo check && cargo test`
预期：编译通过、13 个测试 PASS。

- [ ] **步骤 6：Commit**

```bash
git add src-tauri/src/pipeline.rs src-tauri/src/commands.rs
git commit -m "feat: 宝塔传输层接入流水线与连接测试分发"
```

---

### 任务 7：前端（类型 + mock + 三个组件）

**文件：**
- 修改：`src/ipc/types.ts:1-22`
- 修改：`src/ipc/index.ts:22-72`
- 修改：`src/components/ServerModal.vue`（整体重写）
- 修改：`src/views/ServersView.vue`
- 修改：`src/components/ProfileModal.vue`

- [ ] **步骤 1：扩展 types.ts**

`Server` 接口改为：

```ts
export interface Server {
  id: number;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_type: "password" | "key" | "apikey";
  key_path: string | null;
  has_credential: boolean;
  created_at: string;
  server_type: "ssh" | "bt";
  panel_url: string | null;
  tls_insecure: boolean;
}
```

`ServerInput` 接口改为：

```ts
export interface ServerInput {
  id?: number;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_type: "password" | "key" | "apikey";
  key_path: string | null;
  secret?: string;
  server_type: "ssh" | "bt";
  panel_url?: string | null;
  tls_insecure?: boolean;
}
```

- [ ] **步骤 2：更新 mock**

`src/ipc/index.ts`：两条现有 mockServers 各追加字段 `server_type: "ssh", panel_url: null, tls_insecure: false,`；数组末尾追加第三条：

```ts
  {
    id: 3, name: "宝塔面板（生产）", host: "47.98.12.34", port: 0, username: "",
    auth_type: "apikey", key_path: null,
    has_credential: true, created_at: "2026-11-18 10:08:00",
    server_type: "bt", panel_url: "https://47.98.12.34:8888", tls_insecure: true,
  },
```

`save_server` mock 的新建分支（`const s: Server = { ...input, ... }`）改为：

```ts
    const s: Server = {
      ...input,
      id: ++mockSeq,
      key_path: input.key_path ?? null,
      panel_url: input.panel_url ?? null,
      tls_insecure: input.tls_insecure ?? false,
      has_credential: !!input.secret,
      created_at: new Date().toISOString().slice(0, 19).replace("T", " "),
    };
```

- [ ] **步骤 3：重写 ServerModal.vue**

整文件替换为：

```vue
<script setup lang="ts">
import { reactive, ref } from "vue";
import { api, pickDirectory } from "@/ipc";
import { credStore } from "@/platform";
import type { Server, ServerInput, TestResult } from "@/ipc/types";

const props = defineProps<{ server: Server | null }>();
const emit = defineEmits<{ (e: "close"): void; (e: "saved"): void }>();

const form = reactive({
  server_type: (props.server?.server_type ?? "ssh") as "ssh" | "bt",
  name: props.server?.name ?? "",
  host: props.server?.host ?? "",
  port: props.server?.port ?? 22,
  username: props.server?.username ?? "root",
  auth_type: (props.server?.auth_type === "key" ? "key" : "password") as "password" | "key",
  key_path: props.server?.key_path ?? "",
  panel_url: props.server?.panel_url ?? "",
  tls_insecure: props.server?.tls_insecure ?? false,
  secret: "",
});
const saving = ref(false);
const testing = ref(false);
const error = ref("");
const testResult = ref<TestResult | null>(null);

function toInput(): ServerInput {
  if (form.server_type === "bt") {
    let host = "";
    try {
      host = new URL(form.panel_url.trim()).hostname;
    } catch {
      host = "";
    }
    return {
      id: props.server?.id,
      name: form.name.trim(),
      host,
      port: 0,
      username: "",
      auth_type: "apikey",
      key_path: null,
      secret: form.secret || undefined,
      server_type: "bt",
      panel_url: form.panel_url.trim(),
      tls_insecure: form.tls_insecure,
    };
  }
  return {
    id: props.server?.id,
    name: form.name.trim(),
    host: form.host.trim(),
    port: Number(form.port) || 22,
    username: form.username.trim(),
    auth_type: form.auth_type,
    key_path: form.auth_type === "key" ? form.key_path.trim() || null : null,
    secret: form.secret || undefined,
    server_type: "ssh",
    panel_url: null,
    tls_insecure: false,
  };
}

function validate(): boolean {
  error.value = "";
  if (!form.name.trim()) {
    error.value = "请填写名称";
    return false;
  }
  if (form.server_type === "bt") {
    try {
      const u = new URL(form.panel_url.trim());
      if (u.protocol !== "http:" && u.protocol !== "https:") throw new Error("scheme");
    } catch {
      error.value = "面板地址须为完整 http(s) URL，如 https://1.2.3.4:8888";
      return false;
    }
    if (!props.server && !form.secret) {
      error.value = "请填写 API 密钥";
      return false;
    }
    return true;
  }
  if (!form.host.trim() || !form.username.trim()) {
    error.value = "请填写主机地址和用户名";
    return false;
  }
  if (form.auth_type === "key" && !form.key_path.trim()) {
    error.value = "请选择私钥文件";
    return false;
  }
  if (!props.server && form.auth_type === "password" && !form.secret) {
    error.value = "请填写密码";
    return false;
  }
  return true;
}

async function browseKey() {
  const dir = await pickDirectory();
  if (dir) form.key_path = dir;
}

async function test() {
  if (!validate()) return;
  testing.value = true;
  testResult.value = null;
  try {
    testResult.value =
      props.server && !form.secret
        ? await api.testServer(props.server.id)
        : await api.testServerDraft(toInput());
  } catch (e) {
    testResult.value = { ok: false, latency_ms: 0, os_info: null, message: String(e) };
  } finally {
    testing.value = false;
  }
}

async function save() {
  if (!validate()) return;
  saving.value = true;
  try {
    await api.saveServer(toInput());
    emit("saved");
    emit("close");
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div class="overlay">
    <div class="modal">
      <div class="m-title">{{ server ? "编辑服务器" : "添加服务器" }}</div>
      <div class="form-grid">
        <div class="field full">
          <label>连接方式</label>
          <div class="seg">
            <button class="btn sm" :class="form.server_type === 'ssh' ? 'primary' : 'ghost'" @click="form.server_type = 'ssh'">SSH</button>
            <button class="btn sm" :class="form.server_type === 'bt' ? 'primary' : 'ghost'" @click="form.server_type = 'bt'">宝塔面板</button>
          </div>
        </div>
        <div class="field full">
          <label>显示名称 <span class="req">*</span></label>
          <input v-model="form.name" class="input" placeholder="如：生产服务器" />
        </div>
        <template v-if="form.server_type === 'ssh'">
          <div class="field">
            <label>主机地址 <span class="req">*</span></label>
            <input v-model="form.host" class="input mono" placeholder="IP 或域名" />
          </div>
          <div class="field">
            <label>端口</label>
            <input v-model.number="form.port" class="input mono" type="number" />
          </div>
          <div class="field">
            <label>用户名 <span class="req">*</span></label>
            <input v-model="form.username" class="input mono" placeholder="root" />
          </div>
          <div class="field">
            <label>认证方式</label>
            <div class="seg">
              <button class="btn sm" :class="form.auth_type === 'password' ? 'primary' : 'ghost'" @click="form.auth_type = 'password'">密码</button>
              <button class="btn sm" :class="form.auth_type === 'key' ? 'primary' : 'ghost'" @click="form.auth_type = 'key'">私钥</button>
            </div>
          </div>
          <div v-if="form.auth_type === 'key'" class="field full">
            <label>私钥文件 <span class="req">*</span></label>
            <div class="input-wrap">
              <input v-model="form.key_path" class="input mono" placeholder="C:\Users\me\.ssh\id_ed25519" />
              <button class="btn ghost sm" @click="browseKey">浏览…</button>
            </div>
          </div>
        </template>
        <template v-else>
          <div class="field full">
            <label>面板地址 <span class="req">*</span></label>
            <input v-model="form.panel_url" class="input mono" placeholder="https://1.2.3.4:8888" />
            <div class="hint">需先在 面板设置 → API 接口 开启接口，并将本机公网 IP 加入白名单</div>
          </div>
          <div class="field full">
            <label class="check">
              <input type="checkbox" v-model="form.tls_insecure" />
              允许自签证书（面板使用自签 HTTPS 证书时勾选）
            </label>
          </div>
        </template>
        <div class="field full">
          <label>
            {{ form.server_type === "bt" ? "API 密钥" : form.auth_type === "key" ? "私钥口令（无口令留空）" : "密码" }}
            <span v-if="!server && (form.server_type === 'bt' || form.auth_type === 'password')" class="req">*</span>
          </label>
          <input v-model="form.secret" class="input mono" type="password" :placeholder="server ? '留空 = 保留已保存的凭据' : ''" autocomplete="new-password" />
          <div class="hint">凭据将加密保存到 {{ credStore }}，不写入配置文件</div>
        </div>
      </div>

      <div v-if="testResult" :class="testResult.ok ? 'm-ok' : 'm-error'">
        {{ testResult.ok ? `✓ 连接成功 · ${testResult.latency_ms}ms${testResult.os_info ? " · " + testResult.os_info : ""}` : `✕ ${testResult.message}` }}
      </div>
      <div v-if="error" class="m-error">{{ error }}</div>

      <div class="m-actions">
        <button class="btn ghost" :disabled="testing" @click="test">{{ testing ? "连接中…" : "测试连接" }}</button>
        <button class="btn ghost" @click="emit('close')">取消</button>
        <button class="btn primary" :disabled="saving" @click="save">{{ saving ? "保存中…" : "保存" }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.check {
  display: flex; align-items: center; gap: 8px;
  font-size: 12.5px; color: var(--faint); cursor: pointer;
}
</style>
```

- [ ] **步骤 4：ServersView.vue 展示宝塔**

页面副标题（`page-sub`）改为：

```html
        <div class="page-sub">SSH / 宝塔面板连接配置 · 凭据加密保存在 {{ credStore }}</div>
```

`s-head` 内、状态 tag 之前插入类型徽标：

```html
          <span class="tag blue">{{ s.server_type === "bt" ? "宝塔" : "SSH" }}</span>
```

主机/端口/用户名三行 kv 与认证方式 kv 替换为：

```html
        <template v-if="s.server_type === 'bt'">
          <div class="s-kv"><span class="k">面板地址</span><span class="v mono">{{ s.panel_url }}</span></div>
          <div class="s-kv">
            <span class="k">API 密钥</span>
            <span class="v mono" style="color: var(--faint)">{{ s.has_credential ? "••••••••" : "未设置" }}</span>
          </div>
        </template>
        <template v-else>
          <div class="s-kv"><span class="k">主机</span><span class="v mono">{{ s.host }}</span></div>
          <div class="s-kv"><span class="k">端口</span><span class="v mono">{{ s.port }}</span></div>
          <div class="s-kv"><span class="k">用户名</span><span class="v mono">{{ s.username }}</span></div>
          <div class="s-kv">
            <span class="k">认证方式</span>
            <span class="v">
              {{ s.auth_type === "key" ? "私钥" : "密码" }}
              <span v-if="s.auth_type === 'key'" class="mono" style="color: var(--faint)">{{ s.key_path }}</span>
              <span v-else class="mono" style="color: var(--faint)">{{ s.has_credential ? "••••••••" : "未设置" }}</span>
            </span>
          </div>
        </template>
```

空状态描述（`e-desc`）改为：

```html
      <div class="e-desc">添加一台 Linux 服务器（SSH 密码/私钥），或一个宝塔面板（API 密钥，无需 SSH），之后即可将项目产物一键部署上去。</div>
```

- [ ] **步骤 5：ProfileModal.vue 宝塔默认值**

script 区：import 行改为：

```ts
import { computed, onMounted, reactive, ref } from "vue";
import { api } from "@/ipc";
import { useAppStore } from "@/stores/app";
import type { DeployProfile, Project } from "@/ipc/types";
```

`const defaults = ...` 静态定义替换为：

```ts
const store = useAppStore();
const isBt = computed(() => store.servers.find((s) => s.id === props.serverId)?.server_type === "bt");
const defaults = computed(() =>
  props.project.type === "java"
    ? { restart: `${isBt.value ? "" : "sudo "}systemctl restart ${props.project.name}`, health: "http://127.0.0.1:8080/actuator/health" }
    : { restart: isBt.value ? "" : "sudo systemctl reload nginx", health: "" },
);
```

form 初始化两处引用改为 `defaults.value.restart` / `defaults.value.health`。

template 中重启命令的 hint 行替换为：

```html
          <div class="hint">{{ isBt ? "宝塔模式以 root 执行，无需 sudo；备份目录请放在站点根目录之外，避免被公网下载" : "需要 sudo 时请为目标命令配置 NOPASSWD，工具不支持交互式输入 sudo 密码" }}</div>
```

（template 中 `:placeholder="defaults.restart"` 与 `defaults.health` 无需改动——computed 在模板中自动解包。）

- [ ] **步骤 6：类型检查 + 构建**

运行：仓库根 `npm run build`
预期：vue-tsc 无错误，vite 构建成功。

- [ ] **步骤 7：浏览器 mock golden path**

运行：`npm run dev`，浏览器逐项验证：
1. 服务器页显示 3 条卡片，第三条带「宝塔」徽标、面板地址、API 密钥 ••••；
2. 「添加服务器」→ 切「宝塔面板」→ 只出现 名称/面板地址/自签勾选/API 密钥；面板地址填 `abc` 点保存 → 提示 URL 校验错误；填 `https://1.2.3.4:8888` + 密钥 → 测试连接成功、保存成功；
3. 编辑宝塔服务器：密钥留空保存 → 凭据保留（•••• 仍显示）；
4. SSH 添加/编辑表单与原来一致；
5. 项目页对 mock 项目发起部署 → 事件流完整成功。
确认后 Ctrl+C 停止。

- [ ] **步骤 8：Commit**

```bash
git add src/ipc/types.ts src/ipc/index.ts src/components/ServerModal.vue src/views/ServersView.vue src/components/ProfileModal.vue
git commit -m "feat: 前端支持宝塔面板服务器（表单/列表/部署配置默认值）"
```

---

### 任务 8：README + 全量验证 + 真机验收清单

**文件：**
- 修改：`README.md`

- [ ] **步骤 1：更新 README**

「功能特性」列表中「服务器管理」一条改为：

```markdown
- **服务器管理**：SSH 密码 / 私钥认证，连接测试；**宝塔面板 API 模式（无需 SSH）**：面板设置开启 API 接口 + IP 白名单即可部署；凭据存 Windows 凭据管理器，数据库不落明文
```

- [ ] **步骤 2：全量验证**

运行：

```bash
cd src-tauri && cargo check && cargo test
cd .. && npm run build
```

预期：cargo 无警告、13 个测试 PASS；vue-tsc + vite 构建成功。

- [ ] **步骤 3：Commit**

```bash
git add README.md
git commit -m "docs: README 补充宝塔面板 API 部署模式"
```

- [ ] **步骤 4：真机验收（需用户面板，手动执行并记录结果）**

前置：面板设置 → API 接口已开启，本机公网 IP 已加白名单；`npm run tauri dev` 启动应用。

| # | 场景 | 预期 |
|---|---|---|
| 1 | 添加宝塔服务器 + 测试连接 | 成功，显示面板版本与延迟 |
| 2 | 前端项目部署（remote_dir=/www/wwwroot，site_dir=站点目录名，重启命令留空） | 8 步全绿，站点内容更新，`.backups` 生成于站点根目录之外 |
| 3 | 前端回滚 | 站点恢复旧版 |
| 4 | Java 项目部署（restart_command 按实际服务填写） | jar 替换 + 重启成功 + 健康检查通过 |
| 5 | Java 回滚 | 旧 jar 恢复并重启 |
| 6 | 错误 API 密钥测试连接 | 失败提示含「检查 API 密钥 / 时间误差」指引 |
| 7 | 白名单移除本机 IP 后测试连接 | 失败提示含「加入白名单」指引 |
| 8 | >8MB jar 部署 | 分块上传进度正常，合并校验一致 |
| 9 | 部署中途取消 | 任务停止，面板计划任务列表无 yunsuo-* 残留 |
| 10 | 面板计划任务列表检查（每次部署后） | 无 yunsuo-* 任务残留 |

任何一项失败：记录日志（应用日志目录 + 面板计划任务日志），修复后重跑该项。

---

## 自检记录（编写者已完成）

- **规格覆盖度**：§4 数据模型→任务 1；§5.1 签名/规范化→任务 2；§5.2 exec→任务 3；§5.3 upload→任务 4+6；§5.4 test→任务 2；§6 trait/分发→任务 5+6；§7 前端→任务 7；§8 错误处理→任务 2（friendly/net_err）+3（超时/必删）+6（重试/校验）；§9 测试→各任务单测 + 任务 8 验收；§10 README→任务 8。无遗漏。
- **占位符扫描**：无 TODO/待定；所有代码步骤含完整代码。
- **类型一致性**：`Transport::exec(&self, &Ctx, &str, u64, &AtomicBool)` 在任务 5 定义、任务 6 两实现一致；`ExecOutcome{code,output,cleanup_warning}` 任务 3 定义、任务 6 使用一致；`part_name/merge_script/sh_quote/upload_file/exec_script` 签名跨任务一致；前端 `server_type/panel_url/tls_insecure` 与 Rust serde 字段名一致。