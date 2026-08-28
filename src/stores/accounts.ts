import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { dashboardApi, isRevisionConflict } from "../api/dashboard.ts";
import { providerApi } from "../api/providers.ts";
import type { ProviderUsageResponse } from "../api/providers.ts";
import type {
  Account,
  AccountCustomConfigUpdateInput,
  AccountInput,
  AccountModelCapabilityInput,
  AccountUpdate,
  UsageWindow,
} from "../api/dashboard.ts";

/**
 * Local accounts control plane: the account list plus per-account usage
 * projections (`usage` windows and provider usage). Wizard drafts, browser
 * sessions, and verification progress stay page-local in the views.
 *
 * On 409 the store reloads the affected resource and surfaces the conflict;
 * rejected writes are never replayed automatically.
 */
export const useAccountsStore = defineStore("accounts", () => {
  const accounts = ref<Account[]>([]);
  const loaded = ref(false);
  const loading = ref(false);
  const error = ref("");
  const usageById = ref<Record<string, UsageWindow>>({});
  const providerUsageById = ref<Record<string, ProviderUsageResponse>>({});

  const byId = computed(() => {
    const map = new Map<string, Account>();
    for (const account of accounts.value) map.set(account.id, account);
    return map;
  });

  async function load(): Promise<Account[]> {
    loading.value = true;
    try {
      const list = await dashboardApi.getAccounts();
      accounts.value = list;
      loaded.value = true;
      error.value = "";
      return list;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function loadPresented(): Promise<Account[]> {
    return load();
  }

  function applyAccount(account: Account): Account {
    const index = accounts.value.findIndex((item) => item.id === account.id);
    if (index >= 0) accounts.value.splice(index, 1, account);
    else accounts.value.push(account);
    return account;
  }

  async function mutate<T>(run: () => Promise<T>, onConflict?: () => Promise<unknown>): Promise<T> {
    try {
      return await run();
    } catch (error) {
      if (isRevisionConflict(error)) await (onConflict ?? load)();
      throw error;
    }
  }

  async function create(input: AccountInput): Promise<Account> {
    return mutate(() => dashboardApi.createAccount(input).then(applyAccount));
  }

  async function createManaged(input: { name: string; username?: string; notes?: string }): Promise<Account> {
    return mutate(() => dashboardApi.createManagedAccount(input).then(applyAccount));
  }

  async function update(id: string, update: AccountUpdate): Promise<Account> {
    return mutate(() => dashboardApi.updateAccount(id, update).then(applyAccount));
  }

  async function remove(id: string): Promise<void> {
    await mutate(() => dashboardApi.deleteAccount(id));
    accounts.value = accounts.value.filter((account) => account.id !== id);
    const { [id]: _usage, ...usageRest } = usageById.value;
    usageById.value = usageRest;
    const { [id]: _providerUsage, ...providerRest } = providerUsageById.value;
    providerUsageById.value = providerRest;
  }

  async function reorder(accountIds: string[]): Promise<void> {
    const list = await mutate(() => dashboardApi.reorderAccounts(accountIds));
    accounts.value = list;
  }

  async function toggle(id: string): Promise<Account> {
    return mutate(() => dashboardApi.toggleAccount(id).then(applyAccount));
  }

  async function resetCooldown(id: string): Promise<Account> {
    return mutate(() => dashboardApi.resetAccountCooldown(id).then(applyAccount));
  }

  async function advanceSetup(id: string, setupStep: "google_account" | "opencode_registration" | "payment" | "key_verification" | "ready"): Promise<Account> {
    return mutate(() => dashboardApi.advanceAccountSetup(id, setupStep).then(applyAccount));
  }

  async function verifyManagedKey(id: string, key: string): Promise<Account> {
    return mutate(() => dashboardApi.verifyManagedAccountKey(id, key).then(applyAccount));
  }

  async function verifyConnection(id: string): Promise<Account> {
    return mutate(() => dashboardApi.verifyAccountConnection(id).then(applyAccount));
  }

  async function putCustomConfig(id: string, config: AccountCustomConfigUpdateInput): Promise<Account> {
    return mutate(() => dashboardApi.updateAccountCustomConfig(id, config).then(applyAccount));
  }

  async function putModelCapabilities(id: string, capabilities: AccountModelCapabilityInput[]): Promise<Account> {
    return mutate(() => dashboardApi.updateAccountModelCapabilities(id, capabilities).then(applyAccount));
  }

  async function loadUsage(id: string): Promise<UsageWindow> {
    const usage = await dashboardApi.getAccountUsage(id);
    usageById.value = { ...usageById.value, [id]: usage };
    return usage;
  }

  async function patchUsage(
    id: string,
    update: { window: "window_5h" | "window_week" | "window_month"; percent: number; resetsInMinutes?: number | null },
  ): Promise<UsageWindow> {
    const result = await mutate(
      () => dashboardApi.updateAccountUsage(id, update.window, update.percent, update.resetsInMinutes ?? null),
      () => loadUsage(id),
    );
    usageById.value = { ...usageById.value, [id]: result };
    return result;
  }

  /** Official Go usage calibration; 429 surfaces as DashboardThrottledError. */
  async function refreshUsage(id: string) {
    const result = await mutate(
      () => dashboardApi.refreshAccountUsage(id),
      () => loadUsage(id),
    );
    usageById.value = { ...usageById.value, [id]: result.usage };
    return result;
  }

  async function loadProviderUsage(id: string): Promise<ProviderUsageResponse> {
    const usage = await providerApi.getProviderUsage(id);
    providerUsageById.value = { ...providerUsageById.value, [id]: usage };
    return usage;
  }

  function reset(): void {
    accounts.value = [];
    loaded.value = false;
    loading.value = false;
    error.value = "";
    usageById.value = {};
    providerUsageById.value = {};
  }

  return {
    accounts: computed(() => accounts.value),
    loaded: computed(() => loaded.value),
    loading: computed(() => loading.value),
    error: computed(() => error.value),
    usageById: computed(() => usageById.value),
    providerUsageById: computed(() => providerUsageById.value),
    byId,
    load,
    loadPresented,
    create,
    createManaged,
    update,
    remove,
    reorder,
    toggle,
    resetCooldown,
    advanceSetup,
    verifyManagedKey,
    verifyConnection,
    putCustomConfig,
    putModelCapabilities,
    loadUsage,
    patchUsage,
    refreshUsage,
    loadProviderUsage,
    reset,
  };
});
