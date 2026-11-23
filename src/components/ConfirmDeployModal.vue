<script setup lang="ts">
import { ref } from "vue";
import type { DeployProfile, Project, Server } from "@/ipc/types";

defineProps<{ project: Project; server: Server; profile: DeployProfile }>();
const emit = defineEmits<{ (e: "cancel"): void; (e: "confirm"): void }>();
const confirmed = ref(false);
</script>

<template>
  <div class="overlay">
    <div class="modal" style="width: 520px">
      <div class="m-title" style="display: flex; align-items: center; gap: 11px">
        <span style="width: 34px; height: 34px; border-radius: 50%; background: var(--yellow-dim); display: flex; align-items: center; justify-content: center; font-size: 17px; color: var(--yellow); flex: none">!</span>
        确认部署到生产环境？
      </div>
      <div style="color: var(--muted); font-size: 13px; margin-bottom: 16px">请核对以下信息，部署将替换线上版本并重启服务。</div>
      <div class="kv"><span class="k">项目</span><span class="v"><b>{{ project.name }}</b>　<span class="tag red" style="font-size: 10.5px">{{ profile.env_name === "default" ? "默认环境" : profile.env_name }}</span></span></div>
      <div class="kv"><span class="k">构建命令</span><span class="v mono">{{ project.build_command }}</span></div>
      <div class="kv"><span class="k">目标服务器</span><span class="v">{{ server.name }}　<span class="mono" style="color: var(--muted)">{{ server.username }}@{{ server.host }}:{{ server.port }}</span></span></div>
      <div class="kv"><span class="k">远程目录</span><span class="v mono">{{ profile.remote_dir }}</span></div>
      <div class="kv"><span class="k">部署位置</span><span class="v mono">{{ profile.remote_dir }}/{{ project.type === "java" ? profile.jar_name : profile.site_dir }}</span></div>
      <div class="kv"><span class="k">将执行命令</span><span class="v mono" style="color: var(--yellow)">{{ profile.restart_command || "（无重启命令）" }}</span></div>
      <div class="kv"><span class="k">自动备份</span><span class="v">是 · 保留最近 {{ profile.backup_keep }} 份，失败可一键回滚</span></div>
      <div class="checkbox-row" @click="confirmed = !confirmed">
        <span class="checkbox" :class="{ checked: confirmed }"></span>
        我已确认本次变更内容，知晓将影响线上服务
      </div>
      <div class="m-actions">
        <button class="btn ghost" @click="emit('cancel')">取消</button>
        <button class="btn primary" :disabled="!confirmed" style="padding-left: 26px; padding-right: 26px" @click="emit('confirm')">确认部署</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.kv { display: flex; padding: 8px 0; font-size: 13px; border-bottom: 1px dashed var(--border); }
.kv:last-of-type { border-bottom: none; }
.kv .k { width: 118px; color: var(--faint); flex: none; }
.kv .v { color: var(--text); }
.checkbox-row { display: flex; align-items: center; gap: 9px; margin-top: 16px; font-size: 12.8px; color: var(--muted); cursor: pointer; }
.checkbox { width: 16px; height: 16px; border-radius: 4px; border: 1.5px solid var(--border-light); flex: none; position: relative; }
.checkbox.checked { background: var(--blue-deep); border-color: var(--blue-deep); }
.checkbox.checked::after { content: "✓"; color: #fff; font-size: 11px; position: absolute; top: -1px; left: 2.5px; }
</style>