<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { useDialogs } from "@/stores/dialogs";

const dialogs = useDialogs();
const inputRef = ref<HTMLInputElement | null>(null);

watch(
  () => dialogs.state.visible,
  async (v) => {
    if (v && dialogs.state.kind === "prompt") {
      await nextTick();
      inputRef.value?.focus();
      inputRef.value?.select();
    }
  }
);

function onKey(e: KeyboardEvent) {
  if (!dialogs.state.visible) return;
  if (e.key === "Escape") dialogs.cancel();
  if (e.key === "Enter" && dialogs.state.kind !== "prompt") dialogs.accept();
}
</script>

<template>
  <div v-if="dialogs.state.visible" class="overlay" style="z-index: 200" @keydown="onKey">
    <div class="modal" style="width: 440px">
      <div class="m-title" style="display: flex; align-items: center; gap: 10px; margin-bottom: 12px">
        <span
          v-if="dialogs.state.kind === 'confirm'"
          :style="{
            width: '30px', height: '30px', borderRadius: '50%', flex: 'none',
            display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '15px',
            background: dialogs.state.danger ? 'var(--red-dim)' : 'var(--yellow-dim)',
            color: dialogs.state.danger ? 'var(--red)' : 'var(--yellow)',
          }"
        >{{ dialogs.state.danger ? "✕" : "!" }}</span>
        {{ dialogs.state.title }}
      </div>
      <div style="color: var(--muted); font-size: 13.5px; line-height: 1.7; white-space: pre-line">{{ dialogs.state.message }}</div>
      <input
        v-if="dialogs.state.kind === 'prompt'"
        ref="inputRef"
        v-model="dialogs.state.input"
        class="input"
        style="margin-top: 14px"
        @keyup.enter="dialogs.accept()"
      />
      <div class="m-actions">
        <button v-if="dialogs.state.kind !== 'alert'" class="btn ghost" @click="dialogs.cancel()">取消</button>
        <button class="btn" :class="dialogs.state.danger ? 'danger-ghost' : 'primary'" @click="dialogs.accept()">
          {{ dialogs.state.kind === "alert" ? "知道了" : "确定" }}
        </button>
      </div>
    </div>
  </div>
</template>