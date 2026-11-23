<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useAppStore } from "@/stores/app";
import { useDialogs } from "@/stores/dialogs";
import { api, exportTextFile } from "@/ipc";
import type { HistoryRecord } from "@/ipc/types";

const store = useAppStore();
const dialogs = useDialogs();
const records = ref<HistoryRecord[]>([]);
const filterProject = ref<number | null>(null);
const loading = ref(false);
const logModal = ref<{ id: number; content: string } | null>(null);

const projectName = (id: number) => store.projects.find((p) => p.id === id)?.name ?? `#${id}`;
const serverName = (id: number) => store.servers.find((s) => s.id === id)?.name ?? `#${id}`;

const stats = computed(() => {
  const total = records.value.length;
  const ok = records.value.filter((r) => r.status === "success").length;
  const avg = total ? Math.round(records.value.reduce((a, r) => a + r.duration_ms, 0) / total) : 0;
  return { total, rate: total ? Math.round((ok / total) * 100) : 0, avg };
});

function fmtDuration(ms: number): string {
  const s = Math.round(ms / 1000);
  return s >= 60 ? `${Math.floor(s / 60)}m ${s % 60}s` : `${s}s`;
}

const stepNames: Record<string, string> = {
  prepare: "准备", build: "本地构建", package: "产物打包", connect: "连接服务器",
  upload: "上传", deploy: "远程部署", verify: "健康检查", rollback: "回滚",
};

async function load() {
  loading.value = true;
  try {
    records.value = await api.listHistory(filterProject.value);
  } finally {
    loading.value = false;
  }
}

async function viewLog(r: HistoryRecord) {
  try {
    const content = await api.getHistoryLog(r.id);
    logModal.value = { id: r.id, content };
  } catch (e) {
    await dialogs.alert(String(e), "无法查看日志");
  }
}

async function rollbackTo(r: HistoryRecord) {
  const ok = await dialogs.confirm(
    `确定回滚「${projectName(r.project_id)}」到备份 ${r.backup_ref}？\n将恢复该备份并重启服务，当前线上版本会被替换。`,
    { title: "回滚确认", danger: true }
  );
  if (!ok) return;
  store.rollbackPending = true;
  store.openProject(r.project_id);
  try {
    await api.rollback(r.project_id, r.server_id, r.env_name, r.backup_ref);
  } catch (e) {
    store.rollbackPending = false;
    await dialogs.alert(String(e), "回滚失败");
  }
}

async function exportLog() {
  if (!logModal.value) return;
  await exportTextFile(`deploy_${logModal.value.id}.log`, logModal.value.content);
}

onMounted(load);
</script>

<template>
  <div class="main">
    <div class="page-head">
      <div>
        <div class="page-title">部署历史</div>
        <div class="page-sub">完整日志落盘保存 · 失败记录可定位到具体步骤 · 成功记录可回滚</div>
      </div>
      <div class="head-right">
        <select v-model="filterProject" class="input" style="width: 180px" @change="load">
          <option :value="null">项目：全部</option>
          <option v-for="p in store.projects" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
        <button class="btn ghost sm" :disabled="loading" @click="load">{{ loading ? "刷新中…" : "刷新" }}</button>
      </div>
    </div>

    <div class="stat-cards" v-if="records.length">
      <div class="stat-card"><div class="s-label">记录总数</div><div class="s-num blue">{{ stats.total }}</div></div>
      <div class="stat-card"><div class="s-label">成功率</div><div class="s-num green">{{ stats.rate }}%</div></div>
      <div class="stat-card"><div class="s-label">平均耗时</div><div class="s-num">{{ fmtDuration(stats.avg) }}</div></div>
    </div>

    <div v-if="records.length" class="card" style="padding: 6px 10px; flex: 1">
      <table class="table">
        <tr>
          <th style="width: 44px">#</th><th>时间</th><th>项目</th><th>环境</th><th>类型</th>
          <th>服务器</th><th>状态</th><th>耗时</th><th>失败步骤</th><th style="text-align: right">操作</th>
        </tr>
        <tr v-for="r in records" :key="r.id">
          <td class="mono" style="color: var(--faint)">{{ r.id }}</td>
          <td>{{ r.started_at }}</td>
          <td><b>{{ projectName(r.project_id) }}</b><div class="sub mono">{{ r.remote_dir }}</div></td>
          <td><span class="tag" :class="r.env_name === 'default' ? 'gray' : 'red'">{{ r.env_name === "default" ? "默认" : r.env_name }}</span></td>
          <td><span class="tag" :class="r.kind === 'rollback' ? 'yellow' : 'blue'">{{ r.kind === "rollback" ? "回滚" : "部署" }}</span></td>
          <td>{{ serverName(r.server_id) }}</td>
          <td>
            <span class="tag" :class="r.status === 'success' ? 'green' : r.status === 'failed' ? 'red' : 'gray'">
              {{ r.status === "success" ? "✓ 成功" : r.status === "failed" ? "✕ 失败" : "○ 已取消" }}
            </span>
          </td>
          <td class="mono">{{ fmtDuration(r.duration_ms) }}</td>
          <td>
            <span v-if="r.failed_step" style="color: var(--red)">{{ stepNames[r.failed_step] ?? r.failed_step }}</span>
            <span v-else style="color: var(--faint)">—</span>
            <div v-if="r.message && r.status === 'failed'" class="sub">{{ r.message }}</div>
          </td>
          <td style="text-align: right; white-space: nowrap">
            <span v-if="r.log_file" class="link" @click="viewLog(r)">查看日志</span>
            <template v-if="r.backup_ref && r.kind === 'deploy'">&nbsp;&nbsp;<span class="link" @click="rollbackTo(r)">回滚到此</span></template>
          </td>
        </tr>
      </table>
    </div>

    <div v-else-if="!loading" class="empty-state">
      <div class="e-icon">
        <svg width="52" height="52" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 3" /></svg>
      </div>
      <div class="e-title">暂无部署记录</div>
      <div class="e-desc">完成一次一键部署后，这里会记录时间、耗时、结果与完整日志，并支持回滚到任意备份版本。</div>
    </div>

    <div v-if="logModal" class="overlay">
      <div class="modal" style="width: 760px">
        <div class="m-title">部署日志 #{{ logModal.id }}</div>
        <div class="terminal" style="max-height: 60vh">
          <div class="term-body" style="max-height: 56vh">{{ logModal.content }}</div>
        </div>
        <div class="m-actions">
          <button class="btn ghost" @click="exportLog">导出日志</button>
          <button class="btn ghost" @click="logModal = null">关闭</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.stat-cards { display: flex; gap: 14px; margin-bottom: 18px; }
.stat-card { flex: 1; background: var(--panel); border: 1px solid var(--border); border-radius: 12px; padding: 15px 18px; }
.stat-card .s-num { font-size: 24px; font-weight: 700; margin-top: 4px; }
.stat-card .s-label { font-size: 12px; color: var(--faint); }
.stat-card .s-num.green { color: var(--green); }
.stat-card .s-num.blue { color: var(--blue); }
.table { width: 100%; border-collapse: collapse; font-size: 13px; }
.table th { text-align: left; padding: 10px 14px; color: var(--faint); font-weight: 600; font-size: 12px; border-bottom: 1px solid var(--border); letter-spacing: 0.5px; }
.table td { padding: 12px 14px; border-bottom: 1px solid var(--border); color: var(--text); }
.table tr:last-child td { border-bottom: none; }
.table .sub { color: var(--faint); font-size: 11.5px; margin-top: 2px; max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>