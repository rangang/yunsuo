//! IPC 封装：Tauri 环境走 invoke；纯浏览器（npm run dev 调试 UI）走内存 mock。
import { invoke } from "@tauri-apps/api/core";
import type {
  DeployEvent,
  DeployProfile,
  DetectResult,
  HistoryRecord,
  Project,
  ProjectInput,
  Server,
  ServerInput,
  TestResult,
} from "./types";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/* ---------- 浏览器 mock（仅 UI 调试用） ---------- */
type DeployListener = (e: DeployEvent) => void;
const mockListeners = new Set<DeployListener>();

let mockSeq = 3;
const mockServers: Server[] = [
  {
    id: 1, name: "生产服务器", host: "47.98.12.34", port: 22, username: "root",
    auth_type: "key", key_path: "C:\\Users\\me\\.ssh\\id_ed25519",
    has_credential: true, created_at: "2026-11-18 10:00:00",
  },
  {
    id: 2, name: "测试服务器", host: "192.168.1.100", port: 22, username: "deploy",
    auth_type: "password", key_path: null,
    has_credential: true, created_at: "2026-11-18 10:05:00",
  },
];
const mockProjects: Project[] = [
  {
    id: 1, name: "my-vue-app", type: "frontend", local_path: "D:\\projects\\my-vue-app",
    build_command: "npm run build", artifact_path: "dist",
    pre_build_command: null, created_at: "2026-11-18 10:10:00",
  },
  {
    id: 2, name: "order-service", type: "java", local_path: "D:\\projects\\order-service",
    build_command: "mvn -DskipTests package", artifact_path: "target/*.jar",
    pre_build_command: null, created_at: "2026-11-18 10:12:00",
  },
];
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

const mockApi = {
  async list_servers(): Promise<Server[]> {
    await delay(80);
    return [...mockServers];
  },
  async save_server({ input }: { input: ServerInput }): Promise<Server> {
    await delay(80);
    if (input.id) {
      const s = mockServers.find((x) => x.id === input.id)!;
      Object.assign(s, input, {
        has_credential: s.has_credential || !!input.secret,
      });
      return { ...s };
    }
    const s: Server = {
      ...input,
      id: ++mockSeq,
      key_path: input.key_path ?? null,
      has_credential: !!input.secret,
      created_at: new Date().toISOString().slice(0, 19).replace("T", " "),
    };
    mockServers.push(s);
    return { ...s };
  },
  async delete_server({ id }: { id: number }): Promise<void> {
    const i = mockServers.findIndex((x) => x.id === id);
    if (i >= 0) mockServers.splice(i, 1);
  },
  async test_server(): Promise<TestResult> {
    await delay(600);
    return { ok: true, latency_ms: 32, os_info: "Linux 5.15.0 (mock)", message: "连接成功" };
  },
  async test_server_draft(): Promise<TestResult> {
    await delay(600);
    return { ok: true, latency_ms: 45, os_info: "Linux 5.15.0 (mock)", message: "连接成功" };
  },
  async list_projects(): Promise<Project[]> {
    await delay(80);
    return [...mockProjects];
  },
  async save_project({ input }: { input: ProjectInput }): Promise<Project> {
    await delay(80);
    if (input.id) {
      const p = mockProjects.find((x) => x.id === input.id)!;
      Object.assign(p, input);
      return { ...p };
    }
    const p: Project = {
      ...input,
      id: ++mockSeq,
      created_at: new Date().toISOString().slice(0, 19).replace("T", " "),
    };
    mockProjects.push(p);
    return { ...p };
  },
  async delete_project({ id }: { id: number }): Promise<void> {
    const i = mockProjects.findIndex((x) => x.id === id);
    if (i >= 0) mockProjects.splice(i, 1);
  },
  async detect_project({ path }: { path: string }): Promise<DetectResult> {
    const name = path.split(/[\\/]/).filter(Boolean).pop() ?? null;
    return { type: "frontend", build_command: "npm run build", artifact_path: "dist", name };
  },
  async get_profile(): Promise<DeployProfile | null> {
    return {
      project_id: 1, server_id: 1, env_name: "default", remote_dir: "/var/www/myapp",
      site_dir: "current", jar_name: "app.jar",
      restart_command: "sudo systemctl reload nginx",
      health_check_url: "https://myapp.example.com", backup_keep: 3, is_prod: true,
    };
  },
  async save_profile({ input }: { input: DeployProfile }): Promise<DeployProfile> {
    return { ...input };
  },
  async list_environments(): Promise<string[]> {
    return ["default", "生产"];
  },
  async get_setting({ key }: { key: string }): Promise<string> {
    return key === "maven_path" ? "" : "1";
  },
  async set_setting(): Promise<void> {},
  async save_text_file(): Promise<void> {},
  async open_log_dir(): Promise<void> {},
  async cleanup_old_logs(): Promise<number> {
    return 0;
  },
  async list_history(): Promise<HistoryRecord[]> {
    return [
      {
        id: 3, project_id: 1, server_id: 1, env_name: "生产", kind: "deploy", status: "success",
        started_at: "2026-11-18 14:33:01", duration_ms: 133000, failed_step: null,
        message: "部署成功（mock）", log_file: "deploy_mock.log", backup_ref: "20261118_143201",
        remote_dir: "/var/www/myapp",
      },
      {
        id: 2, project_id: 2, server_id: 1, env_name: "default", kind: "deploy", status: "failed",
        started_at: "2026-11-18 11:20:12", duration_ms: 101000, failed_step: "deploy",
        message: "远程部署失败（退出码 1）", log_file: "deploy_mock2.log", backup_ref: "20261118_111712",
        remote_dir: "/opt/order-service",
      },
      {
        id: 1, project_id: 1, server_id: 1, env_name: "生产", kind: "rollback", status: "success",
        started_at: "2026-11-17 18:05:44", duration_ms: 12000, failed_step: null,
        message: "已回滚到最新备份（mock）", log_file: null, backup_ref: null,
        remote_dir: "/var/www/myapp",
      },
    ];
  },
  async get_history_log(): Promise<string> {
    return "[14:32:01] ▶ 步骤 1/7 · 本地构建 npm run build\n[14:32:39] ✓ built in 38.2s\n[14:33:38] DEPLOY_OK — 部署成功（mock 日志）";
  },
  async rollback({ projectId: project_id }: { projectId: number }): Promise<void> {
    const emit = (e: DeployEvent) => mockListeners.forEach((cb) => cb(e));
    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
    (async () => {
      emit({ type: "step", project_id, step: "connect", status: "running", detail: null });
      await sleep(400);
      emit({ type: "step", project_id, step: "connect", status: "done", detail: null });
      emit({ type: "step", project_id, step: "rollback", status: "running", detail: null });
      emit({ type: "log", project_id, line: "▶ 回滚到 最新备份：恢复备份 → 重启服务", level: "info" });
      await sleep(800);
      emit({ type: "step", project_id, step: "rollback", status: "done", detail: null });
      emit({ type: "finished", project_id, ok: true, cancelled: false, message: "已回滚到最新备份并重启服务（mock）", duration_ms: 1200 });
    })();
  },
  async start_deploy({ projectId: project_id }: { projectId: number }): Promise<void> {
    // 浏览器调试：模拟一次完整流水线事件流
    const emit = (e: DeployEvent) => mockListeners.forEach((cb) => cb(e));
    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
    (async () => {
      const step = (s: string, status: "running" | "done" | "failed", detail: string | null = null) =>
        emit({ type: "step", project_id, step: s, status, detail });
      const log = (line: string, level: "info" | "stdout" | "stderr" = "stdout") =>
        emit({ type: "log", project_id, line, level });
      step("prepare", "running"); await sleep(200); step("prepare", "done");
      step("build", "running"); log("▶ 构建命令: npm run build", "info");
      await sleep(400); log("vite v5.4.8 building for production...");
      await sleep(600); log("✓ 1287 modules transformed.");
      await sleep(300); log("✓ built in 38.2s"); step("build", "done", "38.2s");
      step("package", "running"); await sleep(400);
      log("dist → dist.zip（27.8 MB）", "info"); step("package", "done", "27.8 MB");
      step("connect", "running"); await sleep(400); step("connect", "done");
      step("upload", "running");
      const total = 27.8 * 1048576;
      for (let i = 1; i <= 10; i++) {
        await sleep(250);
        emit({ type: "progress", project_id, uploaded: (total * i) / 10, total, speed_bps: 3.2 * 1048576 });
      }
      step("upload", "done", "27.8 MB");
      step("deploy", "running"); log("▶ 远程部署：备份旧版 → 原子替换 → 重启服务", "info");
      await sleep(600); log("DEPLOY_OK"); step("deploy", "done", "备份+切换+重启");
      step("verify", "running"); await sleep(300);
      log("HTTP 200 OK", "info"); step("verify", "done", "HTTP 200");
      emit({ type: "finished", project_id, ok: true, cancelled: false, message: "部署成功（mock 模拟）", duration_ms: 6500 });
    })();
  },
  async cancel_deploy(): Promise<void> {},
};

/* ---------- 统一出口 ---------- */
function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) return invoke<T>(cmd, args);
  const fn = (mockApi as Record<string, (a: unknown) => Promise<unknown>>)[cmd];
  return fn(args ?? {}) as Promise<T>;
}

export const api = {
  listServers: () => call<Server[]>("list_servers"),
  saveServer: (input: ServerInput) => call<Server>("save_server", { input }),
  deleteServer: (id: number) => call<void>("delete_server", { id }),
  testServer: (id: number) => call<TestResult>("test_server", { id }),
  testServerDraft: (input: ServerInput) => call<TestResult>("test_server_draft", { input }),
  listProjects: () => call<Project[]>("list_projects"),
  saveProject: (input: ProjectInput) => call<Project>("save_project", { input }),
  deleteProject: (id: number) => call<void>("delete_project", { id }),
  detectProject: (path: string) => call<DetectResult>("detect_project", { path }),
  getProfile: (projectId: number, serverId: number, envName: string) =>
    call<DeployProfile | null>("get_profile", { projectId, serverId, envName }),
  saveProfile: (input: DeployProfile) => call<DeployProfile>("save_profile", { input }),
  listEnvironments: (projectId: number, serverId: number) =>
    call<string[]>("list_environments", { projectId, serverId }),
  getSetting: (key: string, defaultVal = "1") => call<string>("get_setting", { key, default: defaultVal }),
  setSetting: (key: string, value: string) => call<void>("set_setting", { key, value }),
  saveTextFile: (path: string, content: string) => call<void>("save_text_file", { path, content }),
  openLogDir: () => call<void>("open_log_dir"),
  cleanupOldLogs: (days: number) => call<number>("cleanup_old_logs", { days }),
  startDeploy: (projectId: number, serverId: number, envName: string, remoteDir: string) =>
    call<void>("start_deploy", { projectId, serverId, envName, remoteDir }),
  cancelDeploy: () => call<void>("cancel_deploy"),
  rollback: (projectId: number, serverId: number, envName: string, backupTs?: string | null) =>
    call<void>("rollback", { projectId, serverId, envName, backupTs: backupTs ?? null }),
  listHistory: (projectId?: number | null) =>
    call<HistoryRecord[]>("list_history", { projectId: projectId ?? null }),
  getHistoryLog: (id: number) => call<string>("get_history_log", { id }),
};

/** 开机自启动（Tauri 插件 / 浏览器 mock） */
export const autostart = {
  async isEnabled(): Promise<boolean> {
    if (isTauri) {
      const { isEnabled } = await import("@tauri-apps/plugin-autostart");
      return isEnabled();
    }
    return localStorage.getItem("mock_autostart") === "1";
  },
  async setEnabled(on: boolean): Promise<void> {
    if (isTauri) {
      const m = await import("@tauri-apps/plugin-autostart");
      if (on) await m.enable();
      else await m.disable();
      return;
    }
    localStorage.setItem("mock_autostart", on ? "1" : "0");
  },
};

/** 导出文本文件：Tauri 用系统保存对话框，浏览器降级为下载 */
export async function exportTextFile(defaultName: string, content: string): Promise<boolean> {
  if (isTauri) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({ defaultPath: defaultName, filters: [{ name: "日志文件", extensions: ["log", "txt"] }] });
    if (!path) return false;
    await api.saveTextFile(path, content);
    return true;
  }
  const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = defaultName;
  a.click();
  URL.revokeObjectURL(a.href);
  return true;
}

/** 订阅部署事件（Tauri event / 浏览器 mock 事件总线） */
export async function onDeployEvent(cb: DeployListener): Promise<() => void> {
  if (isTauri) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<DeployEvent>("deploy-event", (e) => cb(e.payload));
  }
  mockListeners.add(cb);
  return () => {
    mockListeners.delete(cb);
  };
}

/** 选择本地目录（Tauri 用系统对话框，浏览器降级为 prompt） */
export async function pickDirectory(): Promise<string | null> {
  if (isTauri) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({ directory: true, multiple: false });
    return typeof r === "string" ? r : null;
  }
  return window.prompt("输入目录路径（浏览器调试模式）");
}