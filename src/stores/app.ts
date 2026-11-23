import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "@/ipc";
import type { Project, Server, TestResult } from "@/ipc/types";

export type View = { kind: "project"; id: number } | { kind: "servers" } | { kind: "history" } | { kind: "settings" };

export const useAppStore = defineStore("app", () => {
  const servers = ref<Server[]>([]);
  const projects = ref<Project[]>([]);
  const view = ref<View>({ kind: "servers" });
  const loaded = ref(false);
  /** 从历史页发起回滚后，项目页挂载时接管事件展示 */
  const rollbackPending = ref(false);
  /** 各项目最近一次部署/回滚状态（success/failed/cancelled） */
  const projectStatus = ref<Record<number, string>>({});
  /** 服务器连接测试结果（跨页面保留） */
  const serverTests = ref<Record<number, { status: "testing" | "done"; result?: TestResult }>>({});

  async function refresh() {
    const [s, p, h] = await Promise.all([api.listServers(), api.listProjects(), api.listHistory(null)]);
    servers.value = s;
    projects.value = p;
    const map: Record<number, string> = {};
    for (const r of h) {
      if (!(r.project_id in map)) map[r.project_id] = r.status;
    }
    projectStatus.value = map;
    loaded.value = true;
    if (view.value.kind === "project" && !p.some((x) => x.id === (view.value as { id: number }).id)) {
      view.value = p.length ? { kind: "project", id: p[0].id } : { kind: "servers" };
    }
  }

  function openProject(id: number) {
    view.value = { kind: "project", id };
  }

  return { servers, projects, view, loaded, rollbackPending, projectStatus, serverTests, refresh, openProject };
});