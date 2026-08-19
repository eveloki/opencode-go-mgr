<template>
  <n-card
    :data-account-id="account.id"
    size="small"
    class="account-card"
    :class="{
      'account-card--cooling': isCooling(account, now),
      'account-card--pending': !accountIsReady(account),
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
              :type="isGoat ? 'warning' : 'default'"
              size="small"
              :bordered="false"
            >
              {{ providerOfferingLabel(account) }}
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
            <n-tag v-if="!isZen && accountIsReady(account)" size="small" :bordered="false">
              {{ t("购买于 {date}", { date: account.purchase_date }) }}
            </n-tag>
            <n-tag v-if="!isZen && accountIsReady(account)" size="small" :bordered="false">
              {{ t("到期于 {date}", { date: account.expires_on }) }}
            </n-tag>
            <n-tag
              v-if="!isZen && accountIsReady(account)"
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
      <n-space align="center" :size="8">
        <n-tooltip v-if="isGo && accountIsReady(account)" trigger="hover">
          <template #trigger>
            <n-button
              circle
              quaternary
              size="small"
              :aria-label="t('测试账号 {name}', { name: account.name })"
              :loading="pinging"
              @click="emit('ping')"
            >
              <template #icon><n-icon :component="ThunderboltOutlined" /></template>
            </n-button>
          </template>
          {{ t("测试连接") }}
        </n-tooltip>

        <n-tooltip v-if="accountIsReady(account)" trigger="hover">
          <template #trigger>
            <n-switch
              :value="account.enabled"
              :aria-label="account.enabled ? t('禁用账号 {name}', { name: account.name }) : t('启用账号 {name}', { name: account.name })"
              @update:value="emit('toggle')"
            />
          </template>
          {{ account.enabled ? t("禁用账号 {name}", { name: account.name }) : t("启用账号 {name}", { name: account.name }) }}
        </n-tooltip>

        <n-tooltip v-if="isZen && accountIsReady(account) && account.enabled" trigger="hover">
          <template #trigger>
            <n-switch
              :value="account.free_alias_enabled"
              :loading="freeAliasSaving"
              :aria-label="account.free_alias_enabled
                ? t('禁用 {name} 的 Free 别名', { name: account.name })
                : t('启用 {name} 的 Free 别名', { name: account.name })"
              @update:value="emit('toggle-free-alias')"
            />
          </template>
          {{ account.free_alias_enabled
            ? t("禁用 {name} 的 Free 别名", { name: account.name })
            : t("启用 {name} 的 Free 别名", { name: account.name }) }}
        </n-tooltip>

        <n-tooltip
          v-if="isGo && accountIsReady(account)"
          trigger="hover"
        >
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
          {{ usageRefreshTooltip(account, now) }}
        </n-tooltip>

        <n-popover
          v-if="isGo && accountIsReady(account) && edits"
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

        <n-dropdown
          v-if="menuOptions.length > 0"
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
      </n-space>
    </template>

    <div v-if="!accountIsReady(account)" class="managed-pending">
      <div>
        <strong>{{ managedStepLabel(account.setup_step) }}</strong>
        <p>{{ t("注册进度已保存。继续后仍会使用该账号自己的浏览器 Profile。") }}</p>
      </div>
      <n-button type="primary" secondary @click="emit('open-wizard')">
        {{ t("继续注册") }}
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
    <div v-else-if="isGoat" class="provider-unconfigured" role="status">
      {{ t("供应商尚未配置") }}
    </div>
  </n-card>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  NButton,
  NCard,
  NDropdown,
  NIcon,
  NPopover,
  NSpace,
  NSwitch,
  NTag,
  NTooltip,
} from "naive-ui";
import {
  EditOutlined,
  HolderOutlined,
  MoreOutlined,
  ReloadOutlined,
  ThunderboltOutlined,
} from "@vicons/antd";
import type { Account, UsageWindow } from "../api/tauri";
import { isCooling, isUsageLimitReached } from "../views/accounts-usage.ts";
import type { UsageKey } from "../views/accounts-usage.ts";
import {
  accountExpiryLabel,
  accountExpiryTagType,
  accountIsReady,
  accountStatusLabel,
  accountStatusTagType,
  cooldownDetails,
  isUsageRefreshBlocked,
  managedStepLabel,
  usageRefreshTooltip,
  usageSyncCaption,
} from "../views/account-display.ts";
import type { AccountMenuOption } from "../views/account-display.ts";
import { isZenFreeAccount, providerOfferingLabel } from "../views/account-providers.ts";
import type { AccountUsageEdits, UsageLimitView } from "../views/useAccountUsage.ts";
import { t } from "../i18n/index.ts";
import AccountUsageEditor from "./AccountUsageEditor.vue";
import UsageStrip from "./UsageStrip.vue";

const props = defineProps<{
  account: Account;
  usage: UsageWindow;
  limits: UsageLimitView[];
  edits: AccountUsageEdits | undefined;
  now: number;
  orderHandleDisabled: boolean;
  dragging: boolean;
  pinging: boolean;
  usageLoading: boolean;
  usageLoadError: string | null;
  usageRefreshLoading: boolean;
  freeAliasSaving: boolean;
  quotaLimitsFailed: boolean;
  menuOptions: AccountMenuOption[];
}>();

const emit = defineEmits<{
  "order-keydown": [event: KeyboardEvent];
  "order-drag-start": [event: PointerEvent];
  ping: [];
  toggle: [];
  "toggle-free-alias": [];
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
const isGoat = computed(() => (
  props.account.provider_id === "command-code" && props.account.offering_id === "goat"
));

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

.account-card--pending {
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
