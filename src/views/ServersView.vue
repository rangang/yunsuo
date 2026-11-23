<script setup lang="ts">
import { ref } from "vue";
import { useAppStore } from "@/stores/app";
import { useDialogs } from "@/stores/dialogs";
import { api } from "@/ipc";
import type { Server } from "@/ipc/types";
import ServerModal from "@/components/ServerModal.vue";

const store = useAppStore();
const dialogs = useDialogs();
const modal = ref<{ mode: "add" } | { mode: "edit"; server: Server } | null>(null);
const tests = store.serverTests;

async function test(s: Server) {
  tests[s.id] = { status: "testing" };
  try {
    const result = await api.testServer(s.id);
    tests[s.id] = { status: "done", result };
  } catch (e) {
    tests[s.id] = { status: "done", result: { ok: false, latency_ms: 0, os_info: null, message: String(e) } };
  }
}

async function remove(s: Server) {
  const ok = await dialogs.confirm(`确定删除服务器「${s.name}」？\n保存的凭据将一并删除，关联的部署配置也会失效。`, {
    title: "删除服务器",
    danger: true,
  });
  if (!ok) return;
  await api.deleteServer(s.id);
  delete tests[s.id];
  await store.refresh();
}

function testText(s: Server): string {
  const t = tests[s.id];
  if (!t) return "尚未测试连接，点击「测试连接」验证配置";
  if (t.status === "testing") return "正在连接…";
  const r = t.result!;
  return r.ok
    ? `✓ 连接成功 · ${r.latency_ms}ms${r.os_info ? " · " + r.os_info : ""}`
    : `✕ ${r.message}`;
}
function testClass(s: Server): string {
  const t = tests[s.id];
  if (!t || t.status === "testing") return "none";
  return t.result!.ok ? "ok" : "fail";
}
</script>

<template>
  <div class="main">
    <div class="page-head">
      <div>
        <div class="page-title">服务器管理</div>
        <div class="page-sub">SSH 连接配置 · 密码与私钥口令加密保存在 Windows 凭据管理器</div>
      </div>
      <div class="head-right">
        <button class="btn primary" @click="modal = { mode: 'add' }">＋ 添加服务器</button>
      </div>
    </div>

    <div v-if="store.servers.length" class="server-grid">
      <div v-for="s in store.servers" :key="s.id" class="s-card">
        <div class="s-head">
          <span class="dot" :class="tests[s.id]?.result?.ok ? 'green' : 'gray'"></span>
          <span class="s-name">{{ s.name }}</span>
          <span class="tag" :class="tests[s.id]?.result ? (tests[s.id].result!.ok ? 'green' : 'red') : 'gray'">
            {{ tests[s.id]?.result ? (tests[s.id].result!.ok ? "已连接" : "连接失败") : "未测试" }}
          </span>
        </div>
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
        <div class="s-test" :class="testClass(s)">{{ testText(s) }}</div>
        <div class="s-actions">
          <button class="btn ghost sm" :disabled="tests[s.id]?.status === 'testing'" @click="test(s)">
            {{ tests[s.id]?.status === "testing" ? "连接中…" : "测试连接" }}
          </button>
          <span class="spacer"></span>
          <button class="icon-btn" title="编辑" @click="modal = { mode: 'edit', server: s }">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M17 3a2.8 2.8 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5z" /></svg>
          </button>
          <button class="icon-btn danger" title="删除" @click="remove(s)">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" /></svg>
          </button>
        </div>
      </div>
    </div>

    <div v-else-if="store.loaded" class="empty-state">
      <div class="e-icon">
        <svg width="52" height="52" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="3" y="4" width="18" height="7" rx="2" /><rect x="3" y="13" width="18" height="7" rx="2" /><circle cx="7" cy="7.5" r=".5" fill="currentColor" /><circle cx="7" cy="16.5" r=".5" fill="currentColor" /></svg>
      </div>
      <div class="e-title">还没有服务器</div>
      <div class="e-desc">添加一台 Linux 服务器（SSH 密码或私钥认证），之后即可将项目产物一键部署上去。</div>
      <button class="btn primary" @click="modal = { mode: 'add' }">＋ 添加服务器</button>
    </div>

    <ServerModal
      v-if="modal"
      :server="modal.mode === 'edit' ? modal.server : null"
      @close="modal = null"
      @saved="store.refresh()"
    />
  </div>
</template>