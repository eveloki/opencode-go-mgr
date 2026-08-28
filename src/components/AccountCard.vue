<template>
  <n-card
    :data-account-id="account.id"
    size="small"
    class="account-card"
    :class="{
      'account-card--cooling': isCooling(account, now),
      'account-card--pending': !accountIsReady(account),
      'account-card--draft': isDraft,
      'account-card--dragging': dragging,
    }"
  >
    <template #header>
      <div class="account-title">
        <n-tooltip trigger="hover">
          <template #trigger>
            <n-button
              circle
              quaternary
              size="small"
              class="account-order-handle"
              :class="{ 'account-order-handle--dragging': dragging }"
              :disabled="orderHandleDisabled"
              :aria-label="t('拖动调整账号 {name} 的优先级', { name: account.name })"
              aria-describedby="account-order-instructions"
              @click.prevent
              @keydown="emit('order-keydown', $event)"
              @pointerdown="emit('order-drag-start', $event)"
            >
              <template #icon><n-icon :component="HolderOutlined" /></template>
            </n-button>
          </template>
          {{ t("拖动调整账号 {name} 的优先级", { name: account.name }) }}
        </n-tooltip>
        <div class="account-heading">
          <div class="account-name-row">
            <span class="account-name">{{ account.name }}</span>
            <n-tag v-if="account.account_type === 'managed'" type="info" size="small" :bordered="false">
              {{ t("托管注册") }}
            </n-tag>
            <n-tag v-if="isZen" type="info" size="small" :bordered="false">
              {{ t("免费通道") }}
            </n-tag>
            <n-tag
              v-else
              :type="isDraft ? 'warning' : 'default'"
              size="small"
              :bordered="false"
            >
              {{ planLabel(account, catalog) }}
            </n-tag>
            <n-tooltip v-if="account.auth_error || isCooling(account, now)">
              <template #trigger>
                <n-tag :type="accountStatusTagType(account, now)" size="small">
                  {{ accountStatusLabel(account, now) }}
                </n-tag>
              </template>
              {{ account.auth_error || cooldownDetails(account, now, limits) }}
            </n-tooltip>
            <n-tag v-else :type="accountStatusTagType(account, now)" size="small">
              {{ accountStatusLabel(account, now) }}
            </n-tag>
            <n-tag v-if="isGo && accountIsReady(account)" size="small" :bordered="false">
              {{ t("购买于 {date}", { date: account.purchase_date }) }}
            </n-tag>
            <n-tag v-if="isGo && accountIsReady(account)" size="small" :bordered="false">
              {{ t("到期于 {date}", { date: account.expires_on }) }}
            </n-tag>
            <n-tag
              v-if="isGo && accountIsReady(account)"
              :type="accountExpiryTagType(account, now)"
              size="small"
              :bordered="false"
            >
              {{ accountExpiryLabel(account, now) }}
            </n-tag>
          </div>
        </div>
      </div>
    </template>

    <template #header-extra>
      <div class="account-actions">
        <div v-if="accountIsReady(account)" class="account-action account-action--enabled">
          <n-tooltip trigger="hover">
            <template #trigger>
              <n-switch
                :value="account.enabled"
                :disabled="!!toggleBlockedReason"
                :aria-label="account.enabled ? t('禁用账号 {name}', { name: account.name }) : t('启用账号 {name}', { name: account.name })"
                @update:value="emit('toggle')"
              />
            </template>
            {{ toggleBlockedReason || (account.enabled
              ? t("禁用账号 {name}", { name: account.name })
              : t("启用账号 {name}", { name: account.name })) }}
          </n-tooltip>
        </div>

        <div v-if="(isGo || isOfficialCn) && accountIsReady(account)" class="account-action account-action--secondary">
          <n-tooltip trigger="hover">
            <template #trigger>
              <n-button
                circle
                quaternary
                size="small"
                :aria-label="t('刷新额度')"
                :loading="usageRefreshLoading"
                :disabled="isUsageRefreshBlocked(account, now) || usageLoading || !!usageLoadError"
                @click="emit('refresh-usage')"
              >
                <template #icon><n-icon :component="ReloadOutlined" /></template>
              </n-button>
            </template>
            {{ isOfficialCn ? t("刷新额度") : usageRefreshTooltip(account, now) }}
          </n-tooltip>
        </div>

        <div
          v-if="manualUsageCalibration && accountIsReady(account) && edits"
          class="account-action account-action--secondary"
        >
          <n-popover
            trigger="click"
            placement="bottom-end"
            :show-arrow="false"
            :width="320"
            style="max-width: calc(100vw - 64px)"
            @update:show="(show: boolean) => show && emit('usage-editor-open')"
          >
          <template #trigger>
            <n-tooltip trigger="hover">
              <template #trigger>
                <n-button
                  circle
                  quaternary
                  size="small"
                  :aria-label="t('校准用量')"
                  :disabled="!usageEditorAvailable"
                >
                  <template #icon><n-icon :component="EditOutlined" /></template>
                </n-button>
              </template>
              {{ t("校准用量") }}
            </n-tooltip>
          </template>

            <AccountUsageEditor
              :account="account"
              :usage="usage"
              :limits="limits"
              :edits="edits!"
              :loading="usageLoading"
              :now="now"
              @update-draft="(key, value) => emit('usage-update-draft', key, value)"
              @update-resets-first="(key, value) => emit('usage-update-resets-first', key, value)"
              @update-resets-second="(key, value) => emit('usage-update-resets-second', key, value)"
              @save="(key) => emit('usage-save', key)"
            />
          </n-popover>
        </div>

        <div v-if="menuOptions.length > 0" class="account-action account-action--menu">
          <n-dropdown
            :options="menuOptions"
            trigger="click"
            placement="bottom-end"
            @select="(key: string | number) => emit('menu-select', key)"
          >
            <n-tooltip trigger="hover">
              <template #trigger>
                <n-button
                  circle
                  quaternary
                  size="small"
                  :aria-label="t('更多操作')"
                >
                  <template #icon><n-icon :component="MoreOutlined" /></template>
                </n-button>
              </template>
              {{ t("更多操作") }}
            </n-tooltip>
          </n-dropdown>
        </div>
      </div>
    </template>

    <n-alert
      v-if="planWarning"
      class="account-plan-warning"
      :type="planWarning.type"
      :title="planWarning.title"
      :show-icon="false"
    >
      {{ planWarning.message }}
    </n-alert>

    <n-alert
      v-if="verificationError"
      class="account-plan-warning"
      type="error"
      :show-icon="false"
      :title="t('验证失败')"
    >
      {{ verificationError }}
    </n-alert>

    <div v-if="!accountIsReady(account)" class="managed-pending">
      <div>
        <strong>{{ managedStepLabel(account.setup_step) }}</strong>
        <p>{{ t("注册进度已保存。继续后仍会使用该账号自己的浏览器 Profile。") }}</p>
      </div>
      <n-button type="primary" secondary @click="emit('open-wizard')">
        {{ t("继续注册") }}
      </n-button>
    </div>
    <div v-else-if="isDraft" class="provider-unconfigured" role="status">
      <p>{{ draftDescription }}</p>
    </div>
    <div v-else-if="manualUsageCalibration" class="manual-usage-block">
      <div v-if="usageLoadError" class="usage-load-error" role="alert">
        <span>{{ t("用量加载失败") }}</span>
        <n-button
          text
          size="tiny"
          type="primary"
          :loading="usageLoading"
          @click="emit('reload-usage')"
        >
          {{ t("重试") }}
        </n-button>
      </div>
      <UsageStrip
        v-else
        :account="account"
        :usage="usage"
        :limits="limits"
        :editing="!!edits"
      />
      <p v-if="!usageLoadError" class="usage-sync-meta">
        {{ t("根据 OCG 内已定价请求估算；不含其他客户端用量，可手工校准。") }}
      </p>
    </div>
    <div v-else-if="isCustom" class="custom-endpoint">
      <div class="custom-endpoint__meta">
        <span v-if="account.custom_config?.endpoint_url" class="custom-endpoint__url">
          {{ account.custom_config.endpoint_url }}
        </span>
        <span class="custom-endpoint__models">
          {{ t("{count} 个模型", { count: account.model_capabilities.length }) }}
        </span>
      </div>
      <p
        v-if="verificationCaption"
        class="custom-endpoint__status"
        :class="{ 'custom-endpoint__status--unverified': customNeedsVerification }"
      >{{ verificationCaption }}</p>
      <n-button
        v-if="customNeedsVerification"
        size="small"
        type="primary"
        secondary
        :loading="verifying"
        :aria-label="t('验证连接')"
        @click="emit('verify')"
      >
        {{ t("验证连接") }}
      </n-button>
    </div>
    <div v-else-if="isGo && !quotaLimitsFailed">
      <div v-if="usageLoadError" class="usage-load-error" role="alert">
        <span>{{ t("用量加载失败") }}</span>
        <n-button
          text
          size="tiny"
          type="primary"
          :loading="usageLoading"
          @click="emit('reload-usage')"
        >
          {{ t("重试") }}
        </n-button>
      </div>
      <UsageStrip
        v-else
        :account="account"
        :usage="usage"
        :limits="limits"
        :editing="!!edits"
      />
      <p
        v-if="!usageLoadError"
        class="usage-sync-meta"
      >
        {{ usageSyncCaption(account, now) }}
      </p>
    </div>
    <div v-else-if="isOfficialCn" class="official-plan-usage">
      <div v-if="usageLoadError" class="usage-load-error" role="alert">
        <span>{{ t("用量加载失败") }}</span>
        <n-button text size="tiny" type="primary" :loading="usageLoading" @click="emit('reload-usage')">
          {{ t("重试") }}
        </n-button>
      </div>
      <ProviderQuotaSummary v-else :usage="providerUsage" />
    </div>

  </n-card>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  NAlert,
  NButton,
  NCard,
  NDropdown,
  NIcon,
  NPopover,
  NSwitch,
  NTag,
  NTooltip,
} from "naive-ui";
import {
  EditOutlined,
  HolderOutlined,
  MoreOutlined,
  ReloadOutlined,
} from "@vicons/antd";
import type { Account, UsageWindow } from "../api/dashboard";
import type { ProviderCatalogEntry, ProviderUsageResponse } from "../api/providers.ts";
import { isCooling, isUsageLimitReached } from "../domain/accounts-usage.ts";
import type { UsageKey } from "../domain/accounts-usage.ts";
import {
  accountExpiryLabel,
  accountExpiryTagType,
  accountIsReady,
  accountRoutingDraftDescription,
  accountStatusLabel,
  accountStatusTagType,
  cooldownDetails,
  isUsageRefreshBlocked,
  managedStepLabel,
  usageRefreshTooltip,
  usageSyncCaption,
} from "../domain/account-display.ts";
import type { AccountMenuOption } from "../domain/account-display.ts";
import { isOfficialCnPlanAccount, isZenFreeAccount } from "../domain/account-providers.ts";
import {
  customAccountNeedsVerification,
  isCustomApiAccount,
} from "../domain/custom-account.ts";
import { accountPlanWarning, planLabel } from "../domain/plans.ts";
import type { AccountUsageEdits, UsageLimitView } from "../domain/useAccountUsage.ts";
import { t } from "../i18n/index.ts";
import AccountUsageEditor from "./AccountUsageEditor.vue";
import UsageStrip from "./UsageStrip.vue";
import ProviderQuotaSummary from "./ProviderQuotaSummary.vue";

const props = defineProps<{
  account: Account;
  catalog: readonly ProviderCatalogEntry[] | null;
  usage: UsageWindow;
  providerUsage: ProviderUsageResponse | null;
  limits: UsageLimitView[];
  edits: AccountUsageEdits | undefined;
  now: number;
  orderHandleDisabled: boolean;
  dragging: boolean;
  usageLoading: boolean;
  usageLoadError: string | null;
  usageRefreshLoading: boolean;
  /** Connection verification in flight for a pending/failed Custom account. */
  verifying: boolean;
  quotaLimitsFailed: boolean;
  menuOptions: AccountMenuOption[];
}>();

const emit = defineEmits<{
  "order-keydown": [event: KeyboardEvent];
  "order-drag-start": [event: PointerEvent];
  toggle: [];
  verify: [];
  "refresh-usage": [];
  "reload-usage": [];
  "open-wizard": [];
  "menu-select": [key: string | number];
  "usage-editor-open": [];
  "usage-update-draft": [key: UsageKey, value: number | null];
  "usage-update-resets-first": [key: UsageKey, value: number | null];
  "usage-update-resets-second": [key: UsageKey, value: number | null];
  "usage-save": [key: UsageKey];
}>();

const isZen = computed(() => isZenFreeAccount(props.account));
const isGo = computed(() => (
  props.account.provider_id === "opencode" && props.account.offering_id === "go"
));
const isCustom = computed(() => isCustomApiAccount(props.account));
const isOfficialCn = computed(() => isOfficialCnPlanAccount(props.account));
const plan = computed(() => props.catalog?.find((entry) => (
  entry.provider_id === props.account.provider_id
  && entry.offering_id === props.account.offering_id
)));
// Manual calibration display is catalog-driven: no hardcoded per-plan meters.
const manualUsageCalibration = computed(() => (
  plan.value?.manual_usage_calibration ?? false
));
const customNeedsVerification = computed(() => customAccountNeedsVerification(props.account));
const toggleBlockedReason = computed(() => {
  if (!props.account.plan_routable) return t("该方案暂不可路由");
  return "";
});
const verificationCaption = computed(() => {
  const status = props.account.verification_status;
  if (status === "pending") {
    // Custom verification is optional: pending accounts may be enabled, so the
    // card shows an unverified hint instead of a hard gate.
    return isCustom.value
      ? t("尚未验证连接：账号可先启用，也可先验证连接。")
      : t("账号待验证，验证通过前保持禁用。");
  }
  if (status === "failed") return t("上次验证失败，请检查 Key 与端点配置后重试。");
  if (status === "verified") {
    const at = props.account.connection_verified_at;
    const ts = at ? Date.parse(at) : NaN;
    return Number.isFinite(ts)
      ? t("连接已验证：{time}", { time: new Date(ts).toLocaleString() })
      : t("连接已验证");
  }
  return "";
});
const isDraft = computed(() => (
  accountIsReady(props.account)
  && !props.account.plan_routable
));

const draftDescription = computed(() => {
  const key = accountRoutingDraftDescription(props.account);
  return key ? t(key) : "";
});

const verificationError = computed(() => {
  if (!isCustom.value) return null;
  if (props.account.verification_status !== "failed") return null;
  return props.account.verification_error?.trim() || t("验证失败");
});

const planWarning = computed(() => {
  const warning = accountPlanWarning(props.account);
  if (warning === "endpoint-risk") {
    return {
      type: "warning" as const,
      title: t("自定义端点"),
      message: t("目标端点由管理员自行选择并负责：使用 http:// 时 Key 将明文传输；验证连接会发送一次最小真实请求，可能产生服务商费用。"),
    };
  }
  if (warning === "subscription-risk") {
    return {
      type: "warning" as const,
      title: t("订阅制"),
      message: t("订阅制方案：额度、计费与续费由服务商订阅条款管理。"),
    };
  }
  return null;
});

const usageEditorAvailable = computed(() => {
  if (props.usageLoading || props.usageLoadError) return false;
  return props.limits.some(({ key }) => !isUsageLimitReached(props.account, key, props.now));
});
</script>

<style scoped>
.account-card {
  border-radius: 14px;
  box-shadow: var(--ocg-shadow-sm);
  transition: border-color 0.16s ease, box-shadow 0.16s ease, opacity 0.16s ease;
}

.account-card--cooling {
  border-color: color-mix(in srgb, var(--ocg-error) 45%, transparent);
}

.account-card--pending,
.account-card--draft {
  border-color: color-mix(in srgb, var(--ocg-primary) 32%, var(--ocg-divider));
}

.account-card--dragging {
  border-color: var(--ocg-primary);
  box-shadow: 0 10px 28px color-mix(in srgb, var(--ocg-primary) 18%, transparent);
  opacity: 0.72;
}

.provider-unconfigured {
  color: var(--ocg-warning);
  font-size: var(--ocg-font-sm);
}

.provider-unconfigured > p {
  margin: 0;
}

.manual-usage-block {
  display: grid;
  gap: 8px;
  margin-top: 10px;
}

.official-plan-usage {
  margin-top: 10px;
}

.account-actions {
  display: grid;
  grid-template-columns: repeat(4, 40px);
  align-items: center;
  justify-content: end;
  column-gap: 8px;
}

.account-action {
  display: flex;
  align-items: center;
  justify-content: center;
  min-width: 0;
}

.account-action--enabled {
  grid-column: 1;
}

.account-action--secondary {
  grid-column: 2;
}

.account-action--menu {
  grid-column: 4;
}

.custom-endpoint {
  display: grid;
  justify-items: start;
  gap: 8px;
}

.custom-endpoint__meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 12px;
  min-width: 0;
}

.custom-endpoint__url {
  overflow-wrap: anywhere;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-sm);
}

.custom-endpoint__models {
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}

.custom-endpoint__status {
  margin: 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}

.custom-endpoint__status--unverified {
  color: var(--ocg-warning);
}

.account-plan-warning {
  margin-bottom: 12px;
}

.account-title {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  width: 100%;
}

.account-order-handle {
  flex: 0 0 auto;
  cursor: grab;
  touch-action: none;
  user-select: none;
}

.account-order-handle--dragging {
  cursor: grabbing;
}

.account-heading {
  display: flex;
  align-items: center;
  flex: 1 1 auto;
  min-width: 0;
}

.account-name-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 6px;
  min-width: 0;
}

.account-name {
  overflow: hidden;
  color: var(--ocg-ink);
  font-size: var(--ocg-font-md);
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.account-name-row :deep(.n-tag) {
  flex: 0 0 auto;
}

.managed-pending {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 10px 2px 2px;
}

.managed-pending strong {
  color: var(--ocg-ink);
  font-size: var(--ocg-font-md);
}

.managed-pending p {
  margin: 4px 0 0;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}

.usage-sync-meta {
  margin: 8px 0 0;
  color: var(--ocg-text-3);
  font-size: var(--ocg-font-size-12);
  line-height: 1.4;
}

.usage-load-error {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 42px;
  color: var(--ocg-error);
  font-size: var(--ocg-font-sm);
}

@media (max-width: 900px) {
  .account-card :deep(.n-card-header) {
    align-items: flex-start;
  }

  .account-card :deep(.n-card-header__extra) {
    margin-left: 8px;
  }
}

@media (max-width: 640px) {
  .managed-pending {
    align-items: stretch;
    flex-direction: column;
  }

  .account-card :deep(.n-card-header) {
    flex-wrap: wrap;
    gap: 8px;
  }

  .account-card :deep(.n-card-header__main),
  .account-card :deep(.n-card-header__extra) {
    width: 100%;
  }

  .account-card :deep(.n-card-header__extra) {
    display: flex;
    justify-content: flex-end;
    margin-left: 0;
  }
}
</style>
