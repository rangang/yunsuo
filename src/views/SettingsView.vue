<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, autostart, isTauri } from "@/ipc";
import { useDialogs } from "@/stores/dialogs";
import { applyTheme } from "@/theme";
import { credStore, osName } from "@/platform";
import logoUrl from "@/assets/logo.png";

const dialogs = useDialogs();
const section = ref("general");
const navs = [
  { id: "general", name: "通用", icon: "M12 3a9 9 0 1 0 9 9 7 7 0 0 1-9-9z" },
  { id: "deploy", name: "部署默认值", icon: "M12 19V5M5 12l7-7 7 7" },
  { id: "notify", name: "通知", icon: "M18 8a6 6 0 1 0-12 0c0 7-3 9-3 9h18s-3-2-3-9M13.7 21a2 2 0 0 1-3.4 0" },
  { id: "security", name: "安全与数据", icon: "M12 2l8 4v6c0 5-3.5 8.5-8 10-4.5-1.5-8-5-8-10V6z" },
  { id: "about", name: "关于", icon: "M12 16v-4M12 8h.01M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z" },
];

const theme = ref("dark");
const autoStart = ref(false);
const closeToTray = ref(true);
const backupKeep = ref(3);
const autoHealth = ref(true);
const uploadRetry = ref("2");
const sshTimeout = ref("10");
const mavenPath = ref("");
const notifySuccess = ref(true);
const notifyFailure = ref(true);
const quietStart = ref(0);
const quietEnd = ref(0);
const logMasking = ref(true);
const cleaning = ref(false);

async function set(key: string, value: string) {
  await api.setSetting(key, value);
}

async function changeTheme(v: string) {
  theme.value = v;
  applyTheme(v);
  await set("theme", v);
}

async function changeAutostart() {
  autoStart.value = !autoStart.value;
  try {
    await autostart.setEnabled(autoStart.value);
  } catch (e) {
    autoStart.value = !autoStart.value;
    await dialogs.alert(String(e), "设置自启动失败");
  }
}

async function changeCloseToTray(v: string) {
  closeToTray.value = v === "1";
  await set("close_to_tray", v);
}

async function cleanLogs() {
  const ok = await dialogs.confirm("清理 90 天前的部署日志文件？\n历史记录会保留，仅删除过期日志文件。", { title: "清理日志" });
  if (!ok) return;
  cleaning.value = true;
  try {
    const n = await api.cleanupOldLogs(90);
    await dialogs.alert(`已清理 ${n} 个过期日志文件`, "清理完成");
  } finally {
    cleaning.value = false;
  }
}

const hours = Array.from({ length: 24 }, (_, i) => i);

onMounted(async () => {
  theme.value = await api.getSetting("theme", "dark");
  closeToTray.value = (await api.getSetting("close_to_tray", "1")) === "1";
  backupKeep.value = Number(await api.getSetting("default_backup_keep", "3")) || 3;
  autoHealth.value = (await api.getSetting("auto_health_check", "1")) === "1";
  uploadRetry.value = await api.getSetting("upload_retry", "2");
  sshTimeout.value = await api.getSetting("ssh_timeout", "10");
  mavenPath.value = await api.getSetting("maven_path", "");
  notifySuccess.value = (await api.getSetting("notify_success", "1")) === "1";
  notifyFailure.value = (await api.getSetting("notify_failure", "1")) === "1";
  quietStart.value = Number(await api.getSetting("quiet_start", "0"));
  quietEnd.value = Number(await api.getSetting("quiet_end", "0"));
  logMasking.value = (await api.getSetting("log_masking", "1")) === "1";
  try { autoStart.value = await autostart.isEnabled(); } catch { /* 忽略 */ }
});
</script>

<template>
  <div class="main">
    <div class="page-head">
      <div>
        <div class="page-title">设置</div>
        <div class="page-sub">全局偏好 · 部署默认值 · 通知与安全</div>
      </div>
    </div>

    <div class="settings-wrap">
      <div class="settings-nav">
        <div
          v-for="n in navs"
          :key="n.id"
          class="sn-item"
          :class="{ active: section === n.id }"
          @click="section = n.id"
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path :d="n.icon" /></svg>
          {{ n.name }}
        </div>
      </div>

      <div class="settings-body">
        <!-- 通用 -->
        <div v-if="section === 'general'" class="set-card">
          <div class="set-row">
            <div><div class="r-label">主题</div><div class="r-desc">深色 / 浅色 / 跟随系统</div></div>
            <div class="r-ctrl">
              <select class="input" style="width: 140px" :value="theme" @change="changeTheme(($event.target as HTMLSelectElement).value)">
                <option value="dark">深色</option>
                <option value="light">浅色</option>
                <option value="system">跟随系统</option>
              </select>
            </div>
          </div>
          <div class="set-row">
            <div><div class="r-label">界面语言</div><div class="r-desc">更多语言后续版本支持</div></div>
            <div class="r-ctrl"><select class="input" style="width: 140px" disabled><option>简体中文</option></select></div>
          </div>
          <div class="set-row">
            <div><div class="r-label">开机自启动</div><div class="r-desc">登录 {{ osName }} 后自动运行</div></div>
            <div class="r-ctrl"><button class="toggle" :class="{ on: autoStart }" @click="changeAutostart"></button></div>
          </div>
          <div class="set-row">
            <div><div class="r-label">关闭主窗口时</div><div class="r-desc">部署进行中始终最小化到托盘继续执行</div></div>
            <div class="r-ctrl">
              <select class="input" style="width: 160px" :value="closeToTray ? '1' : '0'" @change="changeCloseToTray(($event.target as HTMLSelectElement).value)">
                <option value="1">最小化到托盘</option>
                <option value="0">退出应用</option>
              </select>
            </div>
          </div>
        </div>

        <!-- 部署默认值 -->
        <div v-if="section === 'deploy'" class="set-card">
          <div class="set-row">
            <div><div class="r-label">备份保留份数</div><div class="r-desc">新建部署配置时的默认值，每个配置可单独覆盖</div></div>
            <div class="r-ctrl">
              <input v-model.number="backupKeep" type="number" min="1" max="20" class="input mono" style="width: 90px" @change="set('default_backup_keep', String(backupKeep || 3))" />
            </div>
          </div>
          <div class="set-row">
            <div><div class="r-label">部署后自动健康检查</div><div class="r-desc">配置了检查 URL 的项目部署后自动执行</div></div>
            <div class="r-ctrl"><button class="toggle" :class="{ on: autoHealth }" @click="autoHealth = !autoHealth; set('auto_health_check', autoHealth ? '1' : '0')"></button></div>
          </div>
          <div class="set-row">
            <div><div class="r-label">上传失败自动重试</div><div class="r-desc">SFTP 中断后的重试次数</div></div>
            <div class="r-ctrl">
              <select v-model="uploadRetry" class="input" style="width: 110px" @change="set('upload_retry', uploadRetry)">
                <option value="0">不重试</option><option value="1">1 次</option><option value="2">2 次</option><option value="3">3 次</option>
              </select>
            </div>
          </div>
          <div class="set-row">
            <div><div class="r-label">SSH 连接超时</div><div class="r-desc">部署与回滚时建立 SSH 连接的等待上限</div></div>
            <div class="r-ctrl">
              <select v-model="sshTimeout" class="input" style="width: 110px" @change="set('ssh_timeout', sshTimeout)">
                <option value="5">5 秒</option><option value="10">10 秒</option><option value="15">15 秒</option><option value="30">30 秒</option>
              </select>
            </div>
          </div>
          <div class="set-row">
            <div style="flex: 1">
              <div class="r-label">Maven 路径（可选）</div>
              <div class="r-desc">留空自动探测：PATH → MAVEN_HOME → IDEA 内置 Maven；也可填 mvn.cmd 完整路径</div>
              <input v-model="mavenPath" class="input mono" style="margin-top: 8px" placeholder="如 D:\software\IntelliJ IDEA 2026.2\plugins\maven-plugin\lib\maven3\bin\mvn.cmd" @change="set('maven_path', mavenPath.trim())" />
            </div>
          </div>
        </div>

        <!-- 通知 -->
        <div v-if="section === 'notify'" class="set-card">
          <div class="set-row">
            <div><div class="r-label">部署/回滚成功通知</div><div class="r-desc">任务完成时发送 {{ osName }} 桌面通知</div></div>
            <div class="r-ctrl"><button class="toggle" :class="{ on: notifySuccess }" @click="notifySuccess = !notifySuccess; set('notify_success', notifySuccess ? '1' : '0')"></button></div>
          </div>
          <div class="set-row">
            <div><div class="r-label">部署/回滚失败通知</div><div class="r-desc">失败时及时提醒，便于快速回滚</div></div>
            <div class="r-ctrl"><button class="toggle" :class="{ on: notifyFailure }" @click="notifyFailure = !notifyFailure; set('notify_failure', notifyFailure ? '1' : '0')"></button></div>
          </div>
          <div class="set-row">
            <div><div class="r-label">通知免打扰时段</div><div class="r-desc">起止相同 = 关闭；时段内仅记录不弹通知</div></div>
            <div class="r-ctrl" style="display: flex; gap: 6px; align-items: center">
              <select v-model.number="quietStart" class="input" style="width: 86px" @change="set('quiet_start', String(quietStart))">
                <option v-for="h in hours" :key="h" :value="h">{{ String(h).padStart(2, "0") }}:00</option>
              </select>
              <span style="color: var(--faint)">至</span>
              <select v-model.number="quietEnd" class="input" style="width: 86px" @change="set('quiet_end', String(quietEnd))">
                <option v-for="h in hours" :key="h" :value="h">{{ String(h).padStart(2, "0") }}:00</option>
              </select>
            </div>
          </div>
        </div>

        <!-- 安全与数据 -->
        <div v-if="section === 'security'" class="set-card">
          <div class="set-row">
            <div><div class="r-label">凭据存储</div><div class="r-desc">密码与私钥口令保存在 {{ credStore }}，不落明文盘</div></div>
            <div class="r-ctrl"><span class="tag green">已启用</span></div>
          </div>
          <div class="set-row">
            <div><div class="r-label">日志敏感信息脱敏</div><div class="r-desc">密码等字符串在日志中替换为 ***</div></div>
            <div class="r-ctrl"><button class="toggle" :class="{ on: logMasking }" @click="logMasking = !logMasking; set('log_masking', logMasking ? '1' : '0')"></button></div>
          </div>
          <div class="set-row">
            <div><div class="r-label">日志保存目录</div><div class="r-desc mono" style="font-size: 11px">%APPDATA%\com.deploypilot.app\logs</div></div>
            <div class="r-ctrl"><button class="btn ghost sm" @click="api.openLogDir()">打开目录</button></div>
          </div>
          <div class="set-row">
            <div><div class="r-label">清理过期日志</div><div class="r-desc">删除 90 天前的日志文件，历史记录保留</div></div>
            <div class="r-ctrl"><button class="btn ghost sm" :disabled="cleaning" @click="cleanLogs">{{ cleaning ? "清理中…" : "立即清理" }}</button></div>
          </div>
        </div>

        <!-- 关于 -->
        <div v-if="section === 'about'" class="set-card about-box">
          <div class="a-mark">
            <img :src="logoUrl" alt="云梭" />
          </div>
          <div style="font-size: 17px; font-weight: 700">云梭 <span style="color: var(--faint); font-weight: 400; font-size: 12.5px">v0.1.11</span></div>
          <div style="font-size: 12.5px; color: var(--muted); margin-top: 6px">一键打包部署 · 让发版像穿梭一样快</div>
          <div style="font-size: 12px; color: var(--faint); margin-top: 10px">Tauri 2 · Rust · Vue 3　|　{{ isTauri ? "桌面应用" : "浏览器调试模式" }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-wrap { display: flex; gap: 22px; flex: 1; min-height: 0; }
.settings-nav { width: 168px; flex: none; display: flex; flex-direction: column; gap: 4px; }
.sn-item {
  display: flex; align-items: center; gap: 10px; padding: 10px 14px;
  border-radius: 8px; font-size: 13.5px; color: var(--muted); cursor: pointer;
}
.sn-item:hover { background: var(--hover); }
.sn-item.active { background: rgba(31, 111, 235, 0.16); color: var(--blue); font-weight: 600; }
.settings-body { flex: 1; max-width: 680px; }
.set-card { background: var(--panel); border: 1px solid var(--border); border-radius: 12px; padding: 8px 20px; }
.set-row { display: flex; align-items: center; gap: 16px; padding: 16px 0; border-bottom: 1px solid var(--border); }
.set-row:last-child { border-bottom: none; }
.set-row .r-label { font-size: 13.5px; font-weight: 600; }
.set-row .r-desc { font-size: 12px; color: var(--faint); margin-top: 3px; }
.set-row .r-ctrl { margin-left: auto; flex: none; }
.about-box { text-align: center; padding: 40px 20px; }
.about-box .a-mark {
  width: 64px; height: 64px; border-radius: 18px;
  margin: 0 auto 16px; display: flex; align-items: center; justify-content: center;
  overflow: hidden;
}
.about-box .a-mark img { width: 100%; height: 100%; display: block; }
</style>