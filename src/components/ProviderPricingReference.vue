<template>
  <div class="provider-pricing-reference">
    <div class="provider-pricing-reference__meta">
      <n-tag type="warning" size="small" :bordered="false">
        {{ kind === "goat" ? t("订阅制") : t("已归档") }}
      </n-tag>
      <n-button
        tag="a"
        text
        :href="sourceUrl"
        target="_blank"
        rel="noopener noreferrer"
      >{{ t("官方来源") }}</n-button>
    </div>

    <template v-if="kind === 'goat'">
      <GoatQuotaReference :snapshot="snapshot" />
    </template>

    <template v-else>
      <n-alert type="info" :show-icon="false">
        {{ t("SCNet Token Plan 已归档：历史草稿仅供查看，不支持验证、启用、路由或用量。") }}
      </n-alert>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { NAlert, NButton, NTag } from "naive-ui";
import { t } from "../i18n/index.ts";
import {
  GOAT_PRICING_REFERENCE,
  SCNET_PRICING_REFERENCE,
} from "../domain/pricing-references.ts";
import GoatQuotaReference from "./GoatQuotaReference.vue";
import type { ProviderNeutralPricingSnapshot } from "../api/providers.ts";

const props = defineProps<{
  kind: "goat" | "scnet";
  snapshot?: ProviderNeutralPricingSnapshot | null;
}>();

const sourceUrl = computed(() => props.kind === "goat"
  ? props.snapshot?.source_url ?? GOAT_PRICING_REFERENCE.sourceUrl
  : SCNET_PRICING_REFERENCE.sourceUrl);
</script>

<style scoped>
.provider-pricing-reference {
  display: grid;
  gap: 16px;
}

.provider-pricing-reference__meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.reference-note {
  margin: 0;
  color: var(--ocg-muted);
}

@media (max-width: 640px) {
  .provider-pricing-reference__meta {
    align-items: flex-start;
    flex-direction: column;
  }
}
</style>
