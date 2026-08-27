import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { isRevisionConflict } from "../api/dashboard.ts";
import { providerApi } from "../api/providers.ts";
import type {
  ContractScopeKind,
  ModelProtocolOverrideUpdate,
  ProviderCatalogEntry,
  ProviderContractsResponse,
  ProviderModelCapability,
  ZenFreeModelsResponse,
} from "../api/providers.ts";
import type { PricingSnapshot } from "../api/dashboard-presenters.ts";
import type { ZenFreeSettings } from "../api/generated/dashboard-v3.ts";

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
  const catalog = ref<ProviderCatalogEntry[] | null>(null);
  const modelCapabilities = ref<ProviderModelCapability[]>([]);
  const zenFree = ref<ZenFreeSettings | null>(null);
  const zenModels = ref<ZenFreeModelsResponse | null>(null);
  const contracts = ref<ProviderContractsResponse | null>(null);
  const pricing = ref<PricingSnapshot | null>(null);
  const loading = ref(false);
  const error = ref("");

  async function loadCatalog(): Promise<ProviderCatalogEntry[]> {
    const result = await providerApi.getProviderCatalog();
    catalog.value = result;
    return result;
  }

  async function loadModelCapabilities(): Promise<ProviderModelCapability[]> {
    const result = await providerApi.getProviderModelCapabilities();
    modelCapabilities.value = result;
    return result;
  }

  async function loadZenFree(): Promise<void> {
    const [settings, models] = await Promise.all([
      providerApi.getZenFreeSettings(),
      providerApi.getZenFreeModels(),
    ]);
    zenFree.value = settings;
    zenModels.value = models;
  }

  async function loadContracts(): Promise<ProviderContractsResponse> {
    loading.value = true;
    try {
      const result = await providerApi.getProviderContracts();
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
    const result = await providerApi.getGoPricing();
    pricing.value = result;
    return result;
  }

  async function setZenFreeEnabled(enabled: boolean): Promise<ZenFreeSettings> {
    try {
      const result = await providerApi.setZenFreeEnabled(enabled);
      zenFree.value = result;
      return result;
    } catch (cause) {
      if (isRevisionConflict(cause)) await loadZenFree();
      throw cause;
    }
  }

  /** Explicit admin-triggered catalog refresh; the only upstream catalog call. */
  async function refreshZenModels(): Promise<ZenFreeModelsResponse> {
    try {
      const result = await providerApi.refreshZenFreeModels();
      zenModels.value = result;
      return result;
    } catch (cause) {
      if (isRevisionConflict(cause)) await loadZenFree();
      throw cause;
    }
  }

  async function refreshContractCatalog(
    scopeKind: ContractScopeKind,
    scopeId: string,
  ): Promise<ProviderContractsResponse> {
    const result = await providerApi.refreshContractCatalog(scopeKind, scopeId);
    contracts.value = result;
    return result;
  }

  async function resetStaticModelProtocols(
    scopeId: string,
  ): Promise<ProviderContractsResponse> {
    try {
      const result = await providerApi.resetStaticModelProtocols(scopeId);
      contracts.value = result;
      return result;
    } catch (cause) {
      if (isRevisionConflict(cause)) await loadContracts();
      throw cause;
    }
  }

  async function putModelProtocolOverrides(
    scopeKind: ContractScopeKind,
    scopeId: string,
    overrides: ModelProtocolOverrideUpdate[],
  ): Promise<ProviderContractsResponse> {
    try {
      const result = await providerApi.updateModelProtocolOverrides(scopeKind, scopeId, overrides);
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
    refreshContractCatalog,
    resetStaticModelProtocols,
    putModelProtocolOverrides,
    reset,
  };
});
