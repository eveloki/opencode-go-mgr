import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { dashboardV3, isRevisionConflict, type WithoutExpectation } from "../api/dashboard-v3.ts";
import type {
  ClaudeDesktopModels,
  ClaudeDesktopModelsUpdate,
  Settings,
  SettingsUpdate,
} from "../api/generated/dashboard-v3.ts";
import { useControlPlaneStore } from "./controlPlane.ts";
import {
  presentSettings,
  settingsUpdateInput,
  type AppConfig,
} from "../api/dashboard-presenters.ts";

/**
 * Application settings plus the Claude Desktop three-role mapping.
 *
 * The Settings resource never carries Key plaintext (that lives in the
 * connection store). Update-check / install progress is transient and stays
 * page-local in the Settings view.
 */
export const useSettingsStore = defineStore("settings", () => {
  const controlPlane = useControlPlaneStore();

  const settings = ref<Settings | null>(null);
  const claudeDesktop = ref<ClaudeDesktopModels | null>(null);
  const loading = ref(false);
  const error = ref("");

  async function load(): Promise<Settings> {
    loading.value = true;
    try {
      const result = await dashboardV3.getSettings();
      settings.value = result;
      error.value = "";
      return result;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function loadPresented(): Promise<AppConfig> {
    return presentSettings(await load());
  }

  async function put(update: WithoutExpectation<SettingsUpdate>): Promise<void> {
    try {
      await controlPlane.runMutation((exp) => dashboardV3.putSettings(update, exp));
      await load();
    } catch (cause) {
      if (isRevisionConflict(cause)) await load();
      throw cause;
    }
  }

  async function putPresented(update: AppConfig): Promise<AppConfig> {
    await put(settingsUpdateInput(update));
    if (!settings.value) throw new Error("settings reload returned no resource");
    return presentSettings(settings.value);
  }

  async function loadClaudeDesktop(): Promise<ClaudeDesktopModels> {
    const result = await dashboardV3.getClaudeDesktopModels();
    claudeDesktop.value = result;
    return result;
  }

  async function putClaudeDesktop(models: WithoutExpectation<ClaudeDesktopModelsUpdate>): Promise<ClaudeDesktopModels> {
    try {
      const result = await controlPlane.runMutation((exp) =>
        dashboardV3.putClaudeDesktopModels(models, exp));
      claudeDesktop.value = result;
      return result;
    } catch (cause) {
      if (isRevisionConflict(cause)) await loadClaudeDesktop();
      throw cause;
    }
  }

  function reset(): void {
    settings.value = null;
    claudeDesktop.value = null;
    loading.value = false;
    error.value = "";
  }

  return {
    settings: computed(() => settings.value),
    claudeDesktop: computed(() => claudeDesktop.value),
    loading: computed(() => loading.value),
    error: computed(() => error.value),
    load,
    loadPresented,
    put,
    putPresented,
    loadClaudeDesktop,
    putClaudeDesktop,
    reset,
  };
});
