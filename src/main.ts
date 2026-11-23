import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import "./styles/base.css";
import { api } from "./ipc";
import { applyTheme } from "./theme";

createApp(App).use(createPinia()).mount("#app");

api.getSetting("theme", "dark").then(applyTheme).catch(() => applyTheme("dark"));