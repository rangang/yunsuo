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
    // 编辑已存服务器且未重填凭据时，用已存凭据测试
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