import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import "./styles/main.css";
import { applyTheme, getThemeStorage, getThemeTokens, readTheme, resolveTheme } from "./theme";

// Theme and language resolve before mount so the first paint already uses the
// stored preference; Pinia installs before any store consumer mounts.
const initialTheme = readTheme(getThemeStorage());
const initialOsTheme = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
applyTheme(document.documentElement, resolveTheme(initialTheme, initialOsTheme), getThemeTokens(initialTheme, initialOsTheme));

createApp(App).use(createPinia()).mount("#app");
