<script setup lang="ts">
import { reactive, ref } from "vue";
import { api, pickDirectory } from "@/ipc";
import { credStore } from "@/platform";
import type { Server, ServerInput, TestResult } from "@/ipc/types";

const props = defineProps<{ server: Server | null }>();
const emit = defineEmits<{ (e: "close"): void; (e: "saved"): void }>();

const form = reactive({
  name: props.server?.name ?? "",
  host: props.server?.host ?? "",
  port: props.server?.port ?? 22,
  username: props.server?.username ?? "root",
  auth_type: (props.server?.auth_type ?? "password") as "password" | "key",
  key_path: props.server?.key_path ?? "",
  secret: "",
});
const saving = ref(false);
const testing = ref(false);
const error = ref("");
const testResult = ref<TestResult | null>(null);

function toInput(): ServerInput {
  return {
    id: props.server?.id,
    name: form.name.trim(),
    host: form.host.trim(),
    port: Number(form.port) || 22,
    username: form.username.trim(),
    auth_type: form.auth_type,
    key_path: form.auth_type === "key" ? form.key_path.trim() || null : null,
    secret: form.secret || undefined,
  };
}

function validate(): boolean {
  error.value = "";
  if (!form.name.trim() || !form.host.trim() || !form.username.trim()) {
    error.value = "请填写名称、主机地址和用户名";
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
        <div class="field">
          <label>显示名称 <span class="req">*</span></label>
          <input v-model="form.name" class="input" placeholder="如：生产服务器" />
        </div>
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
        <div v-if="form.auth_type === 'key'" class="field">
          <label>私钥文件 <span class="req">*</span></label>
          <div class="input-wrap">
            <input v-model="form.key_path" class="input mono" placeholder="C:\Users\me\.ssh\id_ed25519" />
            <button class="btn ghost sm" @click="browseKey">浏览…</button>
          </div>
        </div>
        <div class="field full">
          <label>{{ form.auth_type === "key" ? "私钥口令（无口令留空）" : "密码" }} <span v-if="form.auth_type === 'password' && !server" class="req">*</span></label>
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