# 宝塔 API 部署模式 设计规格

日期：2026-11-25
状态：已经用户逐节批准
范围：deploypilot（云梭）v0.1.x

## 1. 背景与目标

云梭目前仅支持 SSH/SFTP 部署。只有宝塔面板管理员账号（无服务器 SSH 凭据）的用户无法使用。本功能新增「宝塔面板 HTTP API」服务器类型，使这类用户无需 SSH 即可完成与 SSH 模式语义一致的部署、回滚、健康检查。

已确认的前提与范围：

- 用户是宝塔管理员，可在「面板设置 → API 接口」开启 API 并管理 IP 白名单。
- 前端静态站点与 Java jar 两种项目类型都要支持。
- 技术路线：官方 API「上传 + 计划任务执行 shell」双原语（方案 A），复用现有部署脚本。

明确不做（范围外）：模拟面板登录、宝塔站点列表自动读取、分片并行上传、宝塔 Java 项目管理器对接、为宝塔单独设计部署脚本。

## 2. 宝塔 API 事实依据

来源：docs.bt.cn/api（基于面板 v11.7.0 源码实测；官方声明接口可能随版本变更）。

- 请求方式：POST，`application/x-www-form-urlencoded`（上传为 multipart）。
- 认证：每次请求携带 `request_time`（秒级时间戳）与 `request_token = md5(request_time + md5(api_sk))`。api_sk 在面板设置 → API 接口获取。本机与服务器时差过大会导致签名失败。
- IP 白名单：面板侧配置；调用方 IP 不在白名单时请求被拒绝。
- 上传：`POST /files`，`action=UploadFile`，multipart 字段 `path`（目标目录，绝对路径）、`zunfile`（文件内容）。文档未提供分片参数，单次上传大小限制未知。
- 执行 shell（无直接 ExecShell 接口，经计划任务模块）：
  1. `POST /crontab` `action=AddCrontab`：`name`、`type`（周期类型）、`where1`（周期值）、`sType=toShell`、`sBody`（脚本内容）→ 返回 `id`。
  2. `POST /crontab` `action=set_execute_script`，参数 `id`：立即执行该任务。
  3. `POST /crontab` `action=GetLogs`，参数 `id`：日志文本在响应 `msg` 字段。
  4. `POST /crontab` `action=DelCrontab`，参数 `id`：删除任务。
- 连通性测试：`POST /system` `action=GetSystemTotal`（系统信息，含面板版本）。
- 业务错误：HTTP 200 但 JSON `status:false`，错误信息在 `msg`。

## 3. 总体架构

新增 Rust 模块 `src-tauri/src/bt.rs`（宝塔 HTTP 客户端），流水线引入传输层 trait，SSH 与宝塔为两个实现。本地步骤（prepare/build/package）、`scripts.rs` 全部脚本、备份/原子替换/回滚/健康检查语义不变。

一次宝塔部署的远程动作序列：

```
exec(mkdir -p $APP_DIR/.deploy_tmp/$TS)     ← 计划任务通道
upload(dist.zip / app.jar)                  ← UploadFile（大文件切块 + cat 合并 + 字节校验）
exec(deploy_script)                         ← 与 SSH 模式同一份脚本
exec(health_check_script)                   ← 远端 curl，同现有逻辑
```

回滚同构：`exec(rollback_script)`。哨兵协议（`DEPLOY_OK` / `NO_BACKUP` / `RESTART_FAILED`）两种通道通用。

## 4. 数据模型与凭据

### 4.1 server 表新增列（忽略式 ALTER 迁移，照 store.rs 现有模式追加）

| 列 | 类型 | 默认 | 说明 |
|---|---|---|---|
| server_type | TEXT NOT NULL | 'ssh' | 取值 `ssh` \| `bt` |
| panel_url | TEXT | NULL | 宝塔面板完整地址，如 `https://1.2.3.4:8888`，允许含安全入口路径 |
| tls_insecure | INTEGER NOT NULL | 0 | 1 = 接受自签/无效证书，须用户显式勾选 |

同步修改：`SERVER_COLS`、`row_to_server` 列下标、insert/update SQL、`model.rs` 的 `Server`/`ServerInput`、前端 `types.ts`。

### 4.2 字段语义

- 宝塔服务器：`auth_type` 存 `"apikey"`；`host` 存面板主机名（仅列表展示用）、`port` 存 0、`username` 存空串、`key_path` 为 NULL，均不参与连接逻辑，连接一律用 `panel_url`。
- SSH 服务器：所有字段与行为不变；存量数据迁移后 `server_type='ssh'`。

### 4.3 凭据

API 密钥复用现有 vault 插槽 `DeployPilot / server-{id}`（vault.rs 零改动）。密钥不落 SQLite；日志脱敏沿用现有 `Ctx.mask` 机制（mask 值即 api_sk）。`has_credential` 语义不变。

## 5. 宝塔客户端（bt.rs）

新增依赖：`reqwest`（features：rustls-tls、multipart；禁默认 native-tls）、`md5`。

### 5.1 构造与签名

`BtClient::new(panel_url, api_sk, tls_insecure)`。内部统一 `call(module, action, params) -> Result<serde_json::Value, String>`：

- 附加 `request_time` / `request_token`（每次请求实时计算）。
- `tls_insecure` 时配置 reqwest 接受无效证书（仅此服务器连接）。
- 解析 JSON；`status:false` → `Err(msg)`；网络/证书错误转为带指引的中文提示（见 §8）。

### 5.2 exec_script(script, timeout_secs) -> Result<(i32, String), String>

1. 包装脚本：`( 原脚本\n); echo "YUNSUO_EXIT=$?"`（子 shell 隔离原脚本的 `set -e` / `exit`，保证退出码标记必达）。
2. `AddCrontab`：`name="yunsuo-<ts>"`、`sType=toShell`、`sBody=包装脚本`、周期 `type=minute-n`、`where1=30`（30 分钟一次，避免执行期间被周期误触发）。
3. `set_execute_script(id)` 立即执行。
4. 每 1 秒轮询 `GetLogs(id)`，直到输出含 `YUNSUO_EXIT=` 标记或超时；解析标记得退出码，标记之前的文本为输出。
5. **无论成功、失败、超时、取消，必调 `DelCrontab(id)`**（删除失败仅记日志，不影响主结果）。

接口与 `ssh::exec_script` 的 `(i32, String)` 对齐，pipeline 判定逻辑零改动。

### 5.3 upload（签名与 §6.1 trait 一致：ctx, local_file, upload_name, remote_dir, ts, total, cancel）-> Result<String, String>

- 目标路径：`{remote_dir}/.deploy_tmp/{ts}/{upload_name}`（目录由前置 exec 创建）。
- ≤8MB：单次 `UploadFile` 直传，文件名即 `upload_name`。
- >8MB：本地按 8MB 切块，逐块上传为 `{upload_name}.part0001...`（序号 4 位补零）；全部完成后 exec：`cat 分块 > upload_name && rm 分块 && wc -c < upload_name`，校验字节数与本地一致，不一致报错。
- 每块失败按现有 `upload_retry` 设置整块重试；块边界检查 cancel。
- 进度：复用现有 `Progress` 事件（uploaded/total/speed_bps），按已确认块字节数累计上报。
- 返回 remote_path 供 deploy 脚本使用。

### 5.4 test_connection() -> TestResult

调 `GetSystemTotal`，返回 ok/latency_ms/os_info（面板版本/系统信息）/message，复用现有 `TestResult` 结构。

## 6. 流水线接入（pipeline.rs / commands.rs）

### 6.1 传输层 trait（async-trait，已在依赖中）

```rust
trait Transport {
    async fn exec_script(&self, script: &str, timeout_secs: u64) -> Result<(i32, String), String>;
    async fn upload(&self, ctx: &Ctx, local_file: &Path, upload_name: &str,
                    remote_dir: &str, ts: &str, total: u64,
                    cancel: &Arc<AtomicBool>) -> Result<String, String>;
    async fn close(self);
}
```

- SSH 实现：包装 `russh::client::Handle<TestHandler>`；现有 `upload_sftp` 逻辑原样搬入；`close` 执行现有 disconnect。
- 宝塔实现：包装 `BtClient`；`close` 为空操作（HTTP 无长连接）。
- connect 步骤按 `server.server_type` 分发构造；upload/deploy/verify/rollback 步骤只调 trait 方法。`scripts.rs` 一行不改。

### 6.2 commands.rs

- `test_server` / `test_server_draft`：按 `server_type` 分发（SSH 走现有 `ssh::test_connection`，宝塔走 `BtClient::test_connection`）。
- `start_deploy` / `rollback` 的现有校验不变：remote_dir 绝对路径、Java 必填 restart_command、生产二次确认。
- 取消语义：上传在块边界检查 cancel；exec 轮询循环检查 cancel，取消时删任务后退出。

## 7. 前端 UI

- `ServerModal.vue`：顶部新增「连接方式」切换 **SSH / 宝塔面板**。
  - SSH 分支：现有表单不动。
  - 宝塔分支：名称、面板地址（placeholder `https://1.2.3.4:8888`）、API 密钥（密码框，编辑态留空=保留旧凭据，同现有语义）、☐ 允许自签证书（默认不勾）。隐藏 host/port/username/私钥行。
  - 校验：面板地址须为 http(s) URL；新增时 API 密钥必填。
- `ServersView.vue`：列表加类型徽标（SSH / 宝塔）；宝塔行显示 panel_url。
- `ProfileModal.vue`：字段不变；所选服务器为宝塔时默认重启命令去掉 `sudo`（计划任务以 root 执行），并显示提示「备份目录请放在站点根目录之外，避免被公网下载」。
- `ipc/types.ts`：`Server`/`ServerInput` 增加 `server_type`、`panel_url`、`tls_insecure`；`auth_type` 联合类型加 `"apikey"`。
- 浏览器 mock（`ipc/index.ts`）：加一条宝塔样例服务器；`test_server*` mock 维持恒成功；部署事件流不变。

## 8. 错误处理

所有错误透传到现有日志/步骤 UI。

| 场景 | 行为 |
|---|---|
| 面板返回 status:false | 直接展示面板 msg |
| 签名/时间戳错误 | 提示「检查 API 密钥与本机时间（与服务器误差需小于 60 秒）」 |
| IP 不在白名单 | 提示「到面板设置 → API 接口，把本机公网 IP 加入白名单」 |
| 自签证书握手失败 | 提示「如面板使用自签证书，请勾选『允许自签证书』」 |
| exec 超时 | 删任务 → 部署失败，沿用现有「建议回滚」路径 |
| 上传块重试耗尽 | 部署失败，沿用现有重试/报错路径 |
| DelCrontab 失败 | 仅记警告日志，不改变主流程结果 |

## 9. 测试与验收

- Rust 单测（bt.rs 内）：签名已知向量；切块命名与合并脚本生成；`YUNSUO_EXIT` 输出解析（含脚本中途 exit、输出含干扰文本）；轮询终止条件。
- `cargo check` 与 `npm run build`（vue-tsc）通过。
- 浏览器 mock 模式：宝塔服务器增删改、测试连接、部署事件流 UI golden path。
- 真机验收（需用户面板）：前端部署+回滚、Java 部署+回滚；失败路径：错误密钥、IP 白名单拦截、上传中断网。

## 10. 改动文件清单

| 文件 | 改动 |
|---|---|
| src-tauri/Cargo.toml | + reqwest、md5 |
| src-tauri/src/bt.rs | 新增：客户端/签名/exec/upload/test |
| src-tauri/src/model.rs | Server/ServerInput + server_type/panel_url/tls_insecure |
| src-tauri/src/store.rs | 迁移 + SERVER_COLS + row_to_server + insert/update |
| src-tauri/src/pipeline.rs | Transport trait + SSH/BT 两实现 + connect 分发 |
| src-tauri/src/commands.rs | test_server* 分发 |
| src-tauri/src/main.rs | mod bt（如需注册命令则同步） |
| src/ipc/types.ts | 类型扩展 |
| src/ipc/index.ts | mock 样例 |
| src/components/ServerModal.vue | 连接方式切换 + 宝塔表单 |
| src/views/ServersView.vue | 类型徽标 |
| src/components/ProfileModal.vue | 宝塔默认值与提示 |

不改动：scripts.rs、ssh.rs、vault.rs、builder.rs、store 其余表。