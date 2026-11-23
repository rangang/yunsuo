<script setup lang="ts">
import { useAppStore } from "@/stores/app";
import logoUrl from "@/assets/logo.png";

const store = useAppStore();
const emit = defineEmits<{ (e: "add-project"): void }>();

const folderIcon = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></svg>`;
const jarIcon = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 8l-9-5-9 5v8l9 5 9-5z"/><path d="M3 8l9 5 9-5"/></svg>`;
</script>

<template>
  <div class="sidebar">
    <div class="logo">
      <div class="mark">
        <img :src="logoUrl" alt="云梭" />
      </div>
      <div>
        <div class="name">云梭</div>
        <div class="sub">一键打包部署</div>
      </div>
    </div>

    <div class="side-section">项目 <span class="add" title="添加项目" @click="emit('add-project')">＋</span></div>
    <div
      v-for="p in store.projects"
      :key="p.id"
      class="proj-item"
      :class="{ active: store.view.kind === 'project' && store.view.id === p.id }"
      @click="store.openProject(p.id)"
    >
      <span class="p-icon" v-html="p.type === 'java' ? jarIcon : folderIcon"></span>
      <span class="p-name">{{ p.name }}</span>
      <span
        class="dot"
        :class="{
          green: store.projectStatus[p.id] === 'success',
          red: store.projectStatus[p.id] === 'failed',
          yellow: store.projectStatus[p.id] === 'cancelled',
          gray: !store.projectStatus[p.id],
        }"
        :title="store.projectStatus[p.id] === 'success' ? '最近部署成功' : store.projectStatus[p.id] === 'failed' ? '最近部署失败' : store.projectStatus[p.id] === 'cancelled' ? '最近任务已取消' : '尚未部署'"
      ></span>
    </div>
    <div v-if="store.loaded && store.projects.length === 0" class="side-empty">暂无项目，点击 ＋ 添加</div>

    <div class="side-nav">
      <div class="nav-item" :class="{ active: store.view.kind === 'servers' }" @click="store.view = { kind: 'servers' }">
        <span class="n-icon"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="7" rx="2" /><rect x="3" y="13" width="18" height="7" rx="2" /><circle cx="7" cy="7.5" r=".5" fill="currentColor" /><circle cx="7" cy="16.5" r=".5" fill="currentColor" /></svg></span>
        服务器
      </div>
      <div class="nav-item" :class="{ active: store.view.kind === 'history' }" @click="store.view = { kind: 'history' }">
        <span class="n-icon"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 3" /></svg></span>
        部署历史
      </div>
      <div class="nav-item" :class="{ active: store.view.kind === 'settings' }" @click="store.view = { kind: 'settings' }">
        <span class="n-icon"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3" /><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M19.1 4.9L17 7M7 17l-2.1 2.1" /></svg></span>
        设置
      </div>
      <div class="side-ver">v0.1.11</div>
    </div>
  </div>
</template>