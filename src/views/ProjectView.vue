<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useAppStore } from "@/stores/app";
import { useDialogs } from "@/stores/dialogs";
import { api, exportTextFile, onDeployEvent } from "@/ipc";
import type { DeployEvent, DeployProfile } from "@/ipc/types";
import ProjectModal from "@/components/ProjectModal.vue";
import ProfileModal from "@/components/ProfileModal.vue";
import ConfirmDeployModal from "@/components/ConfirmDeployModal.vue";

const store = useAppStore();
const dialogs = useDialogs();
const project = computed(() =>
  store.view.kind === "project" ? store.projects.find((p) => p.id === (store.view as { id: number }).id) : undefined
);
const editing = ref(false);
const showProfile = ref(false);
const showConfirm = ref(false);

/* ---------- 部署配置 ---------- */
const serverId = ref<number | null>(null);
const envs = ref<string[]>(["default"]);
const env = ref("default");
const remoteDir = ref("");
const profile = ref<DeployProfile | null>(null);
const currentServer = computed(() => store.servers.find((s) => s.id === serverId.value) ?? null);

async function loadProfile() {
  if (serverId.value && project.value) {
    profile.value = await api.getProfile(project.value.id, serverId.value, env.value);
    remoteDir.value = profile.value?.remote_dir ?? "";
  }
}

watch(serverId, async (id) => {
  if (id && project.value) {
    const list = await api.listEnvironments(project.value.id, id);
    envs.value = list.length ? list : ["default"];
    if (!envs.value.includes(env.value)) env.value = envs.value[0];
    await loadProfile();
  }
});

watch(env, () => loadProfile());

async function newEnv() {
  const name = (await dialogs.prompt("新环境名称（如：测试 / 预发 / 生产）", "", "新建环境"))?.trim();
  if (!name) return;
  if (envs.value.includes(name)) {
    env.value = name;
    return;
  }
  envs.value.push(name);
  env.value = name;
  profile.value = null;
  remoteDir.value = "";
  showProfile.value = true;
}

async function exportLogs() {
  if (!project.value || logs.value.length === 0) return;
  const content = logs.value.map((l) => l.line).join("\n");
  await exportTextFile(`${project.value.name}_${env.value}_${Date.now()}.log`, content);
}

function onProfileSaved(p: DeployProfile) {
  profile.value = p;
  remoteDir.value = p.remote_dir;
}

/* ---------- 部署状态 ---------- */
type StepState = { status: "pending" | "running" | "done" | "failed"; detail: string | null };
const engineSteps = ref<Record<string, StepState>>({});
const logs = ref<{ line: string; level: string }[]>([]);
const progress = ref<{ uploaded: number; total: number; speed_bps: number } | null>(null);
const result = ref<{ ok: boolean; cancelled: boolean; message: string; duration_ms: number } | null>(null);
const running = ref(false);
const mode = ref<"deploy" | "rollback">("deploy");
const elapsed = ref(0);
let timer: ReturnType<typeof setInterval> | null = null;
let unlisten: (() => void) | null = null;
const termRef = ref<HTMLElement | null>(null);

function mergeSteps(...names: string[]): StepState {
  const list = names.map((n) => engineSteps.value[n]).filter(Boolean) as StepState[];
  if (list.some((s) => s.status === "failed")) return { status: "failed", detail: list.find((s) => s.status === "failed")!.detail };
  if (list.some((s) => s.status === "running")) return { status: "running", detail: null };
  if (list.length && list.every((s) => s.status === "done")) return { status: "done", detail: list[list.length - 1].detail };
  return { status: "pending", detail: null };
}

const uiSteps = computed(() => [
  { label: "本地构建", ...mergeSteps("prepare", "build") },
  { label: "产物打包", ...mergeSteps("package") },
  { label: "上传服务器", ...mergeSteps("connect", "upload") },
  { label: "远程部署", ...mergeSteps("deploy", "rollback") },
  { label: "健康检查", ...mergeSteps("verify") },
]);

const pct = computed(() =>
  progress.value && progress.value.total > 0 ? Math.min(100, (progress.value.uploaded / progress.value.total) * 100) : 0
);

function fmtBytes(n: number): string { return (n / 1048576).toFixed(1) + " MB"; }
function fmtElapsed(ms: number): string {
  const s = Math.floor(ms / 1000);
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

function pushLog(line: string, level: string) {
  logs.value.push({ line, level });
  if (logs.value.length > 2000) logs.value.splice(0, logs.value.length - 2000);
  nextTick(() => {
    if (termRef.value) termRef.value.scrollTop = termRef.value.scrollHeight;
  });
}

function handleEvent(e: DeployEvent) {
  if (!project.value || e.project_id !== project.value.id) return;
  switch (e.type) {
    case "step":
      engineSteps.value[e.step] = { status: e.status, detail: e.detail };
      break;
    case "log":
      pushLog(e.line, e.level);
      break;
    case "progress":
      progress.value = { uploaded: e.uploaded, total: e.total, speed_bps: e.speed_bps };
      break;
    case "finished":
      running.value = false;
      if (timer) { clearInterval(timer); timer = null; }
      result.value = e;
      store.refresh();
      break;
  }
}

function resetRun(m: "deploy" | "rollback") {
  mode.value = m;
  engineSteps.value = {};
  logs.value = [];
  progress.value = null;
  result.value = null;
  elapsed.value = 0;
  running.value = true;
  timer = setInterval(() => { elapsed.value += 1000; }, 1000);
}

function failLocal(e: unknown) {
  running.value = false;
  if (timer) { clearInterval(timer); timer = null; }
  result.value = { ok: false, cancelled: false, message: String(e), duration_ms: 0 };
}

function start() {
  if (!project.value || !serverId.value || !remoteDir.value.trim()) return;
  if (profile.value?.is_prod) {
    showConfirm.value = true;
    return;
  }
  doStart();
}

async function doStart() {
  showConfirm.value = false;
  if (!project.value || !serverId.value) return;
  resetRun("deploy");
  try {
    await api.startDeploy(project.value.id, serverId.value, env.value, remoteDir.value.trim());
  } catch (e) {
    failLocal(e);
  }
}

async function rollbackLatest() {
  if (!project.value || !serverId.value) return;
  const ok = await dialogs.confirm("确定回滚到上一版本？\n将恢复最近一次备份并重启服务。", { title: "回滚确认", danger: true });
  if (!ok) return;
  resetRun("rollback");
  try {
    await api.rollback(project.value.id, serverId.value, env.value, null);
  } catch (e) {
    failLocal(e);
  }
}

async function cancel() {
  try { await api.cancelDeploy(); } catch (e) { pushLog(String(e), "error"); }
}

async function remove() {
  if (!project.value) return;
  const ok = await dialogs.confirm(`确定删除项目「${project.value.name}」？\n相关部署配置将一并删除（不影响服务器上的文件）。`, {
    title: "删除项目",
    danger: true,
  });
  if (!ok) return;
  await api.deleteProject(project.value.id);
  await store.refresh();
}

onMounted(async () => {
  unlisten = await onDeployEvent(handleEvent);
  if (store.servers.length) serverId.value = store.servers[0].id;
  if (store.rollbackPending) {
    store.rollbackPending = false;
    resetRun("rollback");
  }
});
onBeforeUnmount(() => {
  unlisten?.();
  if (timer) clearInterval(timer);
});
</script>

<template>
  <div class="main" v-if="project">
    <div class="page-head">
      <div>
        <div style="display: flex; align-items: center; gap: 10px">
          <span class="page-title">{{ project.name }}</span>
          <span class="tag" :class="project.type === 'java' ? 'purple' : 'blue'">
            {{ project.type === "java" ? "Java" : "前端" }}
          </span>
          <span v-if="profile?.is_prod" class="tag red">生产</span>
          <span v-if="result" class="tag" :class="result.ok ? 'green' : result.cancelled ? 'yellow' : 'red'">
            {{ result.ok ? (mode === "rollback" ? "回滚成功" : "部署成功") : result.cancelled ? "已取消" : (mode === "rollback" ? "回滚失败" : "部署失败") }}
          </span>
        </div>
        <div class="page-sub mono">{{ project.local_path }}</div>
      </div>
      <div class="head-right">
        <button class="btn ghost sm" :disabled="running || !serverId" @click="showProfile = true">部署配置</button>
        <button class="btn ghost sm" :disabled="running" @click="editing = true">编辑项目</button>
        <button class="btn danger-ghost sm" :disabled="running" @click="remove">删除</button>
      </div>
    </div>

    <div class="target-card">
      <div class="t-row">
        <span class="t-label">目标服务器</span>
        <select v-model="serverId" class="input" style="flex: 1" :disabled="running">
          <option v-for="s in store.servers" :key="s.id" :value="s.id">{{ s.name }}（{{ s.username }}@{{ s.host }}:{{ s.port }}）</option>
        </select>
        <span class="t-label" style="margin-left: 14px">环境</span>
        <select v-model="env" class="input" style="width: 130px" :disabled="running">
          <option v-for="e in envs" :key="e" :value="e">{{ e === "default" ? "默认" : e }}</option>
        </select>
        <button class="btn ghost sm" :disabled="running" title="新建环境" @click="newEnv">＋</button>
      </div>
      <div class="t-row">
        <span class="t-label">远程目录</span>
        <input v-model="remoteDir" class="input mono" style="flex: 1" placeholder="/var/www/myapp" :disabled="running" />
      </div>
      <div class="t-row t-meta">
        <span>构建命令 <b class="mono">{{ project.build_command }}</b></span>
        <span>产物 <b class="mono">{{ project.artifact_path }}</b></span>
        <span v-if="running || elapsed" style="margin-left: auto">已用时 <b class="mono">{{ fmtElapsed(elapsed) }}</b></span>
      </div>
    </div>

    <div class="steps">
      <template v-for="(s, i) in uiSteps" :key="s.label">
        <div class="step" :class="s.status">
          <div class="s-circle">{{ s.status === "done" ? "✓" : s.status === "failed" ? "✕" : i + 1 }}</div>
          <div class="s-label">{{ s.label }}</div>
          <div class="s-state">{{ s.detail ?? (s.status === "running" ? "进行中" : s.status === "pending" ? "等待" : "") }}</div>
        </div>
        <div v-if="i < uiSteps.length - 1" class="step-line" :class="{ done: s.status === 'done' }"></div>
      </template>
    </div>

    <div v-if="result" class="result-banner" :class="result.ok ? 'ok' : result.cancelled ? 'cancel' : 'fail'">
      <b>{{ result.ok ? "✓" : result.cancelled ? "○" : "✕" }}</b>
      <span>{{ result.message }}</span>
      <button v-if="!result.ok && !result.cancelled && mode === 'deploy'" class="btn danger-ghost sm" style="margin-left: auto" @click="rollbackLatest">立即回滚</button>
      <span v-else-if="result.duration_ms" style="margin-left: auto" class="mono">{{ fmtElapsed(result.duration_ms) }}</span>
    </div>

    <div class="terminal" style="flex: 1; min-height: 220px">
      <div class="term-head">
        <span class="t-dot" style="background: #f85149"></span>
        <span class="t-dot" style="background: #d29922"></span>
        <span class="t-dot" style="background: #3fb950"></span>
        <span style="margin-left: 6px">实时日志{{ mode === "rollback" && running ? " — 回滚" : "" }}</span>
        <span style="margin-left: auto; display: flex; gap: 12px; align-items: center">
          <span class="link" :style="{ opacity: logs.length ? 1 : 0.4 }" @click="exportLogs">导出</span>
          <span style="color: var(--faint)">{{ logs.length }} 行</span>
        </span>
      </div>
      <div ref="termRef" class="term-body">
        <div v-if="logs.length === 0" class="ln lv-muted">选择目标服务器与远程目录后，点击「一键部署」，构建、上传与部署日志将在此实时显示。</div>
        <div v-for="(l, i) in logs" :key="i" class="ln" :class="'lv-' + l.level">{{ l.line }}</div>
      </div>
    </div>

    <div v-if="progress" class="card" style="margin-top: 14px; padding: 13px 18px">
      <div class="progress-row">
        <span style="font-size: 12.5px; color: var(--muted); width: 130px">
          {{ pct >= 100 ? "上传完成" : "产物上传中" }}
        </span>
        <div class="progress-track">
          <div class="progress-fill" :class="{ green: pct >= 100 }" :style="{ width: pct + '%' }"></div>
        </div>
        <span class="mono" style="font-size: 12.5px; width: 210px; text-align: right" :style="{ color: pct >= 100 ? 'var(--green)' : 'var(--blue)' }">
          {{ fmtBytes(progress.uploaded) }} / {{ fmtBytes(progress.total) }}<template v-if="pct < 100"> · {{ fmtBytes(progress.speed_bps) }}/s</template>
        </span>
      </div>
    </div>

    <div style="display: flex; align-items: center; gap: 12px; margin-top: 16px">
      <button class="btn primary lg" :disabled="running || !serverId || !remoteDir.trim()" @click="start">
        {{ running && mode === "deploy" ? "部署中…" : result ? "再次部署" : "一键部署" }}
      </button>
      <button v-if="running" class="btn danger-ghost lg" @click="cancel">取消</button>
      <button class="btn ghost lg" :disabled="running || !serverId || !remoteDir.trim()" @click="rollbackLatest">
        {{ running && mode === "rollback" ? "回滚中…" : "回滚上一版本" }}
      </button>
      <span style="margin-left: auto; font-size: 12.5px; color: var(--faint)">
        {{ store.servers.length === 0 ? "请先在「服务器」页添加服务器" : "备份 → 原子替换 → 重启 → 健康检查，失败可一键回滚" }}
      </span>
    </div>

    <ProjectModal v-if="editing" :project="project" @close="editing = false" @saved="store.refresh()" />
    <ProfileModal
      v-if="showProfile && serverId"
      :project="project"
      :server-id="serverId"
      :env-name="env"
      :profile="profile"
      @close="showProfile = false"
      @saved="onProfileSaved"
    />
    <ConfirmDeployModal
      v-if="showConfirm && currentServer && profile"
      :project="project"
      :server="currentServer"
      :profile="{ ...profile, remote_dir: remoteDir }"
      @cancel="showConfirm = false"
      @confirm="doStart"
    />
  </div>
</template>

<style scoped>
.target-card {
  background: var(--panel); border: 1px solid var(--border); border-radius: 10px;
  padding: 14px 16px; margin-bottom: 16px; display: flex; flex-direction: column; gap: 10px;
}
.t-row { display: flex; align-items: center; gap: 10px; }
.t-label { font-size: 12.5px; color: var(--muted); font-weight: 600; width: 70px; flex: none; }
.t-meta { font-size: 12px; color: var(--faint); gap: 22px; padding-top: 2px; border-top: 1px dashed var(--border); }
.t-meta b { color: var(--muted); font-weight: 600; }
</style>