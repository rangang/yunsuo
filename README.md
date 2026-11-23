# 云梭（YunSuo）

一键打包部署桌面工具 —— 让发版像穿梭一样快。

面向「AI Agent 开发完顺手部署」场景的轻量级桌面 CI/CD：配置一次，之后每次发版点一个按钮，自动完成 **构建 → 打包 → SFTP 上传 → 备份 → 原子替换 → 重启 → 健康检查**，失败可一键回滚。

## 功能特性

- **项目管理**：自动识别前端（package.json）/ Java（pom.xml / build.gradle）项目，Maven 自动探测（PATH / MAVEN_HOME / IDEA 内置）
- **服务器管理**：SSH 密码 / 私钥认证，连接测试；凭据存 Windows 凭据管理器，数据库不落明文
- **一键部署流水线**：构建日志实时流式输出（UTF-8/GBK 自适应）、产物打包、SFTP 分块上传（进度/速度）、远程备份 + 原子替换 + 重启、健康检查
- **多环境**：项目 × 服务器 × 环境三维配置，生产环境部署二次确认
- **一键回滚**：保留最近 N 份备份，面板/历史页均可回滚
- **部署历史**：完整记录 + 日志落盘 + 导出，成功率/耗时统计
- **桌面通知 / 托盘 / 开机自启 / 深浅主题 / 日志脱敏**

## 技术栈

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri 2 |
| 后端 | Rust（russh / russh-sftp / tokio / rusqlite / keyring） |
| 前端 | Vue 3 + TypeScript + Pinia + Vite |
| 安装包 | NSIS（简体中文） |

## 开发

环境要求：Node.js 18+、Rust（MSVC 工具链）、VS Build Tools（C++ 工作负载）。

```bash
npm install
npm run tauri dev    # 开发运行
npm run build        # 前端类型检查 + 构建
npm run tauri build  # 打 NSIS 安装包
```

浏览器调试模式：`npm run dev`（不依赖 Rust，内置 mock 数据）。

## 目录结构

```
src/            Vue 前端（视图/组件/IPC/状态）
src-tauri/      Rust 后端
  src/
    builder.rs    本地构建执行器（流式日志、进程树取消）
    pipeline.rs   部署流水线状态机
    scripts.rs    远程部署/回滚脚本生成
    ssh.rs        SSH/SFTP（russh）
    store.rs      SQLite 存储与迁移
    vault.rs      凭据保险箱（keyring）
    commands.rs   Tauri IPC 命令
```

## 下载安装

前往 [Releases](https://github.com/rangang/yunsuo/releases) 下载最新 Windows x64 安装包（NSIS，简体中文），双击安装即可。Win10/11 一般已自带 WebView2 运行时。

## License

[MIT](./LICENSE)