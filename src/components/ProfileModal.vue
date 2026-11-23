<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { api } from "@/ipc";
import type { DeployProfile, Project } from "@/ipc/types";

const props = defineProps<{ project: Project; serverId: number; envName: string; profile: DeployProfile | null }>();
const emit = defineEmits<{ (e: "close"): void; (e: "saved", p: DeployProfile): void }>();

const defaults = props.project.type === "java"
  ? { restart: `sudo systemctl restart ${props.project.name}`, health: "http://127.0.0.1:8080/actuator/health" }
  : { restart: "sudo systemctl reload nginx", health: "" };

const form = reactive({
  remote_dir: props.profile?.remote_dir ?? "",
  site_dir: props.profile?.site_dir ?? "current",
  jar_name: props.profile?.jar_name ?? "app.jar",
  restart_command: props.profile?.restart_command ?? defaults.restart,
  health_check_url: props.profile?.health_check_url ?? defaults.health,
  backup_keep: props.profile?.backup_keep ?? 3,
  is_prod: props.profile?.is_prod ?? false,
});
const saving = ref(false);
const error = ref("");

onMounted(async () => {
  if (!props.profile) {
    form.backup_keep = Number(await api.getSetting("default_backup_keep", "3")) || 3;
  }
});

async function save() {
  error.value = "";
  if (!form.remote_dir.trim().startsWith("/")) {
    error.value = "远程目录必须是绝对路径（以 / 开头）";
    return;
  }
  if (props.project.type === "java" && !form.restart_command.trim()) {
    error.value = "Java 项目必须配置重启命令";
    return;
  }
  saving.value = true;
  try {
    const p = await api.saveProfile({
      project_id: props.project.id,
      server_id: props.serverId,
      env_name: props.envName,
      remote_dir: form.remote_dir.trim(),
      site_dir: form.site_dir.trim() || "current",
      jar_name: form.jar_name.trim() || "app.jar",
      restart_command: form.restart_command.trim(),
      health_check_url: form.health_check_url.trim(),
      backup_keep: Number(form.backup_keep) || 3,
      is_prod: form.is_prod,
    });
    emit("saved", p);
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
      <div class="m-title">部署配置 — {{ project.name }} <span class="tag blue" style="vertical-align: 2px">{{ envName === "default" ? "默认环境" : envName }}</span></div>
      <div class="form-grid">
        <div class="field full">
          <label>远程目录 <span class="req">*</span></label>
          <input v-model="form.remote_dir" class="input mono" placeholder="/var/www/myapp 或 /opt/order-service" />
          <div class="hint">
            {{ project.type === "java"
              ? "jar 将部署为 目录/app.jar，备份在 目录/.backups/"
              : "备份在 目录/.backups/，站点文件夹名见下方" }}
          </div>
        </div>
        <div v-if="project.type === 'frontend'" class="field full">
          <label>站点文件夹名 <span class="req">*</span></label>
          <input v-model="form.site_dir" class="input mono" placeholder="current" />
          <div class="hint">
            产物解压到 远程目录/{{ form.site_dir || "current" }}，Nginx root 需指向该路径；仅限字母、数字、. _ -
          </div>
        </div>
        <div v-if="project.type === 'java'" class="field full">
          <label>jar 文件名 <span class="req">*</span></label>
          <input v-model="form.jar_name" class="input mono" placeholder="app.jar 或 yjl-system.jar" />
          <div class="hint">
            产物部署为 远程目录/{{ form.jar_name || "app.jar" }}；微服务项目请填服务名（如 yjl-system.jar），与服务器启动脚本保持一致
          </div>
        </div>
        <div class="field full">
          <label>重启命令 {{ project.type === "java" ? "" : "（可选）" }}<span v-if="project.type === 'java'" class="req"> *</span></label>
          <input v-model="form.restart_command" class="input mono" :placeholder="defaults.restart" />
          <div class="hint">需要 sudo 时请为目标命令配置 NOPASSWD，工具不支持交互式输入 sudo 密码</div>
        </div>
        <div class="field full">
          <label>健康检查 URL（可选）</label>
          <input v-model="form.health_check_url" class="input mono" :placeholder="defaults.health || 'https://example.com'" />
          <div class="hint">部署后在服务器上用 curl 检查，HTTP 2xx/3xx 视为通过；失败会提示回滚</div>
        </div>
        <div class="field">
          <label>备份保留份数</label>
          <input v-model.number="form.backup_keep" class="input mono" type="number" min="1" max="20" />
        </div>
      </div>
      <div class="prod-row">
        <div>
          <div class="p-label">生产环境标记</div>
          <div class="p-desc">开启后每次部署前弹窗二次确认，防止误操作线上服务</div>
        </div>
        <button class="toggle" :class="{ on: form.is_prod }" @click="form.is_prod = !form.is_prod"></button>
      </div>
      <div v-if="error" class="m-error">{{ error }}</div>
      <div class="m-actions">
        <button class="btn ghost" @click="emit('close')">取消</button>
        <button class="btn primary" :disabled="saving" @click="save">{{ saving ? "保存中…" : "保存配置" }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.prod-row {
  display: flex; align-items: center; gap: 12px; margin-top: 16px;
  padding: 12px 14px; background: var(--elev); border: 1px solid var(--border); border-radius: 10px;
}
.prod-row .p-label { font-size: 13.5px; font-weight: 600; }
.prod-row .p-desc { font-size: 11.5px; color: var(--faint); margin-top: 2px; }
.prod-row .toggle { margin-left: auto; }
</style>