<script setup lang="ts">
import { reactive, ref } from "vue";
import { api, pickDirectory } from "@/ipc";
import type { Project, ProjectInput } from "@/ipc/types";

const props = defineProps<{ project: Project | null }>();
const emit = defineEmits<{ (e: "close"): void; (e: "saved"): void }>();

const form = reactive({
  name: props.project?.name ?? "",
  type: (props.project?.type ?? "frontend") as "frontend" | "java",
  local_path: props.project?.local_path ?? "",
  build_command: props.project?.build_command ?? "npm run build",
  artifact_path: props.project?.artifact_path ?? "dist",
  pre_build_command: props.project?.pre_build_command ?? "",
});
const saving = ref(false);
const error = ref("");
const detected = ref("");

async function browse() {
  const dir = await pickDirectory();
  if (!dir) return;
  form.local_path = dir;
  detected.value = "";
  try {
    const d = await api.detectProject(dir);
    if (d.type) {
      form.type = d.type;
      form.build_command = d.build_command ?? form.build_command;
      form.artifact_path = d.artifact_path ?? form.artifact_path;
      if (!form.name && d.name) form.name = d.name;
      detected.value = `已识别为${d.type === "java" ? " Java 项目" : "前端项目"}，构建命令与产物路径已自动填写`;
    } else if (d.name && !form.name) {
      form.name = d.name;
    }
  } catch {
    /* 识别失败不阻塞手动填写 */
  }
}

function onTypeChange() {
  if (form.type === "java") {
    form.build_command = "mvn -DskipTests package";
    form.artifact_path = "target/*.jar";
  } else {
    form.build_command = "npm run build";
    form.artifact_path = "dist";
  }
}

async function save() {
  error.value = "";
  if (!form.name.trim() || !form.local_path.trim() || !form.build_command.trim() || !form.artifact_path.trim()) {
    error.value = "请填写名称、项目路径、构建命令和产物路径";
    return;
  }
  saving.value = true;
  const input: ProjectInput = {
    id: props.project?.id,
    name: form.name.trim(),
    type: form.type,
    local_path: form.local_path.trim(),
    build_command: form.build_command.trim(),
    artifact_path: form.artifact_path.trim(),
    pre_build_command: form.pre_build_command.trim() || null,
  };
  try {
    await api.saveProject(input);
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
      <div class="m-title">{{ project ? "编辑项目" : "添加项目" }}</div>
      <div class="form-grid">
        <div class="field full">
          <label>项目路径 <span class="req">*</span></label>
          <div class="input-wrap">
            <input v-model="form.local_path" class="input mono" placeholder="D:\projects\my-app" />
            <button class="btn ghost sm" @click="browse">浏览…</button>
          </div>
          <div v-if="detected" class="hint" style="color: var(--green)">{{ detected }}</div>
        </div>
        <div class="field">
          <label>项目名称 <span class="req">*</span></label>
          <input v-model="form.name" class="input" placeholder="如：my-vue-app" />
        </div>
        <div class="field">
          <label>项目类型</label>
          <div class="seg">
            <button class="btn sm" :class="form.type === 'frontend' ? 'primary' : 'ghost'" @click="form.type = 'frontend'; onTypeChange()">前端</button>
            <button class="btn sm" :class="form.type === 'java' ? 'primary' : 'ghost'" @click="form.type = 'java'; onTypeChange()">Java</button>
          </div>
        </div>
        <div class="field">
          <label>构建命令 <span class="req">*</span></label>
          <input v-model="form.build_command" class="input mono" placeholder="npm run build" />
        </div>
        <div class="field">
          <label>产物路径 <span class="req">*</span></label>
          <input v-model="form.artifact_path" class="input mono" placeholder="dist 或 target/*.jar" />
        </div>
        <div class="field full">
          <label>构建前钩子（可选）</label>
          <input v-model="form.pre_build_command" class="input mono" placeholder="如：git pull" />
        </div>
      </div>
      <div v-if="error" class="m-error">{{ error }}</div>
      <div class="m-actions">
        <button class="btn ghost" @click="emit('close')">取消</button>
        <button class="btn primary" :disabled="saving" @click="save">{{ saving ? "保存中…" : "保存" }}</button>
      </div>
    </div>
  </div>
</template>