<template>
  <n-modal
    :show="show"
    preset="card"
    :title="t('新增账号')"
    class="account-add-modal"
    style="width: 760px; max-width: calc(100vw - 32px)"
    @update:show="$emit('update:show', $event)"
  >
    <div v-if="catalogLoading" class="account-add-loading">
      <n-spin size="large" :description="t('加载中…')" />
    </div>

    <div v-else-if="!catalog" class="account-add-grid">
      <button type="button" class="account-add-option" @click="$emit('importKey')">
        <n-icon :component="KeyOutlined" size="28" aria-hidden="true" />
        <span class="account-add-option__title">{{ t("导入已有 Key") }}</span>
        <span>{{ t("已有 OpenCode Go Key，直接添加并参与账号路由。") }}</span>
      </button>
      <n-tooltip :disabled="managedAvailable">
        <template #trigger>
          <button
            type="button"
            class="account-add-option"
            :class="{ 'account-add-option--disabled': !managedAvailable }"
            :disabled="!managedAvailable"
            @click="$emit('registerManaged')"
          >
            <n-icon :component="UserAddOutlined" size="28" aria-hidden="true" />
            <span class="account-add-option__title">{{ t("注册新账号（Beta）") }}</span>
            <span>{{ t("独立 Profile：登录 → 邀请 → 支付 → 验证 Key。") }}</span>
          </button>
        </template>
        {{ managedReason }}
      </n-tooltip>
    </div>

    <div v-else class="account-add-grid">
      <div
        v-for="option in planOptions"
        :key="option.plan.id"
        class="account-add-option"
        :class="{
          'account-add-option--disabled': option.disabled,
        }"
      >
        <div class="account-add-option__header">
          <n-icon :component="planIcon(option.plan.id)" size="28" aria-hidden="true" />
          <div class="account-add-option__titles">
            <span class="account-add-option__title">{{ option.label }}</span>
            <n-space v-if="planKindTag(option.plan)" :size="6">
              <n-tag
                v-if="planKindTag(option.plan)"
                size="small"
                :bordered="false"
                :type="planKindTag(option.plan)!.type"
              >
                {{ planKindTag(option.plan)!.label }}
              </n-tag>
            </n-space>
          </div>
        </div>

        <span
          v-if="planDescription(option.plan)"
          class="account-add-option__description"
        >
          {{ planDescription(option.plan) }}
        </span>

        <n-tag
          v-if="option.disabled"
          size="small"
          type="warning"
          class="account-add-option__reason"
        >
          {{ option.disabledReason ? t(option.disabledReason) : "" }}
        </n-tag>

        <n-tag
          v-else-if="option.creationHint"
          size="small"
          type="default"
          class="account-add-option__reason"
        >
          {{ option.creationHint ? t(option.creationHint) : "" }}
        </n-tag>

        <n-space v-if="option.managed" :size="8" class="account-add-option__actions">
          <n-button size="small" secondary @click.stop="$emit('importKey')">
            {{ t("导入已有 Key") }}
          </n-button>
          <n-tooltip :disabled="managedAvailable">
            <template #trigger>
              <n-button
                size="small"
                type="primary"
                :disabled="!managedAvailable"
                @click.stop="managedAvailable && $emit('registerManaged')"
              >
                {{ t("注册新账号（Beta）") }}
              </n-button>
            </template>
            {{ managedReason }}
          </n-tooltip>
        </n-space>

        <n-space v-else-if="!option.disabled" :size="8" class="account-add-option__actions">
          <n-button
            size="small"
            :type="option.plan.id === 'custom-endpoint' ? 'primary' : 'default'"
            @click.stop="handleSelect(option)"
          >
            {{ t(planActionLabel(option.plan)) }}
          </n-button>
        </n-space>
      </div>
    </div>

    <n-alert v-if="!catalogLoading && !catalog && !managedAvailable" type="warning" class="account-add-hint">
      <div class="account-add-hint__content">
        <span>{{ managedReason }}</span>
        <n-button v-if="inviteMissing" text type="primary" @click="$emit('openSettings')">
          {{ t("前往设置邀请链接") }}
        </n-button>
      </div>
    </n-alert>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, toRef } from "vue";
import {
  NAlert,
  NButton,
  NIcon,
  NModal,
  NSpace,
  NSpin,
  NTag,
  NTooltip,
} from "naive-ui";
import {
  KeyOutlined,
  UserAddOutlined,
  CloudOutlined,
  GiftOutlined,
  ApiOutlined,
  CreditCardOutlined,
  SwapOutlined,
} from "@vicons/antd";
import type { Component } from "vue";
import { t, type MessageKey } from "../i18n/index.ts";
import { useLocalizedModalCloseLabel } from "../utils/modal-close-label.ts";
import { buildPlanOptions, type PlanOption } from "../views/account-plan-options.ts";
import type { PlanDefinition } from "../views/plans.ts";
import type { ProviderCatalogEntry } from "../api/providers.ts";

const props = defineProps<{
  show: boolean;
  catalog: readonly ProviderCatalogEntry[] | null | undefined;
  catalogLoading: boolean;
  managedAvailable: boolean;
  managedReason: string;
  inviteMissing: boolean;
}>();

const emit = defineEmits<{
  (event: "update:show", value: boolean): void;
  (event: "importKey"): void;
  (event: "registerManaged"): void;
  (event: "openSettings"): void;
  (event: "selectPlan", plan: PlanDefinition): void;
}>();

useLocalizedModalCloseLabel(toRef(props, "show"), "account-add-modal");

const planOptions = computed(() => buildPlanOptions(props.catalog));

const ICONS: Record<string, Component> = {
  "opencode-go": CloudOutlined,
  "zen-free": GiftOutlined,
  "command-code-goat": ApiOutlined,
  scnet: CreditCardOutlined,
  "custom-endpoint": SwapOutlined,
};

function planIcon(planId: string): Component {
  return ICONS[planId] ?? KeyOutlined;
}

function planKindTag(plan: PlanDefinition): { label: string; type: "warning" | "default" } | null {
  if (plan.kind === "subscription") return { label: t("订阅制"), type: "warning" };
  if (plan.kind === "custom") return { label: t("自定义端点"), type: "default" };
  return null;
}

function planDescription(plan: PlanDefinition): string {
  switch (plan.id) {
    case "opencode-go":
      return t("已有 OpenCode Go Key，直接添加并参与账号路由。");
    case "zen-free":
      return t("Zen Free 已由系统管理，请在账号列表中启用。");
    case "scnet":
      return t("订阅制方案：额度、计费与续费由服务商订阅条款管理。");
    case "custom-endpoint":
      return t("自定义端点由你自行维护，Gateway 无法验证其价格、额度与协议兼容性。");
    default:
      return "";
  }
}

function planActionLabel(plan: PlanDefinition): MessageKey {
  if (plan.id === "custom-endpoint") return "添加账号";
  if (plan.id === "command-code-goat" || plan.id === "scnet") return "创建草稿";
  return "添加账号";
}

function handleSelect(option: PlanOption): void {
  if (option.disabled || option.managed) return;
  emit("selectPlan", option.plan);
}
</script>

<style scoped>
.account-add-loading {
  display: grid;
  place-items: center;
  min-height: 220px;
}

.account-add-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
}

.account-add-option {
  display: grid;
  align-content: start;
  gap: 10px;
  min-height: 180px;
  padding: 22px;
  border: 1px solid var(--ocg-divider);
  border-radius: 14px;
  color: var(--ocg-muted);
  font: inherit;
  text-align: left;
  background: var(--ocg-surface);
  cursor: default;
}

button.account-add-option {
  cursor: pointer;
  transition: border-color 0.16s ease, box-shadow 0.16s ease, transform 0.16s ease;
}

button.account-add-option:not(:disabled):hover,
button.account-add-option:not(:disabled):focus-visible {
  border-color: var(--ocg-primary);
  box-shadow: 0 8px 24px color-mix(in srgb, var(--ocg-primary) 14%, transparent);
  transform: translateY(-1px);
  outline: none;
}

.account-add-option__header {
  display: flex;
  align-items: center;
  gap: 12px;
}

.account-add-option__header :deep(.n-icon) {
  color: var(--ocg-primary);
}

.account-add-option__titles {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.account-add-option__title {
  color: var(--ocg-ink);
  font-size: var(--ocg-font-lg);
  font-weight: 700;
}

.account-add-option__description {
  font-size: var(--ocg-font-sm);
  line-height: 1.5;
}

.account-add-option__reason {
  justify-self: start;
}

.account-add-option__actions {
  align-self: end;
}

.account-add-option--disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.account-add-hint {
  margin-top: 14px;
}

.account-add-hint__content {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

@media (max-width: 640px) {
  .account-add-grid {
    grid-template-columns: 1fr;
  }

  .account-add-option {
    min-height: 140px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .account-add-option {
    transition: none;
  }
}
</style>
