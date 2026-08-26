<template>
  <div class="providers-page">
    <header class="providers-header">
      <h1>{{ t("供应商") }}</h1>
    </header>

    <div
      v-if="initialLoading"
      class="providers-state"
      role="status"
      aria-live="polite"
      :aria-label="t('加载中…')"
    >
      <n-spin size="small" />
    </div>

    <n-alert
      v-else-if="loadError && !contracts"
      type="error"
      :title="t('加载供应商失败: {error}', { error: loadError })"
    >
      <n-button size="small" secondary :loading="loading" @click="loadContracts()">
        {{ t("重试") }}
      </n-button>
    </n-alert>

    <n-empty
      v-else-if="!loading && scopes.length === 0"
      :description="t('暂无供应商范围')"
    />

    <div v-else-if="activeScope" class="providers-layout">
      <aside class="providers-rail">
        <n-menu
          :value="activeScope.key"
          :options="scopeMenuOptions"
          :aria-label="t('选择供应商范围')"
          @update:value="selectScopeKey"
        />
      </aside>

      <div class="providers-main">
        <div class="providers-mobile-nav">
          <n-select
            :value="activeScope.key"
            :options="scopeSelectOptions"
            :aria-label="t('选择供应商范围')"
            :disabled="actionLocked"
            :consistent-menu-width="false"
            @update:value="selectScopeKey"
          />
        </div>

        <n-alert
          v-if="loadError && contracts"
          type="warning"
          :title="t('加载供应商失败: {error}', { error: loadError })"
        >
          <n-button size="small" secondary :loading="loading" @click="loadContracts({ retain: true })">
            {{ t("重试") }}
          </n-button>
        </n-alert>

        <n-tabs v-model:value="activeTab" class="providers-tabs" display-directive="if">
          <n-tab-pane name="catalog" :tab="t('模型目录')">
            <section class="providers-section" aria-labelledby="provider-catalog-title">
              <h2 id="provider-catalog-title">{{ t("模型目录") }}</h2>
              <dl class="providers-overview providers-overview--compact">
                <div>
                  <dt>{{ t("目录来源") }}</dt>
                  <dd>{{ catalogSourceLabel(activeScope.catalog.source) }}</dd>
                </div>
                <div v-if="safeSourceUrl">
                  <dt>{{ t("来源地址") }}</dt>
                  <dd>
                    <a :href="safeSourceUrl" target="_blank" rel="noopener noreferrer">{{ t("官方来源") }}</a>
                  </dd>
                </div>
                <div>
                  <dt>{{ t("刷新时间") }}</dt>
                  <dd>{{ activeScope.catalog.refreshed_at ? formatTimestamp(activeScope.catalog.refreshed_at) : t("尚未刷新") }}</dd>
                </div>
              </dl>
              <template v-if="catalogRefreshSupported(activeScope)">
                <n-form-item :label="t('选择用于刷新的账号')">
                  <n-select
                    v-model:value="refreshAccountId"
                    :options="refreshAccountOptions"
                    :placeholder="t('选择用于刷新的账号')"
                    :aria-label="t('选择用于刷新的账号')"
                    :disabled="catalogRefreshing || refreshAccountOptions.length === 0"
                    :consistent-menu-width="false"
                  />
                </n-form-item>
                <n-button
                  type="primary"
                  :loading="catalogRefreshing"
                  :disabled="catalogRefreshing || !refreshAccountId"
                  @click="refreshCatalog"
                >
                  {{ catalogRefreshing ? t("正在刷新模型目录…") : t("刷新模型目录") }}
                </n-button>
              </template>
              <p v-else class="providers-empty">{{ t("该供应商不支持刷新模型目录") }}</p>
              <n-alert
                v-if="catalogRefreshError"
                type="error"
                :title="t('刷新模型目录失败: {error}', { error: catalogRefreshError })"
              />
            </section>

            <section class="providers-section" aria-labelledby="provider-matrix-title">
              <h2 id="provider-matrix-title" class="sr-only">{{ t("模型协议矩阵") }}</h2>
              <ProviderModelMatrix
                :scope="activeScope"
                :loading="overridesLoading"
                :probing-models="probingModels"
                @update:overrides="updateOverrides"
                @probe="runModelProbe"
                @error="matrixError = $event"
              />
              <n-alert
                v-if="matrixError"
                type="error"
                :title="t('保存协议覆盖失败: {error}', { error: matrixError })"
              />
              <n-alert
                v-if="probeError"
                type="error"
                :title="t('探测失败: {error}', { error: probeError })"
              />
            </section>
          </n-tab-pane>

          <n-tab-pane name="pricing" :tab="t('模型价格')">
            <section class="providers-section" aria-labelledby="provider-pricing-title">
              <h2 id="provider-pricing-title" class="sr-only">{{ t("模型价格") }}</h2>
              <PricingCatalog :provider-id="activeScope.provider_id" />
            </section>
          </n-tab-pane>
        </n-tabs>
      </div>
    </div>

    <span class="sr-only" aria-live="polite" aria-atomic="true">{{ actionLive }}</span>
  </div>
</template>

<script setup lang="ts">
import { computed, onActivated, onMounted, onUnmounted, ref, watch } from "vue";
import {
  NAlert,
  NButton,
  NEmpty,
  NFormItem,
  NMenu,
  NSelect,
  NSpin,
  NTabPane,
  NTabs,
  useMessage,
} from "naive-ui";
import type { MenuOption, SelectOption } from "naive-ui";
import { DashboardRequestError } from "../api/dashboard";
import { isCustomCatalogRefreshResponse, providerApi } from "../api/providers.ts";
import { useProvidersStore } from "../stores/providers.ts";
import type {
  ModelProtocolOverrideUpdate,
  ProviderCatalogEntry,
  ProviderContractsResponse,
} from "../api/providers.ts";
import ProviderModelMatrix from "../components/ProviderModelMatrix.vue";
import PricingCatalog from "../components/PricingCatalog.vue";
import { locale, t } from "../i18n/index.ts";
import { dashboardErrorDetail } from "../utils/errors.ts";
import { applyAppViewSearchParams, readProviderScopeQuery } from "./app-navigation.ts";
import {
  applyModelContractToResponse,
  catalogRefreshSupported,
  flattenProviderScopes,
  isSafeSourceUrl,
  normalizeProviderContractsResponse,
  PROVIDER_PROTOCOLS,
  scopeAccounts,
  selectProviderScope,
} from "../domain/provider-contracts.ts";
import {
  CATALOG_SOURCE_CUSTOM_DISCOVERY,
  CATALOG_SOURCE_DECLARED,
  CATALOG_SOURCE_OPENCODE_MODELS,
  CATALOG_SOURCE_COMMAND_CODE_MODELS,
  CATALOG_SOURCE_OFFICIAL_ZEN,
  CATALOG_SOURCE_STATIC,
} from "../domain/provider-contracts.ts";

const message = useMessage();
const providersStore = useProvidersStore();
const contracts = ref<ProviderContractsResponse | null>(null);
const catalog = ref<ProviderCatalogEntry[] | null>(null);
const loading = ref(false);
const loadError = ref("");
const selectedKey = ref<string | null>(null);
const activeTab = ref("catalog");
const catalogRefreshing = ref(false);
const catalogRefreshError = ref("");
const refreshAccountId = ref<string | null>(null);
const overridesLoading = ref(false);
const matrixError = ref("");
const probeError = ref("");
const probingModels = ref<Set<string>>(new Set());
const actionLive = ref("");
let activatedOnce = false;

const scopes = computed(() => (
  contracts.value
    ? flattenProviderScopes(contracts.value, catalog.value)
    : []
));
const activeSelection = computed(() => {
  const query = selectedKey.value?.split(":") ?? [];
  const scopeKind = query[0] ?? null;
  const scopeId = query.length > 1 ? query.slice(1).join(":") : null;
  return selectProviderScope(scopes.value, scopeKind, scopeId);
});
const activeScope = computed(() => activeSelection.value.scope);
const initialLoading = computed(() => loading.value && !contracts.value);
const actionLocked = computed(() => (
  overridesLoading.value || catalogRefreshing.value || probingModels.value.size > 0
));
const scopeMenuOptions = computed<MenuOption[]>(() => scopes.value.map((scope) => ({
  key: scope.key,
  label: scope.label,
})));
const scopeSelectOptions = computed<SelectOption[]>(() => scopes.value.map((scope) => ({
  value: scope.key,
  label: scope.label,
})));
const refreshAccountOptions = computed<SelectOption[]>(() => (
  activeScope.value
    ? scopeAccounts(activeScope.value)
      .filter((account) => (
        activeScope.value?.provider_id !== "command-code"
        || account.verification_status === "verified"
      ))
      .map((account) => ({ value: account.id, label: account.name }))
    : []
));
const safeSourceUrl = computed(() => {
  const url = activeScope.value?.catalog.source_url ?? "";
  return isSafeSourceUrl(url) ? url : "";
});

function catalogSourceLabel(source: string): string {
  if (source === CATALOG_SOURCE_STATIC) return t("静态目录");
  if (source === CATALOG_SOURCE_OFFICIAL_ZEN) return t("官方 Zen 目录");
  if (source === CATALOG_SOURCE_CUSTOM_DISCOVERY) return t("自定义发现");
  if (source === CATALOG_SOURCE_DECLARED) return t("账号声明");
  if (source === CATALOG_SOURCE_OPENCODE_MODELS) return `OpenCode · ${t("官方来源")}`;
  if (source === CATALOG_SOURCE_COMMAND_CODE_MODELS) return `Command Code · ${t("官方来源")}`;
  return source;
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(locale.value, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function writeScopeToUrl(scopeKind: string, scopeId: string) {
  const url = applyAppViewSearchParams(new URL(window.location.href), "providers", {
    scope_kind: scopeKind,
    scope_id: scopeId,
  });
  window.history.replaceState(null, "", url);
}

function applyScopeFromQuery(fellBackNotice = false) {
  const query = readProviderScopeQuery(window.location.search);
  const selected = selectProviderScope(scopes.value, query.scope_kind, query.scope_id);
  if (!selected.scope) {
    selectedKey.value = null;
    return;
  }
  selectedKey.value = selected.scope.key;
  writeScopeToUrl(selected.scope.scope_kind, selected.scope.scope_id);
  if (fellBackNotice && selected.fellBack) {
    actionLive.value = t("已选择过期范围，已回到第一个供应商");
  }
}

function selectScopeKey(key: string | number) {
  const value = String(key);
  const scope = scopes.value.find((item) => item.key === value);
  if (!scope) return;
  selectedKey.value = value;
  writeScopeToUrl(scope.scope_kind, scope.scope_id);
}

function resetScopeActions() {
  catalogRefreshError.value = "";
  matrixError.value = "";
  probeError.value = "";
  const accounts = activeScope.value ? scopeAccounts(activeScope.value) : [];
  refreshAccountId.value = accounts[0]?.id ?? null;
}

async function loadContracts(options: { retain?: boolean } = {}): Promise<{ ok: boolean; error: string }> {
  if (loading.value) {
    return { ok: false, error: loadError.value };
  }
  loading.value = true;
  if (!options.retain) loadError.value = "";
  try {
    const [contractsResult, catalogResult] = await Promise.allSettled([
      providersStore.loadContracts(),
      providersStore.loadCatalog(),
    ]);
    if (catalogResult.status === "fulfilled") catalog.value = catalogResult.value;
    if (contractsResult.status === "fulfilled") {
      contracts.value = normalizeProviderContractsResponse(contractsResult.value);
      loadError.value = "";
      applyScopeFromQuery(true);
      return { ok: true, error: "" };
    }
    const error = dashboardErrorDetail(contractsResult.reason);
    loadError.value = error;
    return { ok: false, error };
  } finally {
    loading.value = false;
  }
}

async function refreshCatalog() {
  const scope = activeScope.value;
  if (!scope || !catalogRefreshSupported(scope) || catalogRefreshing.value || !refreshAccountId.value) return;
  catalogRefreshing.value = true;
  catalogRefreshError.value = "";
  try {
    const refreshed = await providerApi.refreshProviderModels(refreshAccountId.value);
    const loaded = await loadContracts({ retain: true });
    if (!loaded.ok || loadError.value) {
      catalogRefreshError.value = loaded.error || loadError.value;
      message.error(t("刷新模型目录失败: {error}", { error: catalogRefreshError.value }));
      return;
    }
    actionLive.value = t("已刷新模型目录");
    message.success(t("已刷新模型目录"));
    if (isCustomCatalogRefreshResponse(refreshed) && refreshed.truncated) {
      message.warning(t("发现结果已截断"));
    }
  } catch (error) {
    catalogRefreshError.value = dashboardErrorDetail(error);
    message.error(t("刷新模型目录失败: {error}", { error: catalogRefreshError.value }));
  } finally {
    catalogRefreshing.value = false;
  }
}

async function updateOverrides(payload: {
  scopeKind: "provider" | "custom_endpoint";
  scopeId: string;
  overrides: ModelProtocolOverrideUpdate[];
}) {
  if (overridesLoading.value) return;
  overridesLoading.value = true;
  matrixError.value = "";
  try {
    const response = await providersStore.putModelProtocolOverrides(
      payload.scopeKind,
      payload.scopeId,
      payload.overrides,
    );
    contracts.value = normalizeProviderContractsResponse(response);
    actionLive.value = t("协议覆盖已保存");
    message.success(t("协议覆盖已保存"));
  } catch (error) {
    if (error instanceof DashboardRequestError && error.status === 409) {
      await loadContracts({ retain: true });
      actionLive.value = t("供应商设置已在其他位置更新，已重新加载，请重试");
      message.warning(t("供应商设置已在其他位置更新，已重新加载，请重试"));
    } else {
      matrixError.value = dashboardErrorDetail(error);
      message.error(t("保存协议覆盖失败: {error}", { error: matrixError.value }));
    }
  } finally {
    overridesLoading.value = false;
  }
}

async function runModelProbe(payload: { modelId: string; accountId: string }) {
  const scope = activeScope.value;
  if (!scope || probingModels.value.has(payload.modelId)) return;
  probingModels.value = new Set(probingModels.value).add(payload.modelId);
  probeError.value = "";
  try {
    const response = await providerApi.runProtocolProbes(payload.accountId, {
      model_id: payload.modelId,
      protocols: [...PROVIDER_PROTOCOLS],
    });
    if (response.contract && contracts.value) {
      contracts.value = applyModelContractToResponse(contracts.value, {
        scope_kind: scope.scope_kind,
        scope_id: scope.scope_id,
      }, response.contract);
    }
    await loadContracts({ retain: true });
    actionLive.value = t("探测完成");
    message.success(t("探测完成"));
  } catch (error) {
    probeError.value = dashboardErrorDetail(error);
    message.error(t("探测失败: {error}", { error: probeError.value }));
  } finally {
    const next = new Set(probingModels.value);
    next.delete(payload.modelId);
    probingModels.value = next;
  }
}

function onPopState() {
  applyScopeFromQuery();
}

watch(activeScope, (scope, previous) => {
  if (scope?.key !== previous?.key) resetScopeActions();
});

onMounted(() => {
  window.addEventListener("popstate", onPopState);
  void loadContracts();
});
onActivated(() => {
  if (activatedOnce) void loadContracts({ retain: true });
  else activatedOnce = true;
});
onUnmounted(() => {
  window.removeEventListener("popstate", onPopState);
});
</script>

<style scoped>
.providers-page {
  min-width: 0;
  max-width: 1440px;
  margin: 0 auto;
  overflow-x: hidden;
}
.providers-header h1 {
  margin: 0 0 16px;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-xl)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.providers-state {
  min-height: 160px;
  display: grid;
  place-items: center;
}
.providers-layout {
  display: grid;
  grid-template-columns: 208px minmax(0, 1fr);
  gap: 16px;
  min-width: 0;
}
.providers-rail {
  min-width: 0;
  padding: 8px 0;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: var(--ocg-surface);
}
.providers-mobile-nav {
  display: none;
  min-width: 0;
  margin-bottom: 12px;
}
.providers-main {
  display: grid;
  gap: 16px;
  min-width: 0;
}
.providers-tabs :deep(.n-tabs-nav) {
  margin-bottom: 12px;
}
.providers-section {
  min-width: 0;
  padding: 16px;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.providers-section h2 {
  margin: 0 0 10px;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-lg)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.providers-overview {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1px;
  margin: 0 0 12px;
  overflow: hidden;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: var(--ocg-border);
}
.providers-overview--compact {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}
.providers-overview > div {
  min-width: 0;
  padding: 10px 12px;
  background: var(--ocg-canvas);
}
.providers-overview dt {
  margin-bottom: 4px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
}
.providers-overview dd {
  overflow-wrap: anywhere;
  margin: 0;
  color: var(--ocg-ink);
  font-weight: 600;
}
.providers-empty {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}

@media (max-width: 720px) {
  .providers-layout {
    grid-template-columns: minmax(0, 1fr);
  }
  .providers-rail {
    display: none;
  }
  .providers-mobile-nav {
    display: block;
  }
  .providers-overview,
  .providers-overview--compact {
    grid-template-columns: minmax(0, 1fr);
  }
}

@media (max-width: 390px) {
  .providers-page,
  .providers-layout,
  .providers-main,
  .providers-section {
    min-width: 0;
  }
  .providers-overview {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
