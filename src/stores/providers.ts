import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { dashboardV3, isRevisionConflict } from "../api/dashboard-v3.ts";
import type {
  PricingSnapshot,
  ProviderCatalog,
  ProviderContracts,
  ProviderModelCapability,
  ZenFreeModels,
  ZenFreeSettings,
} from "../api/generated/dashboard-v3.ts";
import { useControlPlaneStore } from "./controlPlane.ts";

/**
 * Provider registry state: the built-in Plan catalog, Go protocol-table
 * capabilities, Zen Free enablement/catalog snapshot, effective provider
 * contracts, and the Go pricing snapshot.
 *
 * Transient flows (protocol probe progress, pricing refresh confirmation,
 * per-scope catalog refresh spinners) stay page-local in the Providers view
 * and its components; this store only holds the loaded control-plane data.
 */
export const useProvidersStore = defineStore("providers", () => {
  const controlPlane = useControlPlaneStore();

  const catalog = ref<ProviderCatalog | null>(null);
  const modelCapabilities = ref<ProviderModelCapability[]>([]);
  const zenFree = ref<ZenFreeSettings | null>(null);
  const zenModels = ref<ZenFreeModels | null>(null);
  const contracts = ref<ProviderContracts | null>(null);
  const pricing = ref<PricingSnapshot | null>(null);
  const loading = ref(false);
  const error = ref("");

  async function loadCatalog(): Promise<ProviderCatalog> {
    const result = await dashboardV3.getProviders();
    catalog.value = result;
    return result;
  }

  async function loadModelCapabilities(): Promise<ProviderModelCapability[]> {
    const result = await dashboardV3.getProviderModelCapabilities();
    modelCapabilities.value = result;
    return result;
  }

  async function loadZenFree(): Promise<void> {
    const [settings, models] = await Promise.all([
      dashboardV3.getZenFreeSettings(),
      dashboardV3.getZenFreeModels(),
    ]);
    zenFree.value = settings;
    zenModels.value = models;
  }

  async function loadContracts(): Promise<ProviderContracts> {
    loading.value = true;
    try {
      const result = await dashboardV3.getProviderContracts();
      contracts.value = result;
      error.value = "";
      return result;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function loadPricing(): Promise<PricingSnapshot> {
    const result = await dashboardV3.getPricing();
    pricing.value = result;
    return result;
  }

  async function setZenFreeEnabled(enabled: boolean): Promise<ZenFreeSettings> {
    try {
      const result = await controlPlane.runMutation((exp) =>
        dashboardV3.patchZenFreeSettings(enabled, exp));
      zenFree.value = result;
      return result;
    } catch (cause) {
      if (isRevisionConflict(cause)) await loadZenFree();
      throw cause;
    }
  }

  /** Explicit admin-triggered catalog refresh; the only upstream catalog call. */
  async function refreshZenModels(): Promise<ZenFreeModels> {
    try {
      const result = await controlPlane.runMutation((exp) =>
        dashboardV3.refreshZenFreeModels(exp));
      zenModels.value = result;
      return result;
    } catch (cause) {
      if (isRevisionConflict(cause)) await loadZenFree();
      throw cause;
    }
  }

  async function putProtocolSwitch(scopeId: string, protocol: string, enabled: boolean): Promise<ProviderContracts> {
    if (contracts.value?.customEndpoints.some((scope) => scope.scopeId === scopeId)) {
      throw new Error("Custom API 协议变更尚未纳入 Dashboard V3 合同，请在账号配置中保持创建时协议");
    }
    try {
      const result = await controlPlane.runMutation((exp) =>
        dashboardV3.putProviderProtocolSwitch(scopeId, protocol, enabled, exp));
      contracts.value = result;
      return result;
    } catch (cause) {
      if (isRevisionConflict(cause)) await loadContracts();
      throw cause;
    }
  }

  function reset(): void {
    catalog.value = null;
    modelCapabilities.value = [];
    zenFree.value = null;
    zenModels.value = null;
    contracts.value = null;
    pricing.value = null;
    loading.value = false;
    error.value = "";
  }

  return {
    catalog: computed(() => catalog.value),
    modelCapabilities: computed(() => modelCapabilities.value),
    zenFree: computed(() => zenFree.value),
    zenModels: computed(() => zenModels.value),
    contracts: computed(() => contracts.value),
    pricing: computed(() => pricing.value),
    loading: computed(() => loading.value),
    error: computed(() => error.value),
    loadCatalog,
    loadModelCapabilities,
    loadZenFree,
    loadContracts,
    loadPricing,
    setZenFreeEnabled,
    refreshZenModels,
    putProtocolSwitch,
    reset,
  };
});
