<template>
  <div class="dashboard">
    <section class="connection-hero" aria-labelledby="connection-title">
      <div class="connection-content">
        <div class="connection-head">
          <h2 id="connection-title">⚡ {{ t("接入中心") }}</h2>
          <span
            class="ready-mark"
            :class="{
              'not-ready': summaryLoaded && !summary.gateway_running,
              pending: !summaryLoaded,
            }"
            role="status"
          >
            <span aria-hidden="true" />
            {{ !summaryLoaded ? t("加载中…") : summary.gateway_running ? t("就绪") : t("服务未就绪") }}
          </span>
        </div>

        <div class="connection-rows">
          <div class="connection-row">
            <n-icon size="18" aria-hidden="true"><ApiOutlined /></n-icon>
            <div class="connection-value">
              <span class="sr-only">{{ t("API 地址") }}</span>
              <code>{{ serviceApiUrl }}</code>
            </div>
            <n-tooltip trigger="hover" :delay="200">
              <template #trigger>
                <n-button
                  circle
                  quaternary
                  size="small"
                  :aria-label="t('复制 API Base URL')"
                  @click="copyConnection('api', serviceApiUrl, t('API 地址'))"
                >
                  <template #icon>
                    <n-icon :component="copiedTarget === 'api' ? CheckOutlined : CopyOutlined" />
                  </template>
                </n-button>
              </template>
              {{ t("复制 API Base URL") }}
            </n-tooltip>
          </div>

          <div class="connection-row">
            <n-icon size="18" aria-hidden="true"><KeyOutlined /></n-icon>
            <n-popover
              v-if="enabledGatewayKeys.length > 1"
              trigger="click"
              placement="bottom-start"
              :show="keyMenuOpen"
              @update:show="keyMenuOpen = $event"
            >
              <template #trigger>
                <button
                  type="button"
                  class="key-switcher-trigger"
                  :disabled="refreshingKey || loading"
                  :aria-label="t('选择 Key')"
                  :aria-expanded="keyMenuOpen"
                  aria-haspopup="menu"
                  @keydown.esc="keyMenuOpen = false"
                >
                  <span class="key-switcher-name">{{ selectedKey?.name }}</span>
                  <span v-if="selectedKey?.id === PRIMARY_KEY_ID" class="key-switcher-badge">{{ t("主 Key") }}</span>
                  <n-icon size="12" aria-hidden="true"><DownOutlined /></n-icon>
                </button>
              </template>
              <div class="key-switcher-menu" @keydown.esc="keyMenuOpen = false">
                <button
                  v-for="entry in enabledGatewayKeys"
                  :key="entry.id"
                  type="button"
                  class="key-switcher-option"
                  :class="{ selected: entry.id === selectedKey?.id }"
                  @click="selectGatewayKey(entry.id)"
                >
                  <span class="key-switcher-option-main">
                    <span class="key-switcher-option-name">{{ entry.name }}</span>
                    <span v-if="entry.id === PRIMARY_KEY_ID" class="key-switcher-badge">{{ t("主 Key") }}</span>
                  </span>
                  <code class="key-switcher-option-value">{{ maskConnectionKey(entry.value) }}</code>
                  <n-icon
                    v-if="entry.id === selectedKey?.id"
                    class="key-switcher-check"
                    size="14"
                    aria-hidden="true"
                  ><CheckOutlined /></n-icon>
                </button>
              </div>
            </n-popover>
            <div class="connection-value">
              <span class="sr-only">{{ t("Key") }}</span>
              <code>{{ maskedKey }}</code>
            </div>
            <div class="row-actions">
              <n-popconfirm
                :positive-text="t('生成新 Key')"
                :negative-text="t('取消')"
                @positive-click="regenerateKey"
              >
                <template #trigger>
                  <n-tooltip trigger="hover" :delay="200">
                    <template #trigger>
                      <n-button
                        circle
                        quaternary
                        size="small"
                        :aria-label="t('刷新 Key')"
                        :loading="refreshingKey"
                        :disabled="refreshingKey || loading || !selectedKey"
                      >
                        <template #icon><n-icon :component="ReloadOutlined" /></template>
                      </n-button>
                    </template>
                    {{ t("刷新 Key") }}
                  </n-tooltip>
                </template>
                {{ t("仅当前 Key 的旧值立即失效，其他 Key 不受影响。确定生成新值？") }}
              </n-popconfirm>
              <n-tooltip trigger="hover" :delay="200">
                <template #trigger>
                  <n-button
                    circle
                    quaternary
                    size="small"
                    :aria-label="t('复制 Key')"
                    :disabled="refreshingKey || !selectedKey"
                    @click="copyConnection('key', selectedKey?.value ?? '', t('Key'))"
                  >
                    <template #icon>
                      <n-icon :component="copiedTarget === 'key' ? CheckOutlined : CopyOutlined" />
                    </template>
                  </n-button>
                </template>
                {{ t("复制 Key") }}
              </n-tooltip>
              <n-tooltip trigger="hover" :delay="200">
                <template #trigger>
                  <n-button
                    circle
                    quaternary
                    size="small"
                    :aria-label="t('管理接入 Key')"
                    @click="goToKeys"
                  >
                    <template #icon><n-icon :component="UnorderedListOutlined" /></template>
                  </n-button>
                </template>
                {{ t("管理接入 Key") }}
              </n-tooltip>
            </div>
          </div>

          <div class="connection-row">
            <n-icon size="18" aria-hidden="true"><CloudServerOutlined /></n-icon>
            <div class="connection-value">
              <span class="sr-only">{{ t("上游地址") }}</span>
              <code>{{ serviceConfig.upstream_base_url || t("未设置") }}</code>
            </div>
            <n-tooltip trigger="hover" :delay="200">
              <template #trigger>
                <n-button
                  circle
                  quaternary
                  size="small"
                  :aria-label="t('复制上游地址')"
                  :disabled="!serviceConfig.upstream_base_url"
                  @click="copyConnection('upstream', serviceConfig.upstream_base_url, t('上游地址'))"
                >
                  <template #icon>
                    <n-icon :component="copiedTarget === 'upstream' ? CheckOutlined : CopyOutlined" />
                  </template>
                </n-button>
              </template>
              {{ t("复制上游地址") }}
            </n-tooltip>
          </div>
        </div>
        <p v-if="connectionUrls.insecureHttp" class="connection-warning" role="status">
          {{ t("非本机 HTTP 会明文传输 Key 与请求内容，请仅在可信网络中使用。") }}
        </p>
      </div>
      <img :src="characterImage" alt="" class="hero-character" aria-hidden="true" />
    </section>

    <n-alert v-if="dashboardError" type="error" :title="t('仪表盘数据加载失败')">
      <n-button size="small" secondary :loading="loading" :disabled="refreshingKey" @click="loadDashboard">
        {{ t("重试") }}
      </n-button>
    </n-alert>

    <section class="kpi-row" :aria-label="t('用量摘要')" :aria-busy="!summaryLoaded">
      <article class="kpi-card">
        <span class="kpi-badge success"><n-icon aria-hidden="true"><KeyOutlined /></n-icon></span>
        <div><strong>{{ summaryLoaded ? formatNumber(summary.available_accounts) : "—" }}<small v-if="summaryLoaded">/{{ formatNumber(summary.total_accounts) }}</small></strong><span>{{ t("可用账号") }}</span></div>
      </article>
      <article class="kpi-card">
        <span class="kpi-badge info"><n-icon aria-hidden="true"><CalendarOutlined /></n-icon></span>
        <div><strong>{{ summaryLoaded ? formatCost(summary.today_cost) : "—" }}</strong><span>{{ t("今日") }}</span></div>
      </article>
      <article class="kpi-card">
        <span class="kpi-badge warning"><n-icon aria-hidden="true"><ClockCircleOutlined /></n-icon></span>
        <div><strong>{{ summaryLoaded ? formatCost(summary.week_cost) : "—" }}</strong><span>{{ t("本周") }}</span></div>
      </article>
      <article class="kpi-card">
        <span class="kpi-badge primary"><n-icon aria-hidden="true"><WalletOutlined /></n-icon></span>
        <div><strong>{{ summaryLoaded ? formatCost(summary.month_cost) : "—" }}</strong><span>{{ t("本月") }}</span></div>
      </article>
    </section>

    <section class="card provider-card" :aria-label="t('供应商摘要')" :aria-busy="!providerOverviewLoaded">
      <div class="card-head">
        <div>
          <h3 class="card-title">{{ t("供应商摘要") }}</h3>
          <span class="card-desc">{{ t("账号健康与累计成本") }}</span>
        </div>
      </div>
      <div v-if="!providerOverviewLoaded" class="section-state">
        {{ loading ? t("加载中…") : t("仪表盘数据加载失败") }}
      </div>
      <n-empty v-else-if="providerOverviews.length === 0" :description="t('服务商目录暂无数据')" />
      <div v-else class="provider-grid" role="list">
        <article v-for="provider in providerOverviews" :key="provider.key" class="provider-cell" role="listitem">
          <div class="provider-head">
            <strong>{{ provider.label }}</strong>
            <n-tag
              size="small"
              :type="provider.healthy > 0 ? 'success' : provider.provider_id === 'command-code' ? 'warning' : 'default'"
              :bordered="false"
            >
              {{ provider.provider_id === "command-code" ? t("未配置") : t("健康 {healthy}/{total}", { healthy: provider.healthy, total: provider.total }) }}
            </n-tag>
          </div>
          <dl class="provider-metrics">
            <div><dt>{{ t("账号") }}</dt><dd>{{ formatNumber(provider.total) }}</dd></div>
            <div><dt>{{ t("已启用") }}</dt><dd>{{ formatNumber(provider.enabled) }}</dd></div>
            <div><dt>{{ t("累计成本") }}</dt><dd>{{ providerCostText(provider) }}</dd></div>
          </dl>
        </article>
      </div>
    </section>

    <section class="card chart-card">
      <div class="card-head chart-head">
        <div>
          <h3 class="card-title">{{ t("每日消耗") }}</h3>
        </div>
        <div v-if="costsLoaded" class="chart-stats" role="group" :aria-label="t('图表摘要')">
          <span>{{ t("模型：{count}", { count: formatNumber(legendModels.length) }) }}</span>
          <span><b>{{ formatCost(totalChartCost) }}</b> {{ t("{days} 天合计", { days: 30 }) }}</span>
          <span><b>{{ formatCost(totalChartCost / 30) }}</b> {{ t("日均") }}</span>
        </div>
      </div>
      <div v-if="costsLoaded" class="legend" role="list" :aria-label="t('模型图例')">
        <span v-for="model in legendModels" :key="model.model" class="legend-item" role="listitem">
          <span class="legend-dot" :style="{ background: model.color }" aria-hidden="true" />
          {{ model.model }}
        </span>
      </div>
      <n-spin :show="loading && !costsLoaded">
        <div v-if="!costsLoaded" class="section-state">
          {{ loading ? t("加载中…") : t("仪表盘数据加载失败") }}
        </div>
        <n-empty v-else-if="totalChartCost === 0" :description="t('暂无消耗数据')" />
        <StackedBarChart v-else :data="dailyCosts" :days="30" />
      </n-spin>
    </section>

    <section class="card accounts-card">
      <div class="card-head">
        <h3 class="card-title">{{ t("账号概览") }}</h3>
        <span class="card-desc">{{ accountsLoaded ? t("账号数：{count}", { count: formatNumber(accounts.length) }) : t("加载中…") }}</span>
      </div>
      <div v-if="!accountsLoaded" class="section-state">
        {{ loading ? t("加载中…") : t("仪表盘数据加载失败") }}
      </div>
      <n-empty v-else-if="accounts.length === 0" :description="t('暂无账号')">
        <template #extra>
          <n-button size="small" @click="goToAccounts">{{ t("前往账号页添加") }}</n-button>
        </template>
      </n-empty>
      <div v-else class="account-grid">
        <article v-for="account in accounts" :key="account.id" class="account-cell">
          <div class="account-top">
            <strong>{{ account.name }}</strong>
            <span
              class="account-status"
              :class="accountStatusClass(account)"
            >{{ statusLabel(account) }}</span>
          </div>
          <n-tooltip v-if="account.setup_step === 'ready'" trigger="click">
            <template #trigger>
              <n-button
                text
                class="account-expiry"
                :aria-label="[
                  accountExpiryLabel(account),
                  t('购买于 {date}', { date: account.purchase_date }),
                  t('到期于 {date}', { date: account.expires_on }),
                ].join('; ')"
              >
                <n-tag
                  size="small"
                  :type="accountExpiryType(account)"
                >{{ accountExpiryLabel(account) }}</n-tag>
              </n-button>
            </template>
            <div>{{ t("购买于 {date}", { date: account.purchase_date }) }}</div>
            <div>{{ t("到期于 {date}", { date: account.expires_on }) }}</div>
          </n-tooltip>
          <div v-if="account.setup_step !== 'ready'" class="account-usage-empty account-usage-empty--pending">
            {{ t("注册进度已保存，请前往账号页继续") }}
          </div>
          <div v-else-if="isZenAccount(account)" class="account-usage-empty">
            {{ t("免费 · 出口 IP 共享") }}
          </div>
          <div v-else-if="isUnconfiguredAccount(account)" class="account-usage-empty account-usage-empty--pending">
            {{ t("供应商尚未配置") }}
          </div>
          <div v-else-if="usageMap[account.id]" class="account-usage mono">
            <div v-for="row in getUsageRows(account.id)" :key="row.label" class="account-usage-row">
              <span>{{ row.label }}</span>
              <strong>{{ row.value }}</strong>
            </div>
          </div>
          <div v-else-if="loading" class="account-usage-empty">{{ t("加载中…") }}</div>
          <div v-else-if="usageFailedAccountIds.has(account.id)" class="account-usage-empty">
            {{ t("读取失败") }}
          </div>
          <div v-else class="account-usage-empty">{{ t("暂无用量") }}</div>
        </article>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, onActivated, onDeactivated, onMounted, onUnmounted, ref, watch } from "vue";
import { NAlert, NButton, NEmpty, NIcon, NPopconfirm, NPopover, NSpin, NTag, NTooltip, useMessage } from "naive-ui";
import {
  ApiOutlined,
  CalendarOutlined,
  CheckOutlined,
  ClockCircleOutlined,
  CloudServerOutlined,
  CopyOutlined,
  DownOutlined,
  KeyOutlined,
  ReloadOutlined,
  UnorderedListOutlined,
  WalletOutlined,
} from "@vicons/antd";
import StackedBarChart from "../components/StackedBarChart.vue";
import { PRIMARY_KEY_ID, tauriApi } from "../api/tauri";
import { providerApi } from "../api/providers.ts";
import type { ProviderCatalogEntry } from "../api/providers.ts";
import type {
  Account,
  ConnectionInfo,
  DailyModelCost,
  DashboardSummary,
  UsageWindow,
} from "../api/tauri";
import { CHART_PALETTE } from "../theme";
import { t } from "../i18n/index.ts";
import { formatCost, formatNumber, useClipboard } from "../utils/format.ts";
import { userFacingError } from "../utils/errors.ts";
import { mapWithConcurrency } from "../utils/async.ts";
import { daysUntilDate, expiryTagType } from "./account-lifecycle";
import { maskConnectionKey, resolveConnectionUrls } from "./dashboard-connection";
import {
  buildProviderOverviews,
  providerPairKey,
} from "./dashboard-providers.ts";
import type { ProviderOverview } from "./dashboard-providers.ts";

type ConnectionTarget = "api" | "key" | "upstream";

/** One selectable credential in the connection switcher. */
interface SwitcherKey {
  id: string;
  name: string;
  value: string;
}

const emit = defineEmits<{
  navigate: [view: string];
}>();

const message = useMessage();
const { copiedTarget, copy, cleanup } = useClipboard();
const characterImage = new URL("../../assets/opencode-mascot.png", import.meta.url).href;
const accounts = ref<Account[]>([]);
const usageMap = ref<Record<string, UsageWindow>>({});
const dailyCosts = ref<DailyModelCost[]>([]);
const loading = ref(true);
const accountsLoaded = ref(false);
const summaryLoaded = ref(false);
const costsLoaded = ref(false);
const dashboardError = ref(false);
const usageFailedAccountIds = ref(new Set<string>());
const providerCatalog = ref<ProviderCatalogEntry[]>([]);
const providerCosts = ref<Record<string, number | null>>({});
const providerOverviewLoaded = ref(false);
const refreshingKey = ref(false);
const lifecycleNow = ref(Date.now());

// ponytail: keep this pre-load fallback in sync with ConnectionInfo.
const serviceConfig = ref<ConnectionInfo>({
  gateway_port: 9042,
  client_root_url: "",
  upstream_base_url: "",
  primary_key: "",
  sub_keys: [],
  revision: 0,
});
const selectedKeyId = ref("");
const summary = ref<DashboardSummary>({
  total_accounts: 0,
  available_accounts: 0,
  today_cost: 0,
  week_cost: 0,
  month_cost: 0,
  gateway_running: false,
});

const legendModels = computed(() => {
  const totals = new Map<string, number>();
  for (const row of dailyCosts.value) totals.set(row.model, (totals.get(row.model) ?? 0) + row.cost);
  return [...totals.keys()]
    .sort((a, b) => totals.get(b)! - totals.get(a)!)
    .map((model, index) => ({ model, color: CHART_PALETTE[index % CHART_PALETTE.length] }));
});
const totalChartCost = computed(() => dailyCosts.value.reduce((sum, row) => sum + row.cost, 0));
const providerOverviews = computed(() => buildProviderOverviews(
  accounts.value,
  providerCatalog.value,
  providerCosts.value,
  lifecycleNow.value,
));
const maskedKey = computed(() => maskConnectionKey(selectedKey.value?.value ?? ""));
// The primary key is pinned first; only enabled sub keys join the switcher.
const enabledGatewayKeys = computed<SwitcherKey[]>(() => [
  { id: PRIMARY_KEY_ID, name: t("主 Key"), value: serviceConfig.value.primary_key },
  ...serviceConfig.value.sub_keys
    .filter((entry) => entry.enabled)
    .map((entry) => ({ id: entry.id, name: entry.name, value: entry.value })),
]);
const keyMenuOpen = ref(false);
// Default to the primary key and keep the selection valid across connection
// reloads and key lifecycle changes.
const selectedKey = computed<SwitcherKey | null>(() => {
  const keys = enabledGatewayKeys.value;
  if (keys.length === 0 || !keys[0].value) return null;
  return keys.find((entry) => entry.id === selectedKeyId.value) ?? keys[0];
});
// Keep the switcher explicitly on the primary key until the user picks
// another one, and re-validate whenever the enabled list changes.
watch(enabledGatewayKeys, (keys) => {
  if (keys.length > 0 && !keys.some((entry) => entry.id === selectedKeyId.value)) {
    selectedKeyId.value = keys[0].id;
  }
});
function selectGatewayKey(id: string): void {
  selectedKeyId.value = id;
  keyMenuOpen.value = false;
}
const connectionUrls = computed(() => {
  try {
    return resolveConnectionUrls(
      serviceConfig.value.client_root_url,
      window.location.origin,
      serviceConfig.value.gateway_port,
      import.meta.env.DEV,
    );
  } catch {
    return resolveConnectionUrls(
      "",
      window.location.origin,
      serviceConfig.value.gateway_port,
      import.meta.env.DEV,
    );
  }
});
const serviceApiUrl = computed(() => connectionUrls.value.apiBaseUrl);

function isCoolingDown(account: Account): boolean {
  if (!account.cooldown_until) return false;
  const until = Date.parse(account.cooldown_until);
  return Number.isFinite(until) && until > Date.now();
}

function statusLabel(account: Account): string {
  if (account.setup_step !== "ready") return t("注册中");
  if (isUnconfiguredAccount(account)) return t("未配置");
  if (account.auth_error) {
    return account.enabled
      ? t("认证失效（401 熔断）")
      : `${t("已禁用")} · ${t("认证失效（401 熔断）")}`;
  }
  if (!account.enabled) return t("已禁用");
  return isCoolingDown(account) ? t("冷却中") : t("可用");
}

function accountStatusClass(account: Account): string {
  if (account.setup_step !== "ready") return "pending";
  if (isUnconfiguredAccount(account)) return "pending";
  if (account.auth_error) return "auth-error";
  if (!account.enabled) return "disabled";
  return isCoolingDown(account) ? "cooling" : "active";
}

function isZenAccount(account: Account): boolean {
  return account.provider_id === "opencode-zen-free" && account.offering_id === "anonymous-free";
}

function isUnconfiguredAccount(account: Account): boolean {
  return account.provider_id === "command-code" && account.offering_id === "goat";
}

function providerCostText(provider: ProviderOverview): string {
  if (provider.cost_state === "free") return t("免费");
  if (provider.cost_state === "unknown" || provider.cost === null) return t("未知");
  return formatCost(provider.cost);
}

function accountExpiryDays(account: Account): number {
  return daysUntilDate(account.expires_on, lifecycleNow.value);
}

function accountExpiryLabel(account: Account): string {
  const days = accountExpiryDays(account);
  if (!Number.isFinite(days)) return t("未设置");
  if (days === 1) return t("剩 1 天");
  if (days > 0) return t("剩 {days} 天", { days });
  if (days === 0) return t("今天到期");
  if (days === -1) return t("已到期 1 天");
  return t("已到期 {days} 天", { days: Math.abs(days) });
}

function accountExpiryType(account: Account): "success" | "warning" | "error" {
  return expiryTagType(accountExpiryDays(account));
}

function getUsageRows(accountId: string): Array<{ label: string; value: string }> {
  const usage = usageMap.value[accountId];
  if (!usage) return [];
  return [
    { label: t("5小时"), value: formatCost(usage.window_5h) },
    { label: t("本周"), value: formatCost(usage.window_week) },
    { label: t("本月"), value: formatCost(usage.window_month) },
  ];
}

async function copyConnection(target: ConnectionTarget, value: string, label: string) {
  try {
    await copy(target, value, label);
    message.success(t("已复制 {label}", { label }));
  } catch (e) {
    message.error(e instanceof Error ? e.message : t("复制失败"));
  }
}

async function regenerateKey() {
  const target = selectedKey.value;
  if (refreshingKey.value || dashboardRequestActive || !target) return;
  const previousValue = target.value;
  const isPrimary = target.id === PRIMARY_KEY_ID;
  refreshingKey.value = true;
  let mutationFailed = false;
  let mutationError: unknown = null;
  let newValue = "";
  try {
    try {
      if (isPrimary) {
        // Primary rotation uses the legacy endpoint; sub keys rotate in place.
        const result = await tauriApi.regenerateGatewayKey();
        newValue = result.key;
      } else {
        const result = await tauriApi.regenerateGatewayKeyEntry(
          target.id,
          serviceConfig.value.revision,
        );
        newValue = result.key;
      }
    } catch (error) {
      mutationFailed = true;
      mutationError = error;
    }

    try {
      const latest = await tauriApi.getConnection();
      serviceConfig.value = latest;
      const refreshed = latest.primary_key === newValue
        ? newValue
        : latest.sub_keys.find((entry) => entry.id === target.id)?.value;
      if (!mutationFailed || refreshed !== previousValue) {
        selectedKeyId.value = target.id;
        message.success(t("Key 已刷新"));
      } else {
        message.error(t("刷新 Key 失败: {error}", {
          error: userFacingError(mutationError, t("无法连接到本地服务，请确认程序正在运行后重试")),
        }));
      }
    } catch {
      if (newValue) {
        // Connection refresh failed; apply the regenerated value locally so
        // the panel never shows or copies the now-invalid old key.
        if (isPrimary) {
          serviceConfig.value.primary_key = newValue;
        } else {
          serviceConfig.value.sub_keys = serviceConfig.value.sub_keys.map((entry) =>
            entry.id === target.id ? { ...entry, value: newValue } : entry,
          );
        }
        selectedKeyId.value = target.id;
        message.success(t("Key 已刷新"));
      } else {
        dashboardError.value = true;
        message.error(t("刷新 Key 失败: {error}", {
          error: userFacingError(mutationError, t("无法连接到本地服务，请确认程序正在运行后重试")),
        }));
      }
    }
  } finally {
    refreshingKey.value = false;
  }
}

function goToAccounts() {
  emit("navigate", "accounts");
}

function goToKeys() {
  emit("navigate", "keys");
}

let dashboardRequestActive = false;

async function loadDashboard() {
  if (dashboardRequestActive || refreshingKey.value) return;
  dashboardRequestActive = true;
  loading.value = true;
  dashboardError.value = false;
  accountsLoaded.value = false;
  summaryLoaded.value = false;
  costsLoaded.value = false;
  providerOverviewLoaded.value = false;
  usageFailedAccountIds.value = new Set();
  accounts.value = [];
  usageMap.value = {};
  dailyCosts.value = [];
  providerCatalog.value = [];
  providerCosts.value = {};
  const [loadedAccounts, connection, loadedSummary, costs, catalog] = await Promise.allSettled([
    tauriApi.getAccounts(),
    tauriApi.getConnection(),
    tauriApi.getDashboardSummary(),
    tauriApi.getDailyCostByModel(30),
    providerApi.getProviderCatalog(),
  ]);
  if (loadedAccounts.status === "fulfilled") {
    accounts.value = loadedAccounts.value;
    accountsLoaded.value = true;
    // 限流并发拉取每账号用量，避免账号多时 N 次请求同时打到后端
    const readyAccounts = loadedAccounts.value.filter((account) => (
      account.setup_step === "ready"
      && account.provider_id === "opencode"
      && account.offering_id === "go"
    ));
    const settled = await mapWithConcurrency(
      readyAccounts,
      4,
      async (account) => [account.id, await tauriApi.getAccountUsage(account.id)] as const,
    );
    usageMap.value = Object.fromEntries(
      settled.flatMap((result) => (result.status === "fulfilled" ? [result.value] : [])),
    );
    usageFailedAccountIds.value = new Set(settled.flatMap((result, index) => (
      result.status === "rejected" && readyAccounts[index]
        ? [readyAccounts[index].id]
        : []
    )));
  }
  if (connection.status === "fulfilled") serviceConfig.value = connection.value;
  if (loadedSummary.status === "fulfilled") {
    summary.value = loadedSummary.value;
    summaryLoaded.value = true;
  }
  if (costs.status === "fulfilled") {
    dailyCosts.value = costs.value;
    costsLoaded.value = true;
  }
  let providerCostFailed = false;
  if (catalog.status === "fulfilled") {
    providerCatalog.value = catalog.value;
    const go = catalog.value.find((entry) => entry.provider_id === "opencode" && entry.offering_id === "go");
    if (go) {
      try {
        const page = await tauriApi.getForwardLogs({
          provider_id: go.provider_id,
          offering_id: go.offering_id,
          limit: 1,
          offset: 0,
        });
        providerCosts.value = {
          [providerPairKey(go.provider_id, go.offering_id)]: page.summary.cost,
        };
      } catch {
        providerCostFailed = true;
      }
    }
    providerOverviewLoaded.value = true;
  }
  dashboardError.value = [loadedAccounts, connection, loadedSummary, costs, catalog].some((result) => result.status === "rejected")
    || providerCostFailed
    || usageFailedAccountIds.value.size > 0;
  if (dashboardError.value) {
    message.error(t("部分仪表盘数据加载失败"));
  }
  loading.value = false;
  dashboardRequestActive = false;
}

function refreshWhenVisible() {
  if (document.visibilityState === "visible") void loadDashboard();
}

let lifecycleClock: number | undefined;
let activatedOnce = false;

function startLifecycleClock() {
  if (lifecycleClock === undefined) {
    lifecycleClock = window.setInterval(() => {
      lifecycleNow.value = Date.now();
    }, 60_000);
  }
}

function stopLifecycleClock() {
  if (lifecycleClock !== undefined) {
    window.clearInterval(lifecycleClock);
    lifecycleClock = undefined;
  }
}

// The view stays cached by App.vue's KeepAlive; pause its clock and
// visibility-driven refreshes while another tab is active. Bind/unbind on
// activate/deactivate so leave/return cycles re-register the listener.
function bindVisibilityRefresh() {
  document.addEventListener("visibilitychange", refreshWhenVisible);
}

function unbindVisibilityRefresh() {
  document.removeEventListener("visibilitychange", refreshWhenVisible);
}

onMounted(() => {
  bindVisibilityRefresh();
  void loadDashboard();
});
onActivated(() => {
  bindVisibilityRefresh();
  startLifecycleClock();
  if (activatedOnce) void loadDashboard();
  else activatedOnce = true;
});
onDeactivated(() => {
  stopLifecycleClock();
  unbindVisibilityRefresh();
});
onUnmounted(() => {
  cleanup();
  stopLifecycleClock();
  unbindVisibilityRefresh();
});
</script>

<style scoped>
.dashboard {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 1480px;
  margin: 0 auto;
}

.connection-hero {
  position: relative;
  min-height: 262px;
  overflow: hidden;
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.connection-hero::before {
  content: "";
  position: absolute;
  inset: 0 0 0 54%;
  opacity: 0.42;
  background-image:
    linear-gradient(var(--ocg-border) 1px, transparent 1px),
    linear-gradient(90deg, var(--ocg-border) 1px, transparent 1px);
  background-size: 24px 24px;
  mask-image: linear-gradient(90deg, transparent, #000 35%);
}
.connection-hero::after {
  content: "";
  position: absolute;
  z-index: 0;
  right: -12px;
  bottom: -28px;
  width: 390px;
  height: 300px;
  background: radial-gradient(ellipse at center, var(--ocg-mascot-halo, transparent), transparent 70%);
  pointer-events: none;
}
.connection-content {
  position: relative;
  z-index: 2;
  width: min(760px, calc(100% - 300px));
  padding: 24px;
}
.connection-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 14px;
}
.connection-head h2 {
  margin: 0;
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-xl)/1.2 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.ready-mark {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--ocg-success);
  font: 700 var(--ocg-font-sm)/1 "Cascadia Mono", Consolas, monospace;
  letter-spacing: 0.08em;
}
.ready-mark > span {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--ocg-success);
  box-shadow: 0 0 0 4px var(--ocg-success-soft);
}
.ready-mark.not-ready {
  color: var(--ocg-error);
}
.ready-mark.not-ready > span {
  background: var(--ocg-error);
  box-shadow: 0 0 0 4px color-mix(in srgb, var(--ocg-error) 16%, transparent);
}
.ready-mark.pending {
  color: var(--ocg-subtle);
}
.ready-mark.pending > span {
  background: var(--ocg-subtle);
  box-shadow: 0 0 0 4px color-mix(in srgb, var(--ocg-subtle) 16%, transparent);
}
.connection-rows {
  display: grid;
  gap: 8px;
}
.connection-row {
  display: grid;
  grid-template-columns: 28px minmax(0, 1fr) auto;
  align-items: center;
  min-height: 44px;
  padding: 6px 8px 6px 12px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: color-mix(in srgb, var(--ocg-canvas) 72%, var(--ocg-surface));
  color: var(--ocg-primary);
}
.connection-row:has(.key-switcher-trigger) {
  grid-template-columns: 28px auto minmax(0, 1fr) auto;
}
.key-switcher-trigger {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 200px;
  padding: 3px 8px;
  border: 1px solid var(--ocg-border);
  border-radius: 6px;
  background: var(--ocg-surface);
  color: var(--ocg-ink);
  font-size: var(--ocg-font-sm);
  font-weight: 600;
  line-height: 1.4;
  cursor: pointer;
}
.key-switcher-trigger:hover:not(:disabled) {
  border-color: var(--ocg-primary);
}
.key-switcher-trigger:focus-visible {
  outline: 2px solid var(--ocg-primary);
  outline-offset: 2px;
}
.key-switcher-trigger:disabled {
  cursor: default;
  opacity: 0.6;
}
.key-switcher-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.key-switcher-badge {
  flex: none;
  padding: 0 6px;
  border: 1px solid var(--ocg-border);
  border-radius: 999px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
  font-weight: 400;
}
.key-switcher-menu {
  display: grid;
  gap: 4px;
  min-width: 260px;
}
.key-switcher-option {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  column-gap: 12px;
  padding: 6px 10px;
  border: none;
  border-radius: 6px;
  background: none;
  color: var(--ocg-ink);
  cursor: pointer;
  text-align: left;
}
.key-switcher-option:hover,
.key-switcher-option.selected {
  background: var(--ocg-primary-soft);
}
.key-switcher-option:focus-visible {
  outline: 2px solid var(--ocg-primary);
  outline-offset: -2px;
}
.key-switcher-option-main {
  display: flex;
  align-items: center;
  gap: 6px;
  grid-column: 1;
  min-width: 0;
}
.key-switcher-option-name {
  overflow: hidden;
  font-size: var(--ocg-font-sm);
  font-weight: 600;
  line-height: 1.4;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.key-switcher-option-value {
  grid-column: 1;
  color: var(--ocg-muted);
  font: var(--ocg-font-xs)/1.4 "Cascadia Mono", Consolas, monospace;
}
.key-switcher-check {
  grid-column: 2;
  grid-row: 1 / span 2;
  color: var(--ocg-primary);
}
.connection-value {
  min-width: 0;
  color: var(--ocg-ink);
}
.connection-value code {
  display: block;
  overflow: hidden;
  font: var(--ocg-font-md)/1.4 "Cascadia Mono", Consolas, monospace;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-actions {
  display: flex;
  align-items: center;
  gap: 2px;
}
.connection-warning {
  margin: 10px 2px 0;
  color: var(--ocg-warning);
  font-size: var(--ocg-font-sm);
  line-height: 1.5;
}
.hero-character {
  position: absolute;
  z-index: 1;
  top: 4px;
  right: 28px;
  height: 380px;
  max-width: 34%;
  object-fit: contain;
  filter:
    drop-shadow(0 0 1px var(--ocg-mascot-rim, transparent))
    drop-shadow(0 18px 20px rgba(31, 27, 56, 0.14));
  pointer-events: none;
  user-select: none;
}

.kpi-row {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 12px;
}
.kpi-card {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
  padding: 14px 16px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.kpi-badge {
  display: inline-flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  border-radius: 10px;
}
.kpi-badge.success { color: var(--ocg-success); background: var(--ocg-success-soft); }
.kpi-badge.info { color: var(--ocg-info); background: color-mix(in srgb, var(--ocg-info) 12%, transparent); }
.kpi-badge.warning { color: var(--ocg-warning); background: var(--ocg-warning-soft); }
.kpi-badge.primary { color: var(--ocg-primary); background: var(--ocg-primary-soft); }
.kpi-card > div {
  display: grid;
  min-width: 0;
}
.kpi-card strong {
  color: var(--ocg-ink);
  font: 700 var(--ocg-font-xl)/1.1 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
  font-variant-numeric: tabular-nums;
  overflow: hidden;
  text-overflow: ellipsis;
}
.kpi-card small {
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-sm);
}
.kpi-card span:last-child {
  margin-top: 3px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-xs);
}

.card {
  border: 1px solid var(--ocg-border);
  border-radius: 14px;
  background: var(--ocg-surface);
  box-shadow: var(--ocg-shadow-sm);
}
.card-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 16px 18px 10px;
}
.card-title {
  margin: 0;
  color: var(--ocg-ink);
  font: 650 var(--ocg-font-lg)/1.3 "Bahnschrift", "Segoe UI Variable Display", sans-serif;
}
.card-desc {
  margin: 3px 0 0;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-sm);
}
.provider-card {
  padding-bottom: 16px;
}
.provider-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
  padding: 4px 18px 0;
}
.provider-cell {
  min-width: 0;
  padding: 12px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: color-mix(in srgb, var(--ocg-canvas) 70%, var(--ocg-surface));
}
.provider-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 10px;
}
.provider-head strong {
  overflow: hidden;
  color: var(--ocg-ink);
  text-overflow: ellipsis;
  white-space: nowrap;
}
.provider-metrics {
  display: grid;
  gap: 5px;
  margin: 0;
}
.provider-metrics > div {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
.provider-metrics dt {
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-sm);
}
.provider-metrics dd {
  margin: 0;
  color: var(--ocg-ink);
  font-family: "Cascadia Mono", Consolas, monospace;
  font-size: var(--ocg-font-sm);
}
.chart-card {
  padding-bottom: 12px;
}
.chart-stats {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px 16px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-sm);
}
.chart-stats b {
  color: var(--ocg-ink);
  font-family: "Cascadia Mono", Consolas, monospace;
  font-weight: 600;
}
.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 7px 14px;
  padding: 0 18px 4px;
}
.legend-item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}
.legend-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
}
.chart-card :deep(.n-spin-content) {
  padding: 4px 12px 0;
  overflow: hidden;
}

.accounts-card {
  padding-bottom: 16px;
}
.account-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
  gap: 10px;
  padding: 4px 18px 0;
}
.account-cell {
  padding: 11px 12px;
  border: 1px solid var(--ocg-border);
  border-radius: 10px;
  background: color-mix(in srgb, var(--ocg-canvas) 70%, var(--ocg-surface));
}
.account-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 5px;
}
.account-top strong {
  overflow: hidden;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-md);
  text-overflow: ellipsis;
  white-space: nowrap;
}
.account-status {
  flex: 0 0 auto;
  font-size: var(--ocg-font-sm);
  font-weight: 650;
}
.account-status.active { color: var(--ocg-success); }
.account-status.cooling { color: var(--ocg-warning); }
.account-status.pending { color: var(--ocg-primary); }
.account-status.auth-error { color: var(--ocg-error); }
.account-status.disabled { color: var(--ocg-subtle); }
.account-expiry {
  margin-bottom: 7px;
}
.account-usage {
  display: grid;
  gap: 2px;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-sm);
  line-height: 1.5;
}
.account-usage-row {
  display: grid;
  grid-template-columns: minmax(3.5em, auto) minmax(0, 1fr);
  gap: 8px;
}
.account-usage-row strong {
  overflow: hidden;
  color: var(--ocg-ink);
  font-weight: 500;
  text-align: right;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.account-usage-empty {
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-sm);
}

.account-usage-empty--pending {
  color: var(--ocg-primary);
}
.section-state {
  min-height: 96px;
  display: grid;
  place-items: center;
  color: var(--ocg-subtle);
  font-size: var(--ocg-font-sm);
}

@media (max-width: 900px) {
  .connection-content {
    width: calc(100% - 210px);
  }
  .hero-character {
    right: 6px;
    max-width: 36%;
  }
  .kpi-row {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .provider-grid {
    grid-template-columns: 1fr;
  }
}

@media (max-width: 640px) {
  .dashboard {
    gap: 12px;
  }
  .connection-hero {
    min-height: 256px;
  }
  .connection-content {
    width: 100%;
    padding: 18px 14px;
  }
  .hero-character {
    z-index: 0;
    top: auto;
    right: -50px;
    bottom: -58px;
    height: 282px;
    max-width: 58%;
    opacity: 0.12;
  }
  .connection-hero::after {
    right: -48px;
    bottom: -34px;
    width: 300px;
    height: 250px;
  }
  .connection-hero::before {
    inset: 0;
    opacity: 0.18;
  }
  .connection-row {
    background: color-mix(in srgb, var(--ocg-surface) 88%, transparent);
  }
  .kpi-card {
    padding: 12px;
  }
  .kpi-card strong {
    font-size: var(--ocg-font-lg);
  }
  .chart-head {
    align-items: flex-start;
  }
  .chart-stats {
    display: grid;
    gap: 2px;
  }
}
</style>
