<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useAppStore } from "@/stores/app";
import Sidebar from "@/components/Sidebar.vue";
import ServersView from "@/views/ServersView.vue";
import ProjectView from "@/views/ProjectView.vue";
import HistoryView from "@/views/HistoryView.vue";
import SettingsView from "@/views/SettingsView.vue";
import ProjectModal from "@/components/ProjectModal.vue";
import AppDialogs from "@/components/AppDialogs.vue";

const store = useAppStore();
const showProjectModal = ref(false);

onMounted(() => {
  store.refresh();
});
</script>

<template>
  <div class="layout">
    <Sidebar @add-project="showProjectModal = true" />
    <ProjectView v-if="store.view.kind === 'project'" :key="store.view.id" />
    <ServersView v-else-if="store.view.kind === 'servers'" />
    <HistoryView v-else-if="store.view.kind === 'history'" />
    <SettingsView v-else />
  </div>
  <ProjectModal v-if="showProjectModal" :project="null" @close="showProjectModal = false" @saved="store.refresh()" />
  <AppDialogs />
</template>