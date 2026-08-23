import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { dashboardV3, isRevisionConflict, type WithoutExpectation } from "../api/dashboard-v3.ts";
import type {
  Account,
  AccountCreate,
  AccountCustomConfigUpdate,
  AccountManagedCreate,
  AccountModelCapabilitiesUpdate,
  AccountMutation,
  AccountSetupStep,
  AccountUpdate,
  AccountUsageUpdate,
  MutationExpectation,
  ProviderUsage,
  UsageWindow,
} from "../api/generated/dashboard-v3.ts";
import { useControlPlaneStore } from "./controlPlane.ts";
import { presentAccount, presentUsage, type Account as PresentedAccount, type UsageWindow as PresentedUsageWindow } from "../api/dashboard-presenters.ts";

/**
 * Local accounts control plane: the account list plus per-account usage
 * projections (`usage` windows and provider usage). Wizard drafts, browser
 * sessions, and verification progress stay page-local in the views.
 *
 * On 409 the store reloads the affected resource and surfaces the conflict;
 * rejected writes are never replayed automatically.
 */
export const useAccountsStore = defineStore("accounts", () => {
  const controlPlane = useControlPlaneStore();

  const accounts = ref<Account[]>([]);
  const loaded = ref(false);
  const loading = ref(false);
  const error = ref("");
  const usageById = ref<Record<string, UsageWindow>>({});
  const providerUsageById = ref<Record<string, ProviderUsage>>({});

  const byId = computed(() => {
    const map = new Map<string, Account>();
    for (const account of accounts.value) map.set(account.id, account);
    return map;
  });

  async function load(): Promise<Account[]> {
    loading.value = true;
    try {
      const list = await dashboardV3.listAccounts();
      accounts.value = list.accounts;
      loaded.value = true;
      error.value = "";
      return list.accounts;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function loadPresented(): Promise<PresentedAccount[]> {
    return (await load()).map(presentAccount);
  }

  function applyMutation(result: AccountMutation): Account | null {
    if (result.account === null) {
      return null;
    }
    const index = accounts.value.findIndex((account) => account.id === result.account?.id);
    if (index >= 0) accounts.value.splice(index, 1, result.account);
    else accounts.value.push(result.account);
    return result.account;
  }

  async function mutate(
    run: (exp: MutationExpectation) => Promise<AccountMutation>,
  ): Promise<Account | null> {
    try {
      return applyMutation(await controlPlane.runMutation(run));
    } catch (error) {
      if (isRevisionConflict(error)) await load();
      throw error;
    }
  }

  async function create(input: WithoutExpectation<AccountCreate>): Promise<Account | null> {
    return mutate((exp) => dashboardV3.createAccount(input, exp));
  }

  async function createManaged(input: WithoutExpectation<AccountManagedCreate>): Promise<Account | null> {
    return mutate((exp) => dashboardV3.createManagedAccount(input, exp));
  }

  async function update(id: string, update: WithoutExpectation<AccountUpdate>): Promise<Account | null> {
    return mutate((exp) => dashboardV3.updateAccount(id, update, exp));
  }

  async function remove(id: string): Promise<void> {
    try {
      await controlPlane.runMutation((exp) => dashboardV3.deleteAccount(id, exp));
    } catch (error) {
      if (isRevisionConflict(error)) await load();
      throw error;
    }
    accounts.value = accounts.value.filter((account) => account.id !== id);
    const { [id]: _usage, ...usageRest } = usageById.value;
    usageById.value = usageRest;
    const { [id]: _providerUsage, ...providerRest } = providerUsageById.value;
    providerUsageById.value = providerRest;
  }

  async function reorder(accountIds: string[]): Promise<void> {
    try {
      const list = await controlPlane.runMutation((exp) => dashboardV3.reorderAccounts(accountIds, exp));
      accounts.value = list.accounts;
    } catch (error) {
      if (isRevisionConflict(error)) await load();
      throw error;
    }
  }

  async function toggle(id: string): Promise<Account | null> {
    return mutate((exp) => dashboardV3.toggleAccount(id, exp));
  }

  async function resetCooldown(id: string): Promise<Account | null> {
    return mutate((exp) => dashboardV3.resetAccountCooldown(id, exp));
  }

  async function advanceSetup(id: string, setupStep: AccountSetupStep): Promise<Account | null> {
    return mutate((exp) => dashboardV3.advanceAccountSetup(id, setupStep, exp));
  }

  async function verifyManagedKey(id: string, key: string): Promise<Account | null> {
    return mutate((exp) => dashboardV3.verifyManagedAccountKey(id, key, exp));
  }

  async function verifyConnection(id: string): Promise<Account | null> {
    return mutate((exp) => dashboardV3.verifyAccount(id, exp));
  }

  async function putCustomConfig(id: string, config: WithoutExpectation<AccountCustomConfigUpdate>): Promise<Account | null> {
    return mutate((exp) => dashboardV3.putAccountCustomConfig(id, config, exp));
  }

  async function putModelCapabilities(id: string, update: WithoutExpectation<AccountModelCapabilitiesUpdate>): Promise<Account | null> {
    return mutate((exp) => dashboardV3.putAccountModelCapabilities(id, update, exp));
  }

  async function acknowledge(id: string, acknowledgementId: string, version: string): Promise<Account | null> {
    return mutate((exp) => dashboardV3.createAccountAcknowledgement(id, { acknowledgementId, version }, exp));
  }

  async function loadUsage(id: string): Promise<UsageWindow> {
    const usage = await dashboardV3.getAccountUsage(id);
    usageById.value = { ...usageById.value, [id]: usage };
    return usage;
  }

  async function loadPresentedUsage(id: string): Promise<PresentedUsageWindow> {
    return presentUsage(await loadUsage(id));
  }

  async function patchUsage(id: string, update: WithoutExpectation<AccountUsageUpdate>): Promise<UsageWindow> {
    try {
      const result = await controlPlane.runMutation((exp) => dashboardV3.patchAccountUsage(id, update, exp));
      usageById.value = { ...usageById.value, [id]: result.usage };
      return result.usage;
    } catch (error) {
      if (isRevisionConflict(error)) await loadUsage(id);
      throw error;
    }
  }

  /** Official Go usage calibration; 429 surfaces as DashboardThrottledError. */
  async function refreshUsage(id: string) {
    try {
      const result = await controlPlane.runMutation((exp) => dashboardV3.refreshAccountUsage(id, exp));
      usageById.value = { ...usageById.value, [id]: result.usage };
      return result;
    } catch (error) {
      if (isRevisionConflict(error)) await loadUsage(id);
      throw error;
    }
  }

  async function loadProviderUsage(id: string): Promise<ProviderUsage> {
    const usage = await dashboardV3.getProviderUsage(id);
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
    acknowledge,
    loadUsage,
    loadPresentedUsage,
    patchUsage,
    refreshUsage,
    loadProviderUsage,
    reset,
  };
});
