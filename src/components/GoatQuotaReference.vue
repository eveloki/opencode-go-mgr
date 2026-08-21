<template>
  <div
    class="goat-quota-reference"
    role="group"
    :aria-label="t('GOAT 官方套餐额度；实时用量请在 Command Code CLI 运行 /usage 查看。')"
  >
    <div v-for="quota in quotas" :key="quota.label" class="goat-quota-reference__item">
      <div class="goat-quota-reference__label">
        <span>{{ t(quota.label) }}</span>
        <strong>{{ quota.amount }}</strong>
      </div>
      <div class="goat-quota-reference__track" aria-hidden="true">
        <span class="goat-quota-reference__capacity" />
      </div>
    </div>
    <p>{{ t("GOAT 官方套餐额度；实时用量请在 Command Code CLI 运行 /usage 查看。") }}</p>
  </div>
</template>

<script setup lang="ts">
import type { MessageKey } from "../i18n/index.ts";
import { t } from "../i18n/index.ts";

const quotas: readonly { label: MessageKey; amount: string }[] = [
  { label: "5 小时额度", amount: "$14" },
  { label: "周额度", amount: "$35" },
  { label: "月额度", amount: "$70" },
];
</script>

<style scoped>
.goat-quota-reference {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px 14px;
  margin-top: 10px;
}

.goat-quota-reference__item {
  min-width: 0;
}

.goat-quota-reference__label {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  color: var(--ocg-muted);
  font-size: var(--ocg-font-sm);
}

.goat-quota-reference__label strong {
  color: var(--ocg-ink);
  font-variant-numeric: tabular-nums;
}

.goat-quota-reference__track {
  height: 7px;
  margin-top: 6px;
  overflow: hidden;
  border-radius: 999px;
  background: color-mix(in srgb, var(--ocg-primary) 14%, var(--ocg-divider));
}

.goat-quota-reference__capacity {
  display: block;
  width: 100%;
  height: 100%;
  border-radius: inherit;
  background: color-mix(in srgb, var(--ocg-primary) 72%, var(--ocg-success));
}

.goat-quota-reference p {
  grid-column: 1 / -1;
  margin: 0;
  color: var(--ocg-text-3);
  font-size: var(--ocg-font-size-12);
}

@media (max-width: 640px) {
  .goat-quota-reference {
    grid-template-columns: 1fr;
  }
}
</style>
